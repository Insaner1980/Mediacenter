use crate::{
    app_state::{now_ms, AppState},
    contracts::{AppError, ServiceDefinition, ServiceId, VpnSnapshot},
};
use std::future::Future;
use tokio::sync::MutexGuard;
use tokio_util::sync::CancellationToken;

fn observation_is_fresh(observed_at_ms: u64, now: u64) -> bool {
    observed_at_ms != 0
        && now
            .checked_sub(observed_at_ms)
            .is_some_and(|age| age <= 10_000)
}

pub fn evaluate_launch(required: bool, vpn: &VpnSnapshot, now: u64) -> Result<(), AppError> {
    if !required {
        return Ok(());
    }
    if vpn.state != "connected"
        || vpn.error_code.is_some()
        || !observation_is_fresh(vpn.observed_at_ms, now)
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

fn cancelled() -> AppError {
    AppError::new("cancelled", "Service opening was cancelled.")
}

pub(crate) struct LaunchPermit<'a> {
    _requirement: MutexGuard<'a, ()>,
    provider: Option<(MutexGuard<'a, ()>, u64)>,
}

impl LaunchPermit<'_> {
    pub(crate) async fn refresh_if_expired(
        &mut self,
        cancel: &CancellationToken,
        observe: impl Future<Output = Result<VpnSnapshot, AppError>>,
    ) -> Result<bool, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let Some((_, observed_at_ms)) = &mut self.provider else {
            return Ok(false);
        };
        if observation_is_fresh(*observed_at_ms, now_ms()) {
            return Ok(false);
        }
        let observation = observe.await;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let observation = observation?;
        evaluate_launch(true, &observation, now_ms())?;
        *observed_at_ms = observation.observed_at_ms;
        Ok(true)
    }
}

pub(crate) async fn admit_launch<'a>(
    state: &'a AppState,
    service: ServiceId,
    cancel: &CancellationToken,
    observe: impl Future<Output = Result<VpnSnapshot, AppError>>,
) -> Result<LaunchPermit<'a>, AppError> {
    let requirement = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled()),
        guard = state.vpn_requirement_gate.lock() => guard,
    };
    if !requires_vpn(&state.snapshot()?.services, service) {
        return Ok(LaunchPermit {
            _requirement: requirement,
            provider: None,
        });
    }
    // Keep policy edits available while a provider operation is already in flight.
    drop(requirement);
    let provider = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled()),
        gate = state.vpn_gate.lock() => gate,
    };
    let requirement = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled()),
        guard = state.vpn_requirement_gate.lock() => guard,
    };
    if !requires_vpn(&state.snapshot()?.services, service) {
        return Ok(LaunchPermit {
            _requirement: requirement,
            provider: None,
        });
    }
    let observation = observe.await;
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let observation = observation?;
    evaluate_launch(true, &observation, now_ms())?;
    Ok(LaunchPermit {
        _requirement: requirement,
        provider: Some((provider, observation.observed_at_ms)),
    })
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
        vpn.state = "disconnected".into();
        let outcome = admit_launch(
            &state,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async { Ok(vpn) },
        )
        .await;
        assert!(matches!(outcome, Err(error) if error.code == "vpn_not_ready"));
    }
    #[tokio::test]
    async fn required_launch_keeps_the_provider_gate_through_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let vpn = VpnSnapshot {
            observed_at_ms: now_ms(),
            ..ready()
        };
        let permit = admit_launch(
            &state,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async {
                assert!(state.vpn_gate.try_lock().is_err());
                Ok(vpn)
            },
        )
        .await
        .unwrap();
        assert!(state.vpn_gate.try_lock().is_err());
        drop(permit);
        assert!(state.vpn_gate.try_lock().is_ok());
    }
    #[tokio::test]
    async fn expired_observation_still_requires_a_ready_provider_at_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let cancel = CancellationToken::new();
        let mut observation = VpnSnapshot {
            observed_at_ms: now_ms(),
            ..ready()
        };
        let mut permit = admit_launch(&state, ServiceId::Youtube, &cancel, async {
            Ok(observation.clone())
        })
        .await
        .unwrap();
        permit.provider.as_mut().unwrap().1 = now_ms() - 10_001;
        observation.state = "disconnected".into();
        let outcome = permit
            .refresh_if_expired(&cancel, async {
                assert!(state.vpn_gate.try_lock().is_err());
                assert!(state.vpn_requirement_gate.try_lock().is_err());
                Ok(observation)
            })
            .await;
        assert!(matches!(outcome, Err(error) if error.code == "vpn_not_ready"));
    }
    #[tokio::test]
    async fn handoff_reuses_fresh_observations_and_remembers_a_successful_refresh() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let cancel = CancellationToken::new();
        let observation = VpnSnapshot {
            observed_at_ms: now_ms(),
            ..ready()
        };
        let mut permit = admit_launch(&state, ServiceId::Youtube, &cancel, async {
            Ok(observation.clone())
        })
        .await
        .unwrap();
        assert!(!permit
            .refresh_if_expired(&cancel, async { panic!("the observation is fresh") })
            .await
            .unwrap());
        permit.provider.as_mut().unwrap().1 = now_ms() - 10_001;
        assert!(permit
            .refresh_if_expired(&cancel, async { Ok(observation) })
            .await
            .unwrap());
        assert!(!permit
            .refresh_if_expired(&cancel, async {
                panic!("the refreshed observation is still fresh")
            })
            .await
            .unwrap());
    }
    #[tokio::test]
    async fn direct_launch_skips_provider_observation_even_while_the_gate_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        save_requirement(&state, ServiceId::HboMax, false).unwrap();
        let _busy = state.vpn_gate.lock().await;
        let cancel = CancellationToken::new();
        let mut permit = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            admit_launch(&state, ServiceId::HboMax, &cancel, async {
                panic!("direct launch should skip provider observation");
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!permit
            .refresh_if_expired(&cancel, async {
                panic!("direct handoff should skip provider observation")
            })
            .await
            .unwrap());
    }
    #[tokio::test]
    async fn direct_handoff_keeps_the_current_service_requirement_until_it_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        save_requirement(&state, ServiceId::Youtube, false).unwrap();
        let permit = admit_launch(
            &state,
            ServiceId::Youtube,
            &CancellationToken::new(),
            async {
                panic!("direct launches should not observe the provider");
            },
        )
        .await
        .unwrap();
        let update = save_requirement(&state, ServiceId::Youtube, true);
        assert!(matches!(update, Err(error) if error.code == "busy"));
        assert!(!requires_vpn(
            &state.snapshot().unwrap().services,
            ServiceId::Youtube
        ));
        drop(permit);
        save_requirement(&state, ServiceId::Youtube, true).unwrap();
    }
    #[tokio::test]
    async fn cancellation_during_the_final_vpn_read_blocks_launch() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let cancel = CancellationToken::new();
        let outcome = admit_launch(&state, ServiceId::Youtube, &cancel, async {
            cancel.cancel();
            Ok(VpnSnapshot {
                observed_at_ms: now_ms(),
                ..ready()
            })
        })
        .await;
        assert!(matches!(outcome, Err(error) if error.code == "cancelled"));
    }
}
