//! Real-process regression, deliberately excluded from headless/default test runs.
use super::*;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

#[test]
#[ignore = "requires isolated Xvfb and real mpv; null audio/video outputs only"]
fn task9_native_audio_eof_keeps_observing_owned_player_after_seek_and_pause() {
    native_observer_fixture(false);
}

#[test]
#[ignore = "requires isolated Xvfb and real mpv; null audio/video outputs only"]
fn task9_native_eof_confirmed_music_adopts_queue_without_restart() {
    native_observer_fixture(true);
}

fn native_observer_fixture(handoff: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState::open(temporary.path().join("data")).unwrap();
    // One second of silent mono PCM; no external media or physical audio devices.
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend(16036u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(16000u32.to_le_bytes());
    wav.resize(16044, 0);
    let path = temporary.path().join("observer.wav");
    std::fs::write(&path, wav).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    {
        let db = state.db.lock().unwrap();
        db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('observer-source','local',?,'Synthetic observer test','[]','available')", [temporary.path().as_os_str().as_bytes()]).unwrap();
        db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('observer-media','observer-source',?,'Synthetic observer audio','audio',?,?,'available')",rusqlite::params![b"observer.wav".as_slice(),metadata.len() as i64,format!("{}:{}",metadata.mtime(),metadata.mtime_nsec())]).unwrap();
    }
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    let app_handle = app.handle().clone();
    tauri::async_runtime::block_on(async {
        let media = crate::library::query::open(&state, "observer-media")
            .await
            .unwrap();
        let (parent, socket) = std::os::unix::net::UnixStream::pair().unwrap();
        parent.set_nonblocking(true).unwrap();
        let client = super::super::ipc::Client::new(
            tokio::net::UnixStream::from_std(parent).unwrap(),
            Duration::from_secs(3),
        );
        let mut events = client.subscribe();
        let mut args = arguments(&media);
        let separator = args.len() - 2;
        args.splice(
            separator..separator,
            ["--ao=null", "--vo=null", "--force-window=no"].map(OsString::from),
        );
        let socket: std::os::fd::OwnedFd = socket.into();
        let child = tokio::process::Command::new("/usr/bin/mpv")
            .args(args)
            .stdin(Stdio::from(socket))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let identity = ProcessIdentity::capture(child.id().unwrap()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if events.recv().await.unwrap()["event"] == "file-loaded" {
                    break;
                }
            }
        })
        .await
        .expect("real mpv must load synthetic WAV");
        for (index, property) in [
            "time-pos",
            "duration",
            "pause",
            "volume",
            "mute",
            "seekable",
            "eof-reached",
        ]
        .iter()
        .enumerate()
        {
            client
                .command(json!(["observe_property", index + 1, property]))
                .await
                .unwrap();
        }
        let id = "observer-session".to_string();
        let session = ManagedSession {
            id: id.clone(),
            kind: "local-audio".into(),
            service_id: None,
            media_id: Some("observer-media".into()),
            phase: "ready".into(),
            target: PlaybackTarget::default(),
        };
        state.home.lock().unwrap().sessions.push(session.clone());
        *state.player.lock().await = Some(Player {
            child,
            window: None,
            routing_target: None,
            identity,
            ipc: client,
            media,
            _subtitles: vec![],
            session,
            snapshot: PlayerSnapshot {
                revision: 0,
                session_id: id.clone(),
                media_id: "observer-media".into(),
                phase: "ready".into(),
                position_seconds: 0.,
                duration_seconds: Some(1.),
                paused: true,
                volume: 100.,
                muted: false,
                seekable: true,
                tracks: vec![],
                error: None,
            },
            last_saved: Instant::now(),
            natural_end: false,
            confirmed_music: None,
        });
        let observer = tokio::spawn(observe(
            app_handle.clone(),
            state.clone(),
            id.clone(),
            events,
        ));
        let result: Result<(bool, u64, PlayerSnapshot), AppError> = async {
            pause(&state, &id, false).await?;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let eof = state
                        .player
                        .lock()
                        .await
                        .as_ref()
                        .expect("owned player")
                        .ipc
                        .command(json!(["get_property", "eof-reached"]))
                        .await?;
                    if eof == true {
                        return Ok::<(), AppError>(());
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .map_err(|_| AppError::new("timeout", "Real mpv did not reach EOF."))??;
            // Let the real observer consume EOF and exercise music::on_end's
            // no-queue path before submitting commands against the still-owned IPC.
            tokio::time::sleep(Duration::from_millis(600)).await;
            let finished_at_eof = observer.is_finished();
            let revision = state
                .player
                .lock()
                .await
                .as_ref()
                .unwrap()
                .snapshot
                .revision;
            control(
                &app_handle,
                &state,
                &id,
                PlayerAction::Seek { seconds: 0.1 },
            )
            .await?;
            pause(&state, &id, true).await?;
            tokio::time::sleep(Duration::from_millis(700)).await;
            let snapshot = state.player.lock().await.as_ref().unwrap().snapshot.clone();
            Ok((finished_at_eof, revision, snapshot))
        }
        .await;
        let handoff_result = if handoff && result.is_ok() {
            Some(check_confirmed_handoff(&app_handle, &state, &id).await)
        } else {
            None
        };
        // Always stop only this owned child and cancel the task before assertions.
        if let Some(mut player) = state.player.lock().await.take() {
            player.stop().await.unwrap();
        }
        observer.abort();
        let _ = observer.await;
        if let Some(result) = handoff_result {
            result.expect("confirmed native handoff and next must succeed");
        }
        let (finished_at_eof, revision, snapshot) =
            result.expect("real native IPC actions must succeed");
        assert!(
            !finished_at_eof,
            "observer detached at EOF while its player remained owned"
        );
        assert!(
            snapshot.revision > revision,
            "post-EOF IPC changes must reach native snapshot"
        );
        assert!(snapshot.paused);
        assert!(
            snapshot.position_seconds < 0.5,
            "seek after EOF must be observed"
        );
    });
    drop(app);
}

async fn check_confirmed_handoff(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
) -> Result<(), AppError> {
    use crate::control::playback::QueueAction;
    use crate::music::queue::MusicAction;
    let items = {
        let db = state.db.lock().unwrap();
        crate::music::playlists::items(&db, &["observer-media".into(), "observer-media".into()])?
    };
    state.music.lock().await.replace(items, 0)?;
    let revision = state.music.lock().await.revision;
    let before = state.music.lock().await.snapshot().items;
    let pid = state.player.lock().await.as_ref().unwrap().child.id();
    // This is the exact post-success binding used by activity execution. A
    // standalone player must not be adoptable before that binding exists.
    let denied = crate::music::service::control_entry(
        Some(app.clone()),
        state.clone(),
        Some(id.into()),
        revision,
        QueueAction::Play(1),
        CancellationToken::new(),
    )
    .await;
    if denied
        .as_ref()
        .err()
        .is_none_or(|error| error.code != "stale_state")
    {
        return Err(AppError::new(
            "test_failure",
            "Unconfirmed native player was adoptable.",
        ));
    }
    let identity = crate::control::activity::media_identity(
        &state.player.lock().await.as_ref().unwrap().media,
    )?;
    crate::control::activity::bind_music_handoff(state, "observer-media", identity).await?;
    let adopted = crate::music::service::control_entry(
        Some(app.clone()),
        state.clone(),
        Some(id.into()),
        revision,
        QueueAction::Play(1),
        CancellationToken::new(),
    )
    .await?;
    if adopted.session_id.as_deref() != Some(id)
        || adopted.current_index != Some(1)
        || adopted.items != before
        || state.player.lock().await.as_ref().unwrap().child.id() != pid
    {
        return Err(AppError::new(
            "test_failure",
            "Adoption changed native process or queue identity.",
        ));
    }
    // The selected duplicate is last: Next reaches queue end without launching
    // physical output, and must no longer reject the confirmed player session.
    let ended = crate::music::service::control_entry(
        Some(app.clone()),
        state.clone(),
        Some(id.into()),
        adopted.revision,
        QueueAction::Native(MusicAction::Next),
        CancellationToken::new(),
    )
    .await?;
    if ended.session_id.is_some()
        || ended.current_index.is_some()
        || ended.items != before
        || state.player.lock().await.is_some()
    {
        return Err(AppError::new(
            "test_failure",
            "Next did not continue the adopted queue.",
        ));
    }
    Ok(())
}
