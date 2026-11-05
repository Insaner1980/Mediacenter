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
    if name.trim().is_empty() || name.chars().count() > 200 {
        return Err(AppError::new(
            "playlist_name",
            "Use a playlist name from 1 to 200 characters.",
        ));
    }
    items(db, ids)?;
    let tx = db.transaction()?;
    let now = crate::app_state::now_ms() as i64;
    let id = match id {
        Some(id) => {
            if tx.execute(
                "UPDATE playlists SET name=?1,updated_at_ms=?2 WHERE id=?3",
                rusqlite::params![name, now, id],
            )? == 0
            {
                return Err(AppError::new(
                    "playlist_missing",
                    "This playlist no longer exists.",
                ));
            }
            id.to_string()
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO playlists(id,name,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?3)",
                rusqlite::params![id, name, now],
            )?;
            id
        }
    };
    tx.execute("DELETE FROM playlist_items WHERE playlist_id=?", [&id])?;
    {
        let mut insert = tx
            .prepare("INSERT INTO playlist_items(playlist_id,ordinal,media_id) VALUES(?1,?2,?3)")?;
        for (i, media) in ids.iter().enumerate() {
            insert.execute(rusqlite::params![id, i as i64, media])?;
        }
    }
    tx.commit()?;
    Ok(id)
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
