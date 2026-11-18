//! A process-owned Cinnamon session inhibitor; never changes global power policy.
use super::protocol::{ErrorCode, IdleInhibition, RemoteError};
use crate::contracts::AppError;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
const IDLE_FLAG: u32 = 8;
type Effect<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;
trait Lease: Send + Sync {
    fn release(&self) -> Effect<'_, ()>;
    fn alive(&self) -> Effect<'_, ()>;
}
trait Backend: Send + Sync {
    fn acquire(&self) -> Effect<'_, Box<dyn Lease>>;
}
struct Native;
struct NativeLease {
    connection: zbus::Connection,
    cookie: u32,
    owner: String,
}
fn failed() -> AppError {
    AppError::new("unavailable", "Session idle inhibition is unavailable.")
}
impl NativeLease {
    async fn proxy(&self) -> Result<zbus::Proxy<'_>, zbus::Error> {
        zbus::Proxy::new(
            &self.connection,
            self.owner.as_str(),
            "/org/gnome/SessionManager",
            "org.gnome.SessionManager",
        )
        .await
    }
}
impl Lease for NativeLease {
    fn release(&self) -> Effect<'_, ()> {
        Box::pin(async {
            self.proxy()
                .await
                .map_err(|_| failed())?
                .call::<_, _, ()>("Uninhibit", &self.cookie)
                .await
                .map_err(|_| failed())
        })
    }
    fn alive(&self) -> Effect<'_, ()> {
        Box::pin(async {
            if self
                .proxy()
                .await
                .map_err(|_| failed())?
                .call::<_, _, bool>("IsInhibited", &IDLE_FLAG)
                .await
                .map_err(|_| failed())?
            {
                Ok(())
            } else {
                Err(failed())
            }
        })
    }
}
impl Backend for Native {
    fn acquire(&self) -> Effect<'_, Box<dyn Lease>> {
        Box::pin(async {
            let connection = zbus::Connection::session().await.map_err(|_| failed())?;
            let owner = zbus::fdo::DBusProxy::new(&connection)
                .await
                .map_err(|_| failed())?
                .get_name_owner("org.gnome.SessionManager".try_into().unwrap())
                .await
                .map_err(|_| failed())?
                .to_string();
            let mut lease = NativeLease {
                connection,
                cookie: 0,
                owner,
            };
            lease.cookie = lease
                .proxy()
                .await
                .map_err(|_| failed())?
                .call::<_, _, u32>(
                    "Inhibit",
                    &(
                        "io.emma.mediacenter",
                        0u32,
                        "Remote viewing playback",
                        IDLE_FLAG,
                    ),
                )
                .await
                .map_err(|error| match error {
                    zbus::Error::MethodError(name, _, _)
                        if matches!(
                            name.as_str(),
                            "org.freedesktop.DBus.Error.UnknownMethod"
                                | "org.freedesktop.DBus.Error.UnknownInterface"
                        ) =>
                    {
                        AppError::new("unsupported", "Session idle inhibition is unsupported.")
                    }
                    _ => failed(),
                })?;
            Ok(Box::new(lease) as Box<dyn Lease>)
        })
    }
}
#[derive(Clone)]
pub struct IdleSession {
    viewing: Arc<AtomicBool>,
    status: Arc<Mutex<Option<IdleInhibition>>>,
    owned: Arc<tokio::sync::Mutex<Option<Box<dyn Lease>>>>,
    backend: Arc<dyn Backend>,
}
impl Default for IdleSession {
    fn default() -> Self {
        Self::with_backend(Arc::new(Native))
    }
}
impl IdleSession {
    fn with_backend(backend: Arc<dyn Backend>) -> Self {
        Self {
            viewing: Arc::default(),
            status: Arc::default(),
            owned: Arc::default(),
            backend,
        }
    }
    pub fn begin_viewing(&self) {
        self.viewing.store(true, Ordering::SeqCst);
    }
    pub fn status(&self) -> IdleInhibition {
        self.status
            .lock()
            .ok()
            .and_then(|v| v.clone())
            .unwrap_or(IdleInhibition::Unavailable)
    }
    fn record(&self, value: IdleInhibition) {
        if let Ok(mut status) = self.status.lock() {
            *status = Some(value);
        }
    }
    pub async fn end(&self) -> Result<(), AppError> {
        self.viewing.store(false, Ordering::SeqCst);
        self.release().await
    }
    async fn release(&self) -> Result<(), AppError> {
        let mut owned = self.owned.lock().await;
        if let Some(lease) = owned.as_ref() {
            if !matches!(
                tokio::time::timeout(Duration::from_secs(2), lease.release()).await,
                Ok(Ok(()))
            ) {
                self.record(IdleInhibition::Unavailable);
                return Err(failed());
            }
            *owned = None;
            self.record(IdleInhibition::Inactive);
        }
        Ok(())
    }
    pub async fn reconcile(&self, active: bool) {
        if !self.viewing.load(Ordering::SeqCst) || !active {
            let _ = self.release().await;
            return;
        }
        let mut owned = self.owned.lock().await;
        if let Some(lease) = owned.as_ref() {
            if matches!(
                tokio::time::timeout(Duration::from_secs(2), lease.alive()).await,
                Ok(Ok(()))
            ) {
                self.record(IdleInhibition::Active);
                return;
            }
            // The dedicated connection owns this cookie. Closing it releases the sender's
            // inhibitors; never submit a stale cookie to a replacement session manager.
            *owned = None;
            self.record(IdleInhibition::Unavailable);
            return;
        }
        match tokio::time::timeout(Duration::from_secs(2), self.backend.acquire()).await {
            Ok(Ok(lease)) => {
                *owned = Some(lease);
                self.record(IdleInhibition::Active);
            }
            Ok(Err(error)) if error.code == "unsupported" => {
                self.record(IdleInhibition::Unsupported)
            }
            _ => self.record(IdleInhibition::Unavailable),
        }
    }
}
/// A previous unlocked answer never grants permission to a later input request.
pub async fn require_unlocked() -> Result<(), RemoteError> {
    let observed = tokio::time::timeout(Duration::from_millis(750), async {
        let connection = zbus::Connection::session().await?;
        let proxy = zbus::Proxy::new(
            &connection,
            "org.cinnamon.ScreenSaver",
            "/org/cinnamon/ScreenSaver",
            "org.cinnamon.ScreenSaver",
        )
        .await?;
        proxy.call::<_, _, bool>("GetActive", &()).await
    })
    .await;
    lock_decision(observed.ok().and_then(Result::ok))
}
pub(crate) fn lock_decision(locked: Option<bool>) -> Result<(), RemoteError> {
    match locked {
        Some(false) => Ok(()),
        Some(true) => Err(RemoteError::new(
            ErrorCode::DesktopLocked,
            "The desktop is locked.",
        )),
        None => Err(RemoteError::new(
            ErrorCode::Unavailable,
            "Desktop lock status could not be verified.",
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_unknown_lock_observation_never_grants_input() {
        assert!(lock_decision(Some(false)).is_ok());
        assert_eq!(
            lock_decision(Some(true)).unwrap_err().code,
            ErrorCode::DesktopLocked
        );
        assert_eq!(
            lock_decision(None).unwrap_err().code,
            ErrorCode::Unavailable
        );
        assert_eq!(IdleSession::default().status(), IdleInhibition::Unavailable);
        assert_eq!(IDLE_FLAG, 8);
    }
}
#[cfg(test)]
pub(super) mod lifetime_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[derive(Default)]
    struct Fake {
        acquired: AtomicUsize,
        released: AtomicUsize,
        fail_release: AtomicBool,
        gone: AtomicBool,
    }
    impl Backend for Arc<Fake> {
        fn acquire(&self) -> Effect<'_, Box<dyn Lease>> {
            Box::pin(async {
                self.acquired.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(self.clone()) as Box<dyn Lease>)
            })
        }
    }
    impl Lease for Arc<Fake> {
        fn release(&self) -> Effect<'_, ()> {
            Box::pin(async {
                if self.fail_release.load(Ordering::SeqCst) {
                    return Err(failed());
                }
                self.released.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
        fn alive(&self) -> Effect<'_, ()> {
            Box::pin(async {
                if self.gone.load(Ordering::SeqCst) {
                    Err(failed())
                } else {
                    Ok(())
                }
            })
        }
    }
    pub(in crate::remote) async fn active_session() -> IdleSession {
        let idle = IdleSession::with_backend(Arc::new(Arc::new(Fake::default())));
        idle.begin_viewing();
        idle.reconcile(true).await;
        idle
    }
    #[tokio::test]
    async fn only_explicit_remote_playback_inhibits_and_end_releases_once() {
        let fake = Arc::new(Fake::default());
        let idle = IdleSession::with_backend(Arc::new(fake.clone()));
        idle.reconcile(true).await;
        assert_eq!(fake.acquired.load(Ordering::SeqCst), 0);
        idle.begin_viewing();
        idle.reconcile(false).await;
        assert_eq!(fake.acquired.load(Ordering::SeqCst), 0);
        idle.reconcile(true).await;
        idle.reconcile(true).await;
        assert_eq!(fake.acquired.load(Ordering::SeqCst), 1);
        assert_eq!(idle.status(), IdleInhibition::Active);
        idle.reconcile(false).await;
        assert_eq!(fake.released.load(Ordering::SeqCst), 1);
        idle.reconcile(true).await;
        assert_eq!(fake.acquired.load(Ordering::SeqCst), 2);
        idle.end().await.unwrap();
        idle.reconcile(true).await;
        assert_eq!(fake.released.load(Ordering::SeqCst), 2);
        assert_eq!(fake.acquired.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn failed_release_and_disconnection_never_report_active_or_success() {
        let fake = Arc::new(Fake::default());
        let idle = IdleSession::with_backend(Arc::new(fake.clone()));
        idle.begin_viewing();
        idle.reconcile(true).await;
        fake.fail_release.store(true, Ordering::SeqCst);
        assert!(idle.end().await.is_err());
        assert_eq!(idle.status(), IdleInhibition::Unavailable);
        fake.fail_release.store(false, Ordering::SeqCst);
        idle.end().await.unwrap();
        idle.begin_viewing();
        idle.reconcile(true).await;
        fake.gone.store(true, Ordering::SeqCst);
        idle.reconcile(true).await;
        assert_eq!(idle.status(), IdleInhibition::Unavailable);
    }
}
