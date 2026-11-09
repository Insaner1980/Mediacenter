use crate::{
    app_state::AppState,
    contracts::AppError,
    devices::{actions::SourceLifecycle, registry::Volume},
    library::resources::ResourceBlock,
};
use serde::{Deserialize, Serialize};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{Emitter, Manager};
use ts_rs::TS;
pub struct DeviceLifecycle {
    pub app: tauri::AppHandle,
    pub state: AppState,
}
pub fn affected_sources(state: &AppState, volumes: &[Volume]) -> Result<Vec<String>, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
    Ok(crate::library::repository::records(&db)?
        .into_iter()
        .filter(|source| {
            volumes.iter().any(|v| {
                source.volume.is_some() && source.volume == v.identity
                    || v.mounts.iter().any(|mount| source.root.starts_with(mount))
            })
        })
        .map(|s| s.id)
        .collect())
}
pub async fn stop_sources(
    app: &tauri::AppHandle,
    state: &AppState,
    sources: &[String],
) -> Result<ResourceBlock, AppError> {
    let block = state.resources.block(sources)?;
    {
        let scans = state
            .scans
            .lock()
            .map_err(|_| AppError::new("state_error", "Scan state is unavailable."))?;
        for source in sources {
            if let Some(cancel) = scans.get(source) {
                cancel.cancel();
            }
        }
    }
    let player = {
        let registry = state.player.lock().await;
        registry
            .as_ref()
            .filter(|p| sources.contains(&p.media.source_id))
            .map(|p| p.session.id.clone())
    };
    if let Some(id) = player {
        crate::player::mpv::control(app, state, &id, crate::contracts::PlayerAction::Stop).await?;
    }
    for source in sources {
        crate::photos::presenter::stop_source(app, state, source).await;
    }
    state
        .resources
        .wait_idle(sources, Duration::from_secs(5))
        .await?;
    Ok(block)
}
impl SourceLifecycle for DeviceLifecycle {
    type Guard = ResourceBlock;
    async fn quiesce(&self, volumes: &[Volume]) -> Result<ResourceBlock, AppError> {
        let sources = affected_sources(&self.state, volumes)?;
        stop_sources(&self.app, &self.state, &sources).await
    }
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExitStatus {
    pub local_active: bool,
    pub streaming_active: bool,
    pub background_active: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExitAction {
    KeepOpen,
    StopPlaybackAndQuit,
    LeaveServiceOpenAndQuit,
    CloseServiceAndQuit,
}
pub async fn exit_status(state: &AppState) -> ExitStatus {
    let local_active = state.player.lock().await.is_some()
        || crate::photos::presenter::snapshot(state)
            .await
            .session_id
            .is_some();
    let streaming_active = state.browser.lock().await.is_some();
    let background_active = state
        .operations
        .lock()
        .map_or(true, |operations| !operations.is_empty());
    ExitStatus {
        local_active,
        streaming_active,
        background_active,
    }
}
pub async fn request_close(app: tauri::AppHandle, state: AppState) {
    let status = exit_status(&state).await;
    if !status.local_active && !status.streaming_active && !status.background_active {
        if let Err(error) = exit(&app, &state, ExitAction::StopPlaybackAndQuit).await {
            state.exit_pending.store(false, Ordering::SeqCst);
            let _ = app.emit_to("main", "exit_error", error);
        }
    } else {
        let _ = app.emit_to("main", "exit_requested", status);
    }
}
pub async fn exit(
    app: &tauri::AppHandle,
    state: &AppState,
    action: ExitAction,
) -> Result<(), AppError> {
    if matches!(action, ExitAction::KeepOpen) {
        state.exit_pending.store(false, Ordering::SeqCst);
        return Ok(());
    }
    let status = exit_status(state).await;
    if matches!(
        action,
        ExitAction::LeaveServiceOpenAndQuit | ExitAction::CloseServiceAndQuit
    ) && (status.local_active || status.background_active)
    {
        return Err(AppError::new(
            "exit_state_changed",
            "Local playback or work is active. Choose Stop playback and quit.",
        ));
    }
    {
        let operations = state
            .operations
            .lock()
            .map_err(|_| AppError::new("state_error", "Operation state is unavailable."))?;
        for token in operations.values() {
            token.cancel();
        }
    }
    let sources = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::repository::records(&db)?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>()
    };
    let _blocked = stop_sources(app, state, &sources).await?;
    if !matches!(action, ExitAction::LeaveServiceOpenAndQuit) {
        let id = state
            .browser
            .lock()
            .await
            .as_ref()
            .map(|b| b.session.id.clone());
        if let Some(id) = id {
            crate::streaming::sessions::close(app, state, &id).await?;
        }
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let empty = state.operations.lock().is_ok_and(|ops| ops.is_empty());
            if empty {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| {
        AppError::new(
            "exit_busy",
            "Background work has not finished stopping. Mediacenter remains open.",
        )
    })?;
    if let Some(remote) = app.try_state::<crate::remote::server::RemoteServer>() {
        remote.stop().await;
    }
    app.exit(0);
    Ok(())
}
pub fn on_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle().clone();
        let state = app.state::<AppState>().inner().clone();
        if !state.exit_pending.swap(true, Ordering::SeqCst) {
            tauri::async_runtime::spawn(request_close(app, state));
        }
    }
}

pub async fn reconcile_sources(app: &tauri::AppHandle, state: &AppState) -> Result<(), AppError> {
    let devices = crate::devices::udisks::discover().await?;
    let sources = crate::library::source_service::list(state, &devices)?;
    app.emit_to("main", "sources_changed", sources)
        .map_err(|_| {
            AppError::new(
                "state_refresh_failed",
                "Library source status could not be published.",
            )
        })?;
    crate::library::scanner::publish_summaries(app, state)
}
pub fn with_reconciliation<T>(
    result: Result<T, AppError>,
    reconciled: Result<(), AppError>,
) -> Result<T, AppError> {
    match (result,reconciled){(Ok(value),Ok(()))=>Ok(value),(Err(error),Ok(()))=>Err(error),(Err(mut error),Err(_))=>{error.message.push_str(" Library availability could not be refreshed; refresh drives before using media.");Err(error)},(Ok(_),Err(_))=>Err(AppError::new("device_state_refresh_failed","The device operation finished, but library availability could not be refreshed. Refresh drives to confirm its current state."))}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_failure_keeps_os_error_and_reports_stale_library() {
        let result = with_reconciliation::<()>(
            Err(AppError::new(
                "device_busy",
                "Another application is using a volume.",
            )),
            Err(AppError::new("database_error", "unavailable")),
        )
        .unwrap_err();
        assert_eq!(result.code, "device_busy");
        assert!(result.message.contains("could not be refreshed"));
        assert!(
            with_reconciliation(Ok(()), Err(AppError::new("database_error", "unavailable")))
                .is_err()
        );
    }
}
