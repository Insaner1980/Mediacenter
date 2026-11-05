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
pub async fn refresh_locked(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<VpnSnapshot, AppError> {
    let status = command(&["status"], 5, CancellationToken::new()).await;
    let settings = command(&["settings"], 5, CancellationToken::new()).await;
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
    events::publish(app, state, |home| home.vpn = snapshot.clone())?;
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
    if let Some(phase) = transition {
        events::publish(&app, &state, |home| {
            home.vpn.state = phase.into();
            home.vpn.country = None;
            home.vpn.observed_at_ms = now_ms();
        })?;
    }
    let result = command(&args, 60, cancel).await;
    let observed = refresh_locked(&app, &state).await?;
    match result {
        Ok(output) if output.success => {
            let succeeded = match action {
                Action::Connect(_) => observed.state == "connected",
                Action::Disconnect => observed.state == "disconnected",
                Action::EnableKillSwitch => observed.kill_switch == "enabled",
            };
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
