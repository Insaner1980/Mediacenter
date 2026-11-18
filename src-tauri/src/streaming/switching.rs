use crate::contracts::{AppError, ServiceId};
use std::future::Future;
use tokio_util::sync::CancellationToken;

pub trait ManagedWindow: Send {
    fn service_id(&self) -> Option<ServiceId>;
    fn session_id(&self) -> &str;
    fn focus(&self) -> impl Future<Output = Result<(), AppError>> + Send;
    fn close(&mut self) -> impl Future<Output = Result<(), AppError>> + Send;
}

// None means that the requested service was already open and has been focused.
// Some contains a prepared destination, after the previous owned window closed.
pub async fn prepare<W: ManagedWindow, T>(
    current: &mut Option<W>,
    requested: ServiceId,
    cancel: &CancellationToken,
    preflight: impl Future<Output = Result<T, AppError>> + Send,
    retired: impl FnOnce(&str) -> Result<(), AppError> + Send,
) -> Result<Option<T>, AppError> {
    check_cancelled(cancel)?;
    if let Some(window) = current
        .as_ref()
        .filter(|w| w.service_id() == Some(requested))
    {
        window.focus().await?;
        return Ok(None);
    }
    let destination = preflight.await?;
    check_cancelled(cancel)?;
    if let Some(window) = current.as_mut() {
        window.close().await?;
        let ended = current.take().unwrap();
        retired(ended.session_id())?;
    }
    check_cancelled(cancel)?;
    Ok(Some(destination))
}

fn check_cancelled(cancel: &CancellationToken) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        Err(AppError::new("cancelled", "Service opening was cancelled."))
    } else {
        Ok(())
    }
}

impl ManagedWindow for super::browser::BrowserSession {
    fn service_id(&self) -> Option<ServiceId> {
        self.session.service_id
    }
    fn session_id(&self) -> &str {
        &self.session.id
    }
    async fn focus(&self) -> Result<(), AppError> {
        self.focus().await
    }
    async fn close(&mut self) -> Result<(), AppError> {
        self.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    type Log = Arc<Mutex<Vec<&'static str>>>;
    #[tokio::test]
    #[ignore = "requires an explicitly isolated X11 display and window manager"]
    async fn native_switches_preserve_profiles_and_leave_another_chrome_untouched() {
        assert_eq!(
            std::env::var("MEDIACENTER_PRIVATE_X11_TEST").as_deref(),
            Ok("1")
        );
        assert_ne!(std::env::var("DISPLAY").as_deref(), Ok(":0"));
        use super::super::browser;
        let data = tempfile::tempdir().unwrap();
        let unrelated_data = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let mut unrelated =
            browser::launch(unrelated_data.path(), ServiceId::Youtube, cancel.clone())
                .await
                .unwrap();
        let mut current: Option<browser::BrowserSession> = None;
        let result: Result<(), AppError> = async {
            for service in [ServiceId::Youtube, ServiceId::Viaplay, ServiceId::Netflix, ServiceId::MtvKatsomo, ServiceId::Ruutu, ServiceId::YleAreena, ServiceId::Plex, ServiceId::Youtube] {
                let previous = current.as_ref().map(|b| b.identity.clone());
                let destination = prepare(&mut current, service, &cancel, async {
                    browser::prepare_launch(data.path(), service)?;
                    Ok(service)
                }, |_| Ok(())).await?.unwrap();
                if let Some(previous) = previous { assert!(!previous.still_owned()); }
                assert!(unrelated.identity.still_owned());
                let next = browser::launch(data.path(), destination, cancel.clone()).await?;
                next.verify().await?;
                let sentinel = next.profile.join("switch-test-profile-marker");
                if sentinel.exists() { assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserved"); }
                else { std::fs::write(&sentinel, b"preserved").unwrap(); }
                assert_eq!(next.session.service_id, Some(service));
                println!("Opened {service:?}; previous owned window closed; unrelated Chrome remains open");
                current = Some(next);
            }
            let pid = current.as_ref().unwrap().identity.pid;
            let focused = prepare(&mut current, ServiceId::Youtube, &cancel, async {
                Err::<(), _>(AppError::new("unexpected_preflight", "Already open"))
            }, |_| panic!("must not retire")).await?;
            assert!(focused.is_none());
            assert_eq!(current.as_ref().unwrap().identity.pid, pid);
            Ok(())
        }.await;
        if let Some(mut browser) = current {
            browser.close().await.unwrap();
        }
        unrelated.close().await.unwrap();
        result.unwrap();
    }
    struct Window {
        service: ServiceId,
        log: Log,
        refuse_close: bool,
    }
    impl ManagedWindow for Window {
        fn service_id(&self) -> Option<ServiceId> {
            Some(self.service)
        }
        fn session_id(&self) -> &str {
            "owned-session"
        }
        async fn focus(&self) -> Result<(), AppError> {
            self.log.lock().unwrap().push("focus");
            Ok(())
        }
        async fn close(&mut self) -> Result<(), AppError> {
            self.log.lock().unwrap().push("close-owned");
            if self.refuse_close {
                Err(AppError::new("ownership_uncertain", "Cannot safely close"))
            } else {
                Ok(())
            }
        }
    }
    fn fixture(service: ServiceId) -> (Option<Window>, Log) {
        let log = Arc::new(Mutex::new(vec![]));
        (
            Some(Window {
                service,
                log: log.clone(),
                refuse_close: false,
            }),
            log,
        )
    }
    #[tokio::test]
    async fn every_different_service_preflights_then_retires_only_the_owned_window() {
        for from in super::super::catalog::entries() {
            for to in super::super::catalog::entries()
                .into_iter()
                .filter(|s| s.id != from.id)
            {
                let (mut current, log) = fixture(from.id);
                let destination = prepare(
                    &mut current,
                    to.id,
                    &CancellationToken::new(),
                    async {
                        log.lock().unwrap().push("preflight");
                        Ok(to.id)
                    },
                    |id| {
                        assert_eq!(id, "owned-session");
                        log.lock().unwrap().push("retire");
                        Ok(())
                    },
                )
                .await
                .unwrap();
                assert_eq!(destination, Some(to.id));
                assert!(current.is_none());
                assert_eq!(*log.lock().unwrap(), ["preflight", "close-owned", "retire"]);
            }
        }
    }
    #[tokio::test]
    async fn same_service_focuses_without_closing_or_preflighting_its_locked_profile() {
        let (mut current, log) = fixture(ServiceId::Youtube);
        let result = prepare(
            &mut current,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async { Err::<(), _>(AppError::new("profile_busy", "Already open")) },
            |_| panic!("must not retire"),
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert!(current.is_some());
        assert_eq!(*log.lock().unwrap(), ["focus"]);
    }
    #[tokio::test]
    async fn failed_preflight_or_cancellation_keeps_the_current_service() {
        for cancel_during_check in [false, true] {
            let (mut current, log) = fixture(ServiceId::Youtube);
            let cancel = CancellationToken::new();
            let result = prepare(
                &mut current,
                ServiceId::Viaplay,
                &cancel,
                async {
                    if cancel_during_check {
                        cancel.cancel();
                        Ok(())
                    } else {
                        Err(AppError::new("vpn_not_ready", "VPN unavailable"))
                    }
                },
                |_| panic!("must not retire"),
            )
            .await;
            assert_eq!(
                result.unwrap_err().code,
                if cancel_during_check {
                    "cancelled"
                } else {
                    "vpn_not_ready"
                }
            );
            assert!(current.is_some());
            assert!(log.lock().unwrap().is_empty());
        }
    }
    #[tokio::test]
    async fn uncertain_ownership_prevents_replacement_and_keeps_the_session_registered() {
        let (mut current, log) = fixture(ServiceId::Youtube);
        current.as_mut().unwrap().refuse_close = true;
        let error = prepare(
            &mut current,
            ServiceId::Viaplay,
            &CancellationToken::new(),
            async { Ok(()) },
            |_| panic!("must not retire"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "ownership_uncertain");
        assert!(current.is_some());
        assert_eq!(*log.lock().unwrap(), ["close-owned"]);
    }
}
