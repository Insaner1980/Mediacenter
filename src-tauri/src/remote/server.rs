//! Disabled-by-default native LAN listener. Task 6 supplies selected interfaces.
use super::{
    adapters::checked,
    auth::AuthManager,
    dispatch::{self, Dispatcher},
    event_hub::Subscription,
    limits::*,
    pairing::{PairEndpointRequest, PairingManager, PairingReady},
    protocol::*,
    tls::ServerIdentity,
};
use crate::control::ControlServices;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame, Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

type Body = Full<Bytes>;

/// Native-only validated binding input. No deserializer/IPC command exposes it.
/// `local_addresses` must come from Task 6's selected interface enumeration.
pub struct BindAddresses(Vec<SocketAddr>);
/// Namespace defaults; alternative ports require an explicit native choice.
pub fn default_port(namespace: super::tls::IdentityNamespace) -> u16 {
    match namespace {
        super::tls::IdentityNamespace::Production => 47831,
        super::tls::IdentityNamespace::Development => 47832,
    }
}
impl BindAddresses {
    #[cfg(test)]
    pub(super) fn loopback_fixture(port: u16) -> Self {
        Self(vec![SocketAddr::from(([127, 0, 0, 1], port))])
    }
    pub fn selected(
        addresses: Vec<SocketAddr>,
        local_addresses: &[IpAddr],
    ) -> Result<Self, RemoteError> {
        if addresses.is_empty() || addresses.len() > 8 || addresses.iter().enumerate().any(|(i,a)| {
            a.port()==0 || a.port()!=addresses[0].port() || addresses[..i].contains(a) || !local_addresses.contains(&a.ip()) || !private_address(a.ip()) || matches!(a, SocketAddr::V6(a) if a.ip().is_unicast_link_local() && a.scope_id()==0)
        }) { return Err(RemoteError::invalid()); }
        Ok(Self(addresses))
    }
}
fn private_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

/// Own one runtime for the process/identity and reuse it across listener enables.
/// Identity is loaded before ControlServices; recovery occurs once before admission.
pub struct ServerRuntime {
    identity: Arc<ServerIdentity>,
    pub(super) dispatcher: Dispatcher,
    auth: Arc<AuthManager>,
    pairing: Arc<Mutex<PairingManager>>,
    handshakes: Arc<Semaphore>,
    sockets: Arc<Semaphore>,
    thumbnails: DeviceLimits,
}
impl ServerRuntime {
    pub fn new(
        identity: Arc<ServerIdentity>,
        mut services: ControlServices,
    ) -> Result<Arc<Self>, RemoteError> {
        if identity.id() != &services.server_identity {
            return Err(RemoteError::new(
                ErrorCode::IdentityChanged,
                "Native identity changed.",
            ));
        }
        let registration = services.state.event_hub.register_transport()?;
        services.state.event_hub.recover_transport_once(|| {
            services
                .operations()
                .recover_unfinished()
                .map(|_| ())
                .map_err(dispatch::native_error)
        })?;
        let auth = Arc::new(AuthManager::new(
            identity.id(),
            services.state.event_hub.clone(),
        )?);
        let pairing = Arc::new(Mutex::new(PairingManager::new(&identity)));
        services.admin = Some(Arc::new(super::settings_admin::RemoteAdmin::new(
            auth.clone(),
            pairing.clone(),
            &services,
        )));
        let mut dispatcher = Dispatcher::new(services);
        dispatcher.registration = Some(registration);
        Ok(Arc::new(Self {
            identity,
            dispatcher,
            auth,
            pairing,
            handshakes: Arc::new(Semaphore::new(16)),
            sockets: Arc::new(Semaphore::new(4)),
            thumbnails: DeviceLimits::default(),
        }))
    }
    /// Explicit native setup; remote issuance uses its own confirmed admin boundary.
    pub fn begin_pairing(
        &self,
        addresses: LocalAddresses,
        port: Port,
    ) -> Result<PairingReady, RemoteError> {
        let admin = self
            .dispatcher
            .services
            .admin
            .as_ref()
            .ok_or_else(dispatch::unavailable)?;
        let _gate = admin.gate.lock().map_err(|_| dispatch::unavailable())?;
        let mut pairing = self.pairing.lock().map_err(|_| dispatch::unavailable())?;
        *admin.issued.lock().map_err(|_| dispatch::unavailable())? = None;
        pairing.begin(addresses, port)
    }
    pub(super) fn identity(&self) -> &Arc<ServerIdentity> {
        &self.identity
    }
    pub fn cancel_pairing(&self, attempt: &Id) -> Result<(), RemoteError> {
        self.pairing
            .lock()
            .map_err(|_| dispatch::unavailable())?
            .close_attempt(attempt);
        Ok(())
    }
    pub fn pairing_status(&self) -> Result<Option<(Id, std::time::Duration)>, RemoteError> {
        Ok(self
            .pairing
            .lock()
            .map_err(|_| dispatch::unavailable())?
            .active_attempt())
    }
    pub fn list_devices(&self) -> Result<Vec<PairedDevice>, RemoteError> {
        let mut db = self
            .state()
            .db
            .lock()
            .map_err(|_| dispatch::unavailable())?;
        super::store::RemoteStore::new(&mut db, self.identity.id())
            .active_devices()
            .map_err(dispatch::native_error)
    }
    pub fn revoke_device(&self, device: &Id) -> Result<(), RemoteError> {
        self.dispatcher
            .services
            .admin
            .as_ref()
            .ok_or_else(dispatch::unavailable)?
            .revoke(device)
    }
    fn state(&self) -> &crate::app_state::AppState {
        &self.dispatcher.services.state
    }
}
#[derive(Default)]
pub struct RemoteServer {
    running: tokio::sync::Mutex<Option<RunningServer>>,
}
struct RunningServer {
    cancel: CancellationToken,
    tasks: TaskTracker,
    addresses: Vec<SocketAddr>,
    runtime: Arc<ServerRuntime>,
}
impl RemoteServer {
    pub async fn addresses(&self) -> Vec<SocketAddr> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|r| r.addresses.clone())
            .unwrap_or_default()
    }
    pub async fn start(
        &self,
        runtime: Arc<ServerRuntime>,
        addresses: BindAddresses,
    ) -> Result<Vec<SocketAddr>, RemoteError> {
        let mut running = self.running.lock().await;
        if running.is_some() {
            return Err(RemoteError::new(
                ErrorCode::Busy,
                "The remote listener is already enabled.",
            ));
        }
        let mut listeners = Vec::new();
        // Bind all addresses atomically from the caller's perspective; no partial
        // listener or alternative port survives any failure.
        for address in addresses.0 {
            let listener = TcpListener::bind(address).await.map_err(|_|RemoteError::new(ErrorCode::Unavailable,"Could not bind the selected LAN address and port. Choose an explicit alternative port."))?;
            listeners.push(listener);
        }
        let addresses = listeners
            .iter()
            .map(|l| l.local_addr().expect("bound listener"))
            .collect::<Vec<_>>();
        let cancel = CancellationToken::new();
        let tasks = TaskTracker::new();
        for listener in listeners {
            let runtime = runtime.clone();
            let cancel = cancel.clone();
            let children = tasks.clone();
            tasks.spawn(async move {
                loop {
                    let accepted = tokio::select! { biased; _=cancel.cancelled()=>break, accepted=listener.accept()=>accepted };
                    let Ok((socket,peer)) = accepted else { break };
                    let Ok(permit) = runtime.handshakes.clone().try_acquire_owned() else { drop(socket); continue };
                    let runtime = runtime.clone(); let cancel = cancel.clone(); let descendants = children.clone();
                    children.spawn(async move { connection(socket,peer,runtime,cancel,descendants,permit).await; });
                }
            });
        }
        runtime
            .dispatcher
            .services
            .admin
            .as_ref()
            .ok_or_else(dispatch::unavailable)?
            .replace_network(addresses.clone())?;
        *running = Some(RunningServer {
            cancel,
            tasks,
            addresses: addresses.clone(),
            runtime,
        });
        Ok(addresses)
    }
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        if let Some(running) = running.take() {
            running.cancel.cancel();
            let _ = running
                .runtime
                .dispatcher
                .services
                .admin
                .as_ref()
                .map(|a| a.replace_network(Vec::new()));
            running.tasks.close();
            running.tasks.wait().await;
        }
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct Admission {
    permit: Mutex<Option<OwnedSemaphorePermit>>,
    authenticated: CancellationToken,
    thumbnail_permit: Mutex<Option<OwnedSemaphorePermit>>,
    subscription: Mutex<Option<Subscription>>,
    revocation: tokio::sync::watch::Sender<Option<CancellationToken>>,
}
impl Admission {
    fn finish(&self) {
        self.permit.lock().unwrap().take();
        self.authenticated.cancel();
    }
}
async fn connection(
    socket: TcpStream,
    peer: SocketAddr,
    runtime: Arc<ServerRuntime>,
    cancel: CancellationToken,
    tasks: TaskTracker,
    permit: OwnedSemaphorePermit,
) {
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let tls = tokio::select! { biased; _=cancel.cancelled()=>return, result=tokio::time::timeout_at(deadline,tokio_rustls::TlsAcceptor::from(runtime.identity.server_config()).accept(socket))=>match result { Ok(Ok(stream))=>stream,_=>return } };
    let admission = Arc::new(Admission {
        permit: Mutex::new(Some(permit)),
        authenticated: CancellationToken::new(),
        thumbnail_permit: Mutex::new(None),
        subscription: Mutex::new(None),
        revocation: tokio::sync::watch::channel(None).0,
    });
    let service = hyper::service::service_fn({
        let admission = admission.clone();
        let cancel = cancel.clone();
        move |request| {
            let runtime = runtime.clone();
            let cancel = cancel.clone();
            let tasks = tasks.clone();
            let admission = admission.clone();
            async move {
                Ok::<_, Infallible>(
                    endpoint(request, peer, runtime, cancel, tasks, admission).await,
                )
            }
        }
    });
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .keep_alive(true)
        .max_headers(32)
        .max_buf_size(16 * 1024)
        .timer(TokioTimer::new())
        .header_read_timeout(AUTH_TIMEOUT);
    let http = builder
        .serve_connection(TokioIo::new(tls), service)
        .with_upgrades();
    tokio::pin!(http);
    tokio::select! { biased; _=cancel.cancelled()=>{}, _=async { tokio::select! { _=tokio::time::sleep_until(deadline)=>{}, _=admission.authenticated.cancelled()=>std::future::pending::<()>().await } }=>{}, _=async {
        let mut changes=admission.revocation.subscribe();
        loop {
            let token=changes.borrow().clone();
            if let Some(token)=token { token.cancelled().await; break; }
            if changes.changed().await.is_err() { break; }
        }
    }=>{}, _=async { admission.authenticated.cancelled().await; tokio::time::sleep(PONG_TIMEOUT).await; }=>{}, _=&mut http=>{} }
}
fn response(status: StatusCode, bytes: impl Into<Bytes>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("Cache-Control", "no-store")
        .header("Connection", "close")
        .body(Full::new(bytes.into()))
        .unwrap()
}
fn failure(status: StatusCode) -> Response<Body> {
    response(status, Bytes::new())
}
async fn endpoint(
    mut request: Request<Incoming>,
    peer: SocketAddr,
    runtime: Arc<ServerRuntime>,
    cancel: CancellationToken,
    tasks: TaskTracker,
    admission: Arc<Admission>,
) -> Response<Body> {
    if request.uri().query().is_some() || request.headers().contains_key("origin") {
        return failure(StatusCode::FORBIDDEN);
    }
    let path = request.uri().path().to_owned();
    if path == "/remote/v1/pair" && request.method() == hyper::Method::POST {
        let body = match Limited::new(request.into_body(), MAX_REQUEST_BYTES)
            .collect()
            .await
        {
            Ok(body) => body.to_bytes(),
            Err(_) => return failure(StatusCode::PAYLOAD_TOO_LARGE),
        };
        let Ok(text) = std::str::from_utf8(&body) else {
            return failure(StatusCode::BAD_REQUEST);
        };
        let parsed = match super::pairing::parse_pair_request(text) {
            Ok(parsed) => parsed,
            Err(_) => return failure(StatusCode::BAD_REQUEST),
        };
        let result = (|| -> Result<serde_json::Value, RemoteError> {
            let mut pairing = runtime
                .pairing
                .lock()
                .map_err(|_| dispatch::unavailable())?;
            match parsed {
                PairEndpointRequest::ManualChallenge(_) => {
                    serde_json::to_value(pairing.manual_challenge()?)
                        .map_err(|_| RemoteError::invalid())
                }
                PairEndpointRequest::Qr(request) => {
                    let mut db = runtime
                        .state()
                        .db
                        .lock()
                        .map_err(|_| dispatch::unavailable())?;
                    serde_json::to_value(pairing.pair_qr(
                        peer.ip(),
                        &request,
                        &mut db,
                        crate::app_state::now_ms() as i64,
                    )?)
                    .map_err(|_| RemoteError::invalid())
                }
                PairEndpointRequest::Manual(request) => {
                    let mut db = runtime
                        .state()
                        .db
                        .lock()
                        .map_err(|_| dispatch::unavailable())?;
                    serde_json::to_value(pairing.pair_manual(
                        peer.ip(),
                        &request,
                        &mut db,
                        crate::app_state::now_ms() as i64,
                    )?)
                    .map_err(|_| RemoteError::invalid())
                }
            }
        })();
        return match result {
            Ok(value) => json_response(value),
            Err(_) => failure(StatusCode::FORBIDDEN),
        };
    }
    if request.method() != hyper::Method::GET
        || (path != "/remote/v1/ws" && !path.starts_with("/remote/v1/thumbnails/"))
    {
        return failure(StatusCode::NOT_FOUND);
    }
    let bearer = match bearer(&request) {
        Ok(bearer) => bearer,
        Err(_) => return failure(StatusCode::UNAUTHORIZED),
    };
    let admitted = {
        match runtime.state().db.lock() {
            Ok(mut db) => runtime.auth.authenticate_subscription(&mut db, &bearer),
            Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE),
        }
    };
    let (device, events) = match admitted {
        Ok(admitted) => admitted,
        Err(_) => return failure(StatusCode::UNAUTHORIZED),
    };
    let principal = RemotePrincipal {
        device_id: device.device_id,
        scopes: device.scopes,
    };
    if !principal.scopes.as_slice().contains(&Scope::Browse) {
        return failure(StatusCode::FORBIDDEN);
    }
    if path == "/remote/v1/ws" {
        let Ok(permit) = runtime.sockets.clone().try_acquire_owned() else {
            return failure(StatusCode::TOO_MANY_REQUESTS);
        };
        let mut handshake = Request::builder()
            .method(request.method())
            .uri(request.uri())
            .body(())
            .unwrap();
        *handshake.headers_mut() = request.headers().clone();
        let handshake =
            match tokio_tungstenite::tungstenite::handshake::server::create_response(&handshake) {
                Ok(response) => response,
                Err(_) => return failure(StatusCode::BAD_REQUEST),
            };
        let upgrade = hyper::upgrade::on(&mut request);
        admission.finish();
        tasks.spawn(async move {
            let _permit=permit;
            let revoked=events.revocation();
            let upgraded=tokio::select! { biased; _=cancel.cancelled()=>return, _=revoked.cancelled()=>return, upgraded=tokio::time::timeout(AUTH_TIMEOUT,upgrade)=>match upgraded { Ok(Ok(stream))=>stream,_=>return } };
            let socket=WebSocketStream::from_raw_socket(TokioIo::new(upgraded),Role::Server,Some(websocket_config())).await;
            websocket(socket,runtime,principal,events,cancel).await;
        });
        return handshake.map(|_| Full::new(Bytes::new()));
    }
    let Some(permit) = runtime.thumbnails.acquire(principal.device_id.as_str(), 4) else {
        return failure(StatusCode::TOO_MANY_REQUESTS);
    };
    *admission.thumbnail_permit.lock().unwrap() = Some(permit);
    let revoked = events.revocation();
    admission.revocation.send_replace(Some(revoked.clone()));
    *admission.subscription.lock().unwrap() = Some(events);
    admission.finish();
    let opaque = &path["/remote/v1/thumbnails/".len()..];
    if opaque.len() != 64 || !opaque.bytes().all(|b| b.is_ascii_hexdigit()) {
        return failure(StatusCode::NOT_FOUND);
    }
    let work_cancel = cancel.child_token();
    let result = tokio::select! { biased; _=cancel.cancelled()=>{work_cancel.cancel();return failure(StatusCode::SERVICE_UNAVAILABLE)}, _=revoked.cancelled()=>{work_cancel.cancel();return failure(StatusCode::UNAUTHORIZED)}, result=crate::library::thumbnails::get_remote(runtime.state(),opaque,work_cancel.clone())=>result };
    match result {
        Ok(bytes) if bytes.len() <= THUMBNAIL_BYTES => {
            let mut response = response(StatusCode::OK, bytes);
            response
                .headers_mut()
                .insert("Content-Type", "image/png".parse().unwrap());
            response
        }
        _ => failure(StatusCode::NOT_FOUND),
    }
}
fn bearer(request: &Request<Incoming>) -> Result<BearerCredential, RemoteError> {
    let mut values = request.headers().get_all("authorization").iter();
    let value = values.next().ok_or_else(RemoteError::invalid)?;
    if values.next().is_some() {
        return Err(RemoteError::invalid());
    }
    checked(
        value
            .to_str()
            .map_err(|_| RemoteError::invalid())?
            .strip_prefix("Bearer ")
            .ok_or_else(RemoteError::invalid)?,
    )
}
fn json_response(value: impl serde::Serialize) -> Response<Body> {
    match serde_json::to_vec(&value) {
        Ok(bytes) if bytes.len() <= OUTGOING_BYTES => {
            let mut response = response(StatusCode::OK, bytes);
            response
                .headers_mut()
                .insert("Content-Type", "application/json".parse().unwrap());
            response
        }
        _ => failure(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
fn websocket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(16 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(OUTGOING_BYTES + 1024)
        .max_message_size(Some(MAX_REQUEST_BYTES))
        .max_frame_size(Some(MAX_REQUEST_BYTES))
}
async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    message: Message,
    cancel: &CancellationToken,
    revoked: &CancellationToken,
) -> Result<(), ()> {
    send_until(
        socket,
        message,
        cancel,
        revoked,
        tokio::time::Instant::now() + ACK_TIMEOUT,
    )
    .await
}
async fn send_until<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    message: Message,
    cancel: &CancellationToken,
    revoked: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<(), ()> {
    tokio::select! { biased;
        _=cancel.cancelled()=>Err(()),
        _=revoked.cancelled()=>Err(()),
        _=tokio::time::sleep_until(deadline)=>Err(()),
        sent=socket.send(message)=>sent.map_err(|_|()),
    }
}
fn control_message(value: impl serde::Serialize) -> Result<Message, ()> {
    let text = serde_json::to_string(&value).map_err(|_| ())?;
    if text.len() > OUTGOING_BYTES {
        return Err(());
    }
    Ok(Message::Text(text.into()))
}
async fn close<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    reason: &'static str,
    cancel: &CancellationToken,
    revoked: &CancellationToken,
) {
    close_until(
        socket,
        reason,
        cancel,
        revoked,
        tokio::time::Instant::now() + ACK_TIMEOUT,
    )
    .await;
}
async fn close_until<S: AsyncRead + AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    reason: &'static str,
    cancel: &CancellationToken,
    revoked: &CancellationToken,
    deadline: tokio::time::Instant,
) {
    let _ = send_until(
        socket,
        Message::Close(Some(CloseFrame {
            code: CloseCode::Policy,
            reason: reason.into(),
        })),
        cancel,
        revoked,
        deadline,
    )
    .await;
}
async fn websocket<S: AsyncRead + AsyncWrite + Unpin>(
    socket: WebSocketStream<S>,
    runtime: Arc<ServerRuntime>,
    principal: RemotePrincipal,
    events: Subscription,
    cancel: CancellationToken,
) {
    let (pong_deadline, updates) =
        tokio::sync::watch::channel(tokio::time::Instant::now() + PONG_TIMEOUT);
    // This deadline surrounds the whole session, including snapshot capture,
    // request handling and blocked writes. Ping cadence cannot postpone expiry.
    tokio::select! { biased;
        _=heartbeat_expired(updates)=>{},
        _=websocket_session(socket,runtime,principal,events,cancel,pong_deadline)=>{},
    }
}
async fn heartbeat_expired(mut updates: tokio::sync::watch::Receiver<tokio::time::Instant>) {
    loop {
        let deadline = *updates.borrow_and_update();
        tokio::select! { biased;
            // Consume an already published Pong reset before an old timer expiry.
            changed=updates.changed()=>if changed.is_err() { return; },
            _=tokio::time::sleep_until(deadline)=>return,
        }
    }
}
async fn websocket_session<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: WebSocketStream<S>,
    runtime: Arc<ServerRuntime>,
    principal: RemotePrincipal,
    events: Subscription,
    cancel: CancellationToken,
    pong_deadline: tokio::sync::watch::Sender<tokio::time::Instant>,
) {
    let revoked = events.revocation();
    let initial = tokio::select! { biased;
        _=cancel.cancelled()=>return,
        _=revoked.cancelled()=>return,
        initial=tokio::time::timeout(ACK_TIMEOUT,super::state::snapshot_from_admitted_subscription(&principal,&runtime.dispatcher.services,events))=>initial,
    };
    let initial = match initial {
        Ok(Ok(initial)) => initial,
        Ok(Err(error)) => {
            if error.code == ErrorCode::ResyncRequired {
                close(&mut socket, "resync_required", &cancel, &revoked).await;
            }
            return;
        }
        Err(_) => return,
    };
    let hello = Hello {
        v: checked(1).unwrap(),
        r#type: HelloType::Hello,
        minor: checked(0).unwrap(),
        server_epoch: runtime.state().event_hub.epoch(),
        device_id: runtime.identity.id().clone(),
        version: checked(env!("CARGO_PKG_VERSION")).unwrap(),
        capabilities: super::browser_input::capabilities(dispatch::capabilities(), runtime.state()),
        snapshot_sequence: initial.initial.snapshot.sequence.clone(),
    };
    let mut events = initial.events;
    for message in [
        control_message(hello),
        control_message(initial.initial.snapshot),
    ] {
        let Ok(message) = message else { return };
        if send(&mut socket, message, &cancel, &revoked).await.is_err() {
            return;
        }
    }
    for event in initial.initial.buffered_events.as_slice() {
        let Ok(message) = control_message(event) else {
            return;
        };
        if send(&mut socket, message, &cancel, &revoked).await.is_err() {
            return;
        }
        events.mark_delivered(event);
    }
    let mut rate = CommandRate::new(Instant::now());
    let mut motion_rate = CommandRate::motion(Instant::now());
    let mut heartbeat =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut transient = TransientRequests::default();
    let input_origin = uuid::Uuid::new_v4().to_string();
    let socket_live = CancellationToken::new();
    let _socket_guard = socket_live.clone().drop_guard();
    loop {
        if let Err(error) = events.check_live() {
            if error.code == ErrorCode::ResyncRequired {
                close(&mut socket, "resync_required", &cancel, &revoked).await;
            }
            return;
        }
        tokio::select! { biased;
            _=cancel.cancelled()=>return,
            _=revoked.cancelled()=>return,
            _=heartbeat.tick()=> {
                if send(&mut socket,Message::Ping(Bytes::from_static(b"mc")),&cancel,&revoked).await.is_err() { return; }
            }
            incoming=socket.next()=> {
                let Some(Ok(incoming))=incoming else { return };
                match incoming {
                    Message::Pong(bytes)=> { if bytes.as_ref()==b"mc" { pong_deadline.send_replace(tokio::time::Instant::now()+PONG_TIMEOUT); } },
                    Message::Ping(bytes)=> { if send(&mut socket,Message::Pong(bytes),&cancel,&revoked).await.is_err() { return; } },
                    Message::Close(_)=>return,
                    Message::Text(text)=> {
                        // One elapsed-time budget includes parsing, native state
                        // capture/dispatch, serialization and response delivery.
                        let acknowledgement_deadline=tokio::time::Instant::now()+ACK_TIMEOUT;
                        let request=match parse_request(&text) {
                            Ok(request)=>request,
                            Err(error)=> {
                                if !rate.admit(Instant::now()) { close_until(&mut socket,"rate_limited",&cancel,&revoked,acknowledgement_deadline).await; return; }
                                let id=super::protocol::validate_document(&text,MAX_REQUEST_BYTES).ok().and_then(|v|v.get("id").cloned()).and_then(|v|checked::<Id,_>(v).ok());
                                let Some(id)=id else { close_until(&mut socket,"invalid_request",&cancel,&revoked,acknowledgement_deadline).await;return };
                                let reply=RemoteReply::<serde_json::Value>::Failure{id,error};
                                let Ok(message)=control_message(reply) else { return };
                                if send_until(&mut socket,message,&cancel,&revoked,acknowledgement_deadline).await.is_err() { return; } continue;
                            }
                        };
                        let id=request.id.clone();
                        if super::browser_input::motion(&request.command) {
                            if !motion_rate.admit(Instant::now()) {
                                let error = RemoteError::new(ErrorCode::Busy, "Browser motion was superseded.");
                                let Ok(message)=control_message(RemoteReply::<serde_json::Value>::Failure{id,error}) else { return };
                                if send_until(&mut socket,message,&cancel,&revoked,acknowledgement_deadline).await.is_err() { return; }
                                continue;
                            }
                        } else if !rate.admit(Instant::now()) {
                            close_until(&mut socket,"rate_limited",&cancel,&revoked,acknowledgement_deadline).await;
                            return;
                        }
                        let result=match transient.lookup(&request) {
                            Some(result)=>result,
                            None=> {
                                let work=async {
                                    Dispatcher::authorize(&principal,&request)?;
                                    runtime.dispatcher.services.network.lock().map_err(|_|dispatch::unavailable())?.note(&principal.device_id,socket_live.clone(),revoked.clone(),&input_origin);
                                    if matches!(request.command,RemoteCommand::StateGet(_)) {
                                        let snapshot=super::state::snapshot_on_admitted_subscription(&principal,&runtime.dispatcher.services,&events).await?;
                                        serde_json::to_value(snapshot).map_err(|_|RemoteError::invalid())
                                    } else if matches!(request.command,RemoteCommand::UiNavigate(_)|RemoteCommand::UiMoveFocus(_)|RemoteCommand::UiActivate(_)|RemoteCommand::UiBack(_)|RemoteCommand::UiSearch(_)) { runtime.dispatcher.renderer_input(&principal,request.clone(),&input_origin).await }
                                    else { runtime.dispatcher.dispatch(&principal,request.clone()).await }
                                };
                                let result=tokio::select! { biased;
                                    _=cancel.cancelled()=>return,
                                    _=revoked.cancelled()=>return,
                                    _=tokio::time::sleep_until(acknowledgement_deadline)=>return,
                                    result=work=>result,
                                };
                                transient.remember(&request,&result); result
                            }
                        };
                        let reply=match result { Ok(result)=>RemoteReply::Success{id,result},Err(error)=>RemoteReply::Failure{id,error} };
                        let Ok(message)=control_message(reply) else { close_until(&mut socket,"resync_required",&cancel,&revoked,acknowledgement_deadline).await;return };
                        if send_until(&mut socket,message,&cancel,&revoked,acknowledgement_deadline).await.is_err() { return; }
                    }
                    _=> { close(&mut socket,"invalid_request",&cancel,&revoked).await;return; }
                }
            }
            batch=events.recv()=> {
                match batch {
                    Ok(batch)=>for event in batch {
                        let Ok(message)=control_message(&event) else { close(&mut socket,"resync_required",&cancel,&revoked).await;return };
                        if send(&mut socket,message,&cancel,&revoked).await.is_err() { return; }
                        events.mark_delivered(&event);
                    },
                    Err(error)=> { if error.code==ErrorCode::ResyncRequired { close(&mut socket,"resync_required",&cancel,&revoked).await; } return; }
                }
            }
        }
    }
}
/// Transient input hashes never cross this connection's in-memory boundary.
#[derive(Default)]
struct TransientRequests(
    std::collections::VecDeque<(Id, [u8; 32], Result<serde_json::Value, RemoteError>)>,
);
impl TransientRequests {
    fn hash(request: &RemoteRequest) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        Sha256::digest(
            serde_json::to_vec(
                &serde_json::json!({"command":request.command,"expected":request.expected}),
            )
            .expect("typed request"),
        )
        .into()
    }
    fn lookup(&self, request: &RemoteRequest) -> Option<Result<serde_json::Value, RemoteError>> {
        if request.command.delivery() != Delivery::Transient {
            return None;
        }
        self.0
            .iter()
            .find(|(id, _, _)| id == &request.id)
            .map(|(_, hash, result)| {
                if *hash == Self::hash(request) {
                    result.clone()
                } else {
                    Err(RemoteError::new(
                        ErrorCode::RequestIdConflict,
                        "Use a new request ID for new input.",
                    ))
                }
            })
    }
    fn remember(
        &mut self,
        request: &RemoteRequest,
        result: &Result<serde_json::Value, RemoteError>,
    ) {
        if request.command.delivery() != Delivery::Transient {
            return;
        }
        if self.0.len() == 256 {
            self.0.pop_front();
        }
        self.0
            .push_back((request.id.clone(), Self::hash(request), result.clone()));
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
