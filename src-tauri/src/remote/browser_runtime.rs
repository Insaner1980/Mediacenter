//! Live, user-local Native Messaging connections and the one owned browser target.
use super::{adapters::checked, browser_bridge, dispatch::unavailable, protocol::*};
use crate::{contracts::ServiceId, streaming::catalog};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{mpsc, oneshot, Semaphore},
};
use tokio_util::sync::CancellationToken;

pub const DIRECTORY: &str = "mediacenter-browser";
const MAX_PROFILES: usize = 12;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProfileKind {
    Ordinary,
    Managed,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Incoming {
    Hello {
        profile: Id,
        kind: ProfileKind,
        service: Option<ServiceId>,
    },
    State {
        session: Id,
        generation: Decimal,
        origin: Option<String>,
        ready: bool,
        pending_origin: Option<String>,
        #[serde(default)]
        closed: bool,
    },
    Reply {
        id: Id,
        ok: bool,
        error: Option<String>,
    },
    Wake {
        nonce: uuid::Uuid,
    },
}

#[derive(Serialize)]
struct Outgoing {
    id: Id,
    command: Value,
}
struct Message {
    outgoing: Outgoing,
    reply: oneshot::Sender<Result<(), RemoteError>>,
}

#[derive(Clone)]
struct Profile {
    id: Id,
    connection: uuid::Uuid,
    kind: ProfileKind,
    service: Option<ServiceId>,
    commands: mpsc::Sender<Message>,
    shutdown: CancellationToken,
}

#[derive(Clone)]
pub struct Session {
    pub id: Id,
    pub owner: Id,
    pub service: ServiceId,
    pub profile: Id,
    connection: uuid::Uuid,
    pub generation: Decimal,
    pub ready: bool,
    pub origin: Option<String>,
    pub pending_origin: Option<String>,
    preview_issued: bool,
}

#[derive(Default)]
struct Inner {
    profiles: HashMap<String, Profile>,
    session: Option<Session>,
    wake: Option<Wake>,
}

struct Wake {
    nonce: uuid::Uuid,
    service: ServiceId,
    requested: String,
    reply: oneshot::Sender<Id>,
}

pub struct ReadyProfile {
    id: String,
    nonce: uuid::Uuid,
}

#[derive(Clone, Default)]
pub struct Runtime {
    inner: Arc<Mutex<Inner>>,
}

pub fn bridge_error() -> RemoteError {
    RemoteError::new(
        ErrorCode::Unavailable,
        "Open the enrolled Chrome profile to use browser control.",
    )
}

pub fn stale() -> RemoteError {
    RemoteError::new(
        ErrorCode::StaleState,
        "Refresh the browser activity and try again.",
    )
}

fn reply_error(code: Option<&str>) -> RemoteError {
    match code {
        Some("stale_target" | "stale_viewport" | "origin_pending") => stale(),
        Some("busy" | "superseded") => {
            RemoteError::new(ErrorCode::Busy, "Browser input was superseded.")
        }
        _ => RemoteError::new(
            ErrorCode::OutcomeUnknown,
            "Check the browser before trying the action again.",
        ),
    }
}

impl Runtime {
    pub fn start(&self, runtime_directory: &Path) -> io::Result<()> {
        let (local, _lock) = browser_bridge::bind_runtime(runtime_directory, DIRECTORY)?;
        let listener = local.async_listener()?;
        let secret = local.broker_secret();
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let _lifetime = (local, _lock);
            let Ok(listener) = tokio::net::UnixListener::from_std(listener) else {
                return;
            };
            let slots = Arc::new(Semaphore::new(MAX_PROFILES));
            while let Ok((mut stream, _)) = listener.accept().await {
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    continue;
                };
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _slot = slot;
                    if !stream
                        .peer_cred()
                        .is_ok_and(|creds| creds.uid() == browser_bridge::owner())
                    {
                        return;
                    }
                    if tokio::time::timeout(
                        Duration::from_secs(2),
                        authenticate(&mut stream, &secret),
                    )
                    .await
                    .is_ok_and(|result| result.is_ok())
                    {
                        let _ = runtime.serve(stream).await;
                    }
                });
            }
        });
        Ok(())
    }

    pub fn profile(&self, service: ServiceId, requested: &str) -> Result<String, RemoteError> {
        let inner = self.inner.lock().map_err(|_| unavailable())?;
        let mut matches = inner.profiles.values().filter(|profile| {
            !profile.commands.is_closed() && profile_matches(profile, service, requested)
        });
        let profile = matches.next().ok_or_else(bridge_error)?;
        if matches.next().is_some() {
            return Err(RemoteError::new(
                ErrorCode::Unavailable,
                "Keep one enrolled Chrome profile open for this service.",
            ));
        }
        Ok(profile.id.as_str().into())
    }

    pub fn available(&self) -> bool {
        self.inner
            .lock()
            .is_ok_and(|inner| inner.profiles.values().any(|p| !p.commands.is_closed()))
    }

    pub fn launchable(&self, data: &Path, service: ServiceId, requested: &str) -> bool {
        self.profile(service, requested).is_ok() || managed_profile(data, service).is_some()
    }

    pub async fn prepare_profile(
        &self,
        data: &Path,
        service: ServiceId,
        requested: &str,
        cancel: &CancellationToken,
    ) -> Result<ReadyProfile, RemoteError> {
        let nonce = uuid::Uuid::new_v4();
        if let Ok(id) = self.profile(service, requested) {
            return Ok(ReadyProfile { id, nonce });
        }
        let path = managed_profile(data, service).ok_or_else(bridge_error)?;
        if cancel.is_cancelled() {
            return Err(RemoteError::new(
                ErrorCode::Cancelled,
                "Browser activity cancelled.",
            ));
        }
        let (reply, response) = oneshot::channel();
        {
            let mut inner = self.inner.lock().map_err(|_| unavailable())?;
            if inner.wake.is_some() {
                return Err(RemoteError::new(
                    ErrorCode::Busy,
                    "Chrome is already starting.",
                ));
            }
            inner.wake = Some(Wake {
                nonce,
                service,
                requested: requested.into(),
                reply,
            });
        }
        let result = wake_chrome(&path, nonce, response, cancel).await;
        if let Ok(mut inner) = self.inner.lock() {
            if inner.wake.as_ref().is_some_and(|wake| wake.nonce == nonce) {
                inner.wake = None;
            }
        }
        let id = result?;
        let id = self.profile(service, id.as_str())?;
        Ok(ReadyProfile { id, nonce })
    }

    pub fn session(&self) -> Option<Session> {
        self.inner.lock().ok()?.session.clone()
    }

    pub async fn launch(
        &self,
        owner: Id,
        service: ServiceId,
        profile: &ReadyProfile,
        cancel: &CancellationToken,
    ) -> Result<Session, RemoteError> {
        let session = {
            let mut inner = self.inner.lock().map_err(|_| unavailable())?;
            if inner.session.is_some() {
                return Err(RemoteError::new(
                    ErrorCode::Busy,
                    "A browser session is already open.",
                ));
            }
            let profile = inner
                .profiles
                .get(&profile.id)
                .filter(|p| !p.commands.is_closed() && profile_matches(p, service, &profile.id))
                .ok_or_else(bridge_error)?;
            let session = Session {
                id: checked(uuid::Uuid::new_v4().to_string())?,
                owner,
                service,
                profile: profile.id.clone(),
                connection: profile.connection,
                generation: 0.into(),
                ready: false,
                origin: None,
                pending_origin: None,
                preview_issued: false,
            };
            inner.session = Some(session.clone());
            session
        };
        let result = self.send(&session, serde_json::json!({"type":"launch","url":catalog::url(service),"session":session.id,"nonce":profile.nonce}), Duration::from_secs(8), cancel).await;
        if let Err(error) = result {
            // A timed-out launch can still have created a tab. Retire the port to
            // prevent subsequent commands targeting an uncertain browser state.
            self.disconnect(&session.profile, session.connection);
            return Err(error);
        }
        self.session()
            .filter(|current| current.id == session.id)
            .ok_or_else(bridge_error)
    }

    pub fn claim_origin_preview(&self, expected: &Session) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let Some(current) = inner.session.as_mut().filter(|s| {
            s.id == expected.id
                && s.generation == expected.generation
                && s.pending_origin == expected.pending_origin
                && !s.preview_issued
        }) else {
            return false;
        };
        current.preview_issued = true;
        true
    }

    pub fn retire(&self, expected: &Session) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.session.as_ref().is_some_and(|s| s.id == expected.id) {
                inner.session = None;
            }
        }
    }

    pub async fn command(
        &self,
        session: &Session,
        command: Value,
        cancel: &CancellationToken,
    ) -> Result<(), RemoteError> {
        self.send(session, command, Duration::from_secs(3), cancel)
            .await
    }

    async fn send(
        &self,
        session: &Session,
        command: Value,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<(), RemoteError> {
        let sender = {
            let inner = self.inner.lock().map_err(|_| unavailable())?;
            let current = inner.session.as_ref().ok_or_else(stale)?;
            if current.id != session.id
                || current.generation != session.generation
                || current.connection != session.connection
            {
                return Err(stale());
            }
            inner
                .profiles
                .get(session.profile.as_str())
                .filter(|p| p.connection == session.connection)
                .ok_or_else(bridge_error)?
                .commands
                .clone()
        };
        if cancel.is_cancelled() {
            return Err(stale());
        }
        let (reply, result) = oneshot::channel();
        let id = checked(uuid::Uuid::new_v4().to_string())?;
        sender
            .try_send(Message {
                outgoing: Outgoing { id, command },
                reply,
            })
            .map_err(|_| RemoteError::new(ErrorCode::Busy, "Browser input queue is full."))?;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(reply_error(None)),
            received = tokio::time::timeout(timeout, result) => received.ok().and_then(Result::ok).unwrap_or_else(|| Err(reply_error(None))),
        }
    }

    fn register(
        &self,
        id: Id,
        kind: ProfileKind,
        service: Option<ServiceId>,
        commands: mpsc::Sender<Message>,
    ) -> io::Result<Profile> {
        if (kind == ProfileKind::Ordinary && service.is_some())
            || (kind == ProfileKind::Managed
                && service.is_none_or(|value| value == ServiceId::HboMax))
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let profile = Profile {
            id,
            connection: uuid::Uuid::new_v4(),
            kind,
            service,
            commands,
            shutdown: CancellationToken::new(),
        };
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("browser state unavailable"))?;
        if inner.profiles.contains_key(profile.id.as_str()) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        inner
            .profiles
            .insert(profile.id.as_str().into(), profile.clone());
        Ok(profile)
    }

    fn disconnect(&self, profile: &Id, connection: uuid::Uuid) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner
                .profiles
                .get(profile.as_str())
                .is_some_and(|p| p.connection == connection)
            {
                if let Some(profile) = inner.profiles.remove(profile.as_str()) {
                    profile.shutdown.cancel();
                }
            }
            if inner
                .session
                .as_ref()
                .is_some_and(|s| s.connection == connection)
            {
                inner.session = None;
            }
        }
    }

    fn update(&self, profile: &Profile, update: Incoming) {
        let Incoming::State {
            session,
            generation,
            origin,
            ready,
            pending_origin,
            closed,
        } = update
        else {
            return;
        };
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let Some(current) = inner.session.as_mut().filter(|s| {
            s.connection == profile.connection
                && s.id == session
                && generation.value() >= s.generation.value()
        }) else {
            return;
        };
        if closed {
            inner.session = None;
            return;
        }
        let origin = origin.as_deref().and_then(origin_host);
        let pending = pending_origin.as_deref().and_then(origin_host);
        if current.generation != generation || current.pending_origin.as_deref() != pending {
            current.preview_issued = false;
        }
        current.generation = generation;
        current.ready = ready && origin.is_some() && pending_origin.is_none();
        current.origin = origin.map(str::to_owned);
        current.pending_origin = pending.map(str::to_owned);
    }

    fn redeem_wake(&self, profile: &Profile, nonce: uuid::Uuid) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let matches = inner.wake.as_ref().is_some_and(|wake| {
            wake.nonce == nonce && profile_matches(profile, wake.service, &wake.requested)
        });
        if let Some(wake) = matches.then(|| inner.wake.take()).flatten() {
            let _ = wake.reply.send(profile.id.clone());
        }
    }

    async fn serve(&self, mut stream: UnixStream) -> io::Result<()> {
        let hello =
            tokio::time::timeout(Duration::from_secs(2), read_message(&mut stream)).await??;
        let Incoming::Hello {
            profile,
            kind,
            service,
        } = hello
        else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        let (sender, mut commands) = mpsc::channel::<Message>(8);
        let profile = self.register(profile, kind, service, sender)?;
        let (mut reader, mut writer) = stream.into_split();
        let pending = Arc::new(Mutex::new(HashMap::<
            String,
            oneshot::Sender<Result<(), RemoteError>>,
        >::new()));
        let sending = async {
            tokio::time::timeout(
                Duration::from_secs(1),
                write_frame(&mut writer, br#"{"type":"welcome"}"#),
            )
            .await??;
            while let Some(message) = commands.recv().await {
                if message.reply.is_closed() {
                    continue;
                }
                {
                    let mut requests = pending.lock().map_err(|_| io::ErrorKind::Other)?;
                    requests.retain(|_, reply| !reply.is_closed());
                    if requests.len() >= 8 {
                        continue;
                    }
                    requests.insert(message.outgoing.id.as_str().into(), message.reply);
                }
                let body = serde_json::to_vec(&message.outgoing)?;
                tokio::time::timeout(Duration::from_secs(1), write_frame(&mut writer, &body))
                    .await??;
            }
            Ok::<_, io::Error>(())
        };
        let reading = async {
            loop {
                match read_message(&mut reader).await? {
                    state @ Incoming::State { .. } => self.update(&profile, state),
                    Incoming::Wake { nonce } => self.redeem_wake(&profile, nonce),
                    Incoming::Reply { id, ok, error } => {
                        if let Some(reply) = pending
                            .lock()
                            .map_err(|_| io::ErrorKind::Other)?
                            .remove(id.as_str())
                        {
                            let _ = reply.send(if ok {
                                Ok(())
                            } else {
                                Err(reply_error(error.as_deref()))
                            });
                        }
                    }
                    Incoming::Hello { .. } => return Err(io::ErrorKind::InvalidData.into()),
                }
            }
        };
        let result = tokio::select! {
            biased;
            _ = profile.shutdown.cancelled() => Ok(()),
            result = sending => result,
            result = reading => result,
        };
        self.disconnect(&profile.id, profile.connection);
        result
    }

    #[cfg(test)]
    pub(crate) fn test_profile(
        &self,
        kind: ProfileKind,
        service: Option<ServiceId>,
    ) -> (String, impl Sized) {
        let (sender, receiver) = mpsc::channel(2);
        let id: Id = checked(uuid::Uuid::new_v4().to_string()).unwrap();
        self.register(id.clone(), kind, service, sender).unwrap();
        (id.as_str().into(), receiver)
    }
}

fn profile_matches(profile: &Profile, service: ServiceId, requested: &str) -> bool {
    (requested == "auto" || requested == profile.id.as_str())
        && if service == ServiceId::HboMax {
            profile.kind == ProfileKind::Ordinary
        } else {
            profile.kind == ProfileKind::Managed && profile.service == Some(service)
        }
}

fn managed_profile(data: &Path, service: ServiceId) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    if service == ServiceId::HboMax {
        return None;
    }
    let parent = data.join("browser-profiles");
    let profile = parent.join(catalog::key(service));
    let manifests = profile.join("NativeMessagingHosts");
    for path in [&parent, &profile, &manifests] {
        let metadata = path.symlink_metadata().ok()?;
        if !metadata.is_dir() || metadata.uid() != browser_bridge::owner() {
            return None;
        }
    }
    let registration = manifests.join("io.emma.mediacenter.browser.json");
    let metadata = registration.symlink_metadata().ok()?;
    if !metadata.is_file() || metadata.uid() != browser_bridge::owner() || metadata.len() > 8192 {
        return None;
    }
    let manifest: Value = serde_json::from_slice(&std::fs::read(registration).ok()?).ok()?;
    (manifest["name"] == "io.emma.mediacenter.browser"
        && manifest["type"] == "stdio"
        && manifest["allowed_origins"]
            == serde_json::json!([super::browser_protocol::EXTENSION_ORIGIN]))
    .then_some(profile)
}

async fn wake_chrome(
    path: &Path,
    nonce: uuid::Uuid,
    response: oneshot::Receiver<Id>,
    cancel: &CancellationToken,
) -> Result<Id, RemoteError> {
    let url = format!(
        "{}launch.html#{nonce}",
        super::browser_protocol::EXTENSION_ORIGIN
    );
    let mut chrome = tokio::process::Command::new("/usr/bin/google-chrome")
        .args(crate::streaming::browser::app_arguments(path, &url))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| bridge_error())?;
    // Chrome may hand off to an already running instance and exit before that
    // instance connects. Reap only this launcher without touching any windows.
    tokio::spawn(async move {
        let _ = chrome.wait().await;
    });
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(RemoteError::new(ErrorCode::Cancelled, "Browser activity cancelled.")),
        result = tokio::time::timeout(Duration::from_secs(8), response) => {
            result.ok().and_then(Result::ok).ok_or_else(bridge_error)
        }
    }
}

// Only a sanitized DNS/IPv4 host is exposed across the paired phone protocol.
fn origin_host(origin: &str) -> Option<&str> {
    let host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))?;
    let host = host.split_once(':').map_or(host, |(host, port)| {
        if port.parse::<u16>().is_ok() {
            host
        } else {
            ""
        }
    });
    (!host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-'))
    .then_some(host)
}

async fn authenticate(stream: &mut UnixStream, secret: &[u8; 32]) -> io::Result<()> {
    use subtle::ConstantTimeEq;
    let mut supplied = [0; 32];
    stream.read_exact(&mut supplied).await?;
    if !bool::from(secret.ct_eq(&supplied)) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

async fn read_message(input: &mut (impl tokio::io::AsyncRead + Unpin)) -> io::Result<Incoming> {
    let mut header = [0; 4];
    input.read_exact(&mut header).await?;
    let length = u32::from_ne_bytes(header) as usize;
    if !(1..=super::browser_protocol::MAX_NATIVE_BYTES).contains(&length) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

async fn write_frame(
    output: &mut (impl tokio::io::AsyncWrite + Unpin),
    body: &[u8],
) -> io::Result<()> {
    if !(1..=super::browser_protocol::MAX_NATIVE_BYTES).contains(&body.len()) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    output.write_all(&(body.len() as u32).to_ne_bytes()).await?;
    output.write_all(body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixStream;

    async fn receive(stream: &mut UnixStream) -> Value {
        let mut size = [0; 4];
        stream.read_exact(&mut size).await.unwrap();
        let mut body = vec![0; u32::from_ne_bytes(size) as usize];
        stream.read_exact(&mut body).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn emit(stream: &mut UnixStream, message: Value) {
        write_frame(stream, &serde_json::to_vec(&message).unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn uncertain_launch_closes_the_native_port() {
        let runtime = Runtime::default();
        let (server, mut extension) = UnixStream::pair().unwrap();
        let serving = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.serve(server).await }
        });
        let profile = ReadyProfile {
            id: uuid::Uuid::new_v4().to_string(),
            nonce: uuid::Uuid::new_v4(),
        };
        emit(&mut extension, serde_json::json!({"type":"hello","profile":profile.id,"kind":"managed","service":"youtube"})).await;
        assert_eq!(receive(&mut extension).await["type"], "welcome");
        let cancel = CancellationToken::new();
        let launching = runtime.launch(
            checked("phone").unwrap(),
            ServiceId::Youtube,
            &profile,
            &cancel,
        );
        let cancel_after_handoff = async {
            let launch = receive(&mut extension).await;
            assert_eq!(launch["command"]["type"], "launch");
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(launching, cancel_after_handoff);
        assert!(matches!(result, Err(error) if error.code == ErrorCode::OutcomeUnknown));
        assert!(!runtime.available());
        let mut byte = [0u8; 1];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), extension.read(&mut byte))
                .await
                .expect("retired native port closed")
                .unwrap(),
            0
        );
        serving.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn live_port_owns_one_target_and_retires_generation_on_navigation_and_disconnect() {
        let runtime = Runtime::default();
        let (server, mut extension) = UnixStream::pair().unwrap();
        let serving = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.serve(server).await }
        });
        let profile = ReadyProfile {
            id: uuid::Uuid::new_v4().to_string(),
            nonce: uuid::Uuid::new_v4(),
        };
        emit(&mut extension, serde_json::json!({"type":"hello","profile":profile.id,"kind":"managed","service":"youtube"})).await;
        assert_eq!(receive(&mut extension).await["type"], "welcome");
        let cancel = CancellationToken::new();
        let launching = runtime.launch(
            checked("phone").unwrap(),
            ServiceId::Youtube,
            &profile,
            &cancel,
        );
        let fixture = async {
            let launch = receive(&mut extension).await;
            assert_eq!(launch["command"]["url"], catalog::url(ServiceId::Youtube));
            assert_eq!(launch["command"]["nonce"], profile.nonce.to_string());
            let session = &launch["command"]["session"];
            emit(&mut extension, serde_json::json!({"type":"state","session":session,"generation":"1","origin":"https://www.youtube.com","ready":true})).await;
            emit(
                &mut extension,
                serde_json::json!({"type":"reply","id":launch["id"],"ok":true}),
            )
            .await;
        };
        let (session, ()) = tokio::join!(launching, fixture);
        let session = session.unwrap();
        assert!(session.ready);
        assert_eq!(session.origin.as_deref(), Some("www.youtube.com"));
        emit(&mut extension, serde_json::json!({"type":"state","session":session.id,"generation":"2","origin":"https://login.example.test","ready":false,"pendingOrigin":"https://login.example.test"})).await;
        while runtime.session().unwrap().generation.value() != 2 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            runtime
                .command(&session, serde_json::json!({"type":"input"}), &cancel)
                .await
                .unwrap_err()
                .code,
            ErrorCode::StaleState
        );
        let current = runtime.session().unwrap();
        assert_eq!(
            current.pending_origin.as_deref(),
            Some("login.example.test")
        );
        assert!(runtime.claim_origin_preview(&current));
        assert!(!runtime.claim_origin_preview(&current));
        drop(extension);
        assert!(serving.await.unwrap().is_err());
        assert!(!runtime.available());
        assert!(runtime.session().is_none());
    }

    #[test]
    fn managed_wake_accepts_the_grant_once_from_the_selected_profile() {
        let runtime = Runtime::default();
        let (sender, _receiver) = mpsc::channel(1);
        let registered = |id, service| {
            runtime
                .register(
                    checked(id).unwrap(),
                    ProfileKind::Managed,
                    Some(service),
                    sender.clone(),
                )
                .unwrap()
        };
        let selected = registered("selected", ServiceId::Youtube);
        let other = registered("other", ServiceId::Youtube);
        let unrelated = registered("unrelated", ServiceId::Netflix);
        let (reply, mut response) = oneshot::channel();
        let nonce = uuid::Uuid::new_v4();
        runtime.inner.lock().unwrap().wake = Some(Wake {
            nonce,
            service: ServiceId::Youtube,
            requested: "selected".into(),
            reply,
        });
        runtime.redeem_wake(&selected, uuid::Uuid::new_v4());
        runtime.redeem_wake(&other, nonce);
        runtime.redeem_wake(&unrelated, nonce);
        assert!(matches!(
            response.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        runtime.redeem_wake(&selected, nonce);
        assert_eq!(response.try_recv().unwrap(), selected.id);
        assert!(runtime.inner.lock().unwrap().wake.is_none());
    }
}
