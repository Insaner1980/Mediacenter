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
    pub idle: crate::remote::idle::IdleSession,
    pub renderer: crate::remote::renderer::RendererBridge,
    pub audio_outputs: Arc<Mutex<Vec<AudioSinkDescriptor>>>,
    pub event_hub: Arc<crate::remote::event_hub::EventHub>,
    pub retained_operations: Arc<Mutex<crate::remote::operations::RetainedOperations>>,
    pub geometry_saved_at: Arc<Mutex<std::time::Instant>>,
    pub log_gate: Arc<Mutex<()>>,
    pub diagnostic_preview: Arc<Mutex<Option<crate::diagnostics::DiagnosticReport>>>,
    pub source_confirmations:
        Arc<Mutex<HashMap<String, crate::library::confirmation::MountConfirmation>>>,
    pub player_preferences: Arc<Mutex<crate::player::mpv::TransportPreferences>>,
    pub exit_pending: Arc<std::sync::atomic::AtomicBool>,
    pub photos: Arc<tokio::sync::Mutex<crate::photos::presenter::State>>,
    pub resources: crate::library::resources::Resources,
    pub device_gate: Arc<tokio::sync::Mutex<()>>,
    pub music: Arc<tokio::sync::Mutex<crate::music::queue::Queue>>,
    pub music_gate: Arc<tokio::sync::Mutex<()>>,
    pub player: Arc<tokio::sync::Mutex<Option<crate::player::mpv::Player>>>,
    pub session_gate: Arc<tokio::sync::Mutex<()>>,
    pub cache_dir: PathBuf,
    pub thumbnail_gate: Arc<tokio::sync::Semaphore>,
    pub scans: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub probes: Arc<tokio::sync::Semaphore>,
    pub grants: Arc<Mutex<crate::library::sources::Grants>>,
    pub browser_runtime: crate::remote::browser_runtime::Runtime,
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
                audio_unavailable: false,
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
        let music = crate::music::queue::Queue::load(&db)?;
        let player_preferences = crate::player::mpv::preferences(&db);
        Ok(Self {
            idle: Default::default(),
            renderer: Default::default(),
            audio_outputs: Arc::new(Mutex::new(Vec::new())),
            event_hub: Arc::new(crate::remote::event_hub::EventHub::default()),
            retained_operations: Arc::new(Mutex::new(
                crate::remote::operations::RetainedOperations::default(),
            )),
            geometry_saved_at: Arc::new(Mutex::new(std::time::Instant::now())),
            log_gate: Arc::new(Mutex::new(())),
            diagnostic_preview: Arc::new(Mutex::new(None)),
            source_confirmations: Arc::new(Mutex::new(HashMap::new())),
            player_preferences: Arc::new(Mutex::new(player_preferences)),
            exit_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            photos: Arc::new(tokio::sync::Mutex::new(
                crate::photos::presenter::State::default(),
            )),
            resources: crate::library::resources::Resources::default(),
            device_gate: Arc::new(tokio::sync::Mutex::new(())),
            music: Arc::new(tokio::sync::Mutex::new(music)),
            music_gate: Arc::new(tokio::sync::Mutex::new(())),
            player: Arc::new(tokio::sync::Mutex::new(None)),
            session_gate: Arc::new(tokio::sync::Mutex::new(())),
            cache_dir: data_dir.join("cache"),
            thumbnail_gate: Arc::new(tokio::sync::Semaphore::new(1)),
            scans: Arc::new(Mutex::new(HashMap::new())),
            probes: Arc::new(tokio::sync::Semaphore::new(2)),
            grants: Arc::new(Mutex::new(crate::library::sources::Grants::default())),
            browser_runtime: Default::default(),
            browser: Arc::new(tokio::sync::Mutex::new(None)),
            data_dir,
            db: Arc::new(Mutex::new(db)),
            home: Arc::new(Mutex::new(snapshot)),
            operations: Arc::new(Mutex::new(HashMap::new())),
            vpn_gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
    pub async fn browser_active(&self) -> bool {
        self.browser_runtime.session().is_some() || self.browser.lock().await.is_some()
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
