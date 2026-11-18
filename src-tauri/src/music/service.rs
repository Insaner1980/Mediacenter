use super::{
    playlists,
    queue::{MusicAction, MusicQueue, Queue},
};
use crate::{
    app_state::AppState,
    contracts::{AppError, PlayerAction},
    player::mpv,
};
use tauri::Emitter;
use tokio_util::sync::CancellationToken;

fn database_error() -> AppError {
    AppError::new("database_error", "The music library is unavailable.")
}
fn persist(state: &AppState, queue: &Queue) -> Result<(), AppError> {
    let db = state.db.lock().map_err(|_| database_error())?;
    queue.save(&db)
}
fn publish(
    app: &tauri::AppHandle,
    state: &AppState,
    snapshot: MusicQueue,
    stamp: crate::remote::event_hub::Stamp,
) {
    crate::remote::state::queue_changed(state, &snapshot, stamp);
    let _ = app.emit_to("main", "music_queue_changed", snapshot);
}
fn refresh(state: &AppState, queue: &mut Queue) -> Result<(), AppError> {
    let db = state.db.lock().map_err(|_| database_error())?;
    let mut changed = false;
    for entry in &mut queue.original {
        match playlists::items(&db, std::slice::from_ref(&entry.item.media_id)) {
            Ok(mut items) => {
                let item = items.remove(0);
                if entry.item != item {
                    entry.item = item;
                    changed = true;
                }
            }
            Err(error) if error.code == "media_missing" => {
                if entry.item.availability != "missing" {
                    entry.item.availability = "missing".into();
                    changed = true;
                }
            }
            Err(error) => return Err(error),
        }
    }
    if changed {
        queue.revision = queue.revision.wrapping_add(1);
    }
    Ok(())
}
pub async fn get(state: &AppState) -> Result<MusicQueue, AppError> {
    let _gate = state.music_gate.lock().await;
    let active = state
        .player
        .lock()
        .await
        .as_ref()
        .map(|p| p.session.id.clone());
    let mut queue = state.music.lock().await;
    refresh(state, &mut queue)?;
    if queue.session_id.is_some() && queue.session_id != active {
        queue.session_id = None;
        queue.revision = queue.revision.wrapping_add(1);
    }
    let snapshot = queue.snapshot();
    let stamp = state
        .event_hub
        .reserve(crate::remote::protocol::Topic::QueueSummary);
    drop(queue);
    crate::remote::state::queue_changed(state, &snapshot, stamp);
    Ok(snapshot)
}
pub(crate) async fn clear_session(app: &tauri::AppHandle, state: &AppState, id: &str) {
    let mut queue = state.music.lock().await;
    if queue.session_id.as_deref() == Some(id) {
        queue.session_id = None;
        queue.revision = queue.revision.wrapping_add(1);
        let snapshot = queue.snapshot();
        let stamp = state
            .event_hub
            .reserve(crate::remote::protocol::Topic::QueueSummary);
        drop(queue);
        publish(app, state, snapshot, stamp);
    }
}
async fn stop_owned(
    app: &tauri::AppHandle,
    state: &AppState,
    queue: &mut Queue,
) -> Result<(), AppError> {
    let active = state
        .player
        .lock()
        .await
        .as_ref()
        .map(|p| p.session.id.clone());
    if let Some(active) = active {
        if let Err(error) = mpv::control(app, state, &active, PlayerAction::Stop).await {
            // The observer may already have retired this exact session.
            if error.code != "player_missing" {
                return Err(error);
            }
        }
    }
    queue.session_id = None;
    Ok(())
}
fn skippable(error: &AppError) -> bool {
    matches!(
        error.code.as_str(),
        "media_missing"
            | "media_unavailable"
            | "source_offline"
            | "volume_unmounted"
            | "volume_offline"
            | "media_changed"
            | "file_unavailable"
            | "player_load_failed"
    )
}
async fn start_current(
    app: &tauri::AppHandle,
    state: &AppState,
    queue: &mut Queue,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    let mut skipped = 0;
    for _ in 0..queue.original.len() {
        let Some(id) = queue.current_media_id() else {
            break;
        };
        if cancel.is_cancelled() {
            queue.message = Some("Music playback was cancelled.".into());
            return Err(AppError::new("cancelled", "Music playback was cancelled."));
        }
        match mpv::play_in_session(
            app.clone(),
            state.clone(),
            id.clone(),
            false,
            cancel.clone(),
        )
        .await
        {
            Ok(()) => {
                queue.session_id = state
                    .player
                    .lock()
                    .await
                    .as_ref()
                    .filter(|p| p.snapshot.media_id == id)
                    .map(|p| p.session.id.clone());
                if skipped > 0 {
                    queue.message = Some(format!(
                        "Skipped {skipped} unavailable tracks. They remain in the queue."
                    ));
                }
                queue.revision = queue.revision.wrapping_add(1);
                return Ok(());
            }
            Err(error) if skippable(&error) => {
                for entry in queue.original.iter_mut().filter(|e| e.item.media_id == id) {
                    entry.item.availability = "unavailable".into();
                }
                skipped += 1;
                queue.advance(false, false);
            }
            Err(error) => {
                queue.message = Some(error.message.clone());
                return Err(error);
            }
        }
    }
    queue.session_id = None;
    if skipped > 0 {
        queue.message = Some("No playable tracks remain. Unavailable entries are retained.".into());
    }
    Ok(())
}
// The gate serializes queue playback transitions. The queue mutex itself is never held
// while the mpv service starts/stops a player or the observer asks to advance it.
pub async fn play(
    app: tauri::AppHandle,
    state: AppState,
    media_ids: Vec<String>,
    start_index: usize,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    let _gate = state.music_gate.lock().await;
    play_in_transition(app, state.clone(), media_ids, start_index, cancel, true).await
}
pub(crate) async fn play_in_transition(
    app: tauri::AppHandle,
    state: AppState,
    media_ids: Vec<String>,
    start_index: usize,
    cancel: CancellationToken,
    allow_replace: bool,
) -> Result<(), AppError> {
    let _session = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
    if !allow_replace {
        let active = state.player.lock().await.as_ref().map(|p| p.media.kind);
        if active.is_some_and(|kind| kind != crate::contracts::MediaKind::Audio)
            || state.browser_active().await
            || crate::photos::presenter::snapshot(&state)
                .await
                .session_id
                .is_some()
        {
            return Err(AppError::new(
                "session_switch_required",
                "Prepare an activity replacement first.",
            ));
        }
    }
    let items = {
        let db = state.db.lock().map_err(|_| database_error())?;
        playlists::items(&db, &media_ids)?
    };
    let mut current = state.music.lock().await.clone();
    let mut replacement = current.clone();
    replacement.replace(items, start_index)?;
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Music playback was cancelled."));
    }
    if let Some(id) = replacement.current_media_id() {
        crate::library::query::open(&state, &id).await?;
    }
    stop_owned(&app, &state, &mut current).await?;
    let result = start_current(&app, &state, &mut replacement, cancel).await;
    let saved = store(&state, &mut replacement).await;
    let _ = app.emit_to("main", "music_queue_changed", replacement.snapshot());
    result.and(saved)
}
pub async fn play_playlist(
    app: tauri::AppHandle,
    state: AppState,
    playlist_id: String,
    start_index: usize,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    let ids = {
        let db = state.db.lock().map_err(|_| database_error())?;
        playlists::get(&db, &playlist_id)?
            .items
            .into_iter()
            .map(|i| i.media_id)
            .collect()
    };
    play(app, state, ids, start_index, cancel).await
}
pub async fn control(
    app: tauri::AppHandle,
    state: AppState,
    session_id: Option<String>,
    revision: u32,
    action: MusicAction,
    cancel: CancellationToken,
) -> Result<MusicQueue, AppError> {
    control_inner(
        Some(app),
        state,
        session_id,
        revision,
        (crate::control::playback::QueueAction::Native(action), true),
        cancel,
    )
    .await
}
/// Local and remote queue edits share validation, serialization and effects.
pub async fn control_entry(
    app: Option<tauri::AppHandle>,
    state: AppState,
    session_id: Option<String>,
    revision: u32,
    action: crate::control::playback::QueueAction,
    cancel: CancellationToken,
) -> Result<MusicQueue, AppError> {
    control_inner(app, state, session_id, revision, (action, false), cancel).await
}
// The caller holds music_gate. Adoption does not stop or restart a process.
async fn adopt_confirmed_player(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    mut queue: Queue,
    revision: u32,
    entry: u32,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<MusicQueue, AppError> {
    let _session = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
    if crate::photos::presenter::snapshot(state)
        .await
        .session_id
        .is_some()
        || state.browser_active().await
    {
        return Err(AppError::new(
            "session_switch_required",
            "Prepare an activity replacement first.",
        ));
    }
    let item = queue
        .original
        .iter()
        .find(|item| item.entry_id == entry)
        .ok_or_else(|| AppError::new("not_found", "The queue entry no longer exists."))?
        .item
        .media_id
        .clone();
    let media = crate::library::query::open(state, &item).await?;
    let identity = crate::control::activity::media_identity(&media)?;
    refresh(state, &mut queue)?;
    let mut stored = state.music.lock().await;
    let mut registry = state.player.lock().await;
    let player = registry
        .as_mut()
        .filter(|p| p.session.id == expected && p.media.kind == crate::contracts::MediaKind::Audio)
        .ok_or_else(|| {
            AppError::new(
                "queue_session_stale",
                "The music session changed. Refresh the queue.",
            )
        })?;
    if player.confirmed_music.as_ref() != Some(&identity)
        || crate::control::activity::media_identity(&player.media)? != identity
        || player.media.lease.cancelled()
        || media.lease.cancelled()
    {
        return Err(AppError::new(
            "stale_state",
            "The confirmed music file changed. Review it again.",
        ));
    }
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Music playback was cancelled."));
    }
    crate::control::playback::adopt_confirmed(
        &mut queue,
        revision,
        entry,
        expected,
        &player.session.id,
        &player.snapshot.media_id,
    )?;
    let saved = persist(state, &queue);
    player.confirmed_music = None;
    *stored = queue;
    let snapshot = stored.snapshot();
    let stamp = state
        .event_hub
        .reserve(crate::remote::protocol::Topic::QueueSummary);
    drop(registry);
    drop(stored);
    crate::remote::state::queue_changed(state, &snapshot, stamp);
    if let Some(app) = app {
        let _ = app.emit_to("main", "music_queue_changed", &snapshot);
    }
    saved?;
    Ok(snapshot)
}
async fn control_inner(
    app: Option<tauri::AppHandle>,
    state: AppState,
    session_id: Option<String>,
    revision: u32,
    command: (crate::control::playback::QueueAction, bool),
    cancel: CancellationToken,
) -> Result<MusicQueue, AppError> {
    let (action, allow_replace) = command;
    let _gate = state.music_gate.lock().await;
    let mut queue = state.music.lock().await.clone();
    let explicit_entry = match &action {
        crate::control::playback::QueueAction::Play(entry) => Some(*entry),
        _ => None,
    };
    let action = crate::control::playback::resolve(&queue, revision, action)?;
    if !allow_replace && !queue.matches_session(session_id.as_deref()) {
        if let (Some(entry), Some(expected)) = (explicit_entry, session_id.as_deref()) {
            return adopt_confirmed_player(
                app.as_ref(),
                &state,
                queue,
                revision,
                entry,
                expected,
                &cancel,
            )
            .await;
        }
    }
    if !queue.matches_session(session_id.as_deref()) {
        return Err(AppError::new(
            "queue_session_stale",
            "The music session changed. Refresh the queue.",
        ));
    }
    let items = if let MusicAction::Enqueue { media_ids } = &action {
        let db = state.db.lock().map_err(|_| database_error())?;
        playlists::items(&db, media_ids)?
    } else {
        vec![]
    };
    // Availability may change without changing entry identity. Refresh before selecting a track.
    refresh(&state, &mut queue)?;
    let current_revision = queue.revision;
    let mut next = queue.clone();
    let playback = next.edit(current_revision, action, items, crate::app_state::now_ms())?;
    let result = if playback {
        let _session = state
            .session_gate
            .try_lock()
            .map_err(|_| AppError::new("session_busy", "Another playback session is starting."))?;
        if !allow_replace {
            let active = state
                .player
                .lock()
                .await
                .as_ref()
                .map(|p| p.session.id.clone());
            if active != queue.session_id
                || crate::photos::presenter::snapshot(&state)
                    .await
                    .session_id
                    .is_some()
                || state.browser_active().await
            {
                return Err(AppError::new(
                    "session_switch_required",
                    "Prepare an activity replacement first.",
                ));
            }
        }
        if cancel.is_cancelled() {
            return Err(AppError::new("cancelled", "Music playback was cancelled."));
        }
        let app = app
            .as_ref()
            .ok_or_else(|| AppError::new("unavailable", "The native player is unavailable."))?;
        if let Some(id) = next.current_media_id() {
            crate::library::query::open(&state, &id).await?;
        }
        stop_owned(app, &state, &mut queue).await?;
        next.session_id = None;
        start_current(app, &state, &mut next, cancel).await
    } else {
        Ok(())
    };
    let saved = store(&state, &mut next).await;
    if let Some(app) = app {
        let _ = app.emit_to("main", "music_queue_changed", next.snapshot());
    }
    result?;
    saved?;
    Ok(next.snapshot())
}
// An explicit boxed future breaks the mpv observer/start-next future type cycle.
pub fn on_end(
    app: tauri::AppHandle,
    state: AppState,
    session_id: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(async move {
        let _gate = state.music_gate.lock().await;
        let mut queue = state.music.lock().await.clone();
        if !queue.matches_session(Some(&session_id)) {
            return;
        }
        let active = state
            .player
            .lock()
            .await
            .as_ref()
            .map(|p| p.session.id.clone());
        if !queue.can_advance(&session_id, active.as_deref()) {
            queue.session_id = None;
            queue.revision = queue.revision.wrapping_add(1);
            let _ = store(&state, &mut queue).await;
            let _ = app.emit_to("main", "music_queue_changed", queue.snapshot());
            return;
        }
        let result = async {
            let _session = state.session_gate.try_lock().map_err(|_| {
                AppError::new("session_busy", "Another playback session is starting.")
            })?;
            refresh(&state, &mut queue)?;
            // Stop's session guard is rechecked under the player lock. A user Stop
            // between the check above and this call prevents any next-track launch.
            mpv::control(&app, &state, &session_id, PlayerAction::Stop).await?;
            queue.session_id = None;
            queue.advance(true, false);
            start_current(&app, &state, &mut queue, CancellationToken::new()).await
        }
        .await;
        if let Err(error) = result {
            queue.message = Some(error.message);
        }
        let _ = store(&state, &mut queue).await;
        let _ = app.emit_to("main", "music_queue_changed", queue.snapshot());
    })
}

async fn store(state: &AppState, queue: &mut Queue) -> Result<(), AppError> {
    // Hold the queue while observing the player: observer cleanup releases the
    // player before taking this mutex, so it either precedes this reconciliation
    // or clears the result afterwards. A cloned edit cannot resurrect a session.
    let mut stored = state.music.lock().await;
    let active = state.player.lock().await;
    if queue.session_id.is_some()
        && queue.session_id.as_deref() != active.as_ref().map(|p| p.session.id.as_str())
    {
        queue.session_id = None;
        queue.revision = queue.revision.wrapping_add(1);
    }
    drop(active);
    let saved = persist(state, queue);
    if let Err(error) = &saved {
        queue.message = Some(error.message.clone());
    }
    *stored = queue.clone();
    let snapshot = stored.snapshot();
    let stamp = state
        .event_hub
        .reserve(crate::remote::protocol::Topic::QueueSummary);
    drop(stored);
    crate::remote::state::queue_changed(state, &snapshot, stamp);
    saved
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unplugged_volume_is_skipped_but_player_ownership_failure_is_not() {
        assert!(skippable(&AppError::new("volume_offline", "Drive removed")));
        assert!(!skippable(&AppError::new(
            "ownership_uncertain",
            "Identity changed"
        )));
    }
    #[tokio::test]
    async fn persistence_failure_keeps_live_queue_edits() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        state
            .db
            .lock()
            .unwrap()
            .execute("DROP TABLE settings", [])
            .unwrap();
        let mut queue = state.music.lock().await.clone();
        queue.shuffle = true;
        assert!(store(&state, &mut queue).await.is_err());
        assert!(state.music.lock().await.shuffle);
    }
    #[tokio::test]
    async fn queued_edit_cannot_restore_an_externally_closed_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        state.music.lock().await.session_id = Some("closed-player".into());
        let mut edit = state.music.lock().await.clone();
        edit.shuffle = true;
        // Observer cleanup lands after the edit cloned its input.
        state.music.lock().await.session_id = None;
        store(&state, &mut edit).await.unwrap();
        let stored = state.music.lock().await;
        assert!(stored.shuffle);
        assert_eq!(stored.session_id, None);
    }
    #[tokio::test]
    async fn externally_stopped_player_is_not_reported_as_active() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        state.music.lock().await.session_id = Some("stopped-player".into());
        assert_eq!(get(&state).await.unwrap().session_id, None);
    }
}
