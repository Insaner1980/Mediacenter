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
fn publish(app: &tauri::AppHandle, queue: &Queue) {
    let _ = app.emit_to("main", "music_queue_changed", queue.snapshot());
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
    Ok(queue.snapshot())
}
pub(crate) async fn clear_session(app: &tauri::AppHandle, state: &AppState, id: &str) {
    let mut queue = state.music.lock().await;
    if queue.session_id.as_deref() == Some(id) {
        queue.session_id = None;
        queue.revision = queue.revision.wrapping_add(1);
        publish(app, &queue);
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
        match mpv::play_in_transition(
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
    stop_owned(&app, &state, &mut current).await?;
    let result = start_current(&app, &state, &mut replacement, cancel).await;
    let saved = store(&state, &mut replacement).await;
    publish(&app, &replacement);
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
    let _gate = state.music_gate.lock().await;
    let mut queue = state.music.lock().await.clone();
    queue.validate(revision)?;
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
        if cancel.is_cancelled() {
            return Err(AppError::new("cancelled", "Music playback was cancelled."));
        }
        stop_owned(&app, &state, &mut queue).await?;
        next.session_id = None;
        start_current(&app, &state, &mut next, cancel).await
    } else {
        Ok(())
    };
    let saved = store(&state, &mut next).await;
    publish(&app, &next);
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
            publish(&app, &queue);
            return;
        }
        let result = async {
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
        publish(&app, &queue);
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
