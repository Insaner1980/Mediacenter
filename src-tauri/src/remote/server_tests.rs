use super::*;
use crate::{
    app_state::AppState,
    remote::{
        store::{CredentialHash, RemoteStore},
        tls::{pinned_client_config, IdentityNamespace},
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    _dir: tempfile::TempDir,
    runtime: Arc<ServerRuntime>,
    server: RemoteServer,
    address: SocketAddr,
    credential: BearerCredential,
    principal: RemotePrincipal,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_effect(None).await
    }
    async fn with_effect(
        effect: Option<Arc<dyn Fn() -> Result<(), crate::contracts::AppError> + Send + Sync>>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("app")).unwrap();
        let identity = Arc::new(
            ServerIdentity::load_or_create(
                dir.path(),
                IdentityNamespace::Development,
                &mut state.db.lock().unwrap(),
                1,
            )
            .unwrap(),
        );
        let services = ControlServices::native(state.clone(), identity.id().clone()).unwrap();
        let mut runtime = ServerRuntime::new(identity.clone(), services).unwrap();
        Arc::get_mut(&mut runtime).unwrap().dispatcher.pause_effect = effect;
        let credential: BearerCredential = checked("a".repeat(43)).unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
        RemoteStore::new(&mut state.db.lock().unwrap(), identity.id())
            .add_device(&device, &CredentialHash::from_credential(&credential))
            .unwrap();
        let principal = RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        };
        let server = RemoteServer::default();
        let address = server
            .start(
                runtime.clone(),
                BindAddresses(vec!["127.0.0.1:0".parse().unwrap()]),
            )
            .await
            .unwrap()[0];
        Self {
            _dir: dir,
            runtime,
            server,
            address,
            credential,
            principal,
        }
    }
    async fn tls(&self) -> tokio_rustls::client::TlsStream<TcpStream> {
        let socket = TcpStream::connect(self.address).await.unwrap();
        tokio_rustls::TlsConnector::from(pinned_client_config(self.runtime.identity.fingerprint()))
            .connect("localhost".try_into().unwrap(), socket)
            .await
            .unwrap()
    }
    async fn http(&self, request: &str) -> String {
        let mut stream = self.tls().await;
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            stream.read_to_end(&mut bytes),
        )
        .await
        .unwrap();
        String::from_utf8(bytes).unwrap()
    }
}
#[tokio::test]
async fn unauthenticated_endpoints_never_disclose_metadata() {
    let fixture = Fixture::new().await;
    for path in ["/remote/v1/ws", "/remote/v1/thumbnails/opaque"] {
        let reply = fixture
            .http(&format!(
                "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            ))
            .await;
        assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
        assert!(
            reply.ends_with("\r\n\r\n"),
            "no unauthenticated response body"
        );
    }
    fixture.server.stop().await;
}

type Client = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;
impl Fixture {
    async fn ws(&self) -> Client {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("wss://{}/remote/v1/ws", self.address)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", self.credential.as_str())
                .parse()
                .unwrap(),
        );
        let (mut socket, _) = tokio_tungstenite::client_async(request, self.tls().await)
            .await
            .unwrap();
        let hello = next_json(&mut socket).await;
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["deviceId"], self.runtime.identity.id().as_str());
        assert_eq!(
            hello["capabilities"]["methods"],
            serde_json::json!([
                "state.get",
                "capabilities.get",
                "operations.get",
                "operations.cancel",
                "player.pause",
                "library.query",
                "library.item",
                "player.play",
                "player.seek",
                "player.volume",
                "player.mute",
                "player.audioTrack",
                "player.subtitleTrack",
                "player.stop",
                "queue.get",
                "queue.enqueue",
                "queue.remove",
                "queue.move",
                "queue.playEntry",
                "queue.next",
                "queue.previous",
                "queue.shuffle",
                "queue.repeat",
                "playlists.list",
                "playlists.get",
                "playlists.save",
                "playlists.delete",
                "playlists.play",
                "photos.show",
                "photos.slideshow.start",
                "photos.next",
                "photos.previous",
                "photos.pause",
                "photos.interval",
                "photos.shuffle",
                "photos.fit",
                "photos.stop",
                "activity.prepare",
                "activity.confirm",
                "confirmations.get",
                "confirmations.cancel",
                "audio.outputs",
                "audio.prepare",
                "audio.confirm",
                "audio.test",
                "activity.showMenu",
                "activity.showPlayback"
            ])
        );
        let snapshot = next_json(&mut socket).await;
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["ui"]["available"], false);
        socket
    }
    fn pause(&self, id: &str, paused: bool) -> serde_json::Value {
        serde_json::json!({"v":1,"id":id,"method":"player.pause","params":{"sessionId":"session","paused":paused},"expected":{"serverEpoch":self.runtime.state().event_hub.epoch(),"sessionId":"session"}})
    }
    fn revoke(&self) {
        self.runtime
            .auth
            .revoke_device(
                &mut self.runtime.state().db.lock().unwrap(),
                &self.principal.device_id,
                crate::app_state::now_ms() as i64,
            )
            .unwrap();
    }
}
async fn next_json(socket: &mut Client) -> serde_json::Value {
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}
async fn reply(socket: &mut Client, id: &str) -> serde_json::Value {
    loop {
        let value = next_json(socket).await;
        if value["id"] == id {
            return value;
        }
    }
}
async fn disconnected(socket: &mut Client) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match socket.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                _ => {}
            }
        }
    })
    .await
    .expect("socket must close promptly");
}
#[tokio::test]
async fn rejects_origin_query_credentials_and_unknown_privileged_commands() {
    let fixture = Fixture::new().await;
    for suffix in ["Origin: https://evil.example\r\n", "Origin: null\r\n"] {
        let result=fixture.http(&format!("GET /remote/v1/ws HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n{suffix}\r\n",fixture.credential.as_str())).await;
        assert!(result.starts_with("HTTP/1.1 403"));
    }
    assert!(fixture
        .http("GET /remote/v1/ws?token=secret HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .starts_with("HTTP/1.1 403"));
    let mut socket = fixture.ws().await;
    for (id, method, params) in [
        ("shell", "open_service", serde_json::json!({})),
        (
            "large-query",
            "library.query",
            serde_json::json!({"kind":"video","query":"x".repeat(201)}),
        ),
    ] {
        socket
            .send(Message::Text(
                serde_json::json!({"v":1,"id":id,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let result = reply(&mut socket, id).await;
        assert_eq!(result["ok"], false);
        assert_eq!(
            result["error"]["code"],
            if id == "shell" {
                "unsupported"
            } else {
                "invalid_request"
            }
        );
    }
    assert!(fixture
        .runtime
        .state()
        .operations
        .lock()
        .unwrap()
        .is_empty());
    assert!(fixture.runtime.state().browser.lock().await.is_none());
    fixture.server.stop().await;
    disconnected(&mut socket).await;
}
#[tokio::test]
async fn fragmented_message_over_64k_is_rejected_before_dispatch() {
    use tokio_tungstenite::tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let fixture = Fixture::new().await;
    let mut socket = fixture.ws().await;
    for (opcode, final_frame) in [(Data::Text, false), (Data::Continue, true)] {
        let frame = Frame::message(vec![b' '; 32769], OpCode::Data(opcode), final_frame);
        socket.send(Message::Frame(frame)).await.unwrap();
    }
    disconnected(&mut socket).await;
    assert_eq!(
        fixture
            .runtime
            .state()
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    fixture.server.stop().await;
}
#[tokio::test]
async fn lost_reply_reconnect_duplicate_has_one_effect_and_conflicting_id_fails() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let effects = Arc::new(AtomicUsize::new(0));
    let counter = effects.clone();
    let fixture = Fixture::with_effect(Some(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })))
    .await;
    let mut first = fixture.ws().await;
    let request = fixture.pause("once", true);
    first
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while effects.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(first); // Deliberately discard the successful reply, not the intent.
    let mut second = fixture.ws().await;
    second
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let duplicate = reply(&mut second, "once").await;
    assert_eq!(duplicate["ok"], true);
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    second.send(Message::Text(serde_json::json!({"v":1,"id":"lookup","method":"operations.get","params":{"requestId":"once"}}).to_string().into())).await.unwrap();
    let operation = reply(&mut second, "lookup").await;
    assert_eq!(
        operation["result"]["operationId"],
        duplicate["result"]["operationId"]
    );
    second
        .send(Message::Text(
            fixture.pause("once", false).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        reply(&mut second, "once").await["error"]["code"],
        "request_id_conflict"
    );
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .runtime
            .state()
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    fixture.server.stop().await;
}
#[tokio::test]
async fn socket_limit_revocation_and_disable_release_every_connection() {
    let fixture = Fixture::new().await;
    let mut clients = Vec::new();
    for _ in 0..4 {
        clients.push(fixture.ws().await);
    }
    let result = fixture
        .http(&format!(
            "GET /remote/v1/ws HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\r\n",
            fixture.credential.as_str()
        ))
        .await;
    assert!(result.starts_with("HTTP/1.1 429"));
    fixture.revoke();
    for client in &mut clients {
        disconnected(client).await;
    }
    fixture.server.stop().await;
    assert!(fixture.server.addresses().await.is_empty());
    assert!(TcpStream::connect(fixture.address).await.is_err());
}
#[tokio::test]
async fn slow_writer_is_cancelled_by_revocation_and_stop() {
    // Deterministically saturate the transport's actual writer, independent of
    // operating-system TCP buffer sizes. Uses the production send boundary.
    for revoke in [true, false] {
        let (writer, _reader) = tokio::io::duplex(1);
        let mut socket =
            WebSocketStream::from_raw_socket(writer, Role::Server, Some(websocket_config())).await;
        let cancel = CancellationToken::new();
        let hub = super::super::event_hub::EventHub::default();
        let owner: Id = checked("phone").unwrap();
        let events = hub.subscribe_owner(Some(owner.clone()));
        let revoked = events.revocation();
        let operation = send(
            &mut socket,
            Message::Text("x".repeat(256 * 1024).into()),
            &cancel,
            &revoked,
        );
        tokio::pin!(operation);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut operation)
                .await
                .is_err(),
            "writer must actually block"
        );
        if revoke {
            hub.close_device(&owner);
        } else {
            cancel.cancel();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), operation)
                .await
                .unwrap()
                .is_err()
        );
    }
}
#[tokio::test]
async fn disabled_default_and_native_bind_validation_prevent_wildcards_and_public_addresses() {
    assert!(RemoteServer::default().addresses().await.is_empty());
    for address in [
        "0.0.0.0:47831",
        "8.8.8.8:47831",
        "127.0.0.1:47831",
        "[::]:47831",
        "[2001:4860:4860::8888]:47831",
    ] {
        let address: SocketAddr = address.parse().unwrap();
        assert!(BindAddresses::selected(vec![address], &[address.ip()]).is_err());
    }
    assert!(BindAddresses::selected(vec!["192.168.1.2:47831".parse().unwrap()], &[]).is_err());
    assert!(BindAddresses::selected(
        vec!["192.168.1.2:47831".parse().unwrap()],
        &["192.168.1.2".parse().unwrap()]
    )
    .is_ok());
}
#[tokio::test]
async fn pairing_is_closed_until_native_setup_and_consumes_one_use_secret() {
    let fixture = Fixture::new().await;
    let challenge = r#"{"v":1,"mode":"manual_challenge"}"#;
    let post = |body: &str| {
        format!(
            "POST /remote/v1/pair HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    };
    assert!(fixture
        .http(&post(challenge))
        .await
        .starts_with("HTTP/1.1 403"));
    let ready = fixture
        .runtime
        .begin_pairing(
            checked(vec!["127.0.0.1"]).unwrap(),
            checked(fixture.address.port()).unwrap(),
        )
        .unwrap();
    assert!(fixture
        .http(&post(challenge))
        .await
        .starts_with("HTTP/1.1 200"));
    let body=serde_json::json!({"attemptId":ready.candidate.attempt_id,"secret":ready.candidate.secret,"deviceLabel":"Second phone"}).to_string();
    let result = fixture.http(&post(&body)).await;
    assert!(result.starts_with("HTTP/1.1 200"));
    assert!(result.contains("credential"));
    assert!(fixture.http(&post(&body)).await.starts_with("HTTP/1.1 403"));
    fixture.server.stop().await;
}

#[tokio::test]
async fn unauthenticated_handshakes_are_capped_at_sixteen_and_expire_in_five_seconds() {
    let fixture = Fixture::new().await;
    let mut idle = Vec::new();
    for _ in 0..16 {
        idle.push(TcpStream::connect(fixture.address).await.unwrap());
    }
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while fixture.runtime.handshakes.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut excess = TcpStream::connect(fixture.address).await.unwrap();
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), excess.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        while fixture.runtime.handshakes.available_permits() != 16 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    fixture.server.stop().await;
}
#[tokio::test]
async fn normal_command_flood_is_closed_before_any_mutation() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.ws().await;
    for id in 0..100 {
        if socket.feed(Message::Text(serde_json::json!({"v":1,"id":format!("flood{id}"),"method":"capabilities.get","params":{}}).to_string().into())).await.is_err() { break; }
    }
    let _ = socket.flush().await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Close(Some(close)))) => {
                    assert_eq!(close.reason, "rate_limited");
                    break;
                }
                Some(Ok(Message::Text(_))) => {}
                other => panic!("expected rate-limited close, got {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        fixture
            .runtime
            .state()
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    fixture.server.stop().await;
}
#[tokio::test]
async fn event_overflow_closes_with_resync_and_never_sends_further_deltas() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.ws().await;
    // No yield while publishing: the actual 256-message admitted buffer fills.
    let hub = &fixture.runtime.state().event_hub;
    for _ in 0..257 {
        let stamp = hub.reserve(Topic::Connection);
        hub.publish(stamp, |server_epoch, sequence| RemoteEvent::Connection {
            server_epoch,
            sequence,
            data: ConnectionTopic {
                revision: stamp.revision.into(),
                server_device_id: fixture.runtime.identity.id().clone(),
                display_name: checked("Mediacenter").unwrap(),
            },
        });
    }
    match socket.next().await.unwrap().unwrap() {
        Message::Close(Some(close)) => assert_eq!(close.reason, "resync_required"),
        other => panic!("expected resync close, got {other:?}"),
    }
    fixture.server.stop().await;
}
#[tokio::test]
async fn unresponsive_authenticated_socket_expires_after_heartbeat_deadline() {
    let fixture = Fixture::new().await;
    let socket = fixture.ws().await;
    // Retain TCP without reading through Tungstenite (which auto-answers pings).
    let _unresponsive = socket.into_inner();
    tokio::time::timeout(std::time::Duration::from_secs(33), async {
        while fixture.runtime.sockets.available_permits() != 4 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("three unanswered ten-second heartbeat intervals must disconnect");
    fixture.server.stop().await;
}
#[tokio::test]
async fn a_slow_tls_phone_does_not_block_native_state_or_another_phone_and_revocation_releases_it()
{
    let fixture = Fixture::new().await;
    let slow = fixture.ws().await;
    let _slow = slow.into_inner();
    let other:PairedDevice=checked(serde_json::json!({"deviceId":"other-phone","label":"Other","scopes":["browse"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("b".repeat(43)).unwrap();
    RemoteStore::new(
        &mut fixture.runtime.state().db.lock().unwrap(),
        fixture.runtime.identity.id(),
    )
    .add_device(&other, &CredentialHash::from_credential(&credential))
    .unwrap();
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = format!("wss://{}/remote/v1/ws", fixture.address)
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", credential.as_str()).parse().unwrap(),
    );
    let (mut fast, _) = tokio_tungstenite::client_async(request, fixture.tls().await)
        .await
        .unwrap();
    next_json(&mut fast).await;
    next_json(&mut fast).await;
    // Large owner-private operation topics saturate only the slow phone's TLS
    // writer. The fast phone receives the same sequence with empty projections.
    let request_id: Id = checked("request").unwrap();
    for _ in 0..120 {
        let hub = &fixture.runtime.state().event_hub;
        let stamp = hub.reserve(Topic::Operations);
        hub.publish_projected(stamp, |owner, server_epoch, sequence| {
            let items = if owner == Some(&fixture.principal.device_id) {
                (0..100)
                    .map(|i| OperationState {
                        operation_id: checked(format!("operation-{i}-{}", "x".repeat(100)))
                            .unwrap(),
                        request_id: request_id.clone(),
                        status: OperationStatus::Running,
                        error_code: None,
                        cancellation_requested: false,
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![]
            };
            RemoteEvent::Operations {
                server_epoch,
                sequence,
                data: OperationsTopic {
                    revision: stamp.revision.into(),
                    items: checked(items).unwrap(),
                },
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(100),
        fixture.runtime.state().player.lock()
    )
    .await
    .is_ok());
    fast.send(Message::Text(
        r#"{"v":1,"id":"fast","method":"capabilities.get","params":{}}"#.into(),
    ))
    .await
    .unwrap();
    assert_eq!(reply(&mut fast, "fast").await["ok"], true);
    fixture.revoke();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while fixture.runtime.sockets.available_permits() != 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("slow revoked socket must release its slot while fast phone remains");
    fast.send(Message::Text(
        r#"{"v":1,"id":"still-fast","method":"state.get","params":{}}"#.into(),
    ))
    .await
    .unwrap();
    assert_eq!(reply(&mut fast, "still-fast").await["ok"], true);
    fixture.server.stop().await;
}
#[test]
fn transient_dedup_is_bounded_connection_local_and_conflicting_input_is_rejected() {
    let request = |id: &str, text: &str| {
        parse_request(&serde_json::json!({"v":1,"id":id,"method":"ui.search","params":{"rendererGeneration":"1","focusRevision":"1","route":"home","text":text,"editSequence":"1"},"expected":{"serverEpoch":"epoch"}}).to_string()).unwrap()
    };
    let mut first = TransientRequests::default();
    let result = Err(RemoteError::new(ErrorCode::Unsupported, "Unavailable"));
    first.remember(&request("one", "secret"), &result);
    assert_eq!(
        first.lookup(&request("one", "secret")),
        Some(result.clone())
    );
    assert_eq!(
        first
            .lookup(&request("one", "changed"))
            .unwrap()
            .unwrap_err()
            .code,
        ErrorCode::RequestIdConflict
    );
    let second = TransientRequests::default();
    assert!(second.lookup(&request("one", "secret")).is_none());
    for i in 0..256 {
        first.remember(&request(&format!("id{i}"), "other"), &result);
    }
    assert!(first.lookup(&request("one", "secret")).is_none());
    assert_eq!(first.0.len(), 256);
}

#[test]
fn runtime_registration_is_exclusive_and_dropping_it_does_not_repeat_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().join("app")).unwrap();
    let identity = Arc::new(
        ServerIdentity::load_or_create(
            dir.path(),
            IdentityNamespace::Development,
            &mut state.db.lock().unwrap(),
            1,
        )
        .unwrap(),
    );
    let services = ControlServices::native(state.clone(), identity.id().clone()).unwrap();
    state
        .db
        .lock()
        .unwrap()
        .execute(
            "ALTER TABLE command_journal RENAME TO unavailable_journal",
            [],
        )
        .unwrap();
    assert!(ServerRuntime::new(identity.clone(), services.clone()).is_err());
    state
        .db
        .lock()
        .unwrap()
        .execute(
            "ALTER TABLE unavailable_journal RENAME TO command_journal",
            [],
        )
        .unwrap();
    let first = ServerRuntime::new(identity.clone(), services.clone()).unwrap();
    assert!(
        matches!(ServerRuntime::new(identity.clone(),services.clone()),Err(error) if error.code==ErrorCode::Busy)
    );
    let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("a".repeat(43)).unwrap();
    let request=parse_request(&serde_json::json!({"v":1,"id":"live-work","method":"player.pause","params":{"sessionId":"session","paused":true},"expected":{"serverEpoch":state.event_hub.epoch(),"sessionId":"session"}}).to_string()).unwrap();
    {
        let mut db = state.db.lock().unwrap();
        let mut store = RemoteStore::new(&mut db, identity.id());
        store
            .add_device(&device, &CredentialHash::from_credential(&credential))
            .unwrap();
        store
            .accept_command(
                &device.device_id,
                &request,
                &state.event_hub.epoch(),
                &checked("operation").unwrap(),
                1,
            )
            .unwrap();
    }
    drop(first);
    let _replacement = ServerRuntime::new(identity.clone(), services)
        .expect("released registration must permit another native owner");
    let entry = RemoteStore::new(&mut state.db.lock().unwrap(), identity.id())
        .get_command(&device.device_id, &request.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        entry.outcome.status,
        OperationStatus::Accepted,
        "listener reconstruction must not recover live process work"
    );
}

#[tokio::test]
async fn runtime_registration_remains_owned_by_accepted_native_work_after_runtime_drop() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().join("app")).unwrap();
    let identity = Arc::new(
        ServerIdentity::load_or_create(
            dir.path(),
            IdentityNamespace::Development,
            &mut state.db.lock().unwrap(),
            1,
        )
        .unwrap(),
    );
    let services = ControlServices::native(state.clone(), identity.id().clone()).unwrap();
    let runtime = ServerRuntime::new(identity.clone(), services.clone()).unwrap();
    let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("a".repeat(43)).unwrap();
    RemoteStore::new(&mut state.db.lock().unwrap(), identity.id())
        .add_device(&device, &CredentialHash::from_credential(&credential))
        .unwrap();
    let principal = RemotePrincipal {
        device_id: device.device_id,
        scopes: device.scopes,
    };
    let player = state.player.lock().await;
    let request=parse_request(&serde_json::json!({"v":1,"id":"live-work","method":"player.pause","params":{"sessionId":"session","paused":true},"expected":{"serverEpoch":state.event_hub.epoch(),"sessionId":"session"}}).to_string()).unwrap();
    runtime
        .dispatcher
        .dispatch(&principal, request)
        .await
        .unwrap();
    drop(runtime);
    assert!(
        matches!(ServerRuntime::new(identity.clone(),services.clone()),Err(error) if error.code==ErrorCode::Busy)
    );
    drop(player);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !state.operations.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(ServerRuntime::new(identity, services).is_ok());
}

#[tokio::test]
async fn authenticated_thumbnail_is_bounded_and_four_in_flight_requests_are_revocable() {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let fixture = Fixture::new().await;
    let root = fixture._dir.path().join("photos");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("image.png"),
        include_bytes!("../../icons/icon.png"),
    )
    .unwrap();
    let metadata = std::fs::metadata(root.join("image.png")).unwrap();
    {
        let db = fixture.runtime.state().db.lock().unwrap();
        db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('source','local',?,'Photos','[\"photo\"]','available')",[root.as_os_str().as_bytes()]).unwrap();
        db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('image','source',?1,'Image','photo',?2,?3,'available')",rusqlite::params![b"image.png".as_slice(),metadata.len() as i64,format!("{}:{}",metadata.mtime(),metadata.mtime_nsec())]).unwrap();
    }
    crate::library::thumbnails::get(fixture.runtime.state(), "image", CancellationToken::new())
        .await
        .unwrap();
    let opaque: String = fixture
        .runtime
        .state()
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT thumbnail_id FROM media_items WHERE id='image'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let request=format!("GET /remote/v1/thumbnails/{opaque} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\n\r\n",fixture.credential.as_str());
    let mut connection = fixture.tls().await;
    connection.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    let _ = connection.read_to_end(&mut response).await;
    let boundary = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let preview = &response[boundary..];
    assert!(crate::library::thumbnails::valid_png(preview));
    assert!(preview.len() <= 512 * 1024);
    assert!(u32::from_be_bytes(preview[16..20].try_into().unwrap()) <= 512);
    assert!(u32::from_be_bytes(preview[20..24].try_into().unwrap()) <= 512);
    let gate = fixture
        .runtime
        .state()
        .thumbnail_gate
        .acquire()
        .await
        .unwrap();
    let mut pending = Vec::new();
    for _ in 0..4 {
        let mut stream = fixture.tls().await;
        stream.write_all(request.as_bytes()).await.unwrap();
        pending.push(stream);
    }
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if fixture
                .runtime
                .thumbnails
                .acquire(fixture.principal.device_id.as_str(), 4)
                .is_none()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(fixture.http(&request).await.starts_with("HTTP/1.1 429"));
    fixture.revoke();
    for mut stream in pending {
        let mut byte = [0];
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut byte))
                .await
                .unwrap();
        assert!(
            matches!(result, Ok(0) | Err(_)),
            "revocation must close even blocked thumbnail response"
        );
    }
    drop(gate);
    fixture.server.stop().await;
}
#[tokio::test]
async fn refresh_on_same_socket_keeps_delivered_and_queued_event_sequences_contiguous() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.ws().await;
    for index in 0..4 {
        let hub = &fixture.runtime.state().event_hub;
        let stamp = hub.reserve(Topic::Connection);
        hub.publish(stamp, |server_epoch, sequence| RemoteEvent::Connection {
            server_epoch,
            sequence,
            data: ConnectionTopic {
                revision: stamp.revision.into(),
                server_device_id: fixture.runtime.identity.id().clone(),
                display_name: checked("Mediacenter").unwrap(),
            },
        });
        socket.send(Message::Text(serde_json::json!({"v":1,"id":format!("refresh{index}"),"method":"state.get","params":{}}).to_string().into())).await.unwrap();
        let refreshed = reply(&mut socket, &format!("refresh{index}")).await;
        let snapshot: RemoteSnapshot = serde_json::from_value(refreshed["result"].clone()).unwrap();
        let mut cursor = super::super::event_hub::RevisionCursor::new(&snapshot);
        if snapshot.sequence.value() < (index + 1) as u64 {
            let event: RemoteEvent = serde_json::from_value(next_json(&mut socket).await).unwrap();
            cursor.accept(&event).unwrap();
        }
    }
    fixture.server.stop().await;
}

// Paused-time regressions use the actual WebSocket loop and native snapshot
// capture. Observe only the transport boundary, without replacing dispatch.
struct ObservedDuplex {
    inner: tokio::io::DuplexStream,
    blocked: Arc<std::sync::atomic::AtomicBool>,
    read_bytes: Arc<std::sync::atomic::AtomicUsize>,
}
impl AsyncRead for ObservedDuplex {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        self.read_bytes.fetch_add(
            buf.filled().len() - before,
            std::sync::atomic::Ordering::SeqCst,
        );
        result
    }
}
impl AsyncWrite for ObservedDuplex {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let result = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if result.is_pending() {
            self.blocked
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let result = std::pin::Pin::new(&mut self.inner).poll_flush(cx);
        if result.is_pending() {
            self.blocked
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
struct TimedConnection {
    _dir: tempfile::TempDir,
    runtime: Arc<ServerRuntime>,
    client: WebSocketStream<tokio::io::DuplexStream>,
    server: tokio::task::JoinHandle<()>,
    blocked: Arc<std::sync::atomic::AtomicBool>,
    read_bytes: Arc<std::sync::atomic::AtomicUsize>,
}
impl Drop for TimedConnection {
    fn drop(&mut self) {
        self.server.abort();
    }
}
async fn settle_timers() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}
impl TimedConnection {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("app")).unwrap();
        let identity = Arc::new(
            ServerIdentity::load_or_create(
                dir.path(),
                IdentityNamespace::Development,
                &mut state.db.lock().unwrap(),
                1,
            )
            .unwrap(),
        );
        let runtime = ServerRuntime::new(
            identity.clone(),
            ControlServices::native(state.clone(), identity.id().clone()).unwrap(),
        )
        .unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
        let credential: BearerCredential = checked("a".repeat(43)).unwrap();
        let (_, events) = {
            let mut db = state.db.lock().unwrap();
            RemoteStore::new(&mut db, identity.id())
                .add_device(&device, &CredentialHash::from_credential(&credential))
                .unwrap();
            runtime
                .auth
                .authenticate_subscription(&mut db, &credential)
                .unwrap()
        };
        let principal = RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        };
        let (server_io, client_io) = tokio::io::duplex(256);
        let blocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let read_bytes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_socket = WebSocketStream::from_raw_socket(
            ObservedDuplex {
                inner: server_io,
                blocked: blocked.clone(),
                read_bytes: read_bytes.clone(),
            },
            Role::Server,
            Some(websocket_config()),
        )
        .await;
        let mut client = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        let server = tokio::spawn(websocket(
            server_socket,
            runtime.clone(),
            principal,
            events,
            CancellationToken::new(),
        ));
        for expected in ["hello", "snapshot"] {
            let message = client.next().await.unwrap().unwrap();
            let Message::Text(text) = message else {
                panic!("initial control message required")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap()["type"],
                expected
            );
        }
        settle_timers().await;
        blocked.store(false, std::sync::atomic::Ordering::SeqCst);
        Self {
            _dir: dir,
            runtime,
            client,
            server,
            blocked,
            read_bytes,
        }
    }
    async fn pong_just_after_ping_tick(&mut self) -> tokio::time::Instant {
        tokio::time::advance(std::time::Duration::from_secs(10)).await;
        settle_timers().await;
        assert!(matches!(
            self.client.next().await.unwrap().unwrap(),
            Message::Ping(_)
        ));
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        let at = tokio::time::Instant::now();
        self.client
            .send(Message::Pong(Bytes::from_static(b"mc")))
            .await
            .unwrap();
        settle_timers().await;
        assert!(
            self.read_bytes.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "server must actually receive the Pong"
        );
        at
    }
    async fn state_request(&mut self) {
        self.read_bytes
            .store(0, std::sync::atomic::Ordering::SeqCst);
        self.client
            .send(Message::Text(
                r#"{"v":1,"id":"deadline","method":"state.get","params":{}}"#.into(),
            ))
            .await
            .unwrap();
        settle_timers().await;
        assert!(
            self.read_bytes.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "server must receive the request before advancing time"
        );
    }
}
#[tokio::test(start_paused = true)]
async fn heartbeat_deadline_expires_thirty_seconds_after_pong_between_ping_ticks() {
    let mut connection = TimedConnection::new().await;
    let pong = connection.pong_just_after_ping_tick().await;
    tokio::time::advance(std::time::Duration::from_millis(29999)).await;
    settle_timers().await;
    assert!(
        !connection.server.is_finished(),
        "must not expire before thirty seconds since valid Pong"
    );
    tokio::time::advance(std::time::Duration::from_millis(2)).await;
    settle_timers().await;
    assert_eq!(
        tokio::time::Instant::now() - pong,
        std::time::Duration::from_millis(30001)
    );
    assert!(
        connection.server.is_finished(),
        "heartbeat expiry must not wait for the next ten-second ping tick"
    );
}
#[tokio::test(start_paused = true)]
async fn heartbeat_deadline_interrupts_native_capture_and_blocked_event_write() {
    for stalled_write in [false, true] {
        let mut connection = TimedConnection::new().await;
        connection.pong_just_after_ping_tick().await;
        tokio::time::advance(std::time::Duration::from_secs(29)).await;
        settle_timers().await;
        let player_state = connection.runtime.state().player.clone();
        let player = player_state.lock().await;
        if stalled_write {
            let hub = &connection.runtime.state().event_hub;
            for _ in 0..16 {
                let stamp = hub.reserve(Topic::Connection);
                hub.publish(stamp, |server_epoch, sequence| RemoteEvent::Connection {
                    server_epoch,
                    sequence,
                    data: ConnectionTopic {
                        revision: stamp.revision.into(),
                        server_device_id: connection.runtime.identity.id().clone(),
                        display_name: checked("Mediacenter").unwrap(),
                    },
                });
            }
            settle_timers().await;
            assert!(
                connection.blocked.load(std::sync::atomic::Ordering::SeqCst),
                "the event writer must actually stall"
            );
        } else {
            connection.state_request().await;
            assert!(
                !connection.blocked.load(std::sync::atomic::Ordering::SeqCst),
                "native capture, not a response write, must be pending"
            );
        }
        tokio::time::advance(std::time::Duration::from_millis(1001)).await;
        settle_timers().await;
        assert!(
            connection.server.is_finished(),
            "heartbeat expiry must interrupt pending inner processing/writes"
        );
        drop(player);
    }
}
#[tokio::test(start_paused = true)]
async fn acknowledgement_deadline_combines_delayed_capture_and_blocked_reply() {
    let mut connection = TimedConnection::new().await;
    let player_state = connection.runtime.state().player.clone();
    let player = player_state.lock().await;
    let received_at = tokio::time::Instant::now();
    connection.state_request().await;
    tokio::time::advance(std::time::Duration::from_secs(4)).await;
    settle_timers().await;
    assert!(!connection.server.is_finished());
    assert!(!connection.blocked.load(std::sync::atomic::Ordering::SeqCst));
    drop(player);
    settle_timers().await;
    assert!(
        connection.blocked.load(std::sync::atomic::Ordering::SeqCst),
        "capture must finish and the real response writer must stall"
    );
    tokio::time::advance(std::time::Duration::from_millis(999)).await;
    settle_timers().await;
    assert!(
        !connection.server.is_finished(),
        "reply can use the remaining acknowledgement budget"
    );
    tokio::time::advance(std::time::Duration::from_millis(2)).await;
    settle_timers().await;
    assert_eq!(
        tokio::time::Instant::now() - received_at,
        std::time::Duration::from_millis(5001)
    );
    assert!(
        connection.server.is_finished(),
        "processing and reply delivery must share one five-second budget"
    );
}
