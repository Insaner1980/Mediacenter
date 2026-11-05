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
