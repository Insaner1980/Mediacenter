//! Shared read-only entry point. Native services remain authoritative.
use crate::{app_state::AppState, contracts::AppError, remote::protocol::Id};
#[derive(Clone)]
pub struct ControlServices {
    pub(crate) state: AppState,
    pub(crate) server_identity: Id,
    app: Option<tauri::AppHandle>,
    operations: crate::remote::operations::OperationRegistry,
}
impl ControlServices {
    pub fn new(
        app: tauri::AppHandle,
        state: AppState,
        server_identity: Id,
    ) -> Result<Self, AppError> {
        let mut services = Self::native(state, server_identity)?;
        services.operations = services.operations.with_app(Some(app.clone()));
        services.app = Some(app);
        Ok(services)
    }
    /// One configured remote identity is bound to this native state for its
    /// lifetime. Construction never performs journal startup recovery.
    pub fn native(state: AppState, server_identity: Id) -> Result<Self, AppError> {
        let operations = crate::remote::operations::OperationRegistry::new(
            state.clone(),
            server_identity.clone(),
        )?;
        Ok(Self {
            state,
            server_identity,
            app: None,
            operations,
        })
    }
    pub fn app(&self) -> Option<&tauri::AppHandle> {
        self.app.as_ref()
    }
    pub fn operations(&self) -> crate::remote::operations::OperationRegistry {
        self.operations.clone()
    }
}
