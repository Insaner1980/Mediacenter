use crate::{
    contracts::{AppError, AudioSinkDescriptor},
    process::{
        registry::ProcessIdentity,
        runner::{self, CommandSpec, Executable},
    },
};
use serde_json::Value;
pub fn sinks(json: &str) -> Result<Vec<AudioSinkDescriptor>, String> {
    let rows: Vec<Value> = serde_json::from_str(json).map_err(|_| "invalid_audio_json")?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?;
            let active = row.get("active_port").and_then(Value::as_str);
            let unavailable = active.is_some_and(|active| {
                row.get("ports")
                    .and_then(Value::as_array)
                    .is_some_and(|ports| {
                        ports.iter().any(|port| {
                            port.get("name").and_then(Value::as_str) == Some(active)
                                && port.get("availability").and_then(Value::as_str)
                                    == Some("not available")
                        })
                    })
            });
            Some(AudioSinkDescriptor {
                id: name.into(),
                backend_name: name.into(),
                name: row
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .into(),
                available: !unavailable,
            })
        })
        .collect())
}
pub fn owned_streams(json: &str, owned: impl Fn(u32) -> bool) -> Result<Vec<(u32, u32)>, String> {
    let rows: Vec<Value> = serde_json::from_str(json).map_err(|_| "invalid_audio_json")?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let index = u32::try_from(row.get("index")?.as_u64()?).ok()?;
            let pid = row
                .get("properties")?
                .get("application.process.id")?
                .as_str()?
                .parse::<u32>()
                .ok()?;
            (pid > 0 && owned(pid)).then_some((index, pid))
        })
        .collect())
}
pub async fn pactl(args: &[&str]) -> Result<String, AppError> {
    let out = runner::run(
        CommandSpec {
            executable: Executable::Pactl,
            args: args.iter().map(std::ffi::OsString::from).collect(),
            timeout: std::time::Duration::from_secs(5),
            output_limit: 4 * 1024 * 1024,
        },
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|_| AppError::new("audio_unavailable", "The audio server is unavailable."))?;
    if !out.success {
        return Err(AppError::new(
            "audio_request_failed",
            "The audio server did not accept the request.",
        ));
    }
    String::from_utf8(out.stdout).map_err(|_| {
        AppError::new(
            "audio_unrecognized",
            "The audio server returned unrecognized information.",
        )
    })
}
pub async fn discover() -> Result<Vec<AudioSinkDescriptor>, AppError> {
    sinks(&pactl(&["-f", "json", "list", "sinks"]).await?)
        .map_err(|_| AppError::new("audio_unrecognized", "Audio outputs could not be read."))
}
pub(crate) trait AudioBackend: Sync {
    fn request(
        &self,
        args: &[&str],
    ) -> impl std::future::Future<Output = Result<String, AppError>> + Send;
}
struct Pulse;
impl AudioBackend for Pulse {
    async fn request(&self, args: &[&str]) -> Result<String, AppError> {
        pactl(args).await
    }
}
pub async fn route(root: &ProcessIdentity, target: Option<&str>) -> Result<String, AppError> {
    route_with(&Pulse, root, target).await
}
async fn available_sink(backend: &impl AudioBackend, id: &str) -> Result<(String, u64), AppError> {
    let json = backend.request(&["-f", "json", "list", "sinks"]).await?;
    let list = sinks(&json)
        .map_err(|_| AppError::new("audio_unrecognized", "Audio outputs could not be read."))?;
    let sink = crate::control::audio::validate_output(&list, id)?;
    let rows: Vec<Value> = serde_json::from_str(&json)
        .map_err(|_| AppError::new("audio_unrecognized", "Audio outputs could not be read."))?;
    let index = rows
        .iter()
        .find(|r| r["name"].as_str() == Some(&sink.backend_name))
        .and_then(|r| r["index"].as_u64())
        .ok_or_else(|| AppError::new("sink_unavailable", "Audio output unavailable"))?;
    Ok((sink.backend_name.clone(), index))
}
pub(crate) async fn route_with(
    backend: &impl AudioBackend,
    root: &ProcessIdentity,
    target: Option<&str>,
) -> Result<String, AppError> {
    if !root.still_owned() {
        return Err(AppError::new(
            "ownership_uncertain",
            "The audio owner changed.",
        ));
    }
    let id = match target {
        Some(id) => id.to_string(),
        None => backend
            .request(&["get-default-sink"])
            .await?
            .trim()
            .to_string(),
    };
    available_sink(backend, &id).await?;
    let json = backend
        .request(&["-f", "json", "list", "sink-inputs"])
        .await?;
    let streams = owned_streams(&json, |pid| root.owns_descendant(pid))
        .map_err(|_| AppError::new("audio_unrecognized", "Audio streams could not be read."))?;
    if streams.is_empty() {
        return Err(AppError::new(
            "audio_stream_pending",
            "No verified playback audio stream is available yet.",
        ));
    }
    for (index, pid) in streams {
        let identity = ProcessIdentity::capture(pid)
            .ok_or_else(|| AppError::new("audio_stream_changed", "The audio stream changed."))?;
        let (sink, sink_index) = available_sink(backend, &id).await?;
        let fresh = backend
            .request(&["-f", "json", "list", "sink-inputs"])
            .await?;
        let still = owned_streams(&fresh, |candidate| {
            candidate == pid && identity.still_owned() && root.owns_descendant(candidate)
        })
        .unwrap_or_default();
        if !still.contains(&(index, pid)) {
            return Err(AppError::new(
                "audio_stream_changed",
                "The audio stream changed.",
            ));
        }
        if stream_at(&fresh, index, pid, sink_index) {
            continue;
        }
        backend
            .request(&["move-sink-input", &index.to_string(), &sink])
            .await?;
        let mut confirmed = false;
        for attempt in 0..8 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let (_, now_index) = available_sink(backend, &id).await?;
            let observed = backend
                .request(&["-f", "json", "list", "sink-inputs"])
                .await?;
            if !identity.still_owned() || !root.owns_descendant(pid) {
                return Err(AppError::new(
                    "audio_stream_changed",
                    "The audio stream changed.",
                ));
            }
            if now_index == sink_index && stream_at(&observed, index, pid, sink_index) {
                confirmed = true;
                break;
            }
        }
        if !confirmed {
            return Err(AppError::new(
                "audio_route_unconfirmed",
                "The audio server did not confirm the playback output.",
            ));
        }
    }
    let (_, final_sink) = available_sink(backend, &id).await?;
    let final_json = backend
        .request(&["-f", "json", "list", "sink-inputs"])
        .await?;
    let final_streams = owned_streams(&final_json, |pid| root.owns_descendant(pid))
        .map_err(|_| AppError::new("audio_unrecognized", "Audio streams could not be read."))?;
    if !root.still_owned()
        || final_streams.is_empty()
        || final_streams
            .iter()
            .any(|(index, pid)| !stream_at(&final_json, *index, *pid, final_sink))
    {
        return Err(AppError::new(
            "audio_stream_changed",
            "Playback audio changed during output verification. Retry the sound output.",
        ));
    }
    Ok(id)
}
fn stream_at(json: &str, index: u32, pid: u32, sink: u64) -> bool {
    serde_json::from_str::<Vec<Value>>(json)
        .unwrap_or_default()
        .iter()
        .any(|r| {
            r["index"].as_u64() == Some(index as u64)
                && r["sink"].as_u64() == Some(sink)
                && r["properties"]["application.process.id"].as_str() == Some(&pid.to_string())
        })
}
pub async fn route_or_pause<F, Fut>(
    root: &ProcessIdentity,
    target: Option<&str>,
    pause: F,
) -> Result<String, AppError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppError>>,
{
    route_or_pause_with(&Pulse, root, target, pause).await
}
async fn route_or_pause_with<F, Fut>(
    backend: &impl AudioBackend,
    root: &ProcessIdentity,
    target: Option<&str>,
    pause: F,
) -> Result<String, AppError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppError>>,
{
    let result = route_with(backend, root, target).await;
    if result.as_ref().is_err_and(|e| e.code == "sink_unavailable") {
        if !root.still_owned() {
            return Err(AppError::new(
                "ownership_uncertain",
                "The audio owner changed.",
            ));
        }
        pause().await?;
    }
    result
}
pub async fn subscribe(app: tauri::AppHandle, state: crate::app_state::AppState) {
    use tokio::io::AsyncReadExt;
    loop {
        let child = tokio::process::Command::new("/usr/bin/pactl")
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdout) = child.stdout.take() {
                let mut buffer = [0u8; 4096];
                while let Ok(n) = stdout.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    super::coordinator::reconcile_audio(&app, &state).await;
                }
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        super::coordinator::reconcile_audio(&app, &state).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_chrome_audio_is_untouched() {
        let json = r#"[{"index":3,"properties":{"application.process.id":"42","application.name":"Google Chrome"}},{"index":4,"properties":{"application.process.id":"99","application.name":"Google Chrome"}}]"#;
        assert_eq!(owned_streams(json, |pid| pid == 42).unwrap(), vec![(3, 42)]);
    }
    #[test]
    fn ambiguous_stream_is_not_rerouted() {
        assert!(owned_streams(
            r#"[{"index":3,"properties":{"application.name":"Google Chrome"}}]"#,
            |_| true
        )
        .unwrap()
        .is_empty());
    }
    #[test]
    fn unavailable_port_is_not_a_working_hdmi_sink() {
        let json = r#"[{"name":"hdmi","description":"TV","state":"SUSPENDED","active_port":"hdmi-output-0","ports":[{"name":"hdmi-output-0","availability":"not available"}]}]"#;
        assert!(!sinks(json).unwrap()[0].available);
        assert!(sinks("invalid").is_err());
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use std::sync::Mutex;
    struct Backend {
        requests: Mutex<Vec<Vec<String>>>,
        sink: Mutex<u32>,
        available: bool,
        stream: bool,
        disappear: bool,
        added_stream: bool,
    }
    impl Backend {
        fn new() -> Self {
            Self {
                requests: Mutex::new(vec![]),
                sink: Mutex::new(1),
                available: true,
                stream: true,
                disappear: false,
                added_stream: false,
            }
        }
    }
    impl AudioBackend for Backend {
        async fn request(&self, args: &[&str]) -> Result<String, AppError> {
            self.requests
                .lock()
                .unwrap()
                .push(args.iter().map(|s| s.to_string()).collect());
            let count = self
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.last().is_some_and(|s| s == "sinks"))
                .count();
            match args {
                ["-f", "json", "list", "sinks"] => {
                    Ok(if self.available && !(self.disappear && count > 1) {
                        r#"[{"index":2,"name":"hdmi","description":"TV"}]"#.into()
                    } else {
                        "[]".into()
                    })
                }
                ["-f", "json", "list", "sink-inputs"] => Ok(if self.stream {
                    {
                        let mut rows = serde_json::json!([{"index":7,"sink":*self.sink.lock().unwrap(),"properties":{"application.process.id":std::process::id().to_string()}},{"index":8,"sink":1,"properties":{"application.process.id":"1"}}]);
                        if self.added_stream && *self.sink.lock().unwrap() == 2 {
                            rows.as_array_mut().unwrap().push(serde_json::json!({"index":9,"sink":1,"properties":{"application.process.id":std::process::id().to_string()}}));
                        }
                        rows.to_string()
                    }
                } else {
                    "[]".into()
                }),
                ["move-sink-input", "7", "hdmi"] => {
                    *self.sink.lock().unwrap() = 2;
                    Ok(String::new())
                }
                _ => panic!("Unexpected audio/picture/global-default effect: {args:?}"),
            }
        }
    }
    #[tokio::test]
    async fn task10_audio_only_moves_verified_owned_stream_and_observes_output() {
        let backend = Backend::new();
        let root = ProcessIdentity::capture(std::process::id()).unwrap();
        assert_eq!(
            route_with(&backend, &root, Some("hdmi")).await.unwrap(),
            "hdmi"
        );
        assert_eq!(*backend.sink.lock().unwrap(), 2);
        assert_eq!(
            backend
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.first().is_some_and(|s| s == "move-sink-input"))
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn task10_new_owned_stream_during_routing_cannot_be_reported_as_confirmed() {
        let mut backend = Backend::new();
        backend.added_stream = true;
        let root = ProcessIdentity::capture(std::process::id()).unwrap();
        assert_eq!(
            route_with(&backend, &root, Some("hdmi"))
                .await
                .unwrap_err()
                .code,
            "audio_stream_changed"
        );
    }
    #[tokio::test]
    async fn task10_stale_sink_missing_stream_and_pid_reuse_never_move_any_stream() {
        for mode in 0..4 {
            let mut backend = Backend::new();
            let mut root = ProcessIdentity::capture(std::process::id()).unwrap();
            match mode {
                0 => backend.available = false,
                1 => backend.stream = false,
                2 => backend.disappear = true,
                _ => root.start_time += 1,
            }
            let error = route_with(&backend, &root, Some("hdmi")).await.unwrap_err();
            assert_eq!(
                error.code,
                match mode {
                    0 | 2 => "sink_unavailable",
                    1 => "audio_stream_pending",
                    _ => "ownership_uncertain",
                }
            );
            assert!(!backend
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.first().is_some_and(|s| s == "move-sink-input")));
        }
    }
}

#[cfg(test)]
mod loss_tests {
    use super::*;
    struct Missing;
    impl AudioBackend for Missing {
        async fn request(&self, args: &[&str]) -> Result<String, AppError> {
            assert_eq!(args, ["-f", "json", "list", "sinks"]);
            Ok("[]".into())
        }
    }
    #[tokio::test]
    async fn task10_disappeared_hdmi_pauses_owned_local_media_without_default_fallback() {
        let root = ProcessIdentity::capture(std::process::id()).unwrap();
        let paused = std::sync::atomic::AtomicBool::new(false);
        let result = route_or_pause_with(&Missing, &root, Some("hdmi"), || async {
            paused.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await;
        assert_eq!(result.unwrap_err().code, "sink_unavailable");
        assert!(paused.load(std::sync::atomic::Ordering::SeqCst));
    }
}
