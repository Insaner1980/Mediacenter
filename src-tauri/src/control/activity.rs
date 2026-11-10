//! Preflight and identity-bound native activity replacement.
use crate::{
    app_state::AppState,
    contracts::{AppError, MediaKind},
    remote::protocol::{ActivityKind, ActivityPrepareParams},
};
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Identity {
    pub player: Option<String>,
    pub photos: Option<String>,
    pub browser: Option<String>,
    pub queue_revision: u32,
    pub photo_item: Option<String>,
}
impl Identity {
    pub fn is_idle(&self) -> bool {
        self.player.is_none() && self.photos.is_none() && self.browser.is_none()
    }
}
pub async fn identity(state: &AppState) -> Identity {
    let queue_revision = state.music.lock().await.revision;
    let photos = crate::photos::presenter::snapshot(state).await;
    Identity {
        queue_revision,
        photo_item: photos.media_id,
        player: state
            .player
            .lock()
            .await
            .as_ref()
            .map(|p| p.session.id.clone()),
        photos: photos.session_id,
        browser: state
            .browser
            .lock()
            .await
            .as_ref()
            .map(|b| b.session.id.clone()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MediaIdentity {
    source: String,
    root: (u64, u64),
    file: (u64, u64, u64, i64, i64),
}
pub async fn preflight(
    state: &AppState,
    target: &ActivityPrepareParams,
) -> Result<MediaIdentity, AppError> {
    let expected = match target.kind {
        ActivityKind::Video => MediaKind::Video,
        ActivityKind::Music => MediaKind::Audio,
        ActivityKind::Photo => MediaKind::Photo,
        ActivityKind::Streaming => {
            return Err(AppError::new(
                "unsupported",
                "Streaming activity is not supported by this adapter.",
            ))
        }
    };
    let media = crate::library::query::open(state, target.item_id.as_str()).await?;
    if media.kind != expected {
        return Err(AppError::new("invalid_request", "The media kind changed."));
    }
    if expected == MediaKind::Photo {
        crate::photos::decode::decode_file(&media.file, tokio_util::sync::CancellationToken::new())
            .await?;
    } else if !std::path::Path::new("/usr/bin/mpv").is_file() {
        return Err(AppError::new(
            "player_unavailable",
            "Install mpv to play this media.",
        ));
    }
    use std::os::unix::fs::MetadataExt;
    let meta = media.file.metadata()?;
    Ok(MediaIdentity {
        source: media.source_id.clone(),
        root: (media.root_device, media.root_inode),
        file: (
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
        ),
    })
}
pub fn validate_identity(before: &Identity, now: &Identity) -> Result<(), AppError> {
    if before != now {
        Err(AppError::new(
            "stale_state",
            "The active media changed. Review the replacement again.",
        ))
    } else {
        Ok(())
    }
}
/// The caller owns both music_gate and session_gate across validation and effects.
pub async fn execute_locked(
    app: &tauri::AppHandle,
    state: &AppState,
    target: ActivityPrepareParams,
    before: Identity,
    media_identity: MediaIdentity,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), AppError> {
    if preflight(state, &target).await? != media_identity {
        return Err(AppError::new(
            "stale_state",
            "The target media changed. Review it again.",
        ));
    }
    validate_identity(&before, &identity(state).await)?;
    if before.browser.is_some() {
        return Err(AppError::new(
            "unsupported",
            "Close the streaming activity first.",
        ));
    }
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Activity change cancelled."));
    }
    if let Some(id) = before.player {
        crate::player::mpv::control(app, state, &id, crate::contracts::PlayerAction::Stop).await?;
    }
    if let Some(id) = before.photos {
        crate::photos::presenter::control_session(
            app,
            state,
            Some(&id),
            crate::photos::presenter::SlideshowAction::Stop,
        )
        .await?;
    }
    if target.kind == ActivityKind::Photo {
        crate::photos::presenter::start_in_session(
            app,
            state,
            crate::photos::model::SlideshowRequest {
                media_ids: vec![target.item_id.as_str().into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            },
            true,
            None,
        )
        .await?;
    } else {
        crate::player::mpv::play_in_session(
            app.clone(),
            state.clone(),
            target.item_id.as_str().into(),
            matches!(
                target.start,
                Some(crate::remote::protocol::PlaybackStart::Resume)
            ),
            cancel,
        )
        .await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn task9_missing_media_fails_preflight_without_changing_current_photo_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        crate::photos::presenter::seed_test_session(&state, "already-playing").await;
        let before = identity(&state).await;
        let target = crate::remote::adapters::checked(
            serde_json::json!({"kind":"video","itemId":"missing","start":"beginning"}),
        )
        .unwrap();
        assert_eq!(
            preflight(&state, &target).await.unwrap_err().code,
            "media_missing"
        );
        assert_eq!(identity(&state).await, before);
    }
    #[test]
    fn task9_confirmed_switch_cannot_target_replacement_session() {
        let before = Identity {
            player: Some("previewed".into()),
            ..Default::default()
        };
        let changed = Identity {
            player: Some("desktop-replacement".into()),
            ..Default::default()
        };
        assert_eq!(
            validate_identity(&before, &changed).unwrap_err().code,
            "stale_state"
        );
        validate_identity(&before, &before).unwrap();
    }
}
