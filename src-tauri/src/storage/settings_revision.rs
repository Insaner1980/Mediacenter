//! One preference-domain revision, independent of home events and setup's CAS.
//! Advance inside the same transaction as the preference write. No home lock is
//! taken here: callers that also publish home state always acquire home -> DB.
use crate::{app_state::now_ms, contracts::AppError};
use rusqlite::{Connection, OptionalExtension, Transaction};

const KEY: &str = "routine_settings_revision";

pub fn revision(db: &Connection) -> Result<u64, AppError> {
    let stored: Option<String> = db
        .query_row(
            "SELECT value_json FROM settings WHERE key=?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()?;
    match stored {
        None => Ok(1),
        Some(value) => serde_json::from_str::<u64>(&value)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                AppError::new(
                    "settings_revision_unavailable",
                    "Settings revision is unavailable.",
                )
            }),
    }
}
pub fn check(db: &Connection, expected: Option<u64>) -> Result<u64, AppError> {
    let current = revision(db)?;
    if expected.is_some_and(|expected| expected != current) {
        return Err(AppError::new("stale_state", "Settings changed."));
    }
    Ok(current)
}
pub fn advance(tx: &Transaction<'_>, expected: u64) -> Result<u64, AppError> {
    let current = check(tx, Some(expected))?;
    let next = current
        .checked_add(1)
        .ok_or_else(|| AppError::new("revision_exhausted", "Settings revision is exhausted."))?;
    tx.execute(
        "INSERT INTO settings(key,value_json,updated_at_ms) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",
        rusqlite::params![KEY, next.to_string(), now_ms() as i64],
    )?;
    Ok(next)
}
