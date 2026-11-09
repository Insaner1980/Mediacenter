use crate::{
    app_state::AppState,
    contracts::*,
    remote::{event_hub::Stamp, protocol::Topic},
};
use tauri::Emitter;
/// A committed copy can outlive native guards without retaining those guards.
pub struct HomePublication {
    snapshot: HomeSnapshot,
    stamps: [Stamp; 3],
}
pub fn commit(
    state: &AppState,
    update: impl FnOnce(&mut HomeSnapshot),
) -> Result<HomePublication, AppError> {
    let mut home = state
        .home
        .lock()
        .map_err(|_| AppError::new("state_error", "Application state is unavailable."))?;
    update(&mut home);
    home.revision += 1;
    let stamps =
        [Topic::Activity, Topic::Outputs, Topic::Vpn].map(|topic| state.event_hub.reserve(topic));
    Ok(HomePublication {
        snapshot: home.clone(),
        stamps,
    })
}
impl HomePublication {
    pub fn emit(self, app: &tauri::AppHandle, state: &AppState) {
        crate::remote::state::home_changed(state, &self.snapshot, self.stamps);
        let _ = app.emit_to("main", "home_changed", self.snapshot);
    }
}
pub fn publish(
    app: &tauri::AppHandle,
    state: &AppState,
    update: impl FnOnce(&mut HomeSnapshot),
) -> Result<(), AppError> {
    commit(state, update)?.emit(app, state);
    Ok(())
}
/// Declare before the native guard. Drop order flushes committed publications
/// after that guard on success, cancellation and every early error return.
pub struct DeferredPublications<'a> {
    app: &'a tauri::AppHandle,
    state: &'a AppState,
    pending: std::sync::Mutex<Vec<HomePublication>>,
}
impl<'a> DeferredPublications<'a> {
    pub fn new(app: &'a tauri::AppHandle, state: &'a AppState) -> Self {
        Self {
            app,
            state,
            pending: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub fn publish(&self, update: impl FnOnce(&mut HomeSnapshot)) -> Result<(), AppError> {
        let publication = commit(self.state, update)?;
        self.pending.lock().unwrap().push(publication);
        Ok(())
    }
}
impl Drop for DeferredPublications<'_> {
    fn drop(&mut self) {
        for publication in self.pending.get_mut().unwrap().drain(..) {
            publication.emit(self.app, self.state);
        }
    }
}
