use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ServiceId {
    Netflix,
    Youtube,
    HboMax,
    Viaplay,
    MtvKatsomo,
    Ruutu,
    YleAreena,
    Plex,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Audio,
    Photo,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub operation_id: Option<String>,
}
impl AppError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: true,
            operation_id: None,
        }
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        Self::new(
            "database_error",
            "The library database could not be read or updated.",
        )
    }
}
impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::new("io_error", "The requested local resource is unavailable.")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VpnSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub state: String,
    pub country: Option<String>,
    pub kill_switch: String,
    #[ts(type = "number")]
    pub observed_at_ms: u64,
    pub error_code: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackTarget {
    pub monitor_id: Option<String>,
    pub audio_sink_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub desired: PlaybackTarget,
    pub effective: PlaybackTarget,
    pub state: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDefinition {
    pub id: ServiceId,
    pub name: String,
    pub logo_asset: String,
    pub tile_color: String,
    pub vpn_required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    #[serde(default)]
    #[ts(optional, type = "number | null")]
    pub modified_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub resume_position_ms: Option<u64>,
    pub id: String,
    pub source_id: String,
    pub kind: MediaKind,
    pub display_name: String,
    pub availability: String,
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    pub thumbnail_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSession {
    pub id: String,
    pub kind: String,
    pub service_id: Option<ServiceId>,
    pub media_id: Option<String>,
    pub phase: String,
    pub target: PlaybackTarget,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HomeSummaries {
    pub video: String,
    pub audio: String,
    pub photo: String,
    pub drives: String,
    pub resume_progress: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HomeSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub theme: Theme,
    pub vpn: VpnSnapshot,
    pub outputs: OutputSnapshot,
    pub summaries: HomeSummaries,
    pub services: Vec<ServiceDefinition>,
    pub sessions: Vec<ManagedSession>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OperationRef {
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OperationFinished {
    pub operation_id: String,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CountryOption {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDescriptor {
    pub id: String,
    pub name: String,
    pub connector: String,
    pub primary: bool,
    pub connected: bool,
    pub scale_factor: f64,
    pub bounds: Bounds,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioSinkDescriptor {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub backend_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputList {
    pub monitors: Vec<MonitorDescriptor>,
    pub audio_sinks: Vec<AudioSinkDescriptor>,
    pub computer_monitor_id: Option<String>,
    pub audio_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceGrant {
    pub grant_id: String,
    pub display_path: String,
    pub expires_in_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaSource {
    pub id: String,
    pub label: String,
    pub display_path: String,
    pub kinds: Vec<MediaKind>,
    pub availability: String,
    pub removable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDescriptor {
    pub id: String,
    pub drive_id: String,
    pub label: String,
    pub mounted: bool,
    pub mount_points: Vec<String>,
    #[ts(type = "number")]
    pub size: u64,
    pub ambiguous: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DriveDescriptor {
    pub id: String,
    pub name: String,
    pub removable: bool,
    pub can_power_off: bool,
    pub volumes: Vec<VolumeDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaPage {
    pub items: Vec<MediaItem>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EncodedImage {
    pub mime: String,
    pub base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlayerTrack {
    pub id: i32,
    pub kind: String,
    pub title: String,
    pub selected: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub session_id: String,
    pub media_id: String,
    pub phase: String,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub seekable: bool,
    pub tracks: Vec<PlayerTrack>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlayerAction {
    Pause { paused: bool },
    Seek { seconds: f64 },
    Volume { value: f64 },
    Mute { muted: bool },
    AudioTrack { id: i32 },
    SubtitleTrack { id: Option<i32> },
    Stop,
}

pub fn typescript() -> String {
    let cfg = ts_rs::Config::default();
    let mut declarations = vec![
        PlayerTrack::decl(&cfg),
        PlayerSnapshot::decl(&cfg),
        PlayerAction::decl(&cfg),
        MediaPage::decl(&cfg),
        EncodedImage::decl(&cfg),
        SourceGrant::decl(&cfg),
        MediaSource::decl(&cfg),
        VolumeDescriptor::decl(&cfg),
        DriveDescriptor::decl(&cfg),
        Bounds::decl(&cfg),
        MonitorDescriptor::decl(&cfg),
        AudioSinkDescriptor::decl(&cfg),
        OutputList::decl(&cfg),
        Theme::decl(&cfg),
        ServiceId::decl(&cfg),
        MediaKind::decl(&cfg),
        AppError::decl(&cfg),
        VpnSnapshot::decl(&cfg),
        PlaybackTarget::decl(&cfg),
        OutputSnapshot::decl(&cfg),
        ServiceDefinition::decl(&cfg),
        MediaItem::decl(&cfg),
        ManagedSession::decl(&cfg),
        HomeSummaries::decl(&cfg),
        HomeSnapshot::decl(&cfg),
        OperationRef::decl(&cfg),
        OperationFinished::decl(&cfg),
        CountryOption::decl(&cfg),
    ];
    declarations.extend([
        crate::lifecycle::ExitStatus::decl(&cfg),
        crate::lifecycle::ExitAction::decl(&cfg),
    ]);
    declarations.extend([
        crate::diagnostics::DependencyStatus::decl(&cfg),
        crate::diagnostics::DiagnosticReport::decl(&cfg),
    ]);
    declarations.extend([
        crate::remote::network::NetworkInterface::decl(&cfg),
        crate::remote::setup::RemoteSettingsSnapshot::decl(&cfg),
        crate::remote::setup::RemoteEnableRequest::decl(&cfg),
        crate::remote::setup::RemotePairingDisplay::decl(&cfg),
        crate::remote::setup::RemoteDevice::decl(&cfg),
    ]);
    declarations.extend(crate::music::typescript(&cfg));
    declarations.extend(crate::photos::presenter::typescript(&cfg));
    declarations.extend([
        crate::devices::actions::EjectPreview::decl(&cfg),
        crate::devices::actions::EjectOutcome::decl(&cfg),
        crate::devices::actions::EjectStatus::decl(&cfg),
        crate::devices::actions::MountOutcome::decl(&cfg),
    ]);
    format!(
        "// Generated by Rust contracts; do not edit.\n{}\n",
        declarations
            .iter()
            .map(|s| format!("export {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn contracts_match_generated_types() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/app/contracts.ts");
        assert_eq!(
            std::fs::read_to_string(path).unwrap_or_default(),
            super::typescript()
        );
    }
}
