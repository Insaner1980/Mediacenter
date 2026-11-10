//! Audio-only preferences and routing. This module has no display-placement API.
use crate::{
    app_state::AppState,
    contracts::{AppError, AudioSinkDescriptor},
    events::HomePublication,
};
pub fn validate_output<'a>(
    outputs: &'a [AudioSinkDescriptor],
    id: &str,
) -> Result<&'a AudioSinkDescriptor, AppError> {
    outputs
        .iter()
        .find(|s| s.id == id && s.available)
        .ok_or_else(|| AppError::new("sink_unavailable", "Audio output unavailable"))
}
pub fn save_preference(state: &AppState, id: &str) -> Result<HomePublication, AppError> {
    let mut saved = Ok(());
    let publication = crate::events::commit(state, |home| {
        let mut target = home.outputs.desired.clone();
        target.audio_sink_id = Some(id.into());
        saved = (|| -> Result<(), AppError> {
            state.db.lock().map_err(|_|AppError::new("database_error","Audio preferences unavailable."))?.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('playback_target',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(&target).unwrap(),crate::app_state::now_ms() as i64])?;
            Ok(())
        })();
        if saved.is_ok() {
            home.outputs.desired.audio_sink_id = Some(id.into());
            home.outputs.revision += 1;
        }
    })?;
    saved?;
    Ok(publication)
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Sessions {
    pub player: Option<String>,
    pub browser: Option<String>,
}
pub async fn sessions(state: &AppState) -> Sessions {
    let player = state
        .player
        .lock()
        .await
        .as_ref()
        .map(|p| p.session.id.clone());
    let browser = state
        .browser
        .lock()
        .await
        .as_ref()
        .map(|b| b.session.id.clone());
    Sessions { player, browser }
}
pub async fn discover(state: &AppState) -> Result<Vec<AudioSinkDescriptor>, AppError> {
    let outputs = crate::outputs::audio::discover().await?;
    crate::remote::state::outputs_discovered(state, outputs.clone());
    Ok(outputs)
}
fn publication(
    state: &AppState,
    id: &str,
    actual: Option<String>,
    pending: bool,
) -> Result<HomePublication, AppError> {
    crate::events::commit(state, |home| {
        home.outputs.effective.audio_sink_id = actual.clone();
        home.outputs.state = crate::outputs::coordinator::combined_state(
            home.outputs.effective.monitor_id.is_some(),
            actual.is_some(),
        )
        .into();
        home.outputs.label = if actual.is_some() {
            "Playback audio confirmed"
        } else if pending {
            "Sound preference saved for the next playback stream"
        } else {
            "Audio output unavailable"
        }
        .into();
        home.outputs.revision += 1;
        for session in &mut home.sessions {
            if session.id == id {
                session.target.audio_sink_id = actual.clone();
            }
        }
    })
}
/// Caller owns session_gate. Exact IDs were previewed and are checked again under each owner guard.
pub async fn apply_audio_output(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    before: &Sessions,
) -> Result<(), AppError> {
    let inventory = discover(state).await?;
    validate_output(&inventory, id)?;
    if sessions(state).await != *before {
        return Err(AppError::new(
            "stale_state",
            "Playback changed. Review the sound output again.",
        ));
    }
    save_preference(state, id)?.emit(app, state);
    let mut failure = None;
    if let Some(expected) = &before.player {
        let mut registry = state.player.lock().await;
        let player = registry
            .as_mut()
            .filter(|p| &p.session.id == expected)
            .ok_or_else(|| AppError::new("stale_state", "Playback changed."))?;
        let mut target = player
            .routing_target
            .clone()
            .unwrap_or_else(|| player.session.target.clone());
        target.audio_sink_id = Some(id.into());
        player.routing_target = Some(target);
        // This service never invokes the combined display/audio mover.
        let result = crate::outputs::audio::route_or_pause(&player.identity, Some(id), || async {
            player
                .ipc
                .command(serde_json::json!(["set_property", "pause", true]))
                .await
                .map(|_| ())
        })
        .await;
        let actual = result.as_ref().ok().cloned();
        let pending = result
            .as_ref()
            .is_err_and(|e| e.code == "audio_stream_pending");
        player.session.target.audio_sink_id = actual.clone();
        let output = publication(state, expected, actual, pending)?;
        if let Err(error) = result {
            if !pending {
                failure = Some(error);
            }
        }
        drop(registry);
        output.emit(app, state);
    }
    if let Some(expected) = &before.browser {
        let mut registry = state.browser.lock().await;
        let browser = registry
            .as_mut()
            .filter(|b| &b.session.id == expected)
            .ok_or_else(|| AppError::new("stale_state", "Playback changed."))?;
        browser.verify().await?;
        let mut target = browser
            .routing_target
            .clone()
            .unwrap_or_else(|| browser.session.target.clone());
        target.audio_sink_id = Some(id.into());
        browser.routing_target = Some(target);
        let result = crate::outputs::audio::route(&browser.identity, Some(id)).await;
        let actual = result.as_ref().ok().cloned();
        let pending = result
            .as_ref()
            .is_err_and(|e| e.code == "audio_stream_pending");
        browser.session.target.audio_sink_id = actual.clone();
        let output = publication(state, expected, actual, pending)?;
        if let Err(error) = result {
            if !pending {
                failure = Some(error);
            }
        }
        drop(registry);
        output.emit(app, state);
    }
    if before.player.is_none() && before.browser.is_none() {
        publication(state, "", None, true)?.emit(app, state);
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
/// A separate bounded application-owned stream; never changes the system default.
pub async fn test_output(
    state: &AppState,
    id: &str,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), AppError> {
    validate_output(&discover(state).await?, id)?;
    let mut child = tokio::process::Command::new("/usr/bin/mpv")
        .args([
            "--no-config",
            "--load-scripts=no",
            "--no-video",
            "--force-window=no",
            "--ao=pulse",
            "--terminal=no",
        ])
        .arg(format!("--audio-device=pulse/{id}"))
        .arg("--")
        .arg("av://lavfi:sine=frequency=440:duration=2")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let outcome=async {
        let root=crate::process::registry::ProcessIdentity::capture(child.id().ok_or_else(||AppError::new("audio_unavailable","Sound test unavailable."))?).ok_or_else(||AppError::new("ownership_uncertain","Sound test owner unavailable."))?;
        let mut verified=false;
        for _ in 0..8 {
            if cancel.is_cancelled(){return Err(AppError::new("cancelled","Sound test cancelled."));}
            match crate::outputs::audio::route(&root,Some(id)).await {
                Ok(_)=>{verified=true;break;},
                Err(error) if error.code=="audio_stream_pending"=>tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                Err(error)=>return Err(error),
            }
        }
        if !verified{return Err(AppError::new("audio_route_unconfirmed","The test output could not be confirmed."));}
        tokio::select! {
            _=cancel.cancelled()=>Err(AppError::new("cancelled","Sound test cancelled.")),
            result=tokio::time::timeout(std::time::Duration::from_secs(4),child.wait())=>match result {
                Ok(Ok(status)) if status.success()=>Ok(()),
                _=>Err(AppError::new("audio_route_unconfirmed","The sound test did not complete.")),
            }
        }
    }.await;
    if child.try_wait()?.is_none() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task10_audio_preference_preserves_picture_and_confirmed_output_and_main_geometry() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        {
            let mut home = state.home.lock().unwrap();
            home.outputs.desired.monitor_id = Some("existing-monitor".into());
            home.outputs.effective.monitor_id = Some("existing-monitor".into());
            home.outputs.effective.audio_sink_id = Some("old-output".into());
        }
        state.db.lock().unwrap().execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('main_geometry','{\"x\":40,\"y\":50,\"width\":1100,\"height\":700}',1)",[]).unwrap();
        let before = state.snapshot().unwrap().outputs;
        save_preference(&state, "hdmi").unwrap();
        let after = state.snapshot().unwrap().outputs;
        assert_eq!(after.desired.audio_sink_id.as_deref(), Some("hdmi"));
        assert_eq!(after.desired.monitor_id, before.desired.monitor_id);
        assert_eq!(after.effective, before.effective);
        let reopened = AppState::open(dir.path().into()).unwrap();
        assert_eq!(reopened.snapshot().unwrap().outputs.desired, after.desired);
        let geometry: String = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT value_json FROM settings WHERE key='main_geometry'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(geometry, r#"{"x":40,"y":50,"width":1100,"height":700}"#);
    }
    #[test]
    fn task10_audio_confirmation_preserves_picture_target_and_partial_picture_status() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        {
            let mut home = state.home.lock().unwrap();
            home.outputs.desired.monitor_id = Some("existing-mirrored-desktop".into());
            home.outputs.effective.monitor_id = None;
            home.outputs.state = "partial".into();
        }
        publication(&state, "owned-session", Some("hdmi".into()), false).unwrap();
        let output = state.snapshot().unwrap().outputs;
        assert_eq!(
            output.desired.monitor_id.as_deref(),
            Some("existing-mirrored-desktop")
        );
        assert!(output.effective.monitor_id.is_none());
        assert_eq!(output.effective.audio_sink_id.as_deref(), Some("hdmi"));
        assert_eq!(output.state, "partial");
    }
}
