use rusqlite::Connection;
use std::path::Path;
pub fn migrate(connection: &mut Connection, backup_path: &Path, sql: &str) -> rusqlite::Result<()> {
    let mut backup = Connection::open(backup_path)?;
    rusqlite::backup::Backup::new(connection, &mut backup)?.run_to_completion(
        128,
        std::time::Duration::from_millis(5),
        None,
    )?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(sql)?;
    transaction.commit()
}
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 0 {
        migrate(
            &mut connection,
            &path.with_extension("pre-migration.db"),
            include_str!("../../migrations/0001_initial.sql"),
        )?;
    }
    if version > 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
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
