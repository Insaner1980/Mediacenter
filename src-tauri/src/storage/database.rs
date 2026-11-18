use rusqlite::{backup::Backup, backup::StepResult, ffi, Connection};
use std::{
    path::Path,
    time::{Duration, Instant},
};
pub fn migrate(connection: &mut Connection, backup_path: &Path, sql: &str) -> rusqlite::Result<()> {
    {
        let mut destination = Connection::open(backup_path)?;
        // Retry contention explicitly so SQLite's per-call timeout cannot extend the retry loop.
        destination.busy_timeout(Duration::ZERO)?;
        let backup = Backup::new(connection, &mut destination)?;
        let mut first_contention = None;
        loop {
            let code = match backup.step(128)? {
                StepResult::Done => break,
                StepResult::Busy => Some(ffi::SQLITE_BUSY),
                StepResult::Locked => Some(ffi::SQLITE_LOCKED),
                _ => None,
            };
            if let Some(code) = code {
                if first_contention.get_or_insert_with(Instant::now).elapsed()
                    >= Duration::from_secs(3)
                {
                    return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(code), None));
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let transaction = connection.transaction()?;
    transaction.execute_batch(sql)?;
    transaction.commit()
}
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(3))?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Inspect compatibility before journal_mode: enabling WAL rewrites the header.
    if version > 2 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    for (from, extension, sql) in [
        (
            0,
            "pre-migration.db",
            include_str!("../../migrations/0001_initial.sql"),
        ),
        (
            1,
            "pre-migration-v1.db",
            include_str!("../../migrations/0002_remote_control.sql"),
        ),
    ] {
        let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if current == from {
            migrate(&mut connection, &path.with_extension(extension), sql)?;
        }
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema1(path: &Path) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../tests/fixtures/schema1_content.sql"))
            .unwrap();
    }
    fn preserved(db: &Connection) -> Vec<String> {
        ["SELECT group_concat(id || ':' || hex(root) || ':' || availability) FROM media_sources",
         "SELECT group_concat(id || ':' || source_id || ':' || hex(relative_path)) FROM media_items",
         "SELECT group_concat(id || ':' || name || ':' || created_at_ms || ':' || updated_at_ms) FROM playlists",
         "SELECT group_concat(playlist_id || ':' || ordinal || ':' || media_id) FROM playlist_items",
         "SELECT group_concat(media_id || ':' || position_ms || ':' || duration_ms || ':' || updated_at_ms || ':' || completed) FROM resume_positions",
         "SELECT group_concat(key || ':' || value_json || ':' || updated_at_ms) FROM settings"]
         .iter().map(|sql| db.query_row(sql, [], |r| r.get(0)).unwrap()).collect()
    }
    #[test]
    fn schema1_upgrade_preserves_all_content_and_reopens_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        schema1(&path);
        let before = preserved(&Connection::open(&path).unwrap());
        let db = open(&path).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(preserved(&db), before);
        assert_eq!(
            db.query_row("SELECT revision FROM playlists", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM remote_preferences WHERE enabled != 0",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        drop(db);
        assert_eq!(preserved(&open(&path).unwrap()), before);
        let backup = Connection::open(path.with_extension("pre-migration-v1.db")).unwrap();
        assert_eq!(preserved(&backup), before);
    }
    #[test]
    fn fresh_upgrade_retains_both_sequential_backups() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        drop(open(&path).unwrap());
        for (extension, version) in [("pre-migration.db", 0), ("pre-migration-v1.db", 1)] {
            let backup = Connection::open(path.with_extension(extension)).unwrap();
            assert_eq!(
                backup
                    .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
        }
    }
    #[test]
    fn future_schema_is_rejected_without_changing_journal_or_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE future(value); INSERT INTO future VALUES('keep'); PRAGMA user_version=3;",
        )
        .unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert!(open(&path).is_err());
        assert!(
            std::fs::read(&path).unwrap() == before,
            "future schema bytes changed"
        );
        assert!(!path.with_extension("db-wal").exists());
    }
    #[test]
    fn locked_backup_fails_without_upgrading_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        schema1(&path);
        let lock = Connection::open(path.with_extension("pre-migration-v1.db")).unwrap();
        lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            let _ = send.send(open(&worker_path).map(|_| ()));
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(6));
        lock.execute_batch("ROLLBACK").unwrap();
        worker.join().unwrap();
        let error = result
            .expect("locked migration backup did not return")
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy)
        );
        let unchanged = Connection::open(&path).unwrap();
        assert_eq!(
            unchanged
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(open(&path).is_ok());
    }
    #[test]
    fn interrupted_schema2_migration_rolls_back_fixture_and_keeps_usable_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        schema1(&path);
        let mut db = Connection::open(&path).unwrap();
        let before = preserved(&db);
        let backup = path.with_extension("pre-migration-v1.db");
        assert!(migrate(
            &mut db,
            &backup,
            &format!(
                "{}\nDELETE FROM settings; INVALID SQL;",
                include_str!("../../migrations/0002_remote_control.sql")
            )
        )
        .is_err());
        assert_eq!(preserved(&db), before);
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(preserved(&Connection::open(&backup).unwrap()), before);
        drop(db);
        assert_eq!(preserved(&open(&path).unwrap()), before);
    }
    #[test]
    fn migration_failure_preserves_previous_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let mut conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE personal(value TEXT); INSERT INTO personal VALUES('keep');",
        )
        .unwrap();
        assert!(migrate(
            &mut conn,
            &dir.path().join("backup.db"),
            "DROP TABLE personal; INVALID SQL;"
        )
        .is_err());
        assert_eq!(
            conn.query_row("SELECT value FROM personal", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        let backup = Connection::open(dir.path().join("backup.db")).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT value FROM personal", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
    }
    #[test]
    fn offline_source_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(
            &mut conn,
            &dir.path().join("backup.db"),
            include_str!("../../migrations/0001_initial.sql"),
        )
        .unwrap();
        conn.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('s','local',x'2f746d70','test','video','available')",[]).unwrap();
        conn.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('m','s',x'66','f','video',1,'1','available')",[]).unwrap();
        conn.execute(
            "UPDATE media_sources SET availability='offline' WHERE id='s'",
            [],
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM media_items", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
