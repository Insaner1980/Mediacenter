use crate::{
    app_state::AppState, contracts::*, library::query::OpenMedia,
    process::registry::ProcessIdentity,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    process::Stdio,
    time::{Duration, Instant},
};
use tauri::Emitter;
use tokio_util::sync::CancellationToken;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportPreferences {
    pub volume: f64,
    pub muted: bool,
}
impl Default for TransportPreferences {
    fn default() -> Self {
        Self {
            volume: 100.,
            muted: false,
        }
    }
}
pub fn preferences(db: &rusqlite::Connection) -> TransportPreferences {
    db.query_row(
        "SELECT value_json FROM settings WHERE key='player_transport'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str::<TransportPreferences>(&s).ok())
    .filter(|p| p.volume.is_finite() && (0.0..=100.0).contains(&p.volume))
    .unwrap_or_default()
}
pub struct Player {
    pub child: tokio::process::Child,
    pub window: Option<crate::outputs::x11::WindowInfo>,
    pub routing_target: Option<PlaybackTarget>,
    pub identity: ProcessIdentity,
    pub ipc: super::ipc::Client,
    pub media: OpenMedia,
    _subtitles: Vec<super::subtitles::Sidecar>,
    pub session: ManagedSession,
    pub snapshot: PlayerSnapshot,
    last_saved: Instant,
    natural_end: bool,
}
pub(crate) fn arguments(media: &OpenMedia) -> Vec<OsString> {
    let formats = format!("{},srt,ass,webvtt", crate::library::probe::FORMATS);
    let mut args = [
        "--no-config",
        "--load-scripts=no",
        "--ytdl=no",
        "--sub-auto=no",
        "--audio-file-auto=no",
        "--autoload-files=no",
        "--input-ipc-client=fd://0",
        "--pause=yes",
        "--idle=yes",
        "--keep-open=yes",
        "--terminal=no",
        "--gpu-sw=yes",
        "--demuxer=lavf",
    ]
    .iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.push(format!("--demuxer-lavf-o=protocol_whitelist=file,format_whitelist=%{}%{},enable_drefs=0,use_absolute_path=0",formats.len(),formats).into());
    // Music uses this same visible player even when the file has no artwork.
    args.push("--force-window=yes".into());
    if media.kind == MediaKind::Audio {
        args.push("--script-opts=osc-visibility=always".into());
        if let Some(name) = media.relative.file_name() {
            args.push(format!("--force-media-title={}", name.to_string_lossy()).into());
        }
    }
    args.push("--".into());
    args.push(crate::library::files::descriptor_path(&media.file).into_os_string());
    args
}
impl Player {
    pub async fn stop(&mut self) -> Result<(), AppError> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        if !self.identity.still_owned() {
            return Err(AppError::new(
                "ownership_uncertain",
                "The player process identity changed. It was not terminated.",
            ));
        }
        let _ =
            tokio::time::timeout(Duration::from_secs(1), self.ipc.command(json!(["quit"]))).await;
        if tokio::time::timeout(Duration::from_secs(3), self.child.wait())
            .await
            .is_err()
        {
            if !self.identity.still_owned() {
                return Err(AppError::new(
                    "ownership_uncertain",
                    "The player process identity changed. It was not terminated.",
                ));
            }
            self.child.kill().await?;
            self.child.wait().await?;
        }
        self.snapshot.phase = "ended".into();
        Ok(())
    }
}
async fn launch(
    state: &AppState,
    id: &str,
    resume: bool,
    cancel: CancellationToken,
) -> Result<(Player, tokio::sync::broadcast::Receiver<Value>), AppError> {
    if !std::path::Path::new("/usr/bin/mpv").is_file() {
        return Err(AppError::new(
            "player_unavailable",
            "Install mpv to play local videos and music.",
        ));
    }
    let media = crate::library::query::open(state, id).await?;
    if media.kind == MediaKind::Photo {
        return Err(AppError::new(
            "invalid_media_kind",
            "Open photos in the photo viewer.",
        ));
    }
    let (parent, child_socket) = std::os::unix::net::UnixStream::pair()?;
    parent.set_nonblocking(true)?;
    let client = super::ipc::Client::new(
        tokio::net::UnixStream::from_std(parent)?,
        Duration::from_secs(5),
    );
    let mut events = client.subscribe();
    let socket: std::os::fd::OwnedFd = child_socket.into();
    let mut child = tokio::process::Command::new("/usr/bin/mpv")
        .args(arguments(&media))
        .stdin(Stdio::from(socket))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| AppError::new("player_start_failed", "The player did not start."))?;
    let identity = ProcessIdentity::capture(pid).ok_or_else(|| {
        AppError::new(
            "player_start_failed",
            "The player process could not be identified.",
        )
    })?;
    let loaded=tokio::time::timeout(Duration::from_secs(5),async{loop{tokio::select!{_=cancel.cancelled()=>return Err(AppError::new("cancelled","Player startup cancelled.")),event=events.recv()=>{let event=event.map_err(|_|AppError::new("player_start_failed","The player stopped during startup."))?;match event["event"].as_str(){Some("file-loaded")=>return Ok(()),Some("end-file"|"ipc-disconnected")=>return Err(AppError::new("player_load_failed","The player could not load this media file.")),_=>{}}}}}}).await.unwrap_or_else(|_|Err(AppError::new("player_timeout","The player did not load the file in time.")));
    if let Err(error) = loaded {
        if identity.still_owned() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        return Err(error);
    }
    let duration = client
        .command(json!(["get_property", "duration"]))
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && *v > 0.);
    if resume {
        let saved = {
            let db = state
                .db
                .lock()
                .map_err(|_| AppError::new("database_error", "Resume state is unavailable."))?;
            db.query_row(
                "SELECT position_ms,completed FROM resume_positions WHERE media_id=?",
                [id],
                |r| Ok((r.get::<_, i64>(0)? as f64 / 1000., r.get::<_, bool>(1)?)),
            )
            .ok()
        };
        if let Some((saved, complete)) = saved {
            if let Some(position) = super::resume::position(true, saved, duration, complete) {
                client
                    .command(json!(["seek", position, "absolute+exact"]))
                    .await?;
            }
        }
    }
    // Retain authorized descriptors for the whole player session; never enable mpv's
    // unrestricted adjacent-file discovery or pass a user pathname through JSON.
    let mut subtitles = Vec::new();
    let mut subtitle_error = None;
    if media.kind == MediaKind::Video {
        let root = media.root.clone();
        let relative = media.relative.clone();
        let discovered =
            tokio::task::spawn_blocking(move || super::subtitles::adjacent(&root, &relative)).await;
        match discovered {
            Ok(Ok(found)) => {
                for sidecar in found {
                    if cancel.is_cancelled() {
                        return Err(AppError::new("cancelled", "Player startup cancelled."));
                    }
                    let path = crate::library::files::descriptor_path(&sidecar.file);
                    if client
                        .command(json!(["sub-add", path, "auto", sidecar.title]))
                        .await
                        .is_ok()
                    {
                        subtitles.push(sidecar);
                    } else {
                        subtitle_error = Some("An adjacent subtitle could not be loaded.".into());
                    }
                }
            }
            _ => subtitle_error = Some("Adjacent subtitles could not be inspected.".into()),
        }
    }
    for (index, property) in [
        "time-pos",
        "duration",
        "pause",
        "volume",
        "mute",
        "track-list",
        "seekable",
        "eof-reached",
    ]
    .iter()
    .enumerate()
    {
        client
            .command(json!(["observe_property", index + 1, property]))
            .await?;
    }
    let preferences = state
        .player_preferences
        .lock()
        .map_err(|_| AppError::new("state_error", "Player preferences are unavailable."))?
        .clone();
    client
        .command(json!(["set_property", "volume", preferences.volume]))
        .await?;
    client
        .command(json!(["set_property", "mute", preferences.muted]))
        .await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let kind = if media.kind == MediaKind::Audio {
        "local-audio"
    } else {
        "local-video"
    };
    let session = ManagedSession {
        id: session_id.clone(),
        kind: kind.into(),
        service_id: None,
        media_id: Some(id.into()),
        phase: "ready".into(),
        target: PlaybackTarget::default(),
    };
    let window = crate::outputs::x11::wmctrl(&["-lpG"])
        .await
        .ok()
        .and_then(|text| crate::outputs::x11::owned_window(&text, pid));
    let snapshot = PlayerSnapshot {
        revision: 0,
        session_id,
        media_id: id.into(),
        phase: "ready".into(),
        position_seconds: 0.,
        duration_seconds: duration,
        paused: true,
        volume: preferences.volume,
        muted: preferences.muted,
        seekable: false,
        tracks: vec![],
        error: subtitle_error,
    };
    Ok((
        Player {
            child,
            window,
            routing_target: None,
            identity,
            ipc: client,
            media,
            _subtitles: subtitles,
            session,
            snapshot,
            last_saved: Instant::now(),
            natural_end: false,
        },
        events,
    ))
}
pub async fn play(
    app: tauri::AppHandle,
    state: AppState,
    id: String,
    resume: bool,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    // Share the music transition gate so delayed queue advancement cannot
    // replace an explicitly selected video between its stop and start steps.
    let _music = state.music_gate.lock().await;
    play_in_transition(app, state.clone(), id, resume, cancel).await
}
// The caller holds music_gate for the entire playback transition.
pub(crate) async fn play_in_transition(
    app: tauri::AppHandle,
    state: AppState,
    id: String,
    resume: bool,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Player startup cancelled."));
    }
    let _session = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
    if state.browser.lock().await.is_some() {
        return Err(AppError::new(
            "session_switch_required",
            "Close the current service before playing local media.",
        ));
    }
    let is_video = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        db.query_row(
            "SELECT kind='video' FROM media_items WHERE id=?",
            [&id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    };
    if is_video
        && crate::photos::presenter::snapshot(&state)
            .await
            .session_id
            .is_some()
    {
        return Err(AppError::new(
            "session_switch_required",
            "Stop the slideshow before opening a local video.",
        ));
    }
    let mut registry = state.player.lock().await;
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Player startup cancelled."));
    }
    let previous_id = registry.as_ref().map(|p| p.session.id.clone());
    let retired = retire(&app, &state, &mut registry).await;
    if registry.is_none() {
        if let Some(id) = previous_id {
            // Queue reconciliation uses music -> player lock order.
            drop(registry);
            crate::music::service::clear_session(&app, &state, &id).await;
            registry = state.player.lock().await;
        }
    }
    retired?;
    let (mut player, events) = launch(&state, &id, resume, cancel.clone()).await?;
    if cancel.is_cancelled() {
        player.stop().await?;
        return Err(AppError::new("cancelled", "Player startup cancelled."));
    }
    let session = player.session.clone();
    let session_id = session.id.clone();
    *registry = Some(player);
    drop(registry);
    // Monitor the registered process even if publishing, routing, or unpausing fails.
    tauri::async_runtime::spawn(observe(
        app.clone(),
        state.clone(),
        session_id.clone(),
        events,
    ));
    let result = async {
        crate::events::publish(&app, &state, |home| {
            home.sessions.retain(|s| !s.kind.starts_with("local-"));
            home.sessions.push(session);
        })?;
        let target = state.snapshot()?.outputs.desired;
        let default_target = target.monitor_id.is_none() && target.audio_sink_id.is_none();
        let output =
            crate::outputs::coordinator::move_player(&app, &state, &session_id, target).await;
        if !default_target {
            output?;
        }
        if cancel.is_cancelled() {
            return Err(AppError::new("cancelled", "Player startup cancelled."));
        }
        let registry = state.player.lock().await;
        let player = registry
            .as_ref()
            .filter(|p| p.session.id == session_id)
            .ok_or_else(|| AppError::new("player_missing", "The player closed during startup."))?;
        player
            .ipc
            .command(json!(["set_property", "pause", false]))
            .await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = control(&app, &state, &session_id, PlayerAction::Stop).await;
    }
    result
}
// A registered player is released only after its owned process has stopped.
async fn retire(
    app: &tauri::AppHandle,
    state: &AppState,
    registry: &mut Option<Player>,
) -> Result<Option<String>, AppError> {
    let Some(player) = registry.as_mut() else {
        return Ok(None);
    };
    let saved = save(state, player);
    player.stop().await?;
    let id = player.session.id.clone();
    *registry = None;
    crate::events::publish(app, state, |home| home.sessions.retain(|s| s.id != id))?;
    let _ = app.emit_to("main", "player_changed", Option::<PlayerSnapshot>::None);
    saved?;
    Ok(Some(id))
}

fn apply(snapshot: &mut PlayerSnapshot, event: &Value) {
    if event["event"] != "property-change" {
        return;
    }
    let data = &event["data"];
    match event["name"].as_str() {
        Some("time-pos") => {
            if let Some(value) = data.as_f64().filter(|v| v.is_finite() && *v >= 0.) {
                snapshot.position_seconds = value;
            }
        }
        Some("duration") => {
            snapshot.duration_seconds = data.as_f64().filter(|v| v.is_finite() && *v > 0.)
        }
        Some("pause") => {
            if let Some(value) = data.as_bool() {
                snapshot.paused = value;
                snapshot.phase = if value { "paused" } else { "playing" }.into();
            }
        }
        Some("volume") => {
            if let Some(value) = data.as_f64().filter(|v| v.is_finite()) {
                snapshot.volume = value;
            }
        }
        Some("mute") => {
            if let Some(value) = data.as_bool() {
                snapshot.muted = value;
            }
        }
        Some("seekable") => snapshot.seekable = data.as_bool().unwrap_or(false),
        Some("eof-reached") if data == true => snapshot.phase = "ended".into(),
        Some("track-list") => {
            snapshot.tracks = data
                .as_array()
                .map(|tracks| {
                    tracks
                        .iter()
                        .take(256)
                        .filter_map(|track| {
                            Some(PlayerTrack {
                                id: i32::try_from(track["id"].as_i64()?).ok()?,
                                kind: track["type"].as_str()?.into(),
                                title: track["title"]
                                    .as_str()
                                    .or(track["lang"].as_str())
                                    .unwrap_or("Track")
                                    .chars()
                                    .take(200)
                                    .collect(),
                                selected: track["selected"].as_bool().unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        _ => {}
    }
}
fn save(state: &AppState, player: &mut Player) -> Result<(), AppError> {
    let s = &player.snapshot;
    let completed =
        super::resume::completed(s.position_seconds, s.duration_seconds, player.natural_end);
    state.db.lock().map_err(|_|AppError::new("database_error","Resume state could not be saved."))?.execute("INSERT INTO resume_positions(media_id,position_ms,duration_ms,updated_at_ms,completed) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(media_id) DO UPDATE SET position_ms=excluded.position_ms,duration_ms=excluded.duration_ms,updated_at_ms=excluded.updated_at_ms,completed=excluded.completed",rusqlite::params![s.media_id,(s.position_seconds*1000.) as i64,s.duration_seconds.map(|v|(v*1000.) as i64),crate::app_state::now_ms() as i64,completed])?;
    let transport = TransportPreferences {
        volume: s.volume.clamp(0., 100.),
        muted: s.muted,
    };
    if let Ok(mut preferences) = state.player_preferences.lock() {
        *preferences = transport.clone();
    }
    state.db.lock().map_err(|_|AppError::new("database_error","Player preferences could not be saved."))?.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('player_transport',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(&transport).unwrap(),crate::app_state::now_ms() as i64])?;
    player.last_saved = Instant::now();
    Ok(())
}
async fn observe(
    app: tauri::AppHandle,
    state: AppState,
    id: String,
    mut events: tokio::sync::broadcast::Receiver<Value>,
) {
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let mut registry = state.player.lock().await;
        let Some(player) = registry.as_mut().filter(|p| p.session.id == id) else {
            return;
        };
        let before = player.snapshot.phase.clone();
        let mut changed = false;
        let mut save_soon = false;
        let mut terminal = false;
        use std::os::unix::fs::MetadataExt;
        if player.media.lease.cancelled()
            || !std::fs::metadata(&player.media.root).is_ok_and(|m| {
                m.dev() == player.media.root_device && m.ino() == player.media.root_inode
            })
        {
            let _ = save(&state, player);
            let _ = player.stop().await;
            player.snapshot.phase = "error".into();
            player.snapshot.error = Some("The media source disconnected. Playback stopped.".into());
            changed = true;
            terminal = true;
        }
        while let Ok(event) = events.try_recv() {
            changed = true;
            if event["event"] == "ipc-disconnected" {
                terminal = true;
                break;
            }
            if event["event"] == "property-change"
                && event["name"] == "eof-reached"
                && event["data"] == true
            {
                player.natural_end = true;
            }
            if event["event"] == "playback-restart" || event["event"] == "seek" {
                save_soon = true;
            }
            apply(&mut player.snapshot, &event);
        }
        if let Ok(Some(status)) = player.child.try_wait() {
            terminal = true;
            if player.snapshot.phase != "error" {
                player.snapshot.phase = if player.natural_end {
                    "ended"
                } else if status.success() {
                    "stopped"
                } else {
                    "error"
                }
                .into();
            }
            changed = true;
        }
        if player.last_saved.elapsed() >= Duration::from_secs(5)
            || before != player.snapshot.phase
            || save_soon
        {
            if let Err(error) = save(&state, player) {
                player.snapshot.error = Some(error.message);
            }
        }
        if changed {
            player.snapshot.revision += 1;
            let _ = app.emit_to("main", "player_changed", &player.snapshot);
        }
        if before != player.snapshot.phase {
            player.session.phase = player.snapshot.phase.clone();
            let session = player.session.clone();
            let _ = crate::events::publish(&app, &state, |home| {
                if let Some(current) = home.sessions.iter_mut().find(|s| s.id == id) {
                    *current = session;
                }
            });
        }
        if !terminal && player.natural_end && player.media.kind == MediaKind::Audio {
            drop(registry);
            crate::music::on_end(app.clone(), state.clone(), id.clone()).await;
            return;
        }
        if terminal {
            let _ = retire(&app, &state, &mut registry).await;
            let removed = registry.is_none();
            drop(registry);
            if removed {
                crate::music::service::clear_session(&app, &state, &id).await;
                return;
            }
        }
    }
}
pub async fn control(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    action: PlayerAction,
) -> Result<(), AppError> {
    let mut registry = state.player.lock().await;
    let player = registry
        .as_mut()
        .filter(|p| p.session.id == id)
        .ok_or_else(|| AppError::new("player_missing", "This player session is unavailable."))?;
    let command = match action {
        PlayerAction::Stop => {
            let result = retire(app, state, &mut registry).await;
            let removed = registry.is_none();
            drop(registry);
            if removed {
                crate::music::service::clear_session(app, state, id).await;
            }
            return result.map(|_| ());
        }
        PlayerAction::Pause { paused } => json!(["set_property", "pause", paused]),
        PlayerAction::Mute { muted } => json!(["set_property", "mute", muted]),
        PlayerAction::Volume { value } if value.is_finite() && (0.0..=100.0).contains(&value) => {
            json!(["set_property", "volume", value])
        }
        PlayerAction::Seek { seconds }
            if seconds.is_finite()
                && seconds >= 0.
                && player.snapshot.seekable
                && player
                    .snapshot
                    .duration_seconds
                    .is_none_or(|d| seconds <= d) =>
        {
            json!(["seek", seconds, "absolute+exact"])
        }
        PlayerAction::AudioTrack { id }
            if player
                .snapshot
                .tracks
                .iter()
                .any(|track| track.id == id && track.kind == "audio") =>
        {
            json!(["set_property", "aid", id])
        }
        PlayerAction::SubtitleTrack { id: None } => json!(["set_property", "sid", "no"]),
        PlayerAction::SubtitleTrack { id: Some(id) }
            if player
                .snapshot
                .tracks
                .iter()
                .any(|track| track.id == id && track.kind == "sub") =>
        {
            json!(["set_property", "sid", id])
        }
        _ => {
            return Err(AppError::new(
                "invalid_player_action",
                "The requested player action is not available.",
            ))
        }
    };
    player.ipc.command(command).await?;
    save(state, player)?;
    Ok(())
}
