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
pub async fn route(root: &ProcessIdentity, target: Option<&str>) -> Result<String, AppError> {
    let sink = match target {
        Some(id) => id.to_string(),
        None => pactl(&["get-default-sink"]).await?.trim().to_string(),
    };
    let sink_json = pactl(&["-f", "json", "list", "sinks"]).await?;
    let list = sinks(&sink_json)
        .map_err(|_| AppError::new("audio_unrecognized", "Audio outputs could not be read."))?;
    let sink = list
        .iter()
        .find(|s| s.id == sink && s.available)
        .ok_or_else(|| {
            AppError::new(
                "sink_unavailable",
                "The selected audio output is unavailable.",
            )
        })?;
    let rows: Vec<Value> = serde_json::from_str(&sink_json)
        .map_err(|_| AppError::new("audio_unrecognized", "Audio outputs could not be read."))?;
    let sink_index = rows
        .iter()
        .find(|r| r["name"].as_str() == Some(&sink.backend_name))
        .and_then(|r| r["index"].as_u64())
        .ok_or_else(|| {
            AppError::new(
                "sink_unavailable",
                "The audio output identity is unavailable.",
            )
        })?;
    let json = pactl(&["-f", "json", "list", "sink-inputs"]).await?;
    let streams = owned_streams(&json, |pid| root.owns_descendant(pid))
        .map_err(|_| AppError::new("audio_unrecognized", "Audio streams could not be read."))?;
    if streams.is_empty() {
        return Err(AppError::new("audio_stream_pending","No verified playback audio stream is available. Start playback, then try moving the output again."));
    }
    for (index, pid) in streams {
        let identity = ProcessIdentity::capture(pid).ok_or_else(|| {
            AppError::new(
                "audio_stream_changed",
                "The playback audio stream changed. Retry moving the output.",
            )
        })?;
        let fresh = pactl(&["-f", "json", "list", "sink-inputs"]).await?;
        let still = owned_streams(&fresh, |candidate| {
            candidate == pid && identity.still_owned() && root.owns_descendant(candidate)
        })
        .unwrap_or_default();
        if !still.contains(&(index, pid)) {
            return Err(AppError::new(
                "audio_stream_changed",
                "The playback audio stream changed. Retry moving the output.",
            ));
        }
        let rows: Vec<Value> = serde_json::from_str(&fresh).unwrap_or_default();
        if rows.iter().any(|row| {
            row["index"].as_u64() == Some(index as u64) && row["sink"].as_u64() == Some(sink_index)
        }) {
            continue;
        }
        pactl(&["move-sink-input", &index.to_string(), &sink.backend_name]).await?;
        let observed: Vec<Value> =
            serde_json::from_str(&pactl(&["-f", "json", "list", "sink-inputs"]).await?)
                .unwrap_or_default();
        if !observed.iter().any(|row| {
            row["index"].as_u64() == Some(index as u64)
                && row["sink"].as_u64() == Some(sink_index)
                && row["properties"]["application.process.id"].as_str() == Some(&pid.to_string())
        }) {
            return Err(AppError::new(
                "audio_route_unconfirmed",
                "The audio server did not confirm the playback output.",
            ));
        }
    }
    Ok(sink.id.clone())
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
