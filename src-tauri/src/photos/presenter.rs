pub use super::model::SlideshowRequest;
use super::{
    decode::{self, PhotoImage},
    model::{validate_interval, Queue},
};
use crate::{app_state::AppState, contracts::AppError, library::query::OpenMedia};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    time::{Duration, Instant},
};
use tauri::Manager;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
const LABEL: &str = "presentation";
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SlideshowSnapshot {
    pub session_id: Option<String>,
    pub phase: String,
    pub media_id: Option<String>,
    pub index: usize,
    pub total: usize,
    pub paused: bool,
    pub interval_seconds: u32,
    pub shuffle: bool,
    pub monitor_id: Option<String>,
    pub error: Option<String>,
    pub fit: String,
}
impl Default for SlideshowSnapshot {
    fn default() -> Self {
        Self {
            session_id: None,
            phase: "stopped".into(),
            media_id: None,
            index: 0,
            total: 0,
            paused: true,
            interval_seconds: 5,
            shuffle: false,
            monitor_id: None,
            error: None,
            fit: "fit".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SlideshowAction {
    Next,
    Previous,
    Pause { paused: bool },
    Interval { seconds: u32 },
    Shuffle { enabled: bool },
    Fit { actual: bool },
    ContinueOnComputer,
    Stop,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PhotoFrame {
    pub snapshot: SlideshowSnapshot,
    pub image: Option<PhotoImage>,
}
struct Cached {
    media: OpenMedia,
    image: PhotoImage,
}
struct Session {
    id: String,
    queue: Queue,
    monitor: String,
    sources: Vec<String>,
    cache: HashMap<String, Cached>,
    cancel: CancellationToken,
    next_at: Instant,
    display_missing: bool,
    fit: bool,
}
#[derive(Default)]
pub struct State {
    session: Option<Session>,
    last: SlideshowSnapshot,
    published: Option<SlideshowSnapshot>,
}
impl State {
    fn snapshot(&self) -> SlideshowSnapshot {
        match &self.session {
            None => self.last.clone(),
            Some(s) => SlideshowSnapshot {
                session_id: Some(s.id.clone()),
                phase: if s.display_missing {
                    "display_missing"
                } else if s.queue.paused {
                    "paused"
                } else if !s.cache.contains_key(s.queue.current()) {
                    "loading"
                } else {
                    "playing"
                }
                .into(),
                media_id: Some(s.queue.current().into()),
                index: s.queue.index,
                total: s.queue.ids.len(),
                paused: s.queue.paused,
                interval_seconds: s.queue.interval,
                shuffle: s.queue.shuffled,
                monitor_id: Some(s.monitor.clone()),
                error: s
                    .display_missing
                    .then(|| "Display disconnected. Continue on the computer when ready.".into()),
                fit: if s.fit { "fit" } else { "actual" }.into(),
            },
        }
    }
    fn publish(&mut self, app: &tauri::AppHandle, state: &AppState) {
        let snapshot = self.snapshot();
        if self.published.as_ref() == Some(&snapshot) {
            return;
        }
        self.published = Some(snapshot.clone());
        let _ = crate::events::publish(app, state, |home| {
            home.sessions.retain(|session| session.kind != "slideshow");
            if let Some(id) = snapshot.session_id {
                home.sessions.push(crate::contracts::ManagedSession {
                    id,
                    kind: "slideshow".into(),
                    service_id: None,
                    media_id: snapshot.media_id,
                    phase: snapshot.phase,
                    target: crate::contracts::PlaybackTarget {
                        monitor_id: snapshot.monitor_id,
                        audio_sink_id: None,
                    },
                });
            }
        });
    }
    fn stop(&mut self, error: Option<String>) {
        if let Some(s) = self.session.take() {
            s.cancel.cancel();
        }
        self.last = SlideshowSnapshot {
            error,
            ..Default::default()
        };
    }
}
pub fn typescript(config: &ts_rs::Config) -> Vec<String> {
    vec![
        SlideshowRequest::decl(config),
        SlideshowSnapshot::decl(config),
        SlideshowAction::decl(config),
        PhotoFrame::decl(config),
        PhotoImage::decl(config),
    ]
}
pub async fn snapshot(state: &AppState) -> SlideshowSnapshot {
    state.photos.lock().await.snapshot()
}
pub async fn frame(state: &AppState, label: &str) -> Result<Option<PhotoFrame>, AppError> {
    if label != LABEL {
        return Err(AppError::new(
            "forbidden_window",
            "This window cannot receive presentation images.",
        ));
    }
    let state = state.photos.lock().await;
    Ok(state.session.as_ref().map(|s| PhotoFrame {
        snapshot: state.snapshot(),
        image: s
            .cache
            .get(s.queue.current())
            .filter(|c| live(c))
            .map(|c| c.image.clone()),
    }))
}
fn live(cached: &Cached) -> bool {
    !cached.media.lease.cancelled()
        && std::fs::metadata(&cached.media.root).is_ok_and(|m| {
            m.dev() == cached.media.root_device && m.ino() == cached.media.root_inode
        })
}
fn sources(state: &AppState, ids: &[String]) -> Result<Vec<String>, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
    let mut query=db.prepare("SELECT source_id FROM media_items WHERE id=? AND kind='photo' AND availability='available'")?;
    let mut sources = std::collections::HashSet::new();
    for id in ids {
        sources.insert(
            query
                .query_row([id], |r| r.get::<_, String>(0))
                .map_err(|_| {
                    AppError::new(
                        "photo_unavailable",
                        "A selected photo is unavailable. Refresh the photo library.",
                    )
                })?,
        );
    }
    Ok(sources.into_iter().collect())
}
fn position(
    app: &tauri::AppHandle,
    monitor: &crate::contracts::MonitorDescriptor,
    computer: bool,
) -> Result<(), AppError> {
    let window = app.get_webview_window(LABEL).ok_or_else(|| {
        AppError::new(
            "presentation_unavailable",
            "The photo window could not be opened.",
        )
    })?;
    let apply = || -> Result<(), tauri::Error> {
        window.set_fullscreen(false)?;
        window.set_position(tauri::PhysicalPosition::new(
            monitor.bounds.x,
            monitor.bounds.y,
        ))?;
        window.set_size(tauri::PhysicalSize::new(
            if computer {
                monitor.bounds.width.min(960)
            } else {
                monitor.bounds.width
            },
            if computer {
                monitor.bounds.height.min(600)
            } else {
                monitor.bounds.height
            },
        ))?;
        if !computer {
            window.set_fullscreen_on_monitor(tauri::PhysicalPosition::new(
                f64::from(monitor.bounds.x),
                f64::from(monitor.bounds.y),
            ))?;
        }
        window.show()?;
        Ok(())
    };
    apply().map_err(|_| {
        AppError::new(
            "presentation_unavailable",
            "The photo window could not be placed on the selected display.",
        )
    })
}
pub async fn start(
    app: &tauri::AppHandle,
    state: &AppState,
    request: SlideshowRequest,
) -> Result<SlideshowSnapshot, AppError> {
    let _gate = state
        .session_gate
        .try_lock()
        .map_err(|_| AppError::new("session_busy", "Another session is changing. Try again."))?;
    if state.browser.lock().await.is_some()
        || state
            .player
            .lock()
            .await
            .as_ref()
            .is_some_and(|p| p.media.kind == crate::contracts::MediaKind::Video)
    {
        return Err(AppError::new(
            "session_active",
            "Stop the video or close the streaming session before starting a slideshow.",
        ));
    }
    let monitor_id = request.monitor_id.clone();
    let queue = Queue::new(request)?;
    let source_ids = sources(state, &queue.ids)?;
    let (monitors, computer) = crate::outputs::monitors::discover(app)?;
    let monitor =
        crate::outputs::monitors::resolve(&monitors, monitor_id.as_deref(), computer.as_deref())?;
    let mut photos = state.photos.lock().await;
    if photos.session.is_some() {
        return Err(AppError::new(
            "slideshow_active",
            "Stop the current slideshow before starting another.",
        ));
    }
    if app.get_webview_window(LABEL).is_none() {
        tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("index.html".into()))
            .title("Photos — Mediacenter")
            .visible(false)
            .focused(false)
            .build()
            .map_err(|_| {
                AppError::new(
                    "presentation_unavailable",
                    "The photo window could not be opened.",
                )
            })?;
    }
    if let Err(error) = position(
        app,
        monitor,
        computer.as_deref() == Some(monitor.id.as_str()),
    ) {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
        return Err(error);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = CancellationToken::new();
    photos.session = Some(Session {
        id: id.clone(),
        queue,
        monitor: monitor.id.clone(),
        sources: source_ids,
        cache: HashMap::new(),
        cancel: cancel.clone(),
        next_at: Instant::now() + Duration::from_secs(5),
        display_missing: false,
        fit: true,
    });
    photos.publish(app, state);
    let value = photos.snapshot();
    drop(photos);
    tauri::async_runtime::spawn(worker(app.clone(), state.clone(), id, cancel));
    Ok(value)
}
fn retain(session: &mut Session) {
    let wanted: Vec<String> = std::iter::once(session.queue.current())
        .chain(session.queue.preloads())
        .map(str::to_string)
        .collect();
    session.cache.retain(|id, _| wanted.contains(id));
}
pub async fn control(
    app: &tauri::AppHandle,
    state: &AppState,
    action: SlideshowAction,
) -> Result<SlideshowSnapshot, AppError> {
    let mut photos = state.photos.lock().await;
    if matches!(action, SlideshowAction::Stop) {
        photos.stop(None);
        photos.publish(app, state);
        drop(photos);
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
        return Ok(snapshot(state).await);
    }
    let session = photos
        .session
        .as_mut()
        .ok_or_else(|| AppError::new("slideshow_inactive", "Start a slideshow first."))?;
    match action {
        SlideshowAction::Next => session.queue.next(),
        SlideshowAction::Previous => session.queue.previous(),
        SlideshowAction::Pause { paused } => {
            if !paused && session.display_missing {
                return Err(AppError::new(
                    "display_missing",
                    "Continue on the computer before resuming.",
                ));
            }
            session.queue.pause(paused);
        }
        SlideshowAction::Interval { seconds } => {
            validate_interval(seconds)?;
            session.queue.interval = seconds;
        }
        SlideshowAction::Shuffle { enabled } => session.queue.shuffle(enabled),
        SlideshowAction::Fit { actual } => session.fit = !actual,
        SlideshowAction::ContinueOnComputer => {
            let (monitors, computer) = crate::outputs::monitors::discover(app)?;
            let monitor = crate::outputs::monitors::resolve(&monitors, None, computer.as_deref())?;
            position(app, monitor, true)?;
            session.monitor = monitor.id.clone();
            session.display_missing = false;
            session.queue.pause(false);
        }
        SlideshowAction::Stop => unreachable!(),
    }
    session.next_at = Instant::now() + Duration::from_secs(u64::from(session.queue.interval));
    retain(session);
    Ok(photos.snapshot())
}
pub async fn stop_source(app: &tauri::AppHandle, state: &AppState, source_id: &str) {
    let mut photos = state.photos.lock().await;
    if photos
        .session
        .as_ref()
        .is_some_and(|s| s.sources.iter().any(|id| id == source_id))
    {
        photos.stop(Some(
            "The photo source was removed. Reconnect it before starting again.".into(),
        ));
        photos.publish(app, state);
        drop(photos);
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
    }
}
async fn worker(app: tauri::AppHandle, state: AppState, id: String, cancel: CancellationToken) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let monitors = crate::outputs::monitors::discover(&app).ok().map(|v| v.0);
        let wanted = {
            let mut photos = state.photos.lock().await;
            let Some(s) = photos.session.as_mut().filter(|s| s.id == id) else {
                return;
            };
            let source_missing = {
                state
                    .db
                    .lock()
                    .map(|db| {
                        s.sources.iter().any(|source| {
                            db.query_row(
                                "SELECT availability FROM media_sources WHERE id=?",
                                [source],
                                |r| r.get::<_, String>(0),
                            )
                            .map_or(true, |v| v != "available")
                        })
                    })
                    .unwrap_or(true)
            };
            if source_missing || s.cache.values().any(|cached| !live(cached)) {
                photos.stop(Some(
                    "The photo source is unavailable. Reconnect it before starting again.".into(),
                ));
                photos.publish(&app, &state);
                drop(photos);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.close();
                }
                return;
            }
            if app.get_webview_window(LABEL).is_none() {
                photos.stop(None);
                photos.publish(&app, &state);
                return;
            }
            if monitors
                .as_ref()
                .is_none_or(|m| !m.iter().any(|m| m.connected && m.id == s.monitor))
            {
                s.display_missing = true;
                s.queue.pause(true);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.hide();
                }
            }
            if !s.queue.paused
                && s.cache.contains_key(s.queue.current())
                && Instant::now() >= s.next_at
            {
                s.queue.next();
                s.next_at = Instant::now() + Duration::from_secs(u64::from(s.queue.interval));
                retain(s);
            }
            let wanted = std::iter::once(s.queue.current())
                .chain(s.queue.preloads())
                .find(|key| !s.cache.contains_key(*key))
                .map(str::to_string);
            photos.publish(&app, &state);
            wanted
        };
        if let Some(media_id) = wanted {
            let result = async {
                let _permit = tokio::select! {
                    _ = cancel.cancelled() => return Err(AppError::new("cancelled", "Photo loading stopped.")),
                    permit = state.thumbnail_gate.acquire() => permit.map_err(|_| AppError::new("decoder_unavailable", "Photo decoding is unavailable."))?,
                };
                let _probe = tokio::select! {
                    _ = cancel.cancelled() => return Err(AppError::new("cancelled", "Photo loading stopped.")),
                    permit = state.probes.acquire() => permit.map_err(|_| AppError::new("decoder_unavailable", "Photo decoding is unavailable."))?,
                };
                let media = crate::library::query::open(&state, &media_id).await?;
                let image = decode::from_media(&media, cancel.clone()).await?;
                Ok::<_, AppError>(Cached { media, image })
            }.await;
            let mut photos = state.photos.lock().await;
            let Some(s) = photos.session.as_mut().filter(|s| s.id == id) else {
                return;
            };
            if cancel.is_cancelled() {
                return;
            }
            match result {
                Ok(cached) if live(&cached) => {
                    if s.queue.current() == media_id {
                        s.next_at =
                            Instant::now() + Duration::from_secs(u64::from(s.queue.interval));
                    }
                    s.cache.insert(media_id, cached);
                    retain(s);
                }
                Ok(_) => {
                    photos.stop(Some("The photo source is unavailable.".into()));
                }
                Err(error) => {
                    photos.stop(Some(error.message));
                }
            }
            if photos.session.is_none() {
                photos.publish(&app, &state);
                drop(photos);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.close();
                }
                return;
            }
        }
        tokio::select! {_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(400))=>{}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session {
            id: "test-session".into(),
            queue: Queue::new(SlideshowRequest {
                media_ids: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            })
            .unwrap(),
            monitor: "display".into(),
            sources: vec!["source".into()],
            cache: HashMap::new(),
            cancel: CancellationToken::new(),
            next_at: Instant::now(),
            display_missing: false,
            fit: true,
        }
    }
    fn cached(root: &std::path::Path, resources: &crate::library::resources::Resources) -> Cached {
        let metadata = std::fs::metadata(root).unwrap();
        let file = std::fs::File::open(root.join("image.jpg")).unwrap();
        Cached {
            media: OpenMedia {
                root: root.to_owned(),
                root_device: metadata.dev(),
                root_inode: metadata.ino(),
                file,
                kind: crate::contracts::MediaKind::Photo,
                source_id: "source".into(),
                size: 3,
                mtime: "0:0".into(),
                lease: resources
                    .acquire("source", CancellationToken::new())
                    .unwrap(),
            },
            image: PhotoImage {
                image: crate::contracts::EncodedImage {
                    mime: "image/jpeg".into(),
                    base64: "/9j/".into(),
                },
                width: 1,
                height: 1,
                modified_at_ms: None,
            },
        }
    }
    #[tokio::test]
    async fn source_removal_cancels_frames_and_releases_all_buffers_and_handles() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("image.jpg"), [255, 216, 255]).unwrap();
        let state = AppState::open(directory.path().join("app")).unwrap();
        let mut s = session();
        let cancel = s.cancel.clone();
        for id in ["a", "b", "c"] {
            s.cache
                .insert(id.into(), cached(directory.path(), &state.resources));
        }
        state.photos.lock().await.session = Some(s);
        assert!(frame(&state, "main").await.is_err());
        assert!(frame(&state, "presentation-other").await.is_err());
        assert!(frame(&state, LABEL).await.unwrap().unwrap().image.is_some());
        let source_ids = vec!["source".into()];
        let _block = state.resources.block(&source_ids).unwrap();
        assert!(frame(&state, LABEL).await.unwrap().unwrap().image.is_none());
        state
            .photos
            .lock()
            .await
            .stop(Some("Source removed".into()));
        assert!(cancel.is_cancelled());
        assert!(frame(&state, LABEL).await.unwrap().is_none());
        state
            .resources
            .wait_idle(&source_ids, Duration::from_millis(100))
            .await
            .unwrap();
    }
    #[test]
    fn preload_retention_releases_previous_frame_and_keeps_only_next_two() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("image.jpg"), [255, 216, 255]).unwrap();
        let resources = crate::library::resources::Resources::default();
        let mut s = session();
        for id in ["a", "b", "c", "d"] {
            s.cache
                .insert(id.into(), cached(directory.path(), &resources));
        }
        retain(&mut s);
        assert_eq!(s.cache.len(), 3);
        assert!(!s.cache.contains_key("d"));
        s.queue.next();
        retain(&mut s);
        assert!(!s.cache.contains_key("a"));
        assert!(s.cache.contains_key("b"));
        assert!(s.cache.contains_key("c"));
    }
    #[test]
    fn presentation_capability_grants_no_privileged_operations() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/presentation.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!(["presentation"]));
        for permission in capability["permissions"].as_array().unwrap() {
            assert!(
                matches!(
                    permission.as_str(),
                    Some(
                        "allow-get-presentation-frame"
                            | "core:event:allow-listen"
                            | "core:event:allow-unlisten"
                    )
                ),
                "unexpected presentation grant: {permission}"
            );
        }
    }
}
