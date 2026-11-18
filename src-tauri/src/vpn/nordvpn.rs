use crate::{
    app_state::{now_ms, AppState},
    contracts::*,
    events,
    process::runner::{self, CommandSpec, Executable},
};
use std::{ffi::OsString, time::Duration};
use tokio_util::sync::CancellationToken;
async fn command(
    args: &[&str],
    seconds: u64,
    cancel: CancellationToken,
) -> Result<runner::Output, String> {
    runner::run(
        CommandSpec {
            executable: Executable::NordVpn,
            args: args.iter().map(OsString::from).collect(),
            timeout: Duration::from_secs(seconds),
            output_limit: 1024 * 1024,
        },
        cancel,
    )
    .await
}
pub(crate) async fn observe_locked(state: &AppState) -> Result<VpnSnapshot, AppError> {
    observe_locked_cancellable(state, &CancellationToken::new()).await
}
async fn observe_locked_cancellable(
    state: &AppState,
    cancel: &CancellationToken,
) -> Result<VpnSnapshot, AppError> {
    let status = command(&["status"], 5, cancel.clone()).await;
    if cancel.is_cancelled() {
        return Err(observation_cancelled());
    }
    let settings = command(&["settings"], 5, cancel.clone()).await;
    if cancel.is_cancelled() {
        return Err(observation_cancelled());
    }
    let now = now_ms();
    let mut snapshot = match (&status, &settings) {
        (Ok(status), Ok(settings)) if status.success && settings.success => super::parser::parse(
            &String::from_utf8_lossy(&status.stdout),
            &String::from_utf8_lossy(&settings.stdout),
            now,
        ),
        _ => VpnSnapshot {
            revision: 0,
            state: "unknown".into(),
            country: None,
            kill_switch: "unknown".into(),
            observed_at_ms: now,
            error_code: Some("provider_unavailable".into()),
        },
    };
    if let Err(error) = &status {
        if error == "spawn_failed" {
            snapshot.state = "unavailable".into();
        }
    }
    if let Ok(output) = &status {
        if !output.success {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
            .to_lowercase();
            if text.contains("not logged in") || text.contains("log in") {
                snapshot.state = "unauthenticated".into();
            } else if text.contains("daemon") || text.contains("permission") {
                snapshot.state = "unavailable".into();
            }
        }
    }
    snapshot.revision = state.snapshot()?.vpn.revision + 1;
    Ok(snapshot)
}
fn observation_cancelled() -> AppError {
    AppError::new("cancelled", "VPN status check cancelled.")
}
pub async fn refresh_locked(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<VpnSnapshot, AppError> {
    refresh_locked_cancellable(app, state, &CancellationToken::new()).await
}
pub(crate) async fn refresh_locked_cancellable(
    app: &tauri::AppHandle,
    state: &AppState,
    cancel: &CancellationToken,
) -> Result<VpnSnapshot, AppError> {
    let snapshot = observe_locked_cancellable(state, cancel).await?;
    events::publish(app, state, |home| home.vpn = snapshot.clone())?;
    Ok(snapshot)
}
pub(crate) async fn refresh_deferred(
    state: &AppState,
    publications: &events::DeferredPublications<'_>,
    cancel: &CancellationToken,
) -> Result<VpnSnapshot, AppError> {
    let _guard = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(observation_cancelled()),
        guard = state.vpn_gate.lock() => guard,
    };
    refresh_deferred_locked(state, publications, cancel).await
}
pub(crate) async fn refresh_deferred_locked(
    state: &AppState,
    publications: &events::DeferredPublications<'_>,
    cancel: &CancellationToken,
) -> Result<VpnSnapshot, AppError> {
    let snapshot = observe_locked_cancellable(state, cancel).await?;
    publications.publish(|home| home.vpn = snapshot.clone())?;
    Ok(snapshot)
}
pub async fn refresh(app: &tauri::AppHandle, state: &AppState) -> Result<VpnSnapshot, AppError> {
    let _guard = state.vpn_gate.lock().await;
    refresh_locked(app, state).await
}
pub async fn countries_locked() -> Result<Vec<CountryOption>, AppError> {
    let output = command(&["countries"], 5, CancellationToken::new())
        .await
        .map_err(|_| {
            AppError::new(
                "countries_unavailable",
                "NordVPN countries could not be read.",
            )
        })?;
    if !output.success {
        return Err(AppError::new(
            "countries_unavailable",
            "NordVPN countries could not be read.",
        ));
    }
    let mut countries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let id = line.trim();
        if !id.is_empty()
            && id.len() <= 100
            && id.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        {
            countries.push(CountryOption {
                id: id.into(),
                name: id.replace('_', " "),
            });
        }
    }
    if countries.is_empty() {
        return Err(AppError::new(
            "countries_unrecognized",
            "NordVPN returned an unrecognized country list.",
        ));
    }
    Ok(countries)
}
pub enum Action {
    Connect(Option<String>),
    Disconnect,
    EnableKillSwitch,
}
pub async fn change(
    app: tauri::AppHandle,
    state: AppState,
    action: Action,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    let _guard = state.vpn_gate.try_lock().map_err(|_| {
        AppError::new(
            "vpn_busy",
            "A NordVPN operation is already running. Try again shortly.",
        )
    })?;
    change_locked(&app, &state, action, cancel).await
}
pub(crate) async fn change_locked(
    app: &tauri::AppHandle,
    state: &AppState,
    action: Action,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    change_locked_checked(app, state, action, cancel, || Ok(())).await
}
pub(crate) async fn change_locked_checked(
    app: &tauri::AppHandle,
    state: &AppState,
    action: Action,
    cancel: CancellationToken,
    authorize: impl FnOnce() -> Result<(), AppError>,
) -> Result<(), AppError> {
    let mut args = Vec::new();
    let mut transition = None;
    match &action {
        Action::Connect(country) => {
            args.push("connect");
            if let Some(country) = country {
                if !countries_locked().await?.iter().any(|c| &c.id == country) {
                    return Err(AppError::new(
                        "invalid_country",
                        "Select a country from NordVPN's available countries.",
                    ));
                }
                args.push(country);
            }
            transition = Some("connecting");
        }
        Action::Disconnect => {
            args.push("disconnect");
            transition = Some("disconnecting");
        }
        Action::EnableKillSwitch => args.extend(["set", "killswitch", "on"]),
    }
    authorize()?;
    if let Some(phase) = transition {
        events::publish(app, state, |home| {
            home.vpn.state = phase.into();
            home.vpn.country = None;
            home.vpn.observed_at_ms = now_ms();
        })?;
    }
    let result = command(&args, 60, cancel).await;
    let observed = refresh_locked(app, state).await?;
    match result {
        Ok(output) if output.success => {
            let succeeded = confirmed(&action, &observed);
            if succeeded {
                Ok(())
            } else {
                Err(AppError::new(
                    "vpn_not_confirmed",
                    "NordVPN has not confirmed the requested state. Refresh its status.",
                ))
            }
        }
        Err(code) => Err(AppError::new(
            &code,
            "NordVPN did not complete the request. Its current status has been checked again.",
        )),
        _ => Err(AppError::new(
            "provider_rejected",
            "NordVPN rejected the request. Check the NordVPN application and try again.",
        )),
    }
}

/// Fixed read-only version command. Unrecognized output is never displayed.
pub(crate) async fn version() -> Option<String> {
    let output = command(&["--version"], 5, CancellationToken::new())
        .await
        .ok()?;
    if !output.success {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
}
pub(crate) fn parse_version(text: &str) -> Option<String> {
    let version = text.trim().strip_prefix("NordVPN Version ")?;
    if version.len() > 32
        || version.is_empty()
        || !version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
        || !version.contains('.')
    {
        return None;
    }
    Some(version.into())
}

fn confirmed(action: &Action, observed: &VpnSnapshot) -> bool {
    match action {
        Action::Connect(country) => {
            observed.state == "connected"
                && country.as_ref().is_none_or(|requested| {
                    observed.country.as_ref().is_some_and(|actual| {
                        normalized_country(actual) == normalized_country(requested)
                    })
                })
        }
        Action::Disconnect => observed.state == "disconnected",
        Action::EnableKillSwitch => observed.kill_switch == "enabled",
    }
}
fn normalized_country(country: &str) -> String {
    country
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_identifier_requires_nonempty_numeric_components() {
        for text in [
            "NordVPN Version .",
            "NordVPN Version 3..1",
            "NordVPN Version .3",
            "NordVPN Version 3.",
        ] {
            assert!(parse_version(text).is_none());
        }
        assert_eq!(
            parse_version("NordVPN Version 3.20.1"),
            Some("3.20.1".into())
        );
    }
    fn observed(country: Option<&str>) -> VpnSnapshot {
        VpnSnapshot {
            revision: 1,
            state: "connected".into(),
            country: country.map(str::to_owned),
            kill_switch: "enabled".into(),
            observed_at_ms: 1000,
            error_code: None,
        }
    }
    #[test]
    fn explicit_country_rejects_wrong_or_missing_provider_country() {
        let action = Action::Connect(Some("United_States".into()));
        assert!(!confirmed(&action, &observed(Some("Finland"))));
        assert!(!confirmed(&action, &observed(None)));
    }
    #[test]
    fn explicit_country_matches_only_normalized_country_and_other_actions_keep_their_postconditions(
    ) {
        assert!(confirmed(
            &Action::Connect(Some("United_States".into())),
            &observed(Some("united states"))
        ));
        assert!(confirmed(&Action::Connect(None), &observed(None)));
        let mut snapshot = observed(Some("Finland"));
        assert!(!confirmed(&Action::Disconnect, &snapshot));
        snapshot.state = "disconnected".into();
        assert!(confirmed(&Action::Disconnect, &snapshot));
        assert!(confirmed(&Action::EnableKillSwitch, &snapshot));
        snapshot.kill_switch = "disabled".into();
        assert!(!confirmed(&Action::EnableKillSwitch, &snapshot));
    }
}
