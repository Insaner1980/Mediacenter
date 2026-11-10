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
    selection: Option<String>,
    generation: u64,
    navigation: Option<CancellationToken>,
    id: String,
    queue: Queue,
    monitor: String,
    sources: Vec<String>,
    cache: HashMap<String, Cached>,
    failed_preloads: std::collections::HashSet<String>,
    message: Option<String>,
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
struct Publication {
    snapshot: SlideshowSnapshot,
    stamp: crate::remote::event_hub::Stamp,
    home: Option<crate::events::HomePublication>,
}
impl State {
    fn select(
        &mut self,
        selection: &str,
        item: &str,
        paused: bool,
        interval: u32,
        adoption: Option<&[String]>,
    ) -> Result<bool, AppError> {
        let Some(session) = self.session.as_mut() else {
            return Ok(false);
        };
        validate_interval(interval)?;
        if !paused && session.display_missing {
            return Err(AppError::new(
                "display_missing",
                "The presentation display is unavailable.",
            ));
        }
        let replacement = if session.selection.as_deref() != Some(selection) {
            // activity.confirm(photo) creates a paused singleton without a
            // selection context. Only that exact visible item can adopt a
            // caller-authorized order; never replace a running/other selection.
            let ids = adoption.filter(|ids| ids.first().is_some_and(|id| id == item));
            if session.selection.is_some()
                || !session.queue.paused
                || session.queue.ids.len() != 1
                || session.queue.current() != item
                || ids.is_none()
            {
                return Err(AppError::new(
                    "stale_state",
                    "The photo selection changed. Review the activity first.",
                ));
            }
            Some(Queue::new(SlideshowRequest {
                media_ids: ids.expect("validated selection").to_vec(),
                monitor_id: None,
                interval_seconds: interval,
                shuffle: false,
            })?)
        } else {
            None
        };
        let index = replacement
            .as_ref()
            .unwrap_or(&session.queue)
            .ids
            .iter()
            .position(|id| id == item)
            .ok_or_else(|| {
                AppError::new("not_found", "The photo is outside the active selection.")
            })?;
        if let Some(queue) = replacement {
            session.queue = queue;
            session.selection = Some(selection.into());
        }
        invalidate_navigation(session);
        session.failed_preloads.clear();
        session.message = None;
        session.queue.index = index;
        session.queue.pause(paused);
        session.queue.interval = interval;
        session.next_at = Instant::now() + Duration::from_secs(u64::from(interval));
        retain(session);
        Ok(true)
    }

    pub(crate) fn snapshot(&self) -> SlideshowSnapshot {
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
                    .then(|| "Display disconnected. Continue on the computer when ready.".into())
                    .or_else(|| s.message.clone()),
                fit: if s.fit { "fit" } else { "actual" }.into(),
            },
        }
    }
    fn publication(&mut self, state: &AppState) -> Option<Publication> {
        let snapshot = self.snapshot();
        if self.published.as_ref() == Some(&snapshot) {
            return None;
        }
        self.published = Some(snapshot.clone());
        let stamp = state
            .event_hub
            .reserve(crate::remote::protocol::Topic::Slideshow);
        // Commit native home projection under the photo guard too. Only delivery
        // is deferred, so an old delayed publisher cannot resurrect an old slide.
        let home = crate::events::commit(state, |home| {
            home.sessions.retain(|session| session.kind != "slideshow");
            if let Some(id) = snapshot.session_id.clone() {
                home.sessions.push(crate::contracts::ManagedSession {
                    id,
                    kind: "slideshow".into(),
                    service_id: None,
                    media_id: snapshot.media_id.clone(),
                    phase: snapshot.phase.clone(),
                    target: crate::contracts::PlaybackTarget {
                        monitor_id: snapshot.monitor_id.clone(),
                        audio_sink_id: None,
                    },
                });
            }
        })
        .ok();
        Some(Publication {
            snapshot,
            stamp,
            home,
        })
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
fn publish(app: &tauri::AppHandle, state: &AppState, publication: Option<Publication>) {
    let Some(publication) = publication else {
        return;
    };
    crate::remote::state::slideshow_changed(state, &publication.snapshot, publication.stamp);
    if let Some(home) = publication.home {
        home.emit(app, state);
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
    let mut query=db.prepare("SELECT m.source_id FROM media_items m JOIN media_sources s ON s.id=m.source_id WHERE m.id=? AND m.kind='photo' AND m.availability='available' AND s.availability='available'")?;
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
// Shared preparation for a fresh presentation and confirmed-singleton adoption.
fn prepare(
    state: &AppState,
    request: SlideshowRequest,
    paused: bool,
) -> Result<(Queue, Vec<String>), AppError> {
    let mut queue = Queue::new(request)?;
    queue.pause(paused);
    let source_ids = sources(state, &[queue.current().into()])?;
    Ok((queue, source_ids))
}
fn source_missing(state: &AppState, session: &Session) -> bool {
    state
        .db
        .lock()
        .map(|db| {
            session.sources.iter().any(|source| {
                db.query_row(
                    "SELECT availability FROM media_sources WHERE id=?",
                    [source],
                    |r| r.get::<_, String>(0),
                )
                .map_or(true, |v| v != "available")
            })
        })
        .unwrap_or(true)
}
async fn load(
    state: &AppState,
    media_id: &str,
    cancel: CancellationToken,
) -> Result<Cached, AppError> {
    let _permit = tokio::select! {
        _ = cancel.cancelled() => return Err(AppError::new("cancelled", "Photo loading stopped.")),
        permit = state.thumbnail_gate.acquire() => permit.map_err(|_| AppError::new("decoder_unavailable", "Photo decoding is unavailable."))?,
    };
    let _probe = tokio::select! {
        _ = cancel.cancelled() => return Err(AppError::new("cancelled", "Photo loading stopped.")),
        permit = state.probes.acquire() => permit.map_err(|_| AppError::new("decoder_unavailable", "Photo decoding is unavailable."))?,
    };
    let media = crate::library::query::open(state, media_id).await?;
    let image = decode::from_media(&media, cancel).await?;
    let cached = Cached { media, image };
    if !live(&cached) {
        return Err(AppError::new(
            "photo_unavailable",
            "The photo source is unavailable.",
        ));
    }
    Ok(cached)
}
fn invalidate_navigation(session: &mut Session) {
    if let Some(pending) = session.navigation.take() {
        pending.cancel();
    }
    session.generation = session.generation.wrapping_add(1);
}
struct Navigation {
    session: String,
    generation: u64,
    current: String,
    item: String,
    index: usize,
    automatic: bool,
    cancel: CancellationToken,
}
fn finish_navigation(
    photos: &mut State,
    state: &AppState,
    work: Navigation,
    result: Result<Option<Cached>, AppError>,
) -> Result<(SlideshowSnapshot, Option<Publication>), AppError> {
    let session = photos
        .session
        .as_mut()
        .filter(|session| {
            session.id == work.session
                && session.generation == work.generation
                && session.queue.current() == work.current
                && !work.cancel.is_cancelled()
        })
        .ok_or_else(|| AppError::new("stale_state", "The photo transition changed."))?;
    session.navigation = None;
    let result = result.and_then(|cached| {
        let source_ids = sources(state, std::slice::from_ref(&work.item))?;
        let valid = cached
            .as_ref()
            .or_else(|| session.cache.get(&work.item))
            .is_some_and(live);
        if !valid {
            return Err(AppError::new(
                "photo_unavailable",
                "The photo source is unavailable.",
            ));
        }
        Ok((cached, source_ids))
    });
    match result {
        Ok((cached, source_ids)) => {
            if let Some(cached) = cached {
                session.cache.insert(work.item.clone(), cached);
            }
            session.failed_preloads.remove(&work.item);
            session.queue.index = work.index;
            session.sources = source_ids;
            session.message = None;
            retain(session);
        }
        Err(error) if work.automatic => {
            session.queue.pause(true);
            session.message = Some(format!(
                "Slideshow paused: {} Choose another photo or reconnect its source.",
                error.message
            ));
        }
        Err(error) => return Err(error),
    }
    session.next_at = Instant::now() + Duration::from_secs(u64::from(session.queue.interval));
    let value = photos.snapshot();
    let publication = photos.publication(state);
    Ok((value, publication))
}
// Reserve the intended transition under the domain guard; all permit waits,
// probing and decoding happen after releasing it. Stop can cancel the child token.
async fn navigate(
    state: &AppState,
    id: Option<&str>,
    previous: bool,
    automatic: bool,
) -> Result<(SlideshowSnapshot, Option<Publication>), AppError> {
    let work = {
        let mut photos = state.photos.lock().await;
        if id.is_some() && photos.session.as_ref().map(|session| session.id.as_str()) != id {
            return Err(AppError::new("stale_state", "The photo session changed."));
        }
        let session = photos
            .session
            .as_mut()
            .ok_or_else(|| AppError::new("slideshow_inactive", "Start a slideshow first."))?;
        if automatic
            && (session.queue.paused || session.display_missing || Instant::now() < session.next_at)
        {
            return Err(AppError::new(
                "stale_state",
                "The automatic photo transition changed.",
            ));
        }
        invalidate_navigation(session);
        let cancel = session.cancel.child_token();
        session.navigation = Some(cancel.clone());
        let count = session.queue.ids.len();
        let index = (session.queue.index + if previous { count - 1 } else { 1 }) % count;
        let work = Navigation {
            session: session.id.clone(),
            generation: session.generation,
            current: session.queue.current().into(),
            item: session.queue.ids[index].clone(),
            index,
            automatic,
            cancel,
        };
        // Reuse an already decoded, live preload after the same availability check.
        let available = sources(state, std::slice::from_ref(&work.item));
        if let Err(error) = available {
            return finish_navigation(&mut photos, state, work, Err(error));
        }
        if session.cache.get(&work.item).is_some_and(live) {
            return finish_navigation(&mut photos, state, work, Ok(None));
        }
        work
    };
    let result = load(state, &work.item, work.cancel.clone()).await.map(Some);
    let mut photos = state.photos.lock().await;
    finish_navigation(&mut photos, state, work, result)
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
    start_in_session(app, state, request, false, None).await
}
/// Caller holds session_gate. Still photos start paused before publication/worker startup.
pub(crate) async fn start_in_session(
    app: &tauri::AppHandle,
    state: &AppState,
    request: SlideshowRequest,
    paused: bool,
    selection: Option<String>,
) -> Result<SlideshowSnapshot, AppError> {
    let monitor_id = request.monitor_id.clone();
    let (queue, source_ids) = prepare(state, request, paused)?;
    if let Some(selection) = &selection {
        let item = queue
            .ids
            .first()
            .ok_or_else(|| AppError::new("invalid_slideshow", "Choose a photo."))?;
        let mut photos = state.photos.lock().await;
        if photos.select(selection, item, paused, queue.interval, Some(&queue.ids))? {
            photos.session.as_mut().expect("selected session").sources = source_ids;
            let value = photos.snapshot();
            let publication = photos.publication(state);
            drop(photos);
            publish(app, state, publication);
            return Ok(value);
        }
    }
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
        selection,
        generation: 0,
        navigation: None,
        id: id.clone(),
        queue,
        monitor: monitor.id.clone(),
        sources: source_ids,
        cache: HashMap::new(),
        failed_preloads: Default::default(),
        message: None,
        cancel: cancel.clone(),
        next_at: Instant::now() + Duration::from_secs(5),
        display_missing: false,
        fit: true,
    });
    let publication = photos.publication(state);
    let value = photos.snapshot();
    drop(photos);
    publish(app, state, publication);
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
    control_session(app, state, None, action).await
}
/// Check the session under the same photo lock that applies the action.
pub async fn control_session(
    app: &tauri::AppHandle,
    state: &AppState,
    id: Option<&str>,
    action: SlideshowAction,
) -> Result<SlideshowSnapshot, AppError> {
    if matches!(action, SlideshowAction::Next | SlideshowAction::Previous) {
        let (value, publication) = navigate(
            state,
            id,
            matches!(action, SlideshowAction::Previous),
            false,
        )
        .await?;
        publish(app, state, publication);
        return Ok(value);
    }
    let mut photos = state.photos.lock().await;
    if id.is_some() && photos.session.as_ref().map(|s| s.id.as_str()) != id {
        return Err(AppError::new("stale_state", "The photo session changed."));
    }
    if matches!(action, SlideshowAction::Stop) {
        photos.stop(None);
        let publication = photos.publication(state);
        // Keep the domain guard until the old presentation window is closed;
        // a concurrent start must not reuse it before this close completes.
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
        drop(photos);
        publish(app, state, publication);
        return Ok(snapshot(state).await);
    }
    let session = photos
        .session
        .as_mut()
        .ok_or_else(|| AppError::new("slideshow_inactive", "Start a slideshow first."))?;
    match action {
        SlideshowAction::Next | SlideshowAction::Previous => unreachable!("handled before locking"),
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
    invalidate_navigation(session);
    session.next_at = Instant::now() + Duration::from_secs(u64::from(session.queue.interval));
    retain(session);
    let value = photos.snapshot();
    let publication = photos.publication(state);
    drop(photos);
    publish(app, state, publication);
    Ok(value)
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
        let publication = photos.publication(state);
        // Keep the domain guard until the old presentation window is closed;
        // a concurrent start must not reuse it before this close completes.
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
        drop(photos);
        publish(app, state, publication);
    }
}
async fn worker(app: tauri::AppHandle, state: AppState, id: String, cancel: CancellationToken) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let monitors = crate::outputs::monitors::discover(&app).ok().map(|v| v.0);
        let (advance_due, wanted) = {
            let mut photos = state.photos.lock().await;
            let Some(s) = photos.session.as_mut().filter(|s| s.id == id) else {
                return;
            };
            let source_missing = source_missing(&state, s);
            if source_missing
                || s.cache
                    .get(s.queue.current())
                    .is_some_and(|cached| !live(cached))
            {
                photos.stop(Some(
                    "The photo source is unavailable. Reconnect it before starting again.".into(),
                ));
                let publication = photos.publication(&state);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.close();
                }
                drop(photos);
                publish(&app, &state, publication);
                return;
            }
            if app.get_webview_window(LABEL).is_none() {
                photos.stop(None);
                let publication = photos.publication(&state);
                drop(photos);
                publish(&app, &state, publication);
                return;
            }
            if monitors
                .as_ref()
                .is_none_or(|m| !m.iter().any(|m| m.connected && m.id == s.monitor))
            {
                if !s.display_missing {
                    invalidate_navigation(s);
                }
                s.display_missing = true;
                s.queue.pause(true);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.hide();
                }
            }
            let advance_due = !s.queue.paused
                && s.cache.contains_key(s.queue.current())
                && Instant::now() >= s.next_at;
            s.cache.retain(|_, cached| live(cached));
            let wanted = std::iter::once(s.queue.current())
                .chain(s.queue.preloads())
                .find(|key| !s.cache.contains_key(*key) && !s.failed_preloads.contains(*key))
                .map(|key| (key.to_string(), s.generation));
            let publication = photos.publication(&state);
            drop(photos);
            publish(&app, &state, publication);
            (advance_due, wanted)
        };
        if advance_due {
            if let Ok((_, publication)) = navigate(&state, Some(&id), false, true).await {
                publish(&app, &state, publication);
            }
        } else if let Some((media_id, generation)) = wanted {
            let result = load(&state, &media_id, cancel.clone()).await;
            let mut photos = state.photos.lock().await;
            let Some(s) = photos.session.as_mut().filter(|s| s.id == id) else {
                return;
            };
            if cancel.is_cancelled() {
                return;
            }
            if s.generation != generation {
                continue;
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
                failure => {
                    if s.queue.current() == media_id {
                        let message = failure
                            .err()
                            .map(|error| error.message)
                            .unwrap_or_else(|| "The photo source is unavailable.".into());
                        photos.stop(Some(message));
                    } else {
                        // A speculative neighbor never owns the current presentation.
                        s.failed_preloads.insert(media_id);
                    }
                }
            }
            if photos.session.is_none() {
                let publication = photos.publication(&state);
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.close();
                }
                drop(photos);
                publish(&app, &state, publication);
                return;
            }
        }
        tokio::select! {_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(400))=>{}}
    }
}

#[cfg(test)]
pub(crate) async fn seed_test_session(state: &AppState, id: &str) {
    state.photos.lock().await.session = Some(Session {
        selection: None,
        generation: 0,
        navigation: None,
        id: id.into(),
        queue: Queue::new(SlideshowRequest {
            media_ids: vec!["existing-photo".into()],
            monitor_id: None,
            interval_seconds: 5,
            shuffle: false,
        })
        .unwrap(),
        monitor: "test".into(),
        sources: vec![],
        cache: HashMap::new(),
        failed_preloads: Default::default(),
        message: None,
        cancel: CancellationToken::new(),
        next_at: Instant::now(),
        display_missing: false,
        fit: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task9_still_photo_selection_keeps_session_and_pauses_before_publication() {
        let mut state = State {
            session: Some(session()),
            ..Default::default()
        };
        assert!(state.select("selected-order", "c", true, 5, None).unwrap());
        assert_eq!(state.snapshot().session_id.as_deref(), Some("test-session"));
        assert_eq!(state.snapshot().media_id.as_deref(), Some("c"));
        assert!(state.snapshot().paused);
        assert!(state
            .select("another-selection", "a", false, 5, None)
            .is_err());
        assert_eq!(state.snapshot().media_id.as_deref(), Some("c"));
    }
    #[test]
    fn task9_paused_singleton_adopts_exact_item_selection_without_replacing_session() {
        let mut singleton = session();
        singleton.selection = None;
        singleton.queue = Queue::new(SlideshowRequest {
            media_ids: vec!["c".into()],
            monitor_id: None,
            interval_seconds: 5,
            shuffle: false,
        })
        .unwrap();
        singleton.queue.pause(true);
        let mut state = State {
            session: Some(singleton),
            ..Default::default()
        };
        let ids = vec!["c".into(), "d".into(), "a".into()];
        assert!(state
            .select("owned-selection", "c", true, 5, Some(&ids))
            .unwrap());
        assert_eq!(state.snapshot().session_id.as_deref(), Some("test-session"));
        assert_eq!(state.snapshot().media_id.as_deref(), Some("c"));
        assert!(state.snapshot().paused);
        assert_eq!(state.session.as_ref().unwrap().queue.ids, ids);
        assert_eq!(
            state.session.as_ref().unwrap().selection.as_deref(),
            Some("owned-selection")
        );
    }
    #[test]
    fn task9_singleton_handoff_rejects_wrong_item_and_playing_session() {
        for (current, paused) in [("wrong-item", true), ("c", false)] {
            let mut singleton = session();
            singleton.selection = None;
            singleton.queue = Queue::new(SlideshowRequest {
                media_ids: vec![current.into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            })
            .unwrap();
            singleton.queue.pause(paused);
            let mut state = State {
                session: Some(singleton),
                ..Default::default()
            };
            let before = state.snapshot();
            assert_eq!(
                state
                    .select(
                        "owned-selection",
                        "c",
                        true,
                        5,
                        Some(&["c".into(), "d".into()])
                    )
                    .unwrap_err()
                    .code,
                "stale_state"
            );
            assert_eq!(state.snapshot(), before);
            assert!(state.session.as_ref().unwrap().selection.is_none());
        }
    }
    #[tokio::test]
    async fn task9_review_unavailable_navigation_preserves_current_and_automatic_pauses() {
        let (_dir, state) = mixed_library();
        for neighbor in ["missing", "offline"] {
            let mut current = session();
            current.queue = Queue::new(SlideshowRequest {
                media_ids: vec!["a".into(), neighbor.into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            })
            .unwrap();
            let original = current.queue.ids.clone();
            state.photos.lock().await.session = Some(current);
            assert!(navigate(&state, Some("test-session"), false, false)
                .await
                .is_err());
            assert_eq!(snapshot(&state).await.media_id.as_deref(), Some("a"));
            assert!(navigate(&state, Some("test-session"), true, false)
                .await
                .is_err());
            navigate(&state, Some("test-session"), false, true)
                .await
                .unwrap();
            let photos = state.photos.lock().await;
            let current = photos.session.as_ref().unwrap();
            assert!(current.queue.paused, "automatic transition must pause");
            assert!(current
                .message
                .as_deref()
                .is_some_and(|message| message.contains("Slideshow paused")));
            assert_eq!(current.queue.current(), "a");
            assert_eq!(current.queue.ids, original);
        }
    }
    #[tokio::test]
    async fn task9_fix2_stop_does_not_wait_for_navigation_decoder_permits() {
        let (_dir, state) = mixed_library();
        state
            .db
            .lock()
            .unwrap()
            .execute(
                "UPDATE media_items SET availability='available' WHERE id='missing'",
                [],
            )
            .unwrap();
        for automatic in [false, true] {
            let mut current = session();
            current.queue = Queue::new(SlideshowRequest {
                media_ids: vec!["a".into(), "missing".into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            })
            .unwrap();
            let cancel = current.cancel.clone();
            state.photos.lock().await.session = Some(current);
            let permits = state
                .thumbnail_gate
                .acquire_many(state.thumbnail_gate.available_permits() as u32)
                .await
                .unwrap();
            let mut pending = Box::pin(navigate(&state, Some("test-session"), false, automatic));
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut pending)
                    .await
                    .is_err(),
                "navigation must be waiting for the held decoder permits"
            );
            let stopped = tokio::time::timeout(Duration::from_millis(100), async {
                let mut photos = state.photos.lock().await;
                photos.stop(None);
                photos.publication(&state);
            })
            .await;
            let cancelled = if stopped.is_ok() {
                tokio::time::timeout(Duration::from_millis(100), &mut pending)
                    .await
                    .ok()
                    .is_some_and(|result| result.is_err())
            } else {
                false
            };
            // Release the pending future/permits even on RED before asserting.
            drop(pending);
            drop(permits);
            assert!(
                stopped.is_ok(),
                "Stop must acquire the domain guard while navigation awaits decoding"
            );
            assert!(cancel.is_cancelled());
            assert!(
                cancelled,
                "obsolete load must complete without acquiring decoder permits"
            );
            assert!(snapshot(&state).await.session_id.is_none());
        }
    }
    #[tokio::test]
    async fn task9_fix2_new_navigation_reuses_preload_and_supersedes_pending_step() {
        let (dir, state) = mixed_library();
        state
            .db
            .lock()
            .unwrap()
            .execute(
                "UPDATE media_items SET availability='available' WHERE id='missing'",
                [],
            )
            .unwrap();
        std::fs::write(dir.path().join("image.jpg"), [255, 216, 255]).unwrap();
        let mut current = session();
        current.queue = Queue::new(SlideshowRequest {
            media_ids: vec!["a".into(), "missing".into()],
            monitor_id: None,
            interval_seconds: 5,
            shuffle: false,
        })
        .unwrap();
        state.photos.lock().await.session = Some(current);
        let permits = state
            .thumbnail_gate
            .acquire_many(state.thumbnail_gate.available_permits() as u32)
            .await
            .unwrap();
        let mut older = Box::pin(navigate(&state, Some("test-session"), false, false));
        assert!(tokio::time::timeout(Duration::from_millis(20), &mut older)
            .await
            .is_err());
        let visible = tokio::time::timeout(Duration::from_millis(100), snapshot(&state))
            .await
            .expect("state reads must remain responsive");
        assert_eq!(visible.media_id.as_deref(), Some("a"));
        state
            .photos
            .lock()
            .await
            .session
            .as_mut()
            .unwrap()
            .cache
            .insert("missing".into(), cached(dir.path(), &state.resources));
        let (latest, publication) = tokio::time::timeout(
            Duration::from_millis(100),
            navigate(&state, Some("test-session"), false, false),
        )
        .await
        .expect("valid preload must not acquire decoder permits")
        .unwrap();
        assert_eq!(latest.media_id.as_deref(), Some("missing"));
        assert!(publication.is_some());
        let superseded = tokio::time::timeout(Duration::from_millis(100), &mut older)
            .await
            .expect("superseded decode must cancel promptly");
        assert_eq!(superseded.err().unwrap().code, "stale_state");
        assert_eq!(snapshot(&state).await, latest);
        drop(older);
        drop(permits);
    }
    #[tokio::test]
    async fn task9_fix2_completed_obsolete_decode_cannot_replace_new_session() {
        let (dir, state) = mixed_library();
        std::fs::write(dir.path().join("image.jpg"), [255, 216, 255]).unwrap();
        let mut photos = state.photos.lock().await;
        photos.session = Some(session());
        let work = Navigation {
            session: "test-session".into(),
            generation: 0,
            current: "a".into(),
            item: "b".into(),
            index: 1,
            automatic: false,
            cancel: CancellationToken::new(),
        };
        // Even an already completed decode whose cancellation raced completion
        // must not touch a replacement session or reserve a publication.
        photos.stop(None);
        let mut replacement = session();
        replacement.id = "replacement".into();
        photos.session = Some(replacement);
        let before = photos.snapshot();
        assert_eq!(
            finish_navigation(
                &mut photos,
                &state,
                work,
                Ok(Some(cached(dir.path(), &state.resources)))
            )
            .err()
            .unwrap()
            .code,
            "stale_state"
        );
        assert_eq!(photos.snapshot(), before);
        assert!(photos.published.is_none());
    }
    fn mixed_library() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        {
            let db = state.db.lock().unwrap();
            for (id, availability) in [("source", "available"), ("offline-source", "offline")] {
                db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES(?,'local',x'2f746d70','Photo tests','[]',?)",rusqlite::params![id,availability]).unwrap();
            }
            for (id, source, availability) in [
                ("a", "source", "available"),
                ("missing", "source", "missing"),
                ("offline", "offline-source", "available"),
            ] {
                db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES(?,?,?,?,'photo',1,'1',?)",rusqlite::params![id,source,id.as_bytes(),id,availability]).unwrap();
            }
        }
        (dir, state)
    }
    #[test]
    fn task9_review_still_start_and_adoption_retain_missing_neighbors() {
        let (_dir, state) = mixed_library();
        for neighbor in ["missing", "offline"] {
            let ids = vec!["a".to_string(), neighbor.to_string()];
            let request = SlideshowRequest {
                media_ids: ids.clone(),
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            };
            let (queue, source_ids) = prepare(&state, request, true)
                .expect("available still must not depend on its neighbor");
            assert_eq!(queue.ids, ids);
            assert!(queue.paused);
            let mut singleton = session();
            singleton.selection = None;
            singleton.queue = Queue::new(SlideshowRequest {
                media_ids: vec!["a".into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            })
            .unwrap();
            singleton.queue.pause(true);
            let mut photos = State {
                session: Some(singleton),
                ..Default::default()
            };
            assert!(photos
                .select("owned-selection", "a", true, 5, Some(&queue.ids))
                .unwrap());
            photos.session.as_mut().unwrap().sources = source_ids;
            assert_eq!(
                photos.snapshot().session_id.as_deref(),
                Some("test-session")
            );
            assert_eq!(photos.session.as_ref().unwrap().queue.ids, ids);
            assert!(
                !source_missing(&state, photos.session.as_ref().unwrap()),
                "offline neighbor must not expire current photo"
            );
        }
    }
    #[test]
    fn task9_review_offline_neighbor_does_not_expire_current_still() {
        let (_dir, state) = mixed_library();
        let (queue, source_ids) = prepare(
            &state,
            SlideshowRequest {
                media_ids: vec!["a".into(), "offline".into()],
                monitor_id: None,
                interval_seconds: 5,
                shuffle: false,
            },
            true,
        )
        .unwrap();
        let mut current = session();
        current.queue = queue;
        current.sources = source_ids;
        assert!(
            !source_missing(&state, &current),
            "only the current source owns the presentation lifetime"
        );
    }
    fn session() -> Session {
        Session {
            selection: Some("selected-order".into()),
            generation: 0,
            navigation: None,
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
            failed_preloads: Default::default(),
            message: None,
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
                relative: "image.jpg".into(),
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
