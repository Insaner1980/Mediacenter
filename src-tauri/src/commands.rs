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
        crate::outputs::coordinator::move_browser(&app, &state, &session_id, target).await
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
    let devices = crate::devices::udisks::discover().await?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let root = state
            .grants
            .lock()
            .map_err(|_| AppError::new("state_error", "Folder selection is unavailable."))?
            .consume(&grant_id)?;
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::repository::add(&db, &root, &kinds, &devices)
    })
    .await
    .map_err(|_| AppError::new("source_error", "The source could not be added."))?
}
#[tauri::command]
pub async fn list_media_sources(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MediaSource>, AppError> {
    guard(window.label())?;
    let devices = crate::devices::udisks::discover().await.unwrap_or_default();
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::repository::list(&db, &devices)
    })
    .await
    .map_err(|_| AppError::new("source_error", "Library sources are unavailable."))?
}
#[tauri::command]
pub async fn remove_media_source(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), AppError> {
    guard(window.label())?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        crate::library::repository::remove(&db, &source_id)
    })
    .await
    .map_err(|_| AppError::new("source_error", "The source could not be removed."))?
}
#[tauri::command]
pub async fn list_drives(window: tauri::WebviewWindow) -> Result<Vec<DriveDescriptor>, AppError> {
    guard(window.label())?;
    Ok(crate::devices::udisks::descriptors(
        &crate::devices::udisks::discover().await?,
    ))
}
