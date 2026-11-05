use crate::{app_state::AppState, contracts::*};
use tauri::Emitter;
pub fn publish(
    app: &tauri::AppHandle,
    state: &AppState,
    update: impl FnOnce(&mut HomeSnapshot),
) -> Result<(), AppError> {
    let snapshot = {
        let mut home = state
            .home
            .lock()
            .map_err(|_| AppError::new("state_error", "Application state is unavailable."))?;
        update(&mut home);
        home.revision += 1;
        home.clone()
    };
    app.emit_to("main", "home_changed", snapshot).map_err(|_| {
        AppError::new(
            "event_error",
            "Refresh the application to see the latest state.",
        )
    })
}
