//! Shared native playback validation and queue identity resolution.
use crate::{
    contracts::{AppError, PlayerAction, PlayerSnapshot},
    music::queue::{MusicAction, Queue},
};

pub fn validate(snapshot: &PlayerSnapshot, action: &PlayerAction) -> Result<(), AppError> {
    let valid = match action {
        PlayerAction::Seek { seconds } => {
            seconds.is_finite()
                && *seconds >= 0.
                && snapshot.seekable
                && snapshot.duration_seconds.is_none_or(|d| *seconds <= d)
        }
        PlayerAction::Volume { value } => value.is_finite() && (0.0..=100.0).contains(value),
        PlayerAction::AudioTrack { id } => snapshot
            .tracks
            .iter()
            .any(|t| t.id == *id && t.kind == "audio"),
        PlayerAction::SubtitleTrack { id: Some(id) } => snapshot
            .tracks
            .iter()
            .any(|t| t.id == *id && t.kind == "sub"),
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::new(
            "invalid_player_action",
            "The requested player action is not available.",
        ))
    }
}

#[derive(Clone, Debug)]
pub enum QueueAction {
    Native(MusicAction),
    Remove(u32),
    Move { entry: u32, before: Option<u32> },
    Play(u32),
}
/// Call only while holding the native music transition gate. CAS precedes resolution.
pub(crate) fn resolve(
    queue: &Queue,
    revision: u32,
    action: QueueAction,
) -> Result<MusicAction, AppError> {
    queue.validate(revision)?;
    let index = |id| {
        queue
            .order
            .iter()
            .position(|v| *v == id)
            .ok_or_else(|| AppError::new("not_found", "The queue entry no longer exists."))
    };
    Ok(match action {
        QueueAction::Native(action) => action,
        QueueAction::Remove(id) => MusicAction::RemoveAt { index: index(id)? },
        QueueAction::Play(id) => MusicAction::PlayAt { index: index(id)? },
        QueueAction::Move { entry, before } => {
            let from = index(entry)?;
            let to = match before {
                Some(id) => {
                    let before = index(id)?;
                    if before > from {
                        before - 1
                    } else {
                        before
                    }
                }
                None => queue.order.len() - 1,
            };
            MusicAction::MoveItem { from, to }
        }
    })
}

/// Called under music/session gates after verifying the confirmed player's file identity.
pub(crate) fn adopt_confirmed(
    queue: &mut Queue,
    revision: u32,
    entry: u32,
    expected: &str,
    active: &str,
    media: &str,
) -> Result<(), AppError> {
    queue.validate(revision)?;
    if queue.session_id.is_some() || expected != active {
        return Err(AppError::new(
            "queue_session_stale",
            "The music session changed. Refresh the queue.",
        ));
    }
    let item = queue
        .original
        .iter()
        .find(|item| item.entry_id == entry)
        .filter(|_| queue.order.contains(&entry))
        .ok_or_else(|| AppError::new("not_found", "The queue entry no longer exists."))?;
    if item.item.media_id != media || item.item.availability != "available" {
        return Err(AppError::new(
            "stale_state",
            "The confirmed music does not match this queue entry.",
        ));
    }
    queue.current = Some(entry);
    queue.session_id = Some(active.into());
    queue.revision = queue.revision.wrapping_add(1);
    queue.message = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> PlayerSnapshot {
        serde_json::from_value(serde_json::json!({"revision":1,"sessionId":"s","mediaId":"m","phase":"playing","positionSeconds":2,"durationSeconds":30,"paused":false,"volume":50,"muted":false,"seekable":true,"tracks":[{"id":2,"kind":"audio","title":"Audio","selected":true},{"id":3,"kind":"sub","title":"Sub","selected":false}],"error":null})).unwrap()
    }
    #[test]
    fn task9_player_validation_rejects_invalid_values_and_wrong_track_kind() {
        let s = snapshot();
        for action in [
            PlayerAction::Seek { seconds: 31. },
            PlayerAction::Seek { seconds: f64::NAN },
            PlayerAction::Volume { value: 101. },
            PlayerAction::Volume { value: -1. },
            PlayerAction::AudioTrack { id: 3 },
            PlayerAction::SubtitleTrack { id: Some(2) },
        ] {
            assert_eq!(
                validate(&s, &action).unwrap_err().code,
                "invalid_player_action"
            );
        }
        for action in [
            PlayerAction::Pause { paused: true },
            PlayerAction::Seek { seconds: 30. },
            PlayerAction::Volume { value: 0. },
            PlayerAction::AudioTrack { id: 2 },
            PlayerAction::SubtitleTrack { id: None },
        ] {
            validate(&s, &action).unwrap();
        }
        let mut s = s;
        s.seekable = false;
        assert!(validate(&s, &PlayerAction::Seek { seconds: 0. }).is_err());
    }
    #[test]
    fn task9_review_confirmed_music_adopts_explicit_duplicate_then_next() {
        let mut queue = Queue::default();
        queue
            .replace(
                ["same", "same", "next"]
                    .into_iter()
                    .map(|id| crate::music::queue::MusicItem {
                        media_id: id.into(),
                        title: id.into(),
                        artist: None,
                        album: None,
                        availability: "available".into(),
                    })
                    .collect(),
                0,
            )
            .unwrap();
        let original = queue.snapshot().items;
        let revision = queue.revision;
        adopt_confirmed(&mut queue, revision, 1, "confirmed", "confirmed", "same")
            .expect("confirmed music must attach the explicit queue occurrence");
        assert_eq!(queue.snapshot().items, original);
        assert_eq!(queue.current, Some(1));
        assert_eq!(queue.session_id.as_deref(), Some("confirmed"));
        assert_eq!(queue.revision, revision + 1);
        let revision = queue.revision;
        queue.edit(revision, MusicAction::Next, vec![], 0).unwrap();
        assert_eq!(queue.current, Some(2));
        assert_eq!(queue.current_media_id().as_deref(), Some("next"));
    }
    #[test]
    fn task9_review_music_handoff_rejects_stale_revision_session_and_wrong_media() {
        let mut queue = Queue::default();
        queue
            .replace(
                vec![crate::music::queue::MusicItem {
                    media_id: "same".into(),
                    title: "Same".into(),
                    artist: None,
                    album: None,
                    availability: "available".into(),
                }],
                0,
            )
            .unwrap();
        let revision = queue.revision;
        for (rev, entry, expected, active, media) in [
            (revision - 1, 0, "confirmed", "confirmed", "same"),
            (revision, 0, "old", "confirmed", "same"),
            (revision, 0, "confirmed", "replacement", "same"),
            (revision, 0, "confirmed", "confirmed", "other"),
            (revision, 99, "confirmed", "confirmed", "same"),
        ] {
            assert!(adopt_confirmed(&mut queue, rev, entry, expected, active, media).is_err());
            assert_eq!(queue.revision, revision);
            assert_eq!(queue.current, Some(0));
            assert!(queue.session_id.is_none());
        }
    }
    #[tokio::test]
    async fn task9_entry_identity_is_resolved_after_serialized_desktop_reorder() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let items = (0..3)
            .map(|n| crate::music::queue::MusicItem {
                media_id: format!("m{n}"),
                title: format!("M{n}"),
                artist: None,
                album: None,
                availability: "available".into(),
            })
            .collect();
        state.music.lock().await.replace(items, 0).unwrap();
        let old = state.music.lock().await.revision;
        let gate = state.music_gate.lock().await;
        let other = state.clone();
        let pending = tokio::spawn(async move {
            let _gate = other.music_gate.lock().await;
            let mut q = other.music.lock().await;
            assert_eq!(
                resolve(&q, old, QueueAction::Remove(1)).unwrap_err().code,
                "queue_stale"
            );
            let revision = q.revision;
            let action = resolve(&q, revision, QueueAction::Remove(1)).unwrap();
            q.edit(revision, action, vec![], 0).unwrap();
        });
        {
            let mut q = state.music.lock().await;
            q.edit(old, MusicAction::MoveItem { from: 1, to: 2 }, vec![], 0)
                .unwrap();
        }
        drop(gate);
        pending.await.unwrap();
        assert_eq!(
            state
                .music
                .lock()
                .await
                .snapshot()
                .items
                .iter()
                .map(|e| e.entry_id)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
    }
    #[tokio::test]
    async fn task9_remote_queue_play_requires_confirmation_before_replacing_photos() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        crate::photos::presenter::seed_test_session(&state, "photos-to-preserve").await;
        state
            .music
            .lock()
            .await
            .replace(
                vec![crate::music::queue::MusicItem {
                    media_id: "missing".into(),
                    title: "Missing".into(),
                    artist: None,
                    album: None,
                    availability: "available".into(),
                }],
                0,
            )
            .unwrap();
        let revision = state.music.lock().await.revision;
        let result = crate::music::service::control_entry(
            None,
            state.clone(),
            None,
            revision,
            QueueAction::Play(0),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, "session_switch_required");
        assert_eq!(
            crate::photos::presenter::snapshot(&state)
                .await
                .session_id
                .as_deref(),
            Some("photos-to-preserve")
        );
    }
    #[tokio::test]
    async fn task9_queue_start_reserves_session_transition_before_any_player_effect() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        state
            .music
            .lock()
            .await
            .replace(
                vec![crate::music::queue::MusicItem {
                    media_id: "missing".into(),
                    title: "Missing".into(),
                    artist: None,
                    album: None,
                    availability: "available".into(),
                }],
                0,
            )
            .unwrap();
        let revision = state.music.lock().await.revision;
        let session = state.session_gate.lock().await;
        let result = crate::music::service::control_entry(
            None,
            state.clone(),
            None,
            revision,
            QueueAction::Play(0),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err().code, "session_busy");
        drop(session);
    }
}
