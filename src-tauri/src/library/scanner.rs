use crate::contracts::AppError;
pub fn finish_scan(
    db: &rusqlite::Connection,
    source: &str,
    scan: &str,
    complete: bool,
    cancelled: bool,
) -> Result<(), AppError> {
    if complete && !cancelled {
        db.execute("UPDATE media_items SET availability='missing' WHERE source_id=?1 AND (scan_id IS NULL OR scan_id<>?2)",rusqlite::params![source,scan])?;
        db.execute(
            "UPDATE media_sources SET scanned_at_ms=?1 WHERE id=?2",
            rusqlite::params![crate::app_state::now_ms() as i64, source],
        )?;
    }
    Ok(())
}
pub fn invalidate_changed(
    db: &rusqlite::Connection,
    id: &str,
    size: i64,
    mtime: &str,
) -> Result<(), AppError> {
    let changed: bool = db.query_row(
        "SELECT size<>?1 OR mtime_ns<>?2 FROM media_items WHERE id=?3",
        rusqlite::params![size, mtime, id],
        |r| r.get(0),
    )?;
    if changed {
        db.execute("DELETE FROM thumbnail_entries WHERE media_id=?", [id])?;
        db.execute("UPDATE media_items SET thumbnail_id=NULL WHERE id=?", [id])?;
        db.execute("DELETE FROM resume_positions WHERE media_id=?", [id])?;
    }
    Ok(())
}

use crate::{app_state::AppState, contracts::MediaKind};
use rusqlite::{params, OptionalExtension};
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::PathBuf,
};
use tokio_util::sync::CancellationToken;
struct Candidate {
    relative: PathBuf,
    kind: MediaKind,
    file: std::fs::File,
    size: i64,
    mtime: String,
}
struct ScanLease {
    state: AppState,
    id: String,
}
impl Drop for ScanLease {
    fn drop(&mut self) {
        if let Ok(mut scans) = self.state.scans.lock() {
            scans.remove(&self.id);
        }
    }
}
fn enumerate(
    root: PathBuf,
    kinds: Vec<MediaKind>,
    send: tokio::sync::mpsc::Sender<Candidate>,
    cancel: CancellationToken,
) -> Result<bool, AppError> {
    let device = std::fs::metadata(&root)?.dev();
    let mut stack = vec![std::fs::read_dir(&root)?];
    let mut complete = true;
    while let Some(entries) = stack.last_mut() {
        if cancel.is_cancelled() {
            return Ok(false);
        }
        let Some(entry) = entries.next() else {
            stack.pop();
            continue;
        };
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        let metadata = match entry.path().symlink_metadata() {
            Ok(m) => m,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.dev() != device {
            continue;
        }
        if metadata.is_dir() {
            if stack.len() >= 128 {
                complete = false;
                continue;
            }
            match std::fs::read_dir(entry.path()) {
                Ok(entries) => stack.push(entries),
                Err(_) => complete = false,
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(kind) = super::probe::candidate(&entry.path()).filter(|k| kinds.contains(k))
        else {
            continue;
        };
        let relative = entry
            .path()
            .strip_prefix(&root)
            .map_err(|_| AppError::new("source_escape", "A file escaped its selected source."))?
            .to_path_buf();
        let file = match super::files::open(&root, &relative) {
            Ok(f) => f,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        let metadata = file.metadata()?;
        let size = i64::try_from(metadata.len()).map_err(|_| {
            AppError::new("file_too_large", "This file exceeds the supported size.")
        })?;
        let mtime = format!("{}:{}", metadata.mtime(), metadata.mtime_nsec());
        if send
            .blocking_send(Candidate {
                relative,
                kind,
                file,
                size,
                mtime,
            })
            .is_err()
        {
            return Ok(false);
        }
    }
    Ok(complete)
}
fn write_item(
    db: &rusqlite::Connection,
    source: &str,
    scan: &str,
    candidate: &Candidate,
    metadata: &Result<super::probe::Metadata, AppError>,
) -> Result<(), AppError> {
    let id: Option<String> = db
        .query_row(
            "SELECT id FROM media_items WHERE source_id=?1 AND relative_path=?2",
            params![source, candidate.relative.as_os_str().as_bytes()],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = &id {
        invalidate_changed(db, id, candidate.size, &candidate.mtime)?;
    }
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let kind = serde_json::to_value(candidate.kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let name = candidate
        .relative
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let metadata = metadata.as_ref().ok();
    db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,duration_ms,width,height,metadata_json,availability,scan_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(source_id,relative_path) DO UPDATE SET display_name=excluded.display_name,size=excluded.size,mtime_ns=excluded.mtime_ns,duration_ms=excluded.duration_ms,width=excluded.width,height=excluded.height,metadata_json=excluded.metadata_json,availability=excluded.availability,scan_id=excluded.scan_id",params![id,source,candidate.relative.as_os_str().as_bytes(),name,kind,candidate.size,candidate.mtime,metadata.and_then(|m|m.duration_ms).map(|d|d as i64),metadata.and_then(|m|m.width),metadata.and_then(|m|m.height),metadata.map(|m|m.tags.to_string()).unwrap_or_else(||"{}".into()),if metadata.is_some(){"available"}else{"unsupported"},scan])?;
    Ok(())
}
enum Update {
    Touch(String),
    Probed(Candidate, Result<super::probe::Metadata, AppError>),
}
fn flush(
    state: &AppState,
    source: &str,
    scan: &str,
    updates: &mut Vec<Update>,
) -> Result<(), AppError> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
    let transaction = db.transaction()?;
    for update in updates.iter() {
        match update {
            Update::Touch(id) => {
                transaction.execute(
                    "UPDATE media_items SET scan_id=?1 WHERE id=?2",
                    params![scan, id],
                )?;
            }
            Update::Probed(candidate, metadata) => {
                write_item(&transaction, source, scan, candidate, metadata)?
            }
        }
    }
    transaction.commit()?;
    updates.clear();
    Ok(())
}
pub async fn scan(
    app: tauri::AppHandle,
    state: AppState,
    source: String,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    {
        let mut scans = state
            .scans
            .lock()
            .map_err(|_| AppError::new("state_error", "Scans are unavailable."))?;
        if scans.contains_key(&source) {
            return Err(AppError::new(
                "scan_busy",
                "This source is already being scanned.",
            ));
        }
        if scans.len() >= 2 {
            return Err(AppError::new(
                "scan_limit",
                "Two scans are already running. Wait for one to finish.",
            ));
        }
        scans.insert(source.clone(), cancel.clone());
    }
    let _lease = ScanLease {
        state: state.clone(),
        id: source.clone(),
    };
    let _resource = state.resources.acquire(&source, cancel.clone())?;
    let devices = crate::devices::udisks::discover().await.unwrap_or_default();
    let record = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        super::repository::get(&db, &source)?
    };
    let root = super::repository::root(&record, &devices)?;
    super::confirmation::check(&state, &record, &root)?;
    let initial = std::fs::metadata(&root)?;
    let (send, mut receive) = tokio::sync::mpsc::channel(32);
    let path = root.clone();
    let token = cancel.clone();
    let enumerator =
        tokio::task::spawn_blocking(move || enumerate(path, record.kinds, send, token));
    let scan = uuid::Uuid::new_v4().to_string();
    let mut offline = false;
    let mut last_event = std::time::Instant::now();
    let mut processed = 0u64;
    let mut updates = Vec::with_capacity(16);
    while let Some(candidate) =
        tokio::select! {_=cancel.cancelled()=>None,value=receive.recv()=>value}
    {
        if !std::fs::metadata(&root)
            .is_ok_and(|m| m.dev() == initial.dev() && m.ino() == initial.ino())
        {
            offline = true;
            break;
        }
        let unchanged = {
            let db = state
                .db
                .lock()
                .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
            let existing:Option<String>=db.query_row("SELECT id FROM media_items WHERE source_id=?1 AND relative_path=?2 AND size=?3 AND mtime_ns=?4 AND availability='available'",params![source,candidate.relative.as_os_str().as_bytes(),candidate.size,candidate.mtime],|r|r.get(0)).optional()?;
            existing
        };
        if let Some(id) = unchanged {
            updates.push(Update::Touch(id));
        } else {
            let permit = tokio::select! {_=cancel.cancelled()=>break,permit=state.probes.acquire()=>permit.map_err(|_|AppError::new("scan_unavailable","Media inspection is unavailable."))?};
            let result =
                super::probe::inspect(&candidate.file, candidate.kind, cancel.clone()).await;
            drop(permit);
            if cancel.is_cancelled() {
                break;
            }
            if !std::fs::metadata(&root)
                .is_ok_and(|m| m.dev() == initial.dev() && m.ino() == initial.ino())
            {
                offline = true;
                break;
            }
            updates.push(Update::Probed(candidate, result));
        }
        if updates.len() >= 16 {
            flush(&state, &source, &scan, &mut updates)?;
        }
        processed += 1;
        if last_event.elapsed() >= std::time::Duration::from_millis(250) {
            flush(&state, &source, &scan, &mut updates)?;
            let _ = crate::events::publish(&app, &state, |home| {
                home.summaries.drives = format!("Scanning: {processed} files checked")
            });
            last_event = std::time::Instant::now();
        }
    }
    drop(receive);
    flush(&state, &source, &scan, &mut updates)?;
    let complete = enumerator
        .await
        .map_err(|_| AppError::new("scan_failed", "The folder scan stopped unexpectedly."))??;
    offline |= !std::fs::metadata(&root)
        .is_ok_and(|m| m.dev() == initial.dev() && m.ino() == initial.ino());
    {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        if offline {
            db.execute(
                "UPDATE media_sources SET availability='offline' WHERE id=?",
                [&source],
            )?;
        }
        finish_scan(
            &db,
            &source,
            &scan,
            complete && !offline,
            cancel.is_cancelled(),
        )?;
    }
    publish_summaries(&app, &state)?;
    if cancel.is_cancelled() {
        return Err(AppError::new(
            "cancelled",
            "The scan was cancelled. Existing library records were retained.",
        ));
    }
    if offline {
        return Err(AppError::new(
            "source_offline",
            "The source disconnected during scanning. Existing records were retained.",
        ));
    }
    if !complete {
        return Err(AppError::new(
            "scan_partial",
            "Some folders or files could not be read. Existing records were retained.",
        ));
    }
    Ok(())
}
pub fn publish_summaries(app: &tauri::AppHandle, state: &AppState) -> Result<(), AppError> {
    let counts = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        let count = |kind: &str| {
            db.query_row(
                "SELECT count(*) FROM media_items WHERE kind=?",
                [kind],
                |r| r.get::<_, i64>(0),
            )
        };
        (count("video")?, count("audio")?, count("photo")?)
    };
    crate::events::publish(app, state, |home| {
        home.summaries.video = format!("{} videos in library", counts.0);
        home.summaries.audio = format!("{} tracks in library", counts.1);
        home.summaries.photo = format!("{} photos in library", counts.2);
        home.summaries.drives = "Manage selected folders and drives".into();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::database::open(&dir.path().join("db")).unwrap();
        db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('s','local',x'2f746d70','test','[]','available')",[]).unwrap();
        db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability,scan_id,thumbnail_id) VALUES('m','s',x'66','f','video',1,'1','available','old','thumb')",[]).unwrap();
        (dir, db)
    }
    #[test]
    fn cancelled_scan_does_not_mark_files_missing() {
        let (_dir, db) = fixture();
        finish_scan(&db, "s", "new", true, true).unwrap();
        assert_eq!(
            db.query_row("SELECT availability FROM media_items", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "available"
        );
        finish_scan(&db, "s", "new", true, false).unwrap();
        assert_eq!(
            db.query_row("SELECT availability FROM media_items", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "missing"
        );
    }
    #[test]
    fn incomplete_scan_does_not_mark_files_missing() {
        let (_dir, db) = fixture();
        finish_scan(&db, "s", "new", false, false).unwrap();
        assert_eq!(
            db.query_row("SELECT availability FROM media_items", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "available"
        );
    }
    #[test]
    fn changed_file_invalidates_thumbnail() {
        let (_dir, db) = fixture();
        invalidate_changed(&db, "m", 2, "1").unwrap();
        assert!(db
            .query_row("SELECT thumbnail_id FROM media_items", [], |r| r
                .get::<_, Option<String>>(0))
            .unwrap()
            .is_none());
    }
}
