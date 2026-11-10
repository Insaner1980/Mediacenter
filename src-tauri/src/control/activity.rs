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
    media_identity(&media)
}
pub(crate) fn media_identity(
    media: &crate::library::query::OpenMedia,
) -> Result<MediaIdentity, AppError> {
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
// Only a successful native activity execution grants this one-time handoff.
pub(crate) async fn bind_music_handoff(
    state: &AppState,
    item: &str,
    identity: MediaIdentity,
) -> Result<(), AppError> {
    let mut player = state.player.lock().await;
    let player = player
        .as_mut()
        .filter(|p| p.snapshot.media_id == item && p.media.kind == MediaKind::Audio)
        .ok_or_else(|| AppError::new("stale_state", "The confirmed music session changed."))?;
    if media_identity(&player.media)? != identity || player.media.lease.cancelled() {
        return Err(AppError::new(
            "stale_state",
            "The confirmed music file changed.",
        ));
    }
    player.confirmed_music = Some(identity);
    Ok(())
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
        if target.kind == ActivityKind::Music {
            bind_music_handoff(state, target.item_id.as_str(), media_identity).await?;
        }
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

trait MenuWindow {
    fn show(&self) -> Result<(), AppError>;
    fn focus(&self) -> Result<(), AppError>;
}
impl MenuWindow for tauri::WebviewWindow {
    fn show(&self) -> Result<(), AppError> {
        tauri::WebviewWindow::show(self)
            .map_err(|_| AppError::new("unavailable", "The menu window could not be shown."))
    }
    fn focus(&self) -> Result<(), AppError> {
        self.set_focus()
            .map_err(|_| AppError::new("unavailable", "The menu window could not be focused."))
    }
}
fn focus_menu(window: &impl MenuWindow) -> Result<(), AppError> {
    window.show()?;
    window.focus()
}
pub fn show_menu(app: &tauri::AppHandle) -> Result<(), AppError> {
    use tauri::Manager;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::new("unavailable", "The menu window is unavailable."))?;
    focus_menu(&window)
}
pub async fn show_playback(
    app: Option<&tauri::AppHandle>,
    state: &AppState,
    expected: &str,
) -> Result<(), AppError> {
    show_playback_with(state, expected, |label| {
        use tauri::Manager;
        let window = app
            .and_then(|app| app.get_webview_window(label))
            .filter(|window| window.label() == label)
            .ok_or_else(|| {
                AppError::new(
                    "presentation_unavailable",
                    "The photo window is unavailable.",
                )
            })?;
        focus_menu(&window)
    })
    .await
}
pub(crate) async fn show_playback_with<F>(
    state: &AppState,
    expected: &str,
    photo: F,
) -> Result<(), AppError>
where
    F: FnOnce(&str) -> Result<(), AppError>,
{
    let player = state.player.lock().await;
    if let Some(player) = player.as_ref().filter(|p| p.session.id == expected) {
        if !player.identity.still_owned() {
            return Err(AppError::new(
                "ownership_uncertain",
                "The player process changed.",
            ));
        }
        let window = player
            .window
            .as_ref()
            .ok_or_else(|| AppError::new("unavailable", "The player window is unavailable."))?;
        let listing = crate::outputs::x11::wmctrl(&["-lpG"]).await?;
        let verified = crate::outputs::x11::owned_window(&listing, player.identity.pid)
            .filter(|w| w.id == window.id)
            .ok_or_else(|| AppError::new("ownership_uncertain", "The player window changed."))?;
        if !player.identity.still_owned() {
            return Err(AppError::new(
                "ownership_uncertain",
                "The player process changed.",
            ));
        }
        crate::outputs::x11::wmctrl(&["-ia", &verified.id]).await?;
        return Ok(());
    }
    drop(player);
    if crate::photos::presenter::foreground_session(state, expected, photo).await? {
        return Ok(());
    }
    let browser = state.browser.lock().await;
    let browser = browser
        .as_ref()
        .filter(|b| b.session.id == expected)
        .ok_or_else(|| AppError::new("stale_state", "The playback session changed."))?;
    browser.focus().await
}

#[cfg(test)]
mod foreground_tests {
    use super::*;
    struct Window(std::sync::Mutex<Vec<&'static str>>);
    impl MenuWindow for Window {
        fn show(&self) -> Result<(), AppError> {
            self.0.lock().unwrap().push("show");
            Ok(())
        }
        fn focus(&self) -> Result<(), AppError> {
            self.0.lock().unwrap().push("focus");
            Ok(())
        }
    }
    #[test]
    fn task10_show_menu_only_shows_and_focuses_existing_window_without_picture_or_geometry_changes()
    {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::open(directory.path().into()).unwrap();
        let target = crate::contracts::PlaybackTarget {
            monitor_id: Some("mirrored-desktop".into()),
            audio_sink_id: Some("hdmi".into()),
        };
        state.home.lock().unwrap().outputs.desired = target;
        state.db.lock().unwrap().execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('main_geometry','{\"x\":40,\"y\":50,\"width\":1100,\"height\":700}',1)",[]).unwrap();
        let before = serde_json::to_value(state.snapshot().unwrap()).unwrap();
        let window = Window(Default::default());
        focus_menu(&window).unwrap();
        assert_eq!(*window.0.lock().unwrap(), ["show", "focus"]);
        assert_eq!(
            serde_json::to_value(state.snapshot().unwrap()).unwrap(),
            before
        );
        let geometry: String = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT value_json FROM settings WHERE key='main_geometry'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(geometry, r#"{"x":40,"y":50,"width":1100,"height":700}"#);
    }
    #[tokio::test]
    async fn task10_show_playback_never_relaunches_a_missing_or_unowned_session() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::open(directory.path().into()).unwrap();
        let before = serde_json::to_value(state.snapshot().unwrap()).unwrap();
        assert_eq!(
            show_playback(None, &state, "not-owned")
                .await
                .unwrap_err()
                .code,
            "stale_state"
        );
        assert_eq!(
            serde_json::to_value(state.snapshot().unwrap()).unwrap(),
            before
        );
        assert!(state.player.lock().await.is_none());
        assert!(state.browser.lock().await.is_none());
    }
}
