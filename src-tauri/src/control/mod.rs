//! Shared typed control entry point. Native services remain authoritative.
use crate::{app_state::AppState, contracts::AppError, remote::protocol::Id};
#[derive(Clone)]
pub struct ControlServices {
    pub(crate) confirmations: std::sync::Arc<std::sync::Mutex<confirmations::Confirmations>>,
    pub(crate) source_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
    pub(crate) folders: std::sync::Arc<std::sync::Mutex<crate::remote::folders::Folders>>,
    pub(crate) source_scans:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (Id, u64)>>>,
    pub(crate) media: std::sync::Arc<std::sync::Mutex<crate::remote::media::MediaState>>,
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
            media: Default::default(),
            folders: Default::default(),
            source_gate: Default::default(),
            source_scans: Default::default(),
            confirmations: Default::default(),
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

pub mod activity;
pub mod confirmations;
pub mod playback;

pub mod audio;

pub mod library;
