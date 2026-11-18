use crate::{
    app_state::{now_ms, AppState},
    contracts::{AppError, ServiceDefinition, ServiceId, VpnSnapshot},
};
use std::future::Future;
use tokio_util::sync::CancellationToken;
pub fn evaluate_launch(required: bool, vpn: &VpnSnapshot, now: u64) -> Result<(), AppError> {
    if !required {
        return Ok(());
    }
    if vpn.state != "connected"
        || vpn.error_code.is_some()
        || vpn.observed_at_ms == 0
        || now < vpn.observed_at_ms
        || now - vpn.observed_at_ms > 10_000
    {
        return Err(AppError::new(
            "vpn_not_ready",
            "Connect NordVPN and refresh its status before opening this service.",
        ));
    }
    if vpn.kill_switch != "enabled" {
        return Err(AppError::new(
            "kill_switch_required",
            "Enable NordVPN Kill Switch in connection settings before opening this service.",
        ));
    }
    Ok(())
}
pub(crate) fn requires_vpn(services: &[ServiceDefinition], service: ServiceId) -> bool {
    services
        .iter()
        .find(|entry| entry.id == service)
        .is_none_or(|entry| entry.vpn_required)
}

pub(crate) async fn launch_after_preflight<T>(
    state: &AppState,
    service: ServiceId,
    cancel: &CancellationToken,
    observe: impl Future<Output = Result<VpnSnapshot, AppError>>,
    launch: impl Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    let cancelled = || AppError::new("cancelled", "Service opening was cancelled.");
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if !requires_vpn(&state.snapshot()?.services, service) {
        return launch.await;
    }
    // Keep app-driven provider changes out between the final observation and handoff.
    let _gate = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled()),
        gate = state.vpn_gate.lock() => gate,
    };
    let observation = observe.await;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    if requires_vpn(&state.snapshot()?.services, service) {
        evaluate_launch(true, &observation?, now_ms())?;
    }
    launch.await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::settings::save_requirement;
    fn ready() -> VpnSnapshot {
        VpnSnapshot {
            revision: 1,
            state: "connected".into(),
            country: Some("Finland".into()),
            kill_switch: "enabled".into(),
            observed_at_ms: 1000,
            error_code: None,
        }
    }
    #[test]
    fn required_service_rejects_disconnected_stale_or_unknown() {
        for state in [
            "disconnected",
            "unknown",
            "error",
            "unavailable",
            "connecting",
        ] {
            let mut vpn = ready();
            vpn.state = state.into();
            assert!(evaluate_launch(true, &vpn, 1000).is_err());
        }
        assert!(evaluate_launch(true, &ready(), 11001).is_err());
        assert!(evaluate_launch(true, &ready(), 999).is_err());
        assert!(evaluate_launch(true, &ready(), 1000).is_ok());
    }
    #[test]
    fn required_service_rejects_unverified_kill_switch() {
        for state in ["disabled", "unknown"] {
            let mut vpn = ready();
            vpn.kill_switch = state.into();
            assert!(evaluate_launch(true, &vpn, 1000).is_err());
        }
    }
    #[test]
    fn current_connection_service_does_not_change_vpn() {
        let mut vpn = ready();
        vpn.state = "disconnected".into();
        assert!(evaluate_launch(false, &vpn, 999999).is_ok());
        assert_eq!(vpn.state, "disconnected");
    }
    #[tokio::test]
    async fn replacement_launch_rejects_a_vpn_that_disconnected_after_preflight() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let mut vpn = ready();
        vpn.observed_at_ms = now_ms();
        evaluate_launch(true, &vpn, now_ms()).unwrap();
        state.home.lock().unwrap().vpn = vpn.clone();
        let mut launched = false;
        vpn.state = "disconnected".into();
        let error = launch_after_preflight(
            &state,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async { Ok(vpn) },
            async {
                launched = true;
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "vpn_not_ready");
        assert!(!launched);
    }
    #[tokio::test]
    async fn required_launch_keeps_the_provider_gate_through_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let vpn = VpnSnapshot {
            observed_at_ms: now_ms(),
            ..ready()
        };
        launch_after_preflight(
            &state,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async {
                assert!(state.vpn_gate.try_lock().is_err());
                Ok(vpn)
            },
            async {
                assert!(state.vpn_gate.try_lock().is_err());
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(state.vpn_gate.try_lock().is_ok());
    }
    #[tokio::test]
    async fn direct_launch_skips_provider_observation_even_while_the_gate_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        save_requirement(&state, ServiceId::HboMax, false).unwrap();
        let _busy = state.vpn_gate.lock().await;
        let launched = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            launch_after_preflight(
                &state,
                ServiceId::HboMax,
                &CancellationToken::new(),
                async {
                    panic!("direct launch should skip provider observation");
                },
                async { Ok(true) },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(launched);
    }
    #[tokio::test]
    async fn cancellation_during_the_final_vpn_read_blocks_launch() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let cancel = CancellationToken::new();
        let mut launched = false;
        let error = launch_after_preflight(
            &state,
            ServiceId::Youtube,
            &cancel,
            async {
                cancel.cancel();
                Ok(VpnSnapshot {
                    observed_at_ms: now_ms(),
                    ..ready()
                })
            },
            async {
                launched = true;
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "cancelled");
        assert!(!launched);
    }
}
