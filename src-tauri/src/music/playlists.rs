use super::queue::MusicItem;
use crate::contracts::AppError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub item_count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub items: Vec<MusicItem>,
}
pub fn items(db: &Connection, ids: &[String]) -> Result<Vec<MusicItem>, AppError> {
    if ids.len() > 10000 {
        return Err(AppError::new(
            "playlist_limit",
            "A playlist can contain at most 10,000 entries.",
        ));
    }
    let mut statement=db.prepare("SELECT m.display_name,m.metadata_json,CASE WHEN s.availability='available' THEN m.availability ELSE s.availability END,m.kind FROM media_items m JOIN media_sources s ON s.id=m.source_id WHERE m.id=?")?;
    ids.iter()
        .map(|id| {
            let (name, metadata, availability, kind): (String, String, String, String) = statement
                .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .map_err(|_| {
                    AppError::new(
                        "media_missing",
                        "A selected music item is no longer in the library.",
                    )
                })?;
            if kind != "audio" {
                return Err(AppError::new(
                    "invalid_media_kind",
                    "Music queues accept audio files only.",
                ));
            }
            let tags: serde_json::Value = serde_json::from_str(&metadata).unwrap_or_default();
            let tag = |key: &str| {
                tags[key]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.chars().take(2000).collect::<String>())
            };
            Ok(MusicItem {
                media_id: id.clone(),
                title: tag("title").unwrap_or(name),
                artist: tag("artist"),
                album: tag("album"),
                availability,
            })
        })
        .collect()
}
pub fn save(
    db: &mut Connection,
    id: Option<&str>,
    name: &str,
    ids: &[String],
) -> Result<String, AppError> {
    Ok(save_transaction(db, id, None, name, ids)?.playlist_id)
}
fn save_transaction(
    db: &mut Connection,
    id: Option<&str>,
    expected: Option<u64>,
    name: &str,
    ids: &[String],
) -> Result<PlaylistRevision, AppError> {
    if name.trim().is_empty() || name.chars().count() > 200 {
        return Err(AppError::new(
            "playlist_name",
            "Use a playlist name from 1 to 200 characters.",
        ));
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (id, next) = match id {
        Some(id) => {
            let current = revision(&tx, id)?.revision.value();
            if expected.is_some_and(|expected| expected != current) {
                return Err(AppError::new(
                    "stale_state",
                    "This playlist has changed. Refresh it before editing.",
                ));
            }
            let next = i64::try_from(current)
                .ok()
                .and_then(|r| r.checked_add(1))
                .ok_or_else(|| {
                    AppError::new(
                        "revision_exhausted",
                        "This playlist cannot be updated further.",
                    )
                })?;
            (id.to_string(), next)
        }
        None => (uuid::Uuid::new_v4().to_string(), 1),
    };
    // Validate inside the same write transaction as the revision check.
    items(&tx, ids)?;
    let now = crate::app_state::now_ms() as i64;
    tx.execute("INSERT INTO playlists(id,name,created_at_ms,updated_at_ms,revision) VALUES(?1,?2,?3,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,updated_at_ms=excluded.updated_at_ms,revision=excluded.revision",rusqlite::params![id,name,now,next])?;
    tx.execute("DELETE FROM playlist_items WHERE playlist_id=?", [&id])?;
    {
        let mut insert = tx
            .prepare("INSERT INTO playlist_items(playlist_id,ordinal,media_id) VALUES(?1,?2,?3)")?;
        for (ordinal, media) in ids.iter().enumerate() {
            insert.execute(rusqlite::params![id, ordinal as i64, media])?;
        }
    }
    tx.commit()?;
    Ok(PlaylistRevision {
        playlist_id: id,
        revision: (next as u64).into(),
    })
}
/// Revision shared by desktop saves and remote compare-and-swap edits.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistRevision {
    pub playlist_id: String,
    pub revision: crate::remote::protocol::Decimal,
}
pub fn revision(db: &Connection, id: &str) -> Result<PlaylistRevision, AppError> {
    let value: i64 = db
        .query_row("SELECT revision FROM playlists WHERE id=?", [id], |r| {
            r.get(0)
        })
        .map_err(|_| AppError::new("playlist_missing", "This playlist no longer exists."))?;
    Ok(PlaylistRevision {
        playlist_id: id.into(),
        revision: (value as u64).into(),
    })
}
/// Remote callers must supply the observed revision for every existing playlist.
/// Item handles should bind this revision to their ordinal; resolve them only
/// after checking the same revision in the write transaction.
pub fn save_if_revision(
    db: &mut Connection,
    id: Option<&str>,
    expected: Option<u64>,
    name: &str,
    ids: &[String],
) -> Result<PlaylistRevision, AppError> {
    if id.is_some() != expected.is_some() {
        return Err(AppError::new(
            "invalid_request",
            "An existing playlist requires its current revision.",
        ));
    }
    save_transaction(db, id, expected, name, ids)
}
pub fn remove_if_revision(db: &mut Connection, id: &str, expected: u64) -> Result<(), AppError> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if revision(&tx, id)?.revision.value() != expected {
        return Err(AppError::new(
            "stale_state",
            "This playlist has changed. Refresh it before deleting.",
        ));
    }
    remove(&tx, id)?;
    tx.commit()?;
    Ok(())
}
pub fn list(db: &Connection) -> Result<Vec<PlaylistSummary>, AppError> {
    let mut query=db.prepare("SELECT p.id,p.name,count(i.ordinal) FROM playlists p LEFT JOIN playlist_items i ON i.playlist_id=p.id GROUP BY p.id ORDER BY p.name COLLATE NOCASE,p.id")?;
    let result = query
        .query_map([], |r| {
            Ok(PlaylistSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                item_count: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}
pub fn get(db: &Connection, id: &str) -> Result<Playlist, AppError> {
    let name = db
        .query_row("SELECT name FROM playlists WHERE id=?", [id], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|_| AppError::new("playlist_missing", "This playlist no longer exists."))?;
    let mut query =
        db.prepare("SELECT media_id FROM playlist_items WHERE playlist_id=? ORDER BY ordinal")?;
    let ids = query
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Playlist {
        id: id.into(),
        name,
        items: items(db, &ids)?,
    })
}
pub fn remove(db: &Connection, id: &str) -> Result<(), AppError> {
    db.execute("DELETE FROM playlists WHERE id=?", [id])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
            .unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('s','local',x'2f','Music','[]','available'); INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,metadata_json,availability) VALUES('a','s',x'61','a.flac','audio',1,'1','{\"title\":\"Title\",\"artist\":\"Artist\",\"album\":\"Album\"}','available'),('b','s',x'62','b.ogg','audio',1,'1','{}','offline'),('v','s',x'76','v.mp4','video',1,'1','{}','available');").unwrap();
        db
    }
    #[test]
    fn duplicates_offline_and_order_survive_save_and_reload() {
        let mut db = db();
        let id = save(
            &mut db,
            None,
            "Musiikkia",
            &["b".into(), "a".into(), "a".into()],
        )
        .unwrap();
        let playlist = get(&db, &id).unwrap();
        assert_eq!(
            playlist
                .items
                .iter()
                .map(|i| i.media_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "a"]
        );
        assert_eq!(playlist.items[0].availability, "offline");
        assert_eq!(playlist.items[0].title, "b.ogg");
        assert_eq!(playlist.items[1].artist.as_deref(), Some("Artist"));
        assert_eq!(list(&db).unwrap()[0].item_count, 3);
    }
    #[test]
    fn invalid_edits_are_atomic_and_delete_only_playlist() {
        let mut db = db();
        let id = save(&mut db, None, "Keep", &["a".into()]).unwrap();
        assert!(save(&mut db, Some(&id), "New", &["missing".into()]).is_err());
        assert!(save(&mut db, Some(&id), "New", &["v".into()]).is_err());
        assert_eq!(get(&db, &id).unwrap().name, "Keep");
        remove(&db, &id).unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert_eq!(
            db.query_row("SELECT count(*) FROM media_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn playlist_limits_count_unicode_characters_and_item_occurrences() {
        let mut db = db();
        assert!(save(&mut db, None, &"ä".repeat(200), &[]).is_ok());
        assert!(save(&mut db, None, &"ä".repeat(201), &[]).is_err());
        assert!(save(&mut db, None, " ", &[]).is_err());
        assert!(save(&mut db, None, "Too long", &vec!["a".into(); 10001]).is_err());
    }
    #[test]
    fn unplugged_source_overrides_available_file_and_retains_playlist() {
        let mut db = db();
        let id = save(&mut db, None, "Drive", &["a".into()]).unwrap();
        db.execute("UPDATE media_sources SET availability='offline'", [])
            .unwrap();
        assert_eq!(get(&db, &id).unwrap().items[0].availability, "offline");
        db.execute("UPDATE media_sources SET availability='available'", [])
            .unwrap();
        assert_eq!(get(&db, &id).unwrap().items[0].availability, "available");
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn desktop_edits_advance_remote_revision_and_stale_edits_cannot_replace_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute("INSERT INTO playlists VALUES('p','Original',1,1)", [])
            .unwrap();
        drop(db);
        let mut db = crate::storage::database::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT revision FROM playlists WHERE id='p'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        save(&mut db, Some("p"), "Desktop", &[]).unwrap();
        assert_eq!(
            db.query_row("SELECT revision FROM playlists WHERE id='p'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            save_if_revision(&mut db, Some("p"), Some(1), "Stale remote", &[])
                .unwrap_err()
                .code,
            "stale_state"
        );
        assert_eq!(get(&db, "p").unwrap().name, "Desktop");
        let saved = save_if_revision(&mut db, Some("p"), Some(2), "Remote", &[]).unwrap();
        assert_eq!(saved.revision.value(), 3);
        assert_eq!(
            remove_if_revision(&mut db, "p", 2).unwrap_err().code,
            "stale_state"
        );
        assert_eq!(get(&db, "p").unwrap().name, "Remote");
        remove_if_revision(&mut db, "p", 3).unwrap();
        assert!(get(&db, "p").is_err());
    }
}

#[cfg(test)]
mod compare_and_swap_tests {
    use super::*;
    #[test]
    fn remote_create_requires_no_previous_revision_and_enforces_existing_name_limits() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
            .unwrap();
        let created = save_if_revision(&mut db, None, None, &"ä".repeat(200), &[]).unwrap();
        assert_eq!(created.revision.value(), 1);
        assert_eq!(
            save_if_revision(
                &mut db,
                Some(&created.playlist_id),
                None,
                "No revision",
                &[]
            )
            .unwrap_err()
            .code,
            "invalid_request"
        );
        assert_eq!(
            save_if_revision(&mut db, None, Some(1), "Bad create", &[])
                .unwrap_err()
                .code,
            "invalid_request"
        );
        assert_eq!(
            save_if_revision(
                &mut db,
                Some(&created.playlist_id),
                Some(1),
                &"ä".repeat(201),
                &[]
            )
            .unwrap_err()
            .code,
            "playlist_name"
        );
        assert_eq!(
            revision(&db, &created.playlist_id)
                .unwrap()
                .revision
                .value(),
            1
        );
        assert_eq!(
            get(&db, &created.playlist_id).unwrap().name,
            "ä".repeat(200)
        );
    }
}

#[cfg(test)]
mod cascade_revision_tests {
    use super::*;
    #[test]
    fn revision_exhaustion_rejects_save_without_mutating_content() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
            .unwrap();
        db.execute("INSERT INTO playlists(id,name,created_at_ms,updated_at_ms,revision) VALUES('p','Keep',1,1,9223372036854775807)",[]).unwrap();
        assert_eq!(
            save(&mut db, Some("p"), "Overflow", &[]).unwrap_err().code,
            "revision_exhausted"
        );
        assert_eq!(get(&db, "p").unwrap().name, "Keep");
        assert_eq!(
            revision(&db, "p").unwrap().revision.value(),
            9_223_372_036_854_775_807
        );
    }
    #[test]
    fn source_deletion_advances_affected_playlist_revision_and_preserves_other_duplicates() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
            .unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('s','local',x'2f','source','[]','available'),('other','local',x'2f61','other','[]','available'); INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('a','s',x'61','a','audio',1,'1','available'),('b','other',x'62','b','audio',1,'1','available');").unwrap();
        let playlist = save(&mut db, None, "Keep", &["a".into(), "b".into(), "b".into()]).unwrap();
        let before = revision(&db, &playlist).unwrap().revision.value();
        db.execute("DELETE FROM media_sources WHERE id='s'", [])
            .unwrap();
        assert!(revision(&db, &playlist).unwrap().revision.value() > before);
        assert_eq!(
            get(&db, &playlist)
                .unwrap()
                .items
                .iter()
                .map(|i| i.media_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "b"]
        );
        assert_eq!(
            save_if_revision(&mut db, Some(&playlist), Some(before), "Stale", &[])
                .unwrap_err()
                .code,
            "stale_state"
        );
        db.execute("UPDATE playlists SET revision=9223372036854775807", [])
            .unwrap();
        assert!(db
            .execute("DELETE FROM media_sources WHERE id='other'", [])
            .is_err());
        assert_eq!(get(&db, &playlist).unwrap().items.len(), 2);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM media_sources WHERE id='other'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
