pub fn combined_state(picture: bool, audio: bool) -> &'static str {
    match (picture, audio) {
        (true, true) => "ready",
        (false, false) => "unavailable",
        _ => "partial",
    }
}
use crate::{app_state::AppState, contracts::*, events};
pub async fn list(app: &tauri::AppHandle) -> Result<OutputList, AppError> {
    let (monitors, computer_monitor_id) = super::monitors::discover(app)?;
    let (audio_sinks, audio_error) = match super::audio::discover().await {
        Ok(sinks) => (sinks, None),
        Err(e) => (vec![], Some(e.message)),
    };
    Ok(OutputList {
        monitors,
        audio_sinks,
        computer_monitor_id,
        audio_error,
    })
}
pub async fn select(
    app: &tauri::AppHandle,
    state: &AppState,
    target: PlaybackTarget,
) -> Result<(), AppError> {
    let outputs = list(app).await?;
    super::monitors::resolve(
        &outputs.monitors,
        target.monitor_id.as_deref(),
        outputs.computer_monitor_id.as_deref(),
    )?;
    if target.audio_sink_id.as_ref().is_some_and(|id| {
        !outputs
            .audio_sinks
            .iter()
            .any(|s| &s.id == id && s.available)
    }) {
        return Err(AppError::new(
            "sink_unavailable",
            "The selected audio output is unavailable.",
        ));
    }
    state.db.lock().map_err(|_|AppError::new("database_error","Output preferences could not be saved."))?.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('playback_target',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(&target).unwrap(),crate::app_state::now_ms() as i64])?;
    events::publish(app, state, |home| {
        home.outputs.desired = target;
        home.outputs.revision += 1;
    })
}
#[derive(Clone, Copy)]
enum Owner<'a> {
    Browser(&'a crate::streaming::browser::BrowserSession),
    Player(&'a crate::player::mpv::Player),
}
impl Owner<'_> {
    fn kind(&self) -> &str {
        match self {
            Self::Browser(b) => &b.session.kind,
            Self::Player(p) => &p.session.kind,
        }
    }
    async fn verify(&self) -> Result<super::x11::WindowInfo, AppError> {
        match self {
            Self::Browser(b) => b.verify().await,
            Self::Player(player) => {
                if !player.identity.still_owned() {
                    return Err(AppError::new(
                        "ownership_uncertain",
                        "The player process is unavailable.",
                    ));
                }
                let expected = player.window.as_ref().ok_or_else(|| {
                    AppError::new(
                        "manual_placement",
                        "The player window could not be identified. Move it manually.",
                    )
                })?;
                super::x11::owned_window(&super::x11::wmctrl(&["-lpG"]).await?, player.identity.pid)
                    .filter(|w| w.id == expected.id)
                    .ok_or_else(|| {
                        AppError::new(
                            "ownership_uncertain",
                            "The player window could not be verified.",
                        )
                    })
            }
        }
    }
}
async fn picture(
    owner: Owner<'_>,
    monitor: &MonitorDescriptor,
    computer: bool,
) -> Result<String, AppError> {
    let window = owner.verify().await?;
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let (x, y, w, h) = super::x11::placement_request(&session_type, owner.kind(), true, monitor)?;
    super::x11::wmctrl(&["-ir", &window.id, "-b", "remove,fullscreen,maximized_vert"]).await?;
    owner.verify().await?;
    super::x11::wmctrl(&["-ir", &window.id, "-b", "remove,maximized_horz"]).await?;
    // Cinnamon's EWMH coordinates are logical while discovery reports physical pixels.
    // Requests are always followed by physical geometry validation.
    let scale = monitor.scale_factor;
    let width = if computer {
        (w as f64 / scale).min(1280.)
    } else {
        w as f64 / scale
    };
    let height = if computer {
        (h as f64 / scale - 60.).clamp(1., 800.)
    } else {
        h as f64 / scale
    };
    let request = format!(
        "0,{},{},{},{}",
        (x as f64 / scale).round() as i32,
        (y as f64 / scale).round() as i32,
        width.round() as u32,
        height.round() as u32
    );
    owner.verify().await?;
    super::x11::wmctrl(&["-ir", &window.id, "-e", &request]).await?;
    if !computer {
        owner.verify().await?;
        super::x11::wmctrl(&["-ir", &window.id, "-b", "add,fullscreen"]).await?;
    }
    for _ in 0..8 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let observed = owner.verify().await?;
        if super::monitors::reachable(
            monitor,
            observed.x,
            observed.y,
            observed.width,
            observed.height,
        ) {
            return Ok(monitor.id.clone());
        }
    }
    Err(AppError::new("placement_unconfirmed","The desktop did not confirm placement on the selected display. Move the service window manually."))
}
pub async fn move_browser(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    target: PlaybackTarget,
) -> Result<(), AppError> {
    let outputs = list(app).await?;
    let monitor = super::monitors::resolve(
        &outputs.monitors,
        target.monitor_id.as_deref(),
        outputs.computer_monitor_id.as_deref(),
    )?;
    let mut registry = state
        .browser
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "A service operation is already running."))?;
    let browser = registry
        .as_mut()
        .filter(|b| b.session.id == id)
        .ok_or_else(|| AppError::new("session_missing", "This playback session is unavailable."))?;
    browser.routing_target = Some(target.clone());
    let picture = picture(
        Owner::Browser(browser),
        monitor,
        Some(&monitor.id) == outputs.computer_monitor_id.as_ref(),
    )
    .await;
    let audio = super::audio::route(&browser.identity, target.audio_sink_id.as_deref()).await;
    let status = combined_state(picture.is_ok(), audio.is_ok());
    browser.session.target = PlaybackTarget {
        monitor_id: picture.as_ref().ok().cloned(),
        audio_sink_id: audio.as_ref().ok().cloned(),
    };
    let effective = browser.session.target.clone();
    let session = browser.session.clone();
    let label = match (&picture, &audio) {
        (Ok(_), Ok(_)) => format!("Playback: {}", monitor.name),
        (Ok(_), Err(_)) => format!("Picture: {}; audio unconfirmed", monitor.name),
        (Err(_), Ok(_)) => "Audio moved; picture unconfirmed".into(),
        _ => "Playback output unconfirmed".into(),
    };
    events::publish(app, state, |home| {
        home.outputs.effective = effective;
        home.outputs.state = status.into();
        home.outputs.label = label;
        home.outputs.revision += 1;
        if let Some(entry) = home.sessions.iter_mut().find(|s| s.id == id) {
            *entry = session;
        }
    })?;
    picture?;
    audio?;
    Ok(())
}
pub async fn reconcile(app: &tauri::AppHandle, state: &AppState) {
    let Ok(outputs) = list(app).await else {
        return;
    };
    let snapshot = match state.snapshot() {
        Ok(s) => s,
        Err(_) => return,
    };
    // Recovery only targets a verified managed service; there is no main-window fallback.
    if let Some(session) = snapshot.sessions.iter().find(|s| s.kind == "streaming") {
        if session
            .target
            .monitor_id
            .as_ref()
            .is_some_and(|id| !outputs.monitors.iter().any(|m| &m.id == id && m.connected))
        {
            let _ = move_browser(app, state, &session.id, PlaybackTarget::default()).await;
            let _ = events::publish(app, state, |home| {
                home.outputs.state = "partial".into();
                home.outputs.label = "Display disconnected; service playback was not paused".into();
                home.outputs.revision += 1;
            });
        }
    } else if let Some(session) = snapshot
        .sessions
        .iter()
        .find(|s| s.kind.starts_with("local-"))
    {
        let missing_picture = session
            .target
            .monitor_id
            .as_ref()
            .is_some_and(|id| !outputs.monitors.iter().any(|m| &m.id == id && m.connected));
        let missing_audio = session.target.audio_sink_id.as_ref().is_some_and(|id| {
            !outputs
                .audio_sinks
                .iter()
                .any(|sink| &sink.id == id && sink.available)
        });
        if missing_picture || missing_audio {
            let _ = crate::player::mpv::control(
                app,
                state,
                &session.id,
                PlayerAction::Pause { paused: true },
            )
            .await;
            if missing_picture {
                let _ = move_player(app, state, &session.id, PlaybackTarget::default()).await;
            }
            let _ = events::publish(app, state, |home| {
                home.outputs.state = "partial".into();
                home.outputs.label = "Playback paused after an output disconnected".into();
                home.outputs.revision += 1;
            });
        }
    } else if !snapshot.sessions.iter().any(|s| s.kind == "slideshow") {
        let label = outputs
            .computer_monitor_id
            .as_ref()
            .map(|id| format!("Computer: {id}"))
            .unwrap_or_else(|| "No display available".into());
        if snapshot.outputs.label != label {
            let _ = events::publish(app, state, |home| {
                home.outputs.label = label;
                home.outputs.state = "idle".into();
                home.outputs.effective = PlaybackTarget::default();
                home.outputs.revision += 1;
            });
        }
    }
}

async fn reconcile_browser_audio(app: &tauri::AppHandle, state: &AppState) {
    let Ok(mut registry) = state.browser.try_lock() else {
        return;
    };
    let Some(browser) = registry.as_mut() else {
        return;
    };
    let Some(target) = browser.routing_target.clone() else {
        return;
    };
    if browser.verify().await.is_err() {
        return;
    }
    let result = super::audio::route(&browser.identity, target.audio_sink_id.as_deref()).await;
    let actual = result.as_ref().ok().cloned();
    let snapshot = match state.snapshot() {
        Ok(s) => s,
        Err(_) => return,
    };
    let state_name = combined_state(browser.session.target.monitor_id.is_some(), result.is_ok());
    if actual == snapshot.outputs.effective.audio_sink_id && state_name == snapshot.outputs.state {
        return;
    }
    browser.session.target.audio_sink_id = actual.clone();
    let id = browser.session.id.clone();
    let label = match result {
        Ok(_) => "Playback audio confirmed",
        Err(_) => "Playback audio unavailable or unconfirmed",
    };
    let _ = events::publish(app, state, |home| {
        home.outputs.effective.audio_sink_id = actual.clone();
        home.outputs.state = state_name.into();
        home.outputs.label = label.into();
        home.outputs.revision += 1;
        if let Some(session) = home.sessions.iter_mut().find(|s| s.id == id) {
            session.target.audio_sink_id = actual;
        }
    });
}

pub async fn move_player(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    target: PlaybackTarget,
) -> Result<(), AppError> {
    let outputs = list(app).await?;
    let monitor = super::monitors::resolve(
        &outputs.monitors,
        target.monitor_id.as_deref(),
        outputs.computer_monitor_id.as_deref(),
    )?;
    let mut registry = state.player.lock().await;
    let player = registry
        .as_mut()
        .filter(|p| p.session.id == id)
        .ok_or_else(|| AppError::new("player_missing", "This player session is unavailable."))?;
    player.routing_target = Some(target.clone());
    let picture = if player.media.kind == MediaKind::Audio {
        Ok(None)
    } else {
        picture(
            Owner::Player(player),
            monitor,
            Some(&monitor.id) == outputs.computer_monitor_id.as_ref(),
        )
        .await
        .map(Some)
    };
    if let Some(id) = &target.audio_sink_id {
        if outputs
            .audio_sinks
            .iter()
            .any(|s| &s.id == id && s.available)
        {
            if let Ok(devices) = player
                .ipc
                .command(serde_json::json!(["get_property", "audio-device-list"]))
                .await
            {
                if let Some(device) = devices.as_array().and_then(|entries| {
                    entries
                        .iter()
                        .find(|d| d["name"].as_str() == Some(&format!("pulse/{id}")))
                }) {
                    let _ = player
                        .ipc
                        .command(serde_json::json!([
                            "set_property",
                            "audio-device",
                            device["name"]
                        ]))
                        .await;
                }
            }
        }
    } else {
        let _ = player
            .ipc
            .command(serde_json::json!(["set_property", "audio-device", "auto"]))
            .await;
    }
    let audio = super::audio::route(&player.identity, target.audio_sink_id.as_deref()).await;
    player.session.target = PlaybackTarget {
        monitor_id: picture.as_ref().ok().cloned().flatten(),
        audio_sink_id: audio.as_ref().ok().cloned(),
    };
    let session = player.session.clone();
    let status = combined_state(picture.is_ok(), audio.is_ok());
    events::publish(app, state, |home| {
        home.outputs.effective = session.target.clone();
        home.outputs.state = status.into();
        home.outputs.label = if status == "ready" {
            "Local playback output confirmed".into()
        } else {
            "Local playback output partly unavailable".into()
        };
        home.outputs.revision += 1;
        if let Some(current) = home.sessions.iter_mut().find(|s| s.id == id) {
            *current = session;
        }
    })?;
    picture?;
    audio?;
    Ok(())
}
pub async fn move_session(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    target: PlaybackTarget,
) -> Result<(), AppError> {
    if state
        .snapshot()?
        .sessions
        .iter()
        .any(|s| s.id == id && s.kind.starts_with("local-"))
    {
        move_player(app, state, id, target).await
    } else {
        move_browser(app, state, id, target).await
    }
}

pub async fn reconcile_audio(app: &tauri::AppHandle, state: &AppState) {
    reconcile_browser_audio(app, state).await;
    let Ok(mut registry) = state.player.try_lock() else {
        return;
    };
    let Some(player) = registry.as_mut() else {
        return;
    };
    let Some(target) = player.routing_target.clone() else {
        return;
    };
    if !player.identity.still_owned() {
        return;
    }
    let result = super::audio::route(&player.identity, target.audio_sink_id.as_deref()).await;
    if result
        .as_ref()
        .is_err_and(|error| error.code == "sink_unavailable")
    {
        let _ = player
            .ipc
            .command(serde_json::json!(["set_property", "pause", true]))
            .await;
    }
    let actual = result.as_ref().ok().cloned();
    let status = combined_state(
        player.media.kind == MediaKind::Audio || player.session.target.monitor_id.is_some(),
        result.is_ok(),
    );
    let Ok(snapshot) = state.snapshot() else {
        return;
    };
    if actual == snapshot.outputs.effective.audio_sink_id && status == snapshot.outputs.state {
        return;
    }
    player.session.target.audio_sink_id = actual.clone();
    let id = player.session.id.clone();
    let _ = events::publish(app, state, |home| {
        home.outputs.effective.audio_sink_id = actual.clone();
        home.outputs.state = status.into();
        home.outputs.label = if actual.is_some() {
            "Local playback audio confirmed".into()
        } else {
            "Local playback audio unavailable or unconfirmed".into()
        };
        home.outputs.revision += 1;
        if let Some(session) = home.sessions.iter_mut().find(|s| s.id == id) {
            session.target.audio_sink_id = actual;
        }
    });
}
#[cfg(test)]
mod tests {
    #[test]
    fn audio_failure_does_not_report_complete_success() {
        assert_eq!(super::combined_state(true, false), "partial");
        assert_eq!(super::combined_state(false, true), "partial");
        assert_eq!(super::combined_state(false, false), "unavailable");
        assert_eq!(super::combined_state(true, true), "ready");
    }
}
