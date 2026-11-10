//! Typed native media adapters. No paths or arbitrary command dispatch cross the wire.
use super::{dispatch::native_error, protocol::*};
use crate::control::ControlServices;
use std::collections::HashMap;
#[derive(Default)]
pub struct MediaState {
    pub(crate) selections: HashMap<String, Selection>,
    order: Vec<String>,
}
pub(crate) struct Selection {
    pub owner: Id,
    pub ids: Vec<String>,
}
use super::adapters::{checked, queue_entry_id};
use rusqlite::Connection;
fn error(code: ErrorCode) -> RemoteError {
    RemoteError::new(
        code,
        "The requested media is unavailable. Refresh its state.",
    )
}
fn wire(value: impl serde::Serialize) -> Result<serde_json::Value, RemoteError> {
    serde_json::to_value(value).map_err(|_| RemoteError::invalid())
}
pub(crate) fn item(db: &Connection, id: &str) -> Result<MediaItem, RemoteError> {
    let native = db.query_row("SELECT m.id,m.source_id,m.kind,m.display_name,CASE WHEN s.availability='available' THEN m.availability ELSE s.availability END,m.duration_ms,m.thumbnail_id,(SELECT position_ms FROM resume_positions WHERE media_id=m.id AND completed=0 AND position_ms>=10000) FROM media_items m JOIN media_sources s ON s.id=m.source_id WHERE m.id=?",[id],|r| {
        let kind:String=r.get(2)?;
        Ok(crate::contracts::MediaItem { id:r.get(0)?,source_id:r.get(1)?,kind:match kind.as_str(){"audio"=>crate::contracts::MediaKind::Audio,"photo"=>crate::contracts::MediaKind::Photo,_=>crate::contracts::MediaKind::Video},display_name:r.get(3)?,availability:r.get(4)?,duration_ms:r.get::<_,Option<i64>>(5)?.and_then(|v|u64::try_from(v).ok()),thumbnail_id:r.get(6)?,resume_position_ms:r.get::<_,Option<i64>>(7)?.and_then(|v|u64::try_from(v).ok()),modified_at_ms:None })
    }).map_err(|e| if matches!(e,rusqlite::Error::QueryReturnedNoRows){error(ErrorCode::NotFound)}else{native_error(e.into())})?;
    MediaItem::try_from(&native)
}
fn bounds(
    cursor: Option<&Cursor>,
    resource: &str,
    revision: u64,
    count: usize,
    limit: Option<&PageLimit>,
) -> Result<(usize, usize, Option<Cursor>), RemoteError> {
    let offset = if let Some(cursor) = cursor {
        let (key, rev, offset): (String, u64, usize) =
            serde_json::from_str(cursor.as_str()).map_err(|_| RemoteError::invalid())?;
        if key != resource || rev != revision {
            return Err(error(ErrorCode::StaleState));
        }
        offset
    } else {
        0
    };
    if offset > count {
        return Err(RemoteError::invalid());
    }
    let end = (offset + limit.map_or(50, |l| l.value() as usize)).min(count);
    let next = (end < count)
        .then(|| checked(serde_json::to_string(&(resource, revision, end)).expect("cursor")))
        .transpose()?;
    Ok((offset, end, next))
}
pub(crate) fn selection(
    services: &ControlServices,
    owner: &Id,
    id: &Id,
) -> Result<Vec<String>, RemoteError> {
    services
        .media
        .lock()
        .map_err(|_| error(ErrorCode::Unavailable))?
        .selections
        .get(id.as_str())
        .filter(|s| s.owner == *owner)
        .map(|s| s.ids.clone())
        .ok_or_else(|| error(ErrorCode::NotFound))
}
pub async fn read(
    services: &ControlServices,
    owner: &RemotePrincipal,
    command: &RemoteCommand,
) -> Result<serde_json::Value, RemoteError> {
    use crate::music::playlists;
    match command {
        RemoteCommand::LibraryItem(p) => wire(item(
            &*services
                .state
                .db
                .lock()
                .map_err(|_| error(ErrorCode::Unavailable))?,
            p.item_id.as_str(),
        )?),
        RemoteCommand::LibraryQuery(p) => {
            let db = services
                .state
                .db
                .lock()
                .map_err(|_| error(ErrorCode::Unavailable))?;
            if p.kind == MediaKind::Photo {
                let (id, offset) = if let Some(cursor) = &p.cursor {
                    let (id, offset): (Id, usize) = serde_json::from_str(cursor.as_str())
                        .map_err(|_| RemoteError::invalid())?;
                    (id, offset)
                } else {
                    let mut ids = Vec::new();
                    let mut cursor = None;
                    loop {
                        let page = crate::library::query::page(
                            &db,
                            crate::contracts::MediaKind::Photo,
                            p.source_id.as_ref().map(|v| v.as_str()),
                            p.query.as_ref().map_or("", |v| v.as_str()),
                            cursor.as_deref(),
                            100,
                        )
                        .map_err(native_error)?;
                        ids.extend(page.items.into_iter().map(|i| i.id));
                        if ids.len() > 10000 {
                            return Err(RemoteError::new(
                                ErrorCode::SelectionTooLarge,
                                "Choose at most 10,000 photos.",
                            ));
                        }
                        cursor = page.next_cursor;
                        if cursor.is_none() {
                            break;
                        }
                    }
                    let id: Id = checked(uuid::Uuid::new_v4().to_string())?;
                    let mut state = services
                        .media
                        .lock()
                        .map_err(|_| error(ErrorCode::Unavailable))?;
                    // Keep four recent selections per phone and cap global storage.
                    let own = state
                        .order
                        .iter()
                        .filter(|id| {
                            state
                                .selections
                                .get(*id)
                                .is_some_and(|s| s.owner == owner.device_id)
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if own.len() >= 4 {
                        state.selections.remove(&own[0]);
                        state.order.retain(|v| v != &own[0]);
                    }
                    if state.selections.len() >= 64 {
                        return Err(error(ErrorCode::Busy));
                    }
                    state.order.push(id.as_str().into());
                    state.selections.insert(
                        id.as_str().into(),
                        Selection {
                            owner: owner.device_id.clone(),
                            ids,
                        },
                    );
                    (id, 0)
                };
                let ids = selection(services, &owner.device_id, &id)?;
                if offset > ids.len() {
                    return Err(RemoteError::invalid());
                }
                let end =
                    (offset + p.limit.as_ref().map_or(50, |v| v.value() as usize)).min(ids.len());
                let items = ids[offset..end]
                    .iter()
                    .map(|id| item(&db, id))
                    .collect::<Result<Vec<_>, _>>()?;
                wire(MediaPage {
                    items: checked(items)?,
                    selection_id: Some(id.clone()),
                    next_cursor: (end < ids.len())
                        .then(|| checked(serde_json::to_string(&(id, end)).expect("cursor")))
                        .transpose()?,
                })
            } else {
                let page = crate::library::query::page(
                    &db,
                    p.kind.clone().into(),
                    p.source_id.as_ref().map(|v| v.as_str()),
                    p.query.as_ref().map_or("", |v| v.as_str()),
                    p.cursor.as_ref().map(|v| v.as_str()),
                    p.limit.as_ref().map_or(50, |v| v.value()),
                )
                .map_err(native_error)?;
                wire(MediaPage {
                    items: checked(
                        page.items
                            .iter()
                            .map(MediaItem::try_from)
                            .collect::<Result<Vec<_>, _>>()?,
                    )?,
                    next_cursor: page.next_cursor.map(checked).transpose()?,
                    selection_id: None,
                })
            }
        }
        RemoteCommand::QueueGet(p) => {
            let queue = crate::music::service::get(&services.state)
                .await
                .map_err(native_error)?;
            let (start, end, next) = bounds(
                p.cursor.as_ref(),
                "queue",
                queue.revision.into(),
                queue.items.len(),
                p.limit.as_ref(),
            )?;
            let db = services
                .state
                .db
                .lock()
                .map_err(|_| error(ErrorCode::Unavailable))?;
            let items = queue.items[start..end]
                .iter()
                .map(|e| {
                    Ok(QueueEntry {
                        entry_id: queue_entry_id(e.entry_id),
                        item: item(&db, &e.item.media_id)?,
                    })
                })
                .collect::<Result<Vec<_>, RemoteError>>()?;
            wire(QueuePage {
                revision: u64::from(queue.revision).into(),
                items: checked(items)?,
                next_cursor: next,
            })
        }
        RemoteCommand::PlaylistsList(p) => {
            let db = services
                .state
                .db
                .lock()
                .map_err(|_| error(ErrorCode::Unavailable))?;
            let list = playlists::list(&db).map_err(native_error)?;
            // Bind the paging cursor to every row's revision and identity.
            use sha2::{Digest, Sha256};
            let mut digest = Sha256::new();
            for row in &list {
                digest.update(row.id.as_bytes());
                digest.update(
                    playlists::revision(&db, &row.id)
                        .map_err(native_error)?
                        .revision
                        .value()
                        .to_le_bytes(),
                );
            }
            let revision = u64::from_le_bytes(digest.finalize()[..8].try_into().expect("digest"));
            let (start, end, next) = bounds(
                p.cursor.as_ref(),
                "playlists",
                revision,
                list.len(),
                p.limit.as_ref(),
            )?;
            let items = list[start..end]
                .iter()
                .map(|p| {
                    Ok(Playlist {
                        id: checked(&p.id)?,
                        name: checked(&p.name)?,
                        revision: playlists::revision(&db, &p.id)
                            .map_err(native_error)?
                            .revision,
                        count: u64::from(p.item_count).into(),
                    })
                })
                .collect::<Result<Vec<_>, RemoteError>>()?;
            wire(PlaylistPage {
                items: checked(items)?,
                next_cursor: next,
            })
        }
        RemoteCommand::PlaylistsGet(p) => {
            let db = services
                .state
                .db
                .lock()
                .map_err(|_| error(ErrorCode::Unavailable))?;
            let list = playlists::get(&db, p.playlist_id.as_str()).map_err(native_error)?;
            let revision = playlists::revision(&db, &list.id)
                .map_err(native_error)?
                .revision;
            let (start, end, next) = bounds(
                p.cursor.as_ref(),
                &list.id,
                revision.value(),
                list.items.len(),
                p.limit.as_ref(),
            )?;
            let items = list.items[start..end]
                .iter()
                .enumerate()
                .map(|(n, i)| {
                    Ok(QueueEntry {
                        entry_id: checked(format!(
                            "playlist-entry-{}-{}-{}",
                            list.id,
                            revision.value(),
                            start + n
                        ))?,
                        item: item(&db, &i.media_id)?,
                    })
                })
                .collect::<Result<Vec<_>, RemoteError>>()?;
            wire(PlaylistContents {
                playlist: Playlist {
                    id: checked(&list.id)?,
                    name: checked(&list.name)?,
                    revision: revision.clone(),
                    count: (list.items.len() as u64).into(),
                },
                entries: QueuePage {
                    revision,
                    items: checked(items)?,
                    next_cursor: next,
                },
            })
        }
        _ => Err(error(ErrorCode::Unsupported)),
    }
}

pub(crate) async fn save_playlist(
    services: &ControlServices,
    params: &PlaylistsSaveParams,
    expected: &Expected,
) -> Result<(), crate::contracts::AppError> {
    use crate::{contracts::AppError, music::playlists};
    let invalid = || AppError::new("invalid_request", "Refresh the playlist before editing.");
    if let PlaylistsSaveParams::SaveQueue { name, playlist_id } = params {
        let _gate = services.state.music_gate.lock().await;
        let queue = services.state.music.lock().await;
        queue.validate(
            expected
                .queue_revision
                .as_ref()
                .ok_or_else(invalid)?
                .value()
                .try_into()
                .map_err(|_| invalid())?,
        )?;
        let ids = queue
            .snapshot()
            .items
            .into_iter()
            .map(|e| e.item.media_id)
            .collect::<Vec<_>>();
        let mut db = services.state.db.lock().map_err(|_| invalid())?;
        playlists::save_if_revision(
            &mut db,
            playlist_id.as_ref().map(|v| v.as_str()),
            expected.playlist_revision.as_ref().map(|v| v.value()),
            name.as_str(),
            &ids,
        )?;
        return Ok(());
    }
    let id = match params {
        PlaylistsSaveParams::Append { playlist_id, .. }
        | PlaylistsSaveParams::Remove { playlist_id, .. }
        | PlaylistsSaveParams::Rename { playlist_id, .. } => playlist_id,
        _ => unreachable!(),
    };
    let revision = expected
        .playlist_revision
        .as_ref()
        .ok_or_else(invalid)?
        .value();
    let mut db = services.state.db.lock().map_err(|_| invalid())?;
    playlists::edit_if_revision(&mut db, id.as_str(), revision, |list| {
        let mut name = list.name;
        let mut ids = list
            .items
            .into_iter()
            .map(|i| i.media_id)
            .collect::<Vec<_>>();
        match params {
            PlaylistsSaveParams::Append { item_ids, .. } => {
                ids.extend(item_ids.as_slice().iter().map(|i| i.as_str().into()))
            }
            PlaylistsSaveParams::Rename { name: new_name, .. } => name = new_name.as_str().into(),
            PlaylistsSaveParams::Remove { entry_ids, .. } => {
                let prefix = format!("playlist-entry-{}-{revision}-", id.as_str());
                let mut indices = std::collections::HashSet::new();
                for entry in entry_ids.as_slice() {
                    let raw = entry.as_str().strip_prefix(&prefix).ok_or_else(invalid)?;
                    let index = raw.parse::<usize>().map_err(|_| invalid())?;
                    if index >= ids.len() || index.to_string() != raw {
                        return Err(invalid());
                    }
                    indices.insert(index);
                }
                ids = ids
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !indices.contains(i))
                    .map(|(_, id)| id)
                    .collect();
            }
            _ => unreachable!(),
        }
        Ok((name, ids))
    })?;
    Ok(())
}

pub(crate) fn mutation(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::PlayerPlay(_)
            | RemoteCommand::PlayerSeek(_)
            | RemoteCommand::PlayerVolume(_)
            | RemoteCommand::PlayerMute(_)
            | RemoteCommand::PlayerAudioTrack(_)
            | RemoteCommand::PlayerSubtitleTrack(_)
            | RemoteCommand::PlayerStop(_)
            | RemoteCommand::QueueEnqueue(_)
            | RemoteCommand::QueueRemove(_)
            | RemoteCommand::QueueMove(_)
            | RemoteCommand::QueuePlayEntry(_)
            | RemoteCommand::QueueNext(_)
            | RemoteCommand::QueuePrevious(_)
            | RemoteCommand::QueueShuffle(_)
            | RemoteCommand::QueueRepeat(_)
            | RemoteCommand::PlaylistsSave(_)
            | RemoteCommand::PlaylistsDelete(_)
            | RemoteCommand::PlaylistsPlay(_)
            | RemoteCommand::PhotosShow(_)
            | RemoteCommand::PhotosSlideshowStart(_)
            | RemoteCommand::PhotosNext(_)
            | RemoteCommand::PhotosPrevious(_)
            | RemoteCommand::PhotosPause(_)
            | RemoteCommand::PhotosInterval(_)
            | RemoteCommand::PhotosShuffle(_)
            | RemoteCommand::PhotosFit(_)
            | RemoteCommand::PhotosStop(_)
    )
}
pub(crate) fn normalize(mut error: crate::contracts::AppError) -> crate::contracts::AppError {
    error.code = match error.code.as_str() {
        "player_missing"
        | "queue_stale"
        | "queue_session_stale"
        | "session_switch_required"
        | "session_active"
        | "slideshow_active"
        | "slideshow_inactive" => "stale_state",
        "playlist_missing" | "media_missing" => "not_found",
        "invalid_player_action"
        | "invalid_media_kind"
        | "queue_index"
        | "playlist_name"
        | "invalid_slideshow"
        | "invalid_interval" => "invalid_request",
        "session_busy" | "source_busy" | "resource_limit" => "busy",
        "ownership_uncertain" => "identity_changed",
        "sink_unavailable" => "audio_output_unavailable",
        "audio_unavailable"
        | "audio_unrecognized"
        | "audio_request_failed"
        | "audio_route_unconfirmed"
        | "audio_stream_changed"
        | "audio_stream_pending" => "unavailable",
        "media_unavailable"
        | "source_offline"
        | "volume_unmounted"
        | "volume_offline"
        | "media_changed"
        | "file_unavailable"
        | "player_unavailable"
        | "photo_unavailable"
        | "photo_decode_failed"
        | "unsupported_photo"
        | "image_too_large"
        | "presentation_unavailable"
        | "player_load_failed"
        | "player_start_failed" => "unavailable",
        other => other,
    }
    .into();
    error
}
fn app_error(error: RemoteError) -> crate::contracts::AppError {
    crate::contracts::AppError::new(
        serde_json::to_value(error.code)
            .expect("error")
            .as_str()
            .expect("code"),
        "The media action could not be completed.",
    )
}
fn require_app(
    services: &ControlServices,
) -> Result<&tauri::AppHandle, crate::contracts::AppError> {
    services.app().ok_or_else(|| {
        crate::contracts::AppError::new("unavailable", "The native application is unavailable.")
    })
}
fn player_action(
    command: &RemoteCommand,
) -> Result<Option<(String, crate::contracts::PlayerAction)>, crate::contracts::AppError> {
    let (id, action) = match command {
        RemoteCommand::PlayerSeek(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerSeek { params: p.clone() },
        ),
        RemoteCommand::PlayerVolume(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerVolume { params: p.clone() },
        ),
        RemoteCommand::PlayerMute(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerMute { params: p.clone() },
        ),
        RemoteCommand::PlayerAudioTrack(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerAudioTrack { params: p.clone() },
        ),
        RemoteCommand::PlayerSubtitleTrack(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerSubtitleTrack { params: p.clone() },
        ),
        RemoteCommand::PlayerStop(p) => (
            p.session_id.clone(),
            PlayerAction::PlayerStop { params: p.clone() },
        ),
        _ => return Ok(None),
    };
    Ok(Some((
        id.as_str().into(),
        action.try_into().map_err(app_error)?,
    )))
}
pub(crate) async fn validate(
    services: &ControlServices,
    owner: &RemotePrincipal,
    request: &RemoteRequest,
) -> Result<(), crate::contracts::AppError> {
    use crate::contracts::AppError;
    if let Some((id, action)) = player_action(&request.command)? {
        let player = services.state.player.lock().await;
        let player = player
            .as_ref()
            .filter(|p| p.session.id == id)
            .ok_or_else(|| AppError::new("stale_state", "The player session changed."))?;
        crate::control::playback::validate(&player.snapshot, &action)?;
    }
    if let RemoteCommand::PhotosShow(p) = &request.command {
        let ids = selection(services, &owner.device_id, &p.selection_id).map_err(app_error)?;
        if !ids.iter().any(|id| id == p.item_id.as_str()) {
            return Err(AppError::new(
                "not_found",
                "The photo is outside the selection.",
            ));
        }
    }
    if let RemoteCommand::PhotosSlideshowStart(p) = &request.command {
        selection(services, &owner.device_id, &p.selection_id).map_err(app_error)?;
    }
    Ok(())
}
pub(crate) async fn execute(
    services: ControlServices,
    owner: RemotePrincipal,
    request: RemoteRequest,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::contracts::AppError> {
    use crate::{
        contracts::AppError,
        control::playback::QueueAction,
        music::queue::MusicAction,
        photos::presenter::{self, SlideshowAction},
    };
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Media action cancelled."));
    }
    if let Some((id, action)) = player_action(&request.command)? {
        return crate::player::mpv::control(require_app(&services)?, &services.state, &id, action)
            .await;
    }
    let expected = request
        .expected
        .as_ref()
        .ok_or_else(|| AppError::new("invalid_request", "Expected state is required."))?;
    let queue_action = match &request.command {
        RemoteCommand::QueueEnqueue(p) => Some(QueueAction::Native(MusicAction::Enqueue {
            media_ids: p
                .item_ids
                .as_slice()
                .iter()
                .map(|v| v.as_str().into())
                .collect(),
        })),
        RemoteCommand::QueueRemove(p) => Some(QueueAction::Remove(
            super::adapters::native_queue_entry_id(&p.entry_id).map_err(app_error)?,
        )),
        RemoteCommand::QueueMove(p) => Some(QueueAction::Move {
            entry: super::adapters::native_queue_entry_id(&p.entry_id).map_err(app_error)?,
            before: p
                .before_entry_id
                .as_ref()
                .map(super::adapters::native_queue_entry_id)
                .transpose()
                .map_err(app_error)?,
        }),
        RemoteCommand::QueuePlayEntry(p) => Some(QueueAction::Play(
            super::adapters::native_queue_entry_id(&p.entry_id).map_err(app_error)?,
        )),
        RemoteCommand::QueueNext(_) => Some(QueueAction::Native(MusicAction::Next)),
        RemoteCommand::QueuePrevious(_) => Some(QueueAction::Native(MusicAction::Previous)),
        RemoteCommand::QueueShuffle(p) => Some(QueueAction::Native(MusicAction::SetShuffle {
            enabled: p.enabled,
        })),
        RemoteCommand::QueueRepeat(p) => Some(QueueAction::Native(MusicAction::SetRepeat {
            mode: match p.mode {
                RepeatMode::Off => crate::music::queue::RepeatMode::Off,
                RepeatMode::One => crate::music::queue::RepeatMode::One,
                RepeatMode::All => crate::music::queue::RepeatMode::All,
            },
        })),
        _ => None,
    };
    if let Some(action) = queue_action {
        let revision = super::adapters::native_queue_revision(
            expected
                .queue_revision
                .as_ref()
                .ok_or_else(|| AppError::new("invalid_request", "Queue revision required."))?,
        )
        .map_err(app_error)?;
        crate::music::service::control_entry(
            services.app().cloned(),
            services.state.clone(),
            expected.session_id.as_ref().map(|id| id.as_str().into()),
            revision,
            action,
            cancel,
        )
        .await?;
        return Ok(());
    }
    let photo_action = match &request.command {
        RemoteCommand::PhotosNext(p) => Some((&p.session_id, SlideshowAction::Next)),
        RemoteCommand::PhotosPrevious(p) => Some((&p.session_id, SlideshowAction::Previous)),
        RemoteCommand::PhotosPause(p) => {
            Some((&p.session_id, SlideshowAction::Pause { paused: p.paused }))
        }
        RemoteCommand::PhotosInterval(p) => Some((
            &p.session_id,
            SlideshowAction::Interval {
                seconds: p.interval_seconds.value(),
            },
        )),
        RemoteCommand::PhotosShuffle(p) => Some((
            &p.session_id,
            SlideshowAction::Shuffle { enabled: p.enabled },
        )),
        RemoteCommand::PhotosFit(p) => Some((&p.session_id, p.fit.clone().into())),
        RemoteCommand::PhotosStop(p) => Some((&p.session_id, SlideshowAction::Stop)),
        _ => None,
    };
    if let Some((id, action)) = photo_action {
        presenter::control_session(
            require_app(&services)?,
            &services.state,
            Some(id.as_str()),
            action,
        )
        .await?;
        return Ok(());
    }
    match &request.command {
        RemoteCommand::PlayerPlay(p) => {
            let _music = services.state.music_gate.lock().await;
            let _session = services.state.session_gate.lock().await;
            if !crate::control::activity::identity(&services.state)
                .await
                .is_idle()
            {
                return Err(AppError::new(
                    "stale_state",
                    "Prepare an activity replacement first.",
                ));
            }
            crate::player::mpv::play_in_session(
                require_app(&services)?.clone(),
                services.state.clone(),
                p.item_id.as_str().into(),
                p.start == PlaybackStart::Resume,
                cancel,
            )
            .await
        }
        RemoteCommand::PlaylistsSave(p) => save_playlist(&services, p, expected).await,
        RemoteCommand::PlaylistsDelete(p) => {
            let mut db = services
                .state
                .db
                .lock()
                .map_err(|_| AppError::new("unavailable", "Playlists unavailable."))?;
            crate::music::playlists::remove_if_revision(
                &mut db,
                p.playlist_id.as_str(),
                expected
                    .playlist_revision
                    .as_ref()
                    .ok_or_else(|| AppError::new("invalid_request", "Playlist revision required."))?
                    .value(),
            )
        }
        RemoteCommand::PlaylistsPlay(p) => {
            let _gate = services.state.music_gate.lock().await;
            let ids = {
                let db = services
                    .state
                    .db
                    .lock()
                    .map_err(|_| AppError::new("unavailable", "Playlists unavailable."))?;
                crate::music::playlists::get(&db, p.playlist_id.as_str())?
                    .items
                    .into_iter()
                    .map(|i| i.media_id)
                    .collect()
            };
            crate::music::service::play_in_transition(
                require_app(&services)?.clone(),
                services.state.clone(),
                ids,
                0,
                cancel,
                false,
            )
            .await
        }
        RemoteCommand::PhotosShow(_) | RemoteCommand::PhotosSlideshowStart(_) => {
            let (selection_id, start, interval, paused) = match &request.command {
                RemoteCommand::PhotosShow(p) => (&p.selection_id, Some(&p.item_id), 5, true),
                RemoteCommand::PhotosSlideshowStart(p) => (
                    &p.selection_id,
                    p.start_item_id.as_ref(),
                    p.interval_seconds.value(),
                    false,
                ),
                _ => unreachable!(),
            };
            let mut ids =
                selection(&services, &owner.device_id, selection_id).map_err(app_error)?;
            if ids.is_empty() {
                return Err(AppError::new("not_found", "The selection is empty."));
            }
            if let Some(start) = start {
                let index = ids
                    .iter()
                    .position(|id| id == start.as_str())
                    .ok_or_else(|| {
                        AppError::new("not_found", "The photo is outside the selection.")
                    })?;
                ids.rotate_left(index);
            }
            let _session = services.state.session_gate.lock().await;
            let first = crate::library::query::open(&services.state, &ids[0]).await?;
            crate::photos::decode::decode_file(&first.file, cancel).await?;
            presenter::start_in_session(
                require_app(&services)?,
                &services.state,
                crate::photos::model::SlideshowRequest {
                    media_ids: ids,
                    monitor_id: None,
                    interval_seconds: interval,
                    shuffle: false,
                },
                paused,
                Some(selection_id.as_str().into()),
            )
            .await?;
            Ok(())
        }
        _ => Err(AppError::new(
            "unsupported",
            "The media action is unavailable.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_state::AppState,
        remote::{adapters::checked, dispatch::Dispatcher},
    };
    fn fixture(count: usize, kind: &str) -> (tempfile::TempDir, ControlServices, RemotePrincipal) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        {
            let mut db = state.db.lock().unwrap();
            db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('source','local',x'2f746d70','Test','[]','offline')",[]).unwrap();
            let tx = db.transaction().unwrap();
            for n in 0..count {
                tx.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES(?1,'source',?2,?3,?4,1,'1','available')",rusqlite::params![format!("m{n:05}"),format!("file{n}").as_bytes(),format!("Item {n:05}"),kind]).unwrap();
            }
            tx.commit().unwrap();
        }
        let svc = ControlServices::native(state, checked("server").unwrap()).unwrap();
        let principal =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","playback"]}))
                .unwrap();
        (dir, svc, principal)
    }
    fn request(method: &str, params: serde_json::Value) -> RemoteRequest {
        parse_request(
            &serde_json::json!({"v":1,"id":"read","method":method,"params":params}).to_string(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn task9_photo_handoff_rejects_foreign_owner_before_journal_or_native_effect() {
        let (_dir, svc, owner) = fixture(2, "photo");
        let dispatcher = Dispatcher::new(svc.clone());
        let page = dispatcher
            .dispatch(
                &owner,
                request("library.query", serde_json::json!({"kind":"photo"})),
            )
            .await
            .unwrap();
        let foreign =
            checked(serde_json::json!({"deviceId":"other","scopes":["browse","playback"]}))
                .unwrap();
        let req=parse_request(&serde_json::json!({"v":1,"id":"adopt-photo","method":"photos.show","params":{"itemId":"m00000","selectionId":page["selectionId"]},"expected":{"serverEpoch":svc.state.event_hub.epoch()}}).to_string()).unwrap();
        validate(&svc, &owner, &req).await.unwrap();
        assert_eq!(
            dispatcher.dispatch(&foreign, req).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            svc.state
                .db
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM command_journal", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(crate::photos::presenter::snapshot(&svc.state)
            .await
            .session_id
            .is_none());
    }
    #[tokio::test]
    async fn task9_playlist_handles_are_resolved_only_after_revision_cas() {
        let (_dir, svc, _owner) = fixture(3, "audio");
        let id = crate::music::playlists::save(
            &mut svc.state.db.lock().unwrap(),
            None,
            "List",
            &["m00000".into(), "m00001".into(), "m00002".into()],
        )
        .unwrap();
        let old = crate::music::playlists::revision(&svc.state.db.lock().unwrap(), &id)
            .unwrap()
            .revision;
        crate::music::playlists::save(
            &mut svc.state.db.lock().unwrap(),
            Some(&id),
            "List",
            &["m00002".into(), "m00001".into(), "m00000".into()],
        )
        .unwrap();
        let params=checked(serde_json::json!({"action":"remove","playlistId":id,"entryIds":[format!("playlist-entry-{id}-{}-0",old.value())]})).unwrap();
        let expected = checked(
            serde_json::json!({"serverEpoch":svc.state.event_hub.epoch(),"playlistRevision":old}),
        )
        .unwrap();
        assert_eq!(
            save_playlist(&svc, &params, &expected)
                .await
                .unwrap_err()
                .code,
            "stale_state"
        );
        assert_eq!(
            crate::music::playlists::get(&svc.state.db.lock().unwrap(), &id)
                .unwrap()
                .items[0]
                .media_id,
            "m00002"
        );
        let revision = crate::music::playlists::revision(&svc.state.db.lock().unwrap(), &id)
            .unwrap()
            .revision;
        let params=checked(serde_json::json!({"action":"remove","playlistId":id,"entryIds":[format!("playlist-entry-{id}-{}-0",revision.value())]})).unwrap();
        let expected=checked(serde_json::json!({"serverEpoch":svc.state.event_hub.epoch(),"playlistRevision":revision})).unwrap();
        save_playlist(&svc, &params, &expected).await.unwrap();
        assert_eq!(
            crate::music::playlists::get(&svc.state.db.lock().unwrap(), &id)
                .unwrap()
                .items
                .iter()
                .map(|i| i.media_id.as_str())
                .collect::<Vec<_>>(),
            vec!["m00001", "m00000"]
        );
    }
    #[tokio::test]
    async fn task9_queue_read_pages_actual_metadata_and_retains_offline_items() {
        let (_dir, svc, owner) = fixture(101, "audio");
        let ids = (0..101).map(|n| format!("m{n:05}")).collect::<Vec<_>>();
        let items = crate::music::playlists::items(&svc.state.db.lock().unwrap(), &ids).unwrap();
        svc.state.music.lock().await.replace(items, 0).unwrap();
        let dispatcher = Dispatcher::new(svc);
        let first = dispatcher
            .dispatch(
                &owner,
                request("queue.get", serde_json::json!({"limit":100})),
            )
            .await
            .unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 100);
        assert_eq!(first["items"][0]["item"]["availability"], "offline");
        let second = dispatcher
            .dispatch(
                &owner,
                request(
                    "queue.get",
                    serde_json::json!({"cursor":first["nextCursor"],"limit":100}),
                ),
            )
            .await
            .unwrap();
        assert_eq!(second["items"][0]["item"]["id"], "m00100");
        assert!(second.get("nextCursor").is_none());
        assert!(!second.to_string().contains("file100"));
    }
    #[tokio::test]
    async fn task9_photo_selection_is_owner_bound_and_rejects_oversize_without_truncation() {
        let (_dir, svc, owner) = fixture(10001, "photo");
        let dispatcher = Dispatcher::new(svc.clone());
        let req = request(
            "library.query",
            serde_json::json!({"kind":"photo","limit":100}),
        );
        assert_eq!(
            dispatcher
                .dispatch(&owner, req.clone())
                .await
                .unwrap_err()
                .code,
            ErrorCode::SelectionTooLarge
        );
        svc.state
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM media_items WHERE id='m10000'", [])
            .unwrap();
        let first = dispatcher.dispatch(&owner, req).await.unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 100);
        let foreign = checked(serde_json::json!({"deviceId":"other","scopes":["browse"]})).unwrap();
        let req = request(
            "library.query",
            serde_json::json!({"kind":"photo","cursor":first["nextCursor"]}),
        );
        assert_eq!(
            dispatcher.dispatch(&foreign, req).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }
}
