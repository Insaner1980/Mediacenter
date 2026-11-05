use crate::{
    app_state::{now_ms, AppState},
    contracts::*,
    events,
};
use tauri::{Manager, State};
pub fn guard(label: &str) -> Result<(), AppError> {
    if label == "main" {
        Ok(())
    } else {
        Err(AppError::new(
            "permission_denied",
            "This window cannot perform that action.",
        ))
    }
}
#[tauri::command]
pub fn get_home_snapshot(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<HomeSnapshot, AppError> {
    guard(window.label())?;
    state.snapshot()
}
pub fn save_theme(app: &tauri::AppHandle, theme: Theme) -> Result<Theme, AppError> {
    let state = app.state::<AppState>();
    state.db.lock().map_err(|_|AppError::new("database_error","The library is unavailable."))?.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('theme',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(&theme).unwrap(),now_ms() as i64])?;
    events::publish(app, &state, |home| home.theme = theme)?;
    Ok(theme)
}
#[tauri::command]
pub fn set_theme(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    theme: Theme,
) -> Result<Theme, AppError> {
    guard(window.label())?;
    save_theme(&app, theme)
}
#[tauri::command]
pub async fn refresh_vpn(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<VpnSnapshot, AppError> {
    guard(window.label())?;
    crate::vpn::nordvpn::refresh(&app, &state).await
}
#[tauri::command]
pub async fn list_vpn_countries(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<CountryOption>, AppError> {
    guard(window.label())?;
    let _guard = state.vpn_gate.lock().await;
    crate::vpn::nordvpn::countries_locked().await
}
#[tauri::command]
pub fn connect_vpn(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    country: Option<String>,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::vpn::nordvpn::change(
            app,
            state,
            crate::vpn::nordvpn::Action::Connect(country),
            cancel,
        )
    })
}
#[tauri::command]
pub fn disconnect_vpn(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::vpn::nordvpn::change(app, state, crate::vpn::nordvpn::Action::Disconnect, cancel)
    })
}
#[tauri::command]
pub fn enable_kill_switch(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::vpn::nordvpn::change(
            app,
            state,
            crate::vpn::nordvpn::Action::EnableKillSwitch,
            cancel,
        )
    })
}
#[tauri::command]
pub fn cancel_operation(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    let operations = state
        .operations
        .lock()
        .map_err(|_| AppError::new("state_error", "Operations are unavailable."))?;
    if let Some(token) = operations.get(&operation_id) {
        token.cancel();
    }
    Ok(())
}
#[tauri::command]
pub fn set_vpn_requirement(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    service_id: ServiceId,
    required: bool,
) -> Result<bool, AppError> {
    guard(window.label())?;
    state.db.lock().map_err(|_|AppError::new("database_error","The library is unavailable."))?.execute("INSERT INTO service_preferences(service_id,vpn_required) VALUES(?1,?2) ON CONFLICT(service_id) DO UPDATE SET vpn_required=excluded.vpn_required",rusqlite::params![crate::streaming::catalog::key(service_id),required])?;
    events::publish(&app, &state, |home| {
        if let Some(service) = home.services.iter_mut().find(|s| s.id == service_id) {
            service.vpn_required = required;
        }
    })?;
    Ok(required)
}
#[cfg(test)]
mod tests {
    #[test]
    fn presentation_cannot_invoke_privileged_operations() {
        assert!(super::guard("presentation").is_err());
        assert!(super::guard("main").is_ok());
    }
}

#[tauri::command]
pub fn open_service(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    service_id: ServiceId,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::streaming::sessions::open(app, state, service_id, cancel)
    })
}
#[tauri::command]
pub async fn focus_session(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    crate::streaming::sessions::focus(&state, &session_id).await
}
#[tauri::command]
pub fn close_session(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |_| async move {
        crate::streaming::sessions::close(&app, &state, &session_id).await
    })
}

#[tauri::command]
pub async fn list_outputs(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<OutputList, AppError> {
    guard(window.label())?;
    crate::outputs::coordinator::list(&app).await
}
#[tauri::command]
pub async fn select_output(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    target: PlaybackTarget,
) -> Result<(), AppError> {
    guard(window.label())?;
    crate::outputs::coordinator::select(&app, &state, target).await
}
#[tauri::command]
pub fn move_session(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    target: PlaybackTarget,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| async move {
        if cancel.is_cancelled() {
            return Err(AppError::new("cancelled", "Output change cancelled."));
        }
        crate::outputs::coordinator::move_session(&app, &state, &session_id, target).await
    })
}

#[tauri::command]
pub async fn choose_media_source(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<SourceGrant>, AppError> {
    guard(window.label())?;
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a media folder")
        .pick_folder(move |path| {
            let _ = sender.send(path);
        });
    let selected = receiver.await.map_err(|_| {
        AppError::new(
            "picker_failed",
            "The folder picker could not return a selection.",
        )
    })?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| AppError::new("invalid_source", "Select a local folder."))?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let display_path = path.to_string_lossy().into_owned();
        let grant_id = state
            .grants
            .lock()
            .map_err(|_| AppError::new("state_error", "Folder selection is unavailable."))?
            .issue(path)?;
        Ok(Some(SourceGrant {
            grant_id,
            display_path,
            expires_in_seconds: 600,
        }))
    })
    .await
    .map_err(|_| {
        AppError::new(
            "picker_failed",
            "The folder selection could not be validated.",
        )
    })?
}
#[tauri::command]
pub async fn add_media_source(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    grant_id: String,
    kinds: Vec<MediaKind>,
) -> Result<String, AppError> {
    guard(window.label())?;
    crate::library::source_service::add(state.inner().clone(), grant_id, kinds).await
}

#[tauri::command]
pub async fn list_media_sources(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MediaSource>, AppError> {
    guard(window.label())?;
    let devices = crate::devices::udisks::discover().await.unwrap_or_default();
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || crate::library::source_service::list(&state, &devices))
        .await
        .map_err(|_| AppError::new("source_error", "Library sources are unavailable."))?
}

#[tauri::command]
pub async fn remove_media_source(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    let _gate = state.device_gate.try_lock().map_err(|_| {
        AppError::new(
            "device_busy",
            "A source or device operation is already running.",
        )
    })?;
    let _blocked =
        crate::lifecycle::stop_sources(&app, &state, std::slice::from_ref(&source_id)).await?;
    {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::repository::remove(&db, &source_id)?;
    }
    if let Ok(mut confirmations) = state.source_confirmations.lock() {
        confirmations.remove(&source_id);
    }
    crate::library::scanner::publish_summaries(&app, &state)
}

#[tauri::command]
pub async fn list_drives(window: tauri::WebviewWindow) -> Result<Vec<DriveDescriptor>, AppError> {
    guard(window.label())?;
    Ok(crate::devices::udisks::descriptors(
        &crate::devices::udisks::discover().await?,
    ))
}

#[tauri::command]
pub fn rescan_source(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::library::scanner::scan(app, state, source_id, cancel)
    })
}
#[tauri::command]
pub fn cancel_source_scan(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    if let Some(token) = state
        .scans
        .lock()
        .map_err(|_| AppError::new("state_error", "Scans are unavailable."))?
        .get(&source_id)
    {
        token.cancel();
    }
    Ok(())
}
#[tauri::command]
pub async fn query_media(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    kind: MediaKind,
    source_id: Option<String>,
    query: String,
    cursor: Option<String>,
    limit: u32,
) -> Result<MediaPage, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::query::page(
            &db,
            kind,
            source_id.as_deref(),
            &query,
            cursor.as_deref(),
            limit,
        )
    })
    .await
    .map_err(|_| AppError::new("search_error", "The library search failed."))?
}
#[tauri::command]
pub async fn get_thumbnail(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    media_id: String,
) -> Result<EncodedImage, AppError> {
    guard(window.label())?;
    crate::library::thumbnails::get(
        &state,
        &media_id,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}

#[tauri::command]
pub fn play_media(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    media_id: String,
    resume: bool,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::player::mpv::play(app, state, media_id, resume, cancel)
    })
}
#[tauri::command]
pub async fn get_player(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<PlayerSnapshot>, AppError> {
    guard(window.label())?;
    Ok(state
        .player
        .lock()
        .await
        .as_ref()
        .map(|p| p.snapshot.clone()))
}
#[tauri::command]
pub async fn control_player(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    action: PlayerAction,
) -> Result<(), AppError> {
    guard(window.label())?;
    crate::player::mpv::control(&app, &state, &session_id, action).await
}

#[tauri::command]
pub async fn get_music_queue(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<crate::music::queue::MusicQueue, AppError> {
    guard(window.label())?;
    crate::music::service::get(&state).await
}
#[tauri::command]
pub async fn control_music(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: Option<String>,
    revision: u32,
    action: crate::music::queue::MusicAction,
) -> Result<crate::music::queue::MusicQueue, AppError> {
    guard(window.label())?;
    crate::music::service::control(
        app,
        state.inner().clone(),
        session_id,
        revision,
        action,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}
#[tauri::command]
pub fn play_music_queue(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    media_ids: Vec<String>,
    start_index: usize,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::music::service::play(app, state, media_ids, start_index, cancel)
    })
}
#[tauri::command]
pub fn play_playlist(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    start_index: usize,
) -> Result<OperationRef, AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    crate::operations::start(app.clone(), state.clone(), move |cancel| {
        crate::music::service::play_playlist(app, state, playlist_id, start_index, cancel)
    })
}
#[tauri::command]
pub fn save_playlist(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    playlist_id: Option<String>,
    name: String,
    media_ids: Vec<String>,
) -> Result<String, AppError> {
    guard(window.label())?;
    let mut db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Playlists are unavailable."))?;
    crate::music::playlists::save(&mut db, playlist_id.as_deref(), &name, &media_ids)
}
#[tauri::command]
pub fn list_playlists(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<crate::music::playlists::PlaylistSummary>, AppError> {
    guard(window.label())?;
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Playlists are unavailable."))?;
    crate::music::playlists::list(&db)
}
#[tauri::command]
pub fn get_playlist(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    playlist_id: String,
) -> Result<crate::music::playlists::Playlist, AppError> {
    guard(window.label())?;
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Playlists are unavailable."))?;
    crate::music::playlists::get(&db, &playlist_id)
}
#[tauri::command]
pub fn remove_playlist(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    playlist_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Playlists are unavailable."))?;
    crate::music::playlists::remove(&db, &playlist_id)
}
#[tauri::command]
pub async fn start_slideshow(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: crate::photos::presenter::SlideshowRequest,
) -> Result<crate::photos::presenter::SlideshowSnapshot, AppError> {
    guard(window.label())?;
    crate::photos::presenter::start(&app, &state, request).await
}
#[tauri::command]
pub async fn control_slideshow(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    action: crate::photos::presenter::SlideshowAction,
) -> Result<crate::photos::presenter::SlideshowSnapshot, AppError> {
    guard(window.label())?;
    crate::photos::presenter::control(&app, &state, action).await
}
#[tauri::command]
pub async fn get_slideshow(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<crate::photos::presenter::SlideshowSnapshot, AppError> {
    guard(window.label())?;
    Ok(crate::photos::presenter::snapshot(&state).await)
}
#[tauri::command]
pub async fn get_presentation_frame(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<crate::photos::presenter::PhotoFrame>, AppError> {
    crate::photos::presenter::frame(&state, window.label()).await
}
#[tauri::command]
pub async fn get_photo_image(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    media_id: String,
) -> Result<crate::photos::decode::PhotoImage, AppError> {
    guard(window.label())?;
    crate::photos::decode::get(
        &state,
        &media_id,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}

#[tauri::command]
pub async fn prepare_eject_drive(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    drive_id: String,
) -> Result<crate::devices::actions::EjectPreview, AppError> {
    guard(window.label())?;
    let mut preview = crate::devices::actions::prepare_eject_drive(
        &crate::devices::actions::UDisksBackend,
        &drive_id,
    )
    .await?;
    let devices = crate::devices::udisks::discover().await?;
    let volumes = devices
        .volumes
        .into_iter()
        .filter(|v| preview.volume_ids.contains(&v.id))
        .collect::<Vec<_>>();
    let sources = crate::lifecycle::affected_sources(&state, &volumes)?;
    if state
        .scans
        .lock()
        .is_ok_and(|scans| sources.iter().any(|id| scans.contains_key(id)))
    {
        preview.active_uses.push("Media scanning".into());
    }
    if state
        .player
        .lock()
        .await
        .as_ref()
        .is_some_and(|p| sources.contains(&p.media.source_id))
    {
        preview.active_uses.push("Local playback".into());
    }
    if crate::photos::presenter::snapshot(&state)
        .await
        .session_id
        .is_some()
    {
        preview
            .active_uses
            .push("Photo presentation will be checked before removal".into());
    }
    if state.resources.active_count(&sources)? > 0 {
        preview
            .active_uses
            .push("Open media files or image decoding".into());
    }
    let _ = app;
    Ok(preview)
}
#[tauri::command]
pub async fn mount_volume(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    volume_id: String,
) -> Result<crate::devices::actions::MountOutcome, AppError> {
    guard(window.label())?;
    let _gate = state
        .device_gate
        .try_lock()
        .map_err(|_| AppError::new("device_busy", "A device operation is already running."))?;
    let result =
        crate::devices::actions::mount_volume(&crate::devices::actions::UDisksBackend, &volume_id)
            .await;
    crate::lifecycle::with_reconciliation(
        result,
        crate::lifecycle::reconcile_sources(&app, &state).await,
    )
}

#[tauri::command]
pub async fn eject_drive(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    drive_id: String,
    confirmed_drive_ids: Vec<String>,
) -> Result<crate::devices::actions::EjectOutcome, AppError> {
    guard(window.label())?;
    let _gate = state
        .device_gate
        .try_lock()
        .map_err(|_| AppError::new("device_busy", "A device operation is already running."))?;
    let lifecycle = crate::lifecycle::DeviceLifecycle {
        app: app.clone(),
        state: state.inner().clone(),
    };
    let result = crate::devices::actions::eject_drive(
        &crate::devices::actions::UDisksBackend,
        &lifecycle,
        &drive_id,
        &confirmed_drive_ids,
    )
    .await;
    crate::lifecycle::with_reconciliation(
        result,
        crate::lifecycle::reconcile_sources(&app, &state).await,
    )
}
#[tauri::command]
pub async fn request_exit(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    action: crate::lifecycle::ExitAction,
) -> Result<(), AppError> {
    guard(window.label())?;
    let result = crate::lifecycle::exit(&app, &state, action).await;
    if result.is_err() {
        state
            .exit_pending
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    result
}

#[tauri::command]
pub async fn get_diagnostics(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::DiagnosticReport, AppError> {
    guard(window.label())?;
    let report = crate::diagnostics::report(&state).await?;
    *state
        .diagnostic_preview
        .lock()
        .map_err(|_| AppError::new("state_error", "Diagnostics preview is unavailable."))? =
        Some(report.clone());
    Ok(report)
}
#[tauri::command]
pub async fn export_diagnostics(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    guard(window.label())?;
    use tauri_plugin_dialog::DialogExt;
    let report = state
        .diagnostic_preview
        .lock()
        .map_err(|_| AppError::new("state_error", "Diagnostics preview is unavailable."))?
        .clone()
        .ok_or_else(|| {
            AppError::new("preview_required", "Preview diagnostics before exporting.")
        })?;
    let (send, receive) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name("mediacenter-diagnostics.json")
        .add_filter("JSON", &["json"])
        .save_file(move |path| {
            let _ = send.send(path);
        });
    let Some(path) = receive.await.map_err(|_| {
        AppError::new(
            "save_dialog_failed",
            "The save dialog could not return a filename.",
        )
    })?
    else {
        return Ok(false);
    };
    let path = path
        .into_path()
        .map_err(|_| AppError::new("save_path_invalid", "Choose a local file for the report."))?;
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|_| AppError::new("report_error", "The report could not be encoded."))?;
    tokio::fs::write(path, bytes).await?;
    Ok(true)
}

#[tauri::command]
pub fn set_control_monitor(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    monitor_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    crate::outputs::control_window::select(&app, &state, &monitor_id)
}
