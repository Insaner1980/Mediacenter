use crate::contracts::{AppError, VpnSnapshot};
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
#[cfg(test)]
mod tests {
    use super::*;
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
}
