use crate::contracts::*;
use rusqlite::{params, Connection};
pub fn page(
    db: &Connection,
    kind: MediaKind,
    source: Option<&str>,
    query: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<MediaPage, AppError> {
    if query.chars().count() > 200 || cursor.is_some_and(|s| s.len() > 2048) {
        return Err(AppError::new("query_limit", "The search text is too long."));
    }
    let (name, id) = match cursor {
        Some(cursor) => serde_json::from_str::<(String, String)>(cursor)
            .map_err(|_| AppError::new("invalid_cursor", "Restart this library search."))?,
        None => (String::new(), String::new()),
    };
    let kind = serde_json::to_value(kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let limit = limit.clamp(1, 100);
    let pattern = format!(
        "%{}%",
        query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let mut statement=db.prepare(r"SELECT m.id,m.source_id,m.kind,m.display_name,CASE WHEN s.availability='available' THEN m.availability ELSE s.availability END,m.duration_ms,m.thumbnail_id,(SELECT position_ms FROM resume_positions WHERE media_id=m.id AND completed=0 AND position_ms>=10000),m.mtime_ns FROM media_items m JOIN media_sources s ON m.source_id=s.id WHERE m.kind=?1 AND (?2 IS NULL OR m.source_id=?2) AND m.display_name LIKE ?3 ESCAPE '\' AND (m.display_name COLLATE NOCASE,m.id)>(?4 COLLATE NOCASE,?5) ORDER BY m.display_name COLLATE NOCASE,m.id LIMIT ?6")?;
    let mut items = statement
        .query_map(params![kind, source, pattern, name, id, limit + 1], |r| {
            let kind: String = r.get(2)?;
            let duration: Option<i64> = r.get(5)?;
            let resume: Option<i64> = r.get(7)?;
            let mtime: String = r.get(8)?;
            Ok(MediaItem {
                modified_at_ms: mtime
                    .split(':')
                    .next()
                    .and_then(|s| s.parse::<i64>().ok())
                    .and_then(|v| v.checked_mul(1000)),
                resume_position_ms: resume.and_then(|v| u64::try_from(v).ok()),
                id: r.get(0)?,
                source_id: r.get(1)?,
                kind: match kind.as_str() {
                    "audio" => MediaKind::Audio,
                    "photo" => MediaKind::Photo,
                    _ => MediaKind::Video,
                },
                display_name: r.get(3)?,
                availability: r.get(4)?,
                duration_ms: duration.and_then(|v| u64::try_from(v).ok()),
                thumbnail_id: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| serde_json::to_string(&(&item.display_name, &item.id)).unwrap())
    } else {
        None
    };
    Ok(MediaPage { items, next_cursor })
}
pub struct OpenMedia {
    pub lease: super::resources::ResourceLease,
    pub root: std::path::PathBuf,
    pub root_device: u64,
    pub root_inode: u64,
    pub file: std::fs::File,
    pub kind: MediaKind,
    pub source_id: String,
    pub size: i64,
    pub mtime: String,
}
pub async fn open(state: &crate::app_state::AppState, id: &str) -> Result<OpenMedia, AppError> {
    use std::os::unix::{ffi::OsStringExt, fs::MetadataExt};
    let (source, relative, kind, size, mtime, available) = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        db.query_row("SELECT source_id,relative_path,kind,size,mtime_ns,availability FROM media_items WHERE id=?",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(|_|AppError::new("media_missing","This media item is unavailable."))?
    };
    let lease = state
        .resources
        .acquire(&source, tokio_util::sync::CancellationToken::new())?;
    if available != "available" {
        return Err(AppError::new(
            "media_unavailable",
            "This media item is unavailable. Rescan its source to check it again.",
        ));
    }
    let devices = crate::devices::udisks::discover().await.unwrap_or_default();
    let record = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        super::repository::get(&db, &source)?
    };
    let root = super::repository::root(&record, &devices)?;
    super::confirmation::check(state, &record, &root)?;
    let relative = std::path::PathBuf::from(std::ffi::OsString::from_vec(relative));
    tokio::task::spawn_blocking(move || {
        let file = super::files::open(&root, &relative)?;
        let metadata = file.metadata()?;
        if i64::try_from(metadata.len()).ok() != Some(size)
            || format!("{}:{}", metadata.mtime(), metadata.mtime_nsec()) != mtime
        {
            return Err(AppError::new(
                "media_changed",
                "This file has changed. Rescan the source before opening it.",
            ));
        }
        let root_metadata = std::fs::metadata(&root)?;
        Ok(OpenMedia {
            lease,
            root,
            root_device: root_metadata.dev(),
            root_inode: root_metadata.ino(),
            file,
            kind: match kind.as_str() {
                "audio" => MediaKind::Audio,
                "photo" => MediaKind::Photo,
                _ => MediaKind::Video,
            },
            source_id: source,
            size,
            mtime,
        })
    })
    .await
    .map_err(|_| AppError::new("file_unavailable", "The media file could not be opened."))?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ten_thousand_items_are_paged_with_stable_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::storage::database::open(&dir.path().join("db")).unwrap();
        db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('s','local',x'2f746d70','test','[]','available')",[]).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0i32..10000 {
            tx.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES(?1,'s',?2,?3,'video',1,'1','available')",params![format!("id{n:05}"),n.to_le_bytes().as_slice(),format!("Film {n:05}")]).unwrap();
        }
        tx.commit().unwrap();
        let start = std::time::Instant::now();
        let first = page(&db, MediaKind::Video, None, "Film", None, 100).unwrap();
        let second = page(
            &db,
            MediaKind::Video,
            None,
            "Film",
            first.next_cursor.as_deref(),
            100,
        )
        .unwrap();
        assert_eq!(first.items.len(), 100);
        assert_eq!(second.items[0].display_name, "Film 00100");
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
        println!("10,000-item search and two pages: {:?}", start.elapsed());
    }
}
