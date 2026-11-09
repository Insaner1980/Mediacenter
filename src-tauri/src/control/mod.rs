//! Shared read-only entry point. Native services remain authoritative.
use crate::{app_state::AppState, remote::protocol::Id};
#[derive(Clone)]
pub struct ControlServices {
    pub(crate) state: AppState,
    pub(crate) server_identity: Id,
    app: Option<tauri::AppHandle>,
}
impl ControlServices {
    pub fn new(app: tauri::AppHandle, state: AppState, server_identity: Id) -> Self {
        Self {
            state,
            server_identity,
            app: Some(app),
        }
    }
    /// Also usable before the renderer starts. No remote capability is inferred
    /// from the existence of an AppHandle.
    pub fn native(state: AppState, server_identity: Id) -> Self {
        Self {
            state,
            server_identity,
            app: None,
        }
    }
    pub fn app(&self) -> Option<&tauri::AppHandle> {
        self.app.as_ref()
    }
    pub fn operations(&self) -> crate::remote::operations::OperationRegistry {
        crate::remote::operations::OperationRegistry::new(
            self.state.clone(),
            self.server_identity.clone(),
        )
        .with_app(self.app.clone())
    }
}
