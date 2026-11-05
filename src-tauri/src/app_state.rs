use crate::{contracts::*, storage::database, streaming::catalog};
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub struct AppState {
    pub grants: Arc<Mutex<crate::library::sources::Grants>>,
    pub browser: Arc<tokio::sync::Mutex<Option<crate::streaming::browser::BrowserSession>>>,
    pub data_dir: PathBuf,
    pub db: Arc<Mutex<Connection>>,
    pub home: Arc<Mutex<HomeSnapshot>>,
    pub operations: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub vpn_gate: Arc<tokio::sync::Mutex<()>>,
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl AppState {
    pub fn open(data_dir: PathBuf) -> Result<Self, AppError> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&data_dir)?;
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
        let db = database::open(&data_dir.join("library.db"))?;
        let theme: Option<String> = db
            .query_row(
                "SELECT value_json FROM settings WHERE key='theme'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let theme = theme
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or(Theme::Dark);
        let mut services = catalog::entries();
        for service in &mut services {
            service.vpn_required = db
                .query_row(
                    "SELECT vpn_required FROM service_preferences WHERE service_id=?",
                    [catalog::key(service.id)],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(true);
        }
        let saved_target: Option<String> = db
            .query_row(
                "SELECT value_json FROM settings WHERE key='playback_target'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let target = saved_target
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let snapshot = HomeSnapshot {
            revision: 1,
            theme,
            vpn: VpnSnapshot {
                revision: 0,
                state: "unknown".into(),
                country: None,
                kill_switch: "unknown".into(),
                observed_at_ms: 0,
                error_code: None,
            },
            outputs: OutputSnapshot {
                revision: 0,
                desired: target,
                effective: PlaybackTarget::default(),
                state: "unavailable".into(),
                label: "Detecting outputs".into(),
            },
            summaries: HomeSummaries {
                video: "Add a video folder".into(),
                audio: "Add a music folder".into(),
                photo: "Add a photo folder".into(),
                drives: "Checking drives".into(),
                resume_progress: None,
            },
            services,
            sessions: vec![],
        };
        Ok(Self {
            grants: Arc::new(Mutex::new(crate::library::sources::Grants::default())),
            browser: Arc::new(tokio::sync::Mutex::new(None)),
            data_dir,
            db: Arc::new(Mutex::new(db)),
            home: Arc::new(Mutex::new(snapshot)),
            operations: Arc::new(Mutex::new(HashMap::new())),
            vpn_gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
    pub fn snapshot(&self) -> Result<HomeSnapshot, AppError> {
        self.home.lock().map(|h| h.clone()).map_err(|_| {
            AppError::new(
                "state_error",
                "Application state is unavailable. Restart Mediacenter.",
            )
        })
    }
}
