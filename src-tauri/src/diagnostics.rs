use crate::{
    app_state::AppState,
    contracts::AppError,
    process::runner::{self, CommandSpec, Executable},
};
use serde::Serialize;
use std::{io::Write, path::Path, time::Duration};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, TS)]
pub struct DependencyStatus {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub application_version: String,
    #[ts(type = "number")]
    pub captured_at_ms: u64,
    pub desktop_session: String,
    pub dependencies: Vec<DependencyStatus>,
    pub vpn_state: String,
    pub kill_switch: String,
    pub output_state: String,
    pub active_sessions: usize,
    pub active_operations: usize,
}
#[derive(Serialize)]
struct LogEntry<'a> {
    timestamp_ms: u64,
    component: &'a str,
    event: &'a str,
    operation_id: Option<&'a str>,
    duration_ms: u64,
    error_code: Option<&'a str>,
}
fn append(directory: &Path, entry: &LogEntry<'_>) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(directory)?;
    let path = directory.join("events.jsonl");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= 5 * 1024 * 1024) {
        let oldest = directory.join("events.2.jsonl");
        if oldest.exists() {
            std::fs::remove_file(&oldest)?;
        }
        let previous = directory.join("events.1.jsonl");
        if previous.exists() {
            std::fs::rename(previous, oldest)?;
        }
        std::fs::rename(&path, directory.join("events.1.jsonl"))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    serde_json::to_writer(&mut file, entry)?;
    file.write_all(b"\n")
}
pub fn record(
    state: &AppState,
    component: &str,
    event: &str,
    operation_id: Option<&str>,
    duration_ms: u64,
    error_code: Option<&str>,
) {
    let Ok(_guard) = state.log_gate.lock() else {
        return;
    };
    let _ = append(
        &state.data_dir.join("logs"),
        &LogEntry {
            timestamp_ms: crate::app_state::now_ms(),
            component,
            event,
            operation_id,
            duration_ms,
            error_code,
        },
    );
}
pub async fn report(state: &AppState) -> Result<DiagnosticReport, AppError> {
    let mut dependencies = vec![];
    for (name, executable, args) in [
        ("Google Chrome", Executable::Chrome, vec!["--version"]),
        ("NordVPN", Executable::NordVpn, vec!["--version"]),
        ("mpv", Executable::Mpv, vec!["--version"]),
        ("FFmpeg", Executable::Ffmpeg, vec!["-version"]),
        ("PulseAudio tools", Executable::Pactl, vec!["--version"]),
    ] {
        let output = runner::run(
            CommandSpec {
                executable,
                args: args.into_iter().map(std::ffi::OsString::from).collect(),
                timeout: Duration::from_secs(3),
                output_limit: 16 * 1024,
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
        let version = output
            .ok()
            .filter(|o| o.success)
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.chars().filter(|c| !c.is_control()).take(160).collect())
            });
        dependencies.push(DependencyStatus {
            name: name.into(),
            available: version.is_some(),
            version,
        });
    }
    let home = state.snapshot()?;
    let operations = state
        .operations
        .lock()
        .map_err(|_| AppError::new("state_error", "Operation state is unavailable."))?
        .len();
    Ok(DiagnosticReport {
        application_version: env!("CARGO_PKG_VERSION").into(),
        captured_at_ms: crate::app_state::now_ms(),
        desktop_session: match std::env::var("XDG_SESSION_TYPE").as_deref() {
            Ok("x11") => "X11",
            Ok("wayland") => "Wayland",
            _ => "Unknown",
        }
        .into(),
        dependencies,
        vpn_state: home.vpn.state,
        kill_switch: home.vpn.kill_switch,
        output_state: home.outputs.state,
        active_sessions: home.sessions.len(),
        active_operations: operations,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_rotate_with_only_structured_fields() {
        let dir = tempfile::tempdir().unwrap();
        let entry = LogEntry {
            timestamp_ms: 0,
            component: "test",
            event: "finished",
            operation_id: Some("opaque"),
            duration_ms: 1,
            error_code: Some("cancelled"),
        };
        for _ in 0..4 {
            let file = std::fs::File::create(dir.path().join("events.jsonl")).unwrap();
            file.set_len(5 * 1024 * 1024).unwrap();
            append(dir.path(), &entry).unwrap();
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
        let data = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
        let value: serde_json::Value = serde_json::from_str(data.trim()).unwrap();
        assert!(value.get("path").is_none());
        assert!(value.get("stdout").is_none());
        assert_eq!(value["error_code"], "cancelled");
    }
}
