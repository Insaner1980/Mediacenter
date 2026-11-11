//! Synchronous, identity-scoped persistence. Hold the application DB mutex only
//! for these calls, never across native work or awaits. Construction has no effects.
use super::protocol::*;
use crate::contracts::AppError;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;
const CAPACITY: i64 = 10_000;
type Result<T> = std::result::Result<T, AppError>;

/// Only the hash crosses the persistence boundary; credentials never enter SQL.
#[derive(Clone)]
pub struct CredentialHash([u8; 32]);
impl CredentialHash {
    pub fn from_credential(credential: &BearerCredential) -> Self {
        Self(Sha256::digest(credential.as_str().as_bytes()).into())
    }
}
impl std::fmt::Debug for CredentialHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub device_id: Id,
    pub request_id: Id,
    pub server_epoch: Id,
    pub outcome: JournalOutcome,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}
/// Deliberately cannot contain a reply, pairing candidate, text, label or token.
#[derive(Clone, Debug, PartialEq)]
pub struct JournalOutcome {
    pub operation_id: Id,
    pub status: OperationStatus,
    pub error_code: Option<WireErrorCode>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CommandAcceptance {
    New(JournalEntry),
    Existing(JournalEntry),
    /// The ephemeral pairing result was issued once and must never be replayed.
    AlreadyIssued {
        operation_id: Id,
    },
}

pub struct RemoteStore<'a> {
    db: &'a mut Connection,
    identity: Id,
}
fn error(code: &str) -> AppError {
    AppError::new(code, "Remote persistence operation could not be completed.")
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| error("database_error"))
}
fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|_| error("database_error"))
}
fn opaque(value: String) -> Result<Id> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| error("database_error"))
}
fn terminal(status: &OperationStatus) -> bool {
    !matches!(status, OperationStatus::Accepted | OperationStatus::Running)
}
impl<'a> RemoteStore<'a> {
    /// The identity is native configuration, never supplied by a LAN request.
    pub fn new(db: &'a mut Connection, server_identity: &Id) -> Self {
        Self {
            db,
            identity: server_identity.clone(),
        }
    }
    pub fn add_device(&mut self, device: &PairedDevice, hash: &CredentialHash) -> Result<()> {
        if device.revoked {
            return Err(error("invalid_request"));
        }
        let timestamp = |value: &TimestampMs| -> Result<i64> {
            serde_json::to_value(value)
                .ok()
                .and_then(|v| v.as_i64())
                .ok_or_else(|| error("invalid_request"))
        };
        let created = timestamp(&device.created_at_ms)?;
        let seen = device.last_seen_at_ms.as_ref().map(timestamp).transpose()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: i64 = tx.query_row(
            "SELECT count(*) FROM paired_devices WHERE server_identity=? AND revoked_at_ms IS NULL",
            [self.identity.as_str()],
            |r| r.get(0),
        )?;
        if active >= 8 {
            return Err(error("busy"));
        }
        tx.execute("INSERT INTO paired_devices(server_identity,device_id,token_hash,label,scopes_json,created_at_ms,last_seen_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![self.identity.as_str(),device.device_id.as_str(),hash.0.as_slice(),device.label.as_str(),json(&device.scopes)?,created,seen])?;
        tx.commit()?;
        Ok(())
    }
    pub fn revoke_device(&mut self, device: &Id, now_ms: i64) -> Result<bool> {
        Ok(self.db.execute("UPDATE paired_devices SET revoked_at_ms=?1 WHERE server_identity=?2 AND device_id=?3 AND revoked_at_ms IS NULL", params![now_ms,self.identity.as_str(),device.as_str()])? > 0)
    }
    pub fn lookup_credential(&self, hash: &CredentialHash) -> Result<Option<PairedDevice>> {
        let row = self.db.query_row("SELECT device_id,label,scopes_json,created_at_ms,last_seen_at_ms FROM paired_devices WHERE server_identity=?1 AND token_hash=?2 AND revoked_at_ms IS NULL", params![self.identity.as_str(),hash.0.as_slice()], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<i64>>(4)?))).optional()?;
        row.map(|(id,label,scopes,created,seen)| {
            let scopes: ScopeSet = decode(&scopes)?;
            serde_json::from_value(serde_json::json!({"deviceId":id,"label":label,"scopes":scopes,"createdAtMs":created,"lastSeenAtMs":seen,"revoked":false})).map_err(|_| error("database_error"))
        }).transpose()
    }
    /// Local setup projection only; hashes never leave the store.
    pub fn active_devices(&self) -> Result<Vec<PairedDevice>> {
        let mut statement=self.db.prepare("SELECT device_id,label,scopes_json,created_at_ms,last_seen_at_ms FROM paired_devices WHERE server_identity=? AND revoked_at_ms IS NULL ORDER BY created_at_ms,device_id LIMIT 8")?;
        let rows = statement
            .query_map([self.identity.as_str()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter().map(|(id,label,scopes,created,seen)| {
            let scopes:ScopeSet=decode(&scopes)?;
            serde_json::from_value(serde_json::json!({"deviceId":id,"label":label,"scopes":scopes,"createdAtMs":created,"lastSeenAtMs":seen,"revoked":false})).map_err(|_|error("database_error"))
        }).collect()
    }
    pub fn get_command(&self, device: &Id, request: &Id) -> Result<Option<JournalEntry>> {
        Ok(read_command(self.db, &self.identity, device, request)?.map(|stored| stored.entry))
    }
    pub fn active_device_ids(&self) -> Result<Vec<Id>> {
        let mut statement=self.db.prepare("SELECT device_id FROM paired_devices WHERE server_identity=? AND revoked_at_ms IS NULL ORDER BY device_id LIMIT 8")?;
        let values = statement
            .query_map([self.identity.as_str()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values.into_iter().map(opaque).collect()
    }
    /// Bounded recent operation projection; individual lookup remains available
    /// for all retained records. Ownership and server identity are SQL predicates.
    pub fn recent_commands(&self, device: &Id) -> Result<Vec<JournalEntry>> {
        let mut statement=self.db.prepare("SELECT request_id FROM command_journal WHERE server_identity=?1 AND device_id=?2 ORDER BY updated_at_ms DESC,rowid DESC LIMIT 100")?;
        let ids = statement
            .query_map(params![self.identity.as_str(), device.as_str()], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                self.get_command(device, &opaque(id)?)?
                    .ok_or_else(|| error("database_error"))
            })
            .collect()
    }
    pub fn get_operation(&self, device: &Id, operation: &Id) -> Result<Option<JournalEntry>> {
        let request=self.db.query_row("SELECT request_id FROM command_journal WHERE server_identity=?1 AND device_id=?2 AND operation_id=?3",params![self.identity.as_str(),device.as_str(),operation.as_str()],|r|r.get::<_,String>(0)).optional()?;
        request
            .map(|id| self.get_command(device, &opaque(id)?))
            .transpose()
            .map(Option::flatten)
    }
    pub fn accept_command(
        &mut self,
        device: &Id,
        request: &RemoteRequest,
        epoch: &Id,
        operation: &Id,
        now_ms: i64,
    ) -> Result<CommandAcceptance> {
        // Never compute a hash for transient text/navigation, nor persist reads.
        if request.command.delivery() != Delivery::Durable {
            return Err(error("invalid_request"));
        }
        let normalized = serde_json::json!({"command":request.command,"expected":request.expected});
        let hash: [u8; 32] = Sha256::digest(json(&normalized)?.as_bytes()).into();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active = tx.query_row("SELECT 1 FROM paired_devices WHERE server_identity=?1 AND device_id=?2 AND revoked_at_ms IS NULL", params![self.identity.as_str(),device.as_str()], |_| Ok(())).optional()?.is_some();
        if !active {
            return Err(error("unauthorized"));
        }
        // Deduplication precedes pruning and capacity rejection, including old IDs.
        if let Some(stored) = read_command(&tx, &self.identity, device, &request.id)? {
            if stored.hash != hash {
                return Err(error("request_id_conflict"));
            }
            return Ok(
                if stored.pairing && terminal(&stored.entry.outcome.status) {
                    CommandAcceptance::AlreadyIssued {
                        operation_id: stored.entry.outcome.operation_id,
                    }
                } else {
                    CommandAcceptance::Existing(stored.entry)
                },
            );
        }
        prune_rows(&tx, &self.identity, now_ms, CAPACITY - 1)?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM command_journal WHERE server_identity=?",
            [self.identity.as_str()],
            |r| r.get(0),
        )?;
        if count >= CAPACITY {
            return Err(error("busy"));
        }
        let pairing = matches!(request.command, RemoteCommand::PairingBegin(..));
        tx.execute("INSERT INTO command_journal(server_identity,device_id,request_id,parameter_hash,server_epoch,operation_id,status,pairing_begin,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'accepted',?7,?8,?8)",params![self.identity.as_str(),device.as_str(),request.id.as_str(),hash.as_slice(),epoch.as_str(),operation.as_str(),pairing,now_ms])?;
        let entry = read_command(&tx, &self.identity, device, &request.id)?
            .ok_or_else(|| error("database_error"))?
            .entry;
        tx.commit()?;
        Ok(CommandAcceptance::New(entry))
    }
    pub fn mark_running(&mut self, device: &Id, request: &Id, now_ms: i64) -> Result<()> {
        if self.db.execute("UPDATE command_journal SET status='running',updated_at_ms=?1 WHERE server_identity=?2 AND device_id=?3 AND request_id=?4 AND status='accepted'",params![now_ms,self.identity.as_str(),device.as_str(),request.as_str()])? == 0 { return Err(error("stale_state")); }
        Ok(())
    }
    pub fn finish_command(
        &mut self,
        device: &Id,
        request: &Id,
        outcome: &JournalOutcome,
        now_ms: i64,
    ) -> Result<()> {
        if !terminal(&outcome.status) {
            return Err(error("invalid_request"));
        }
        let status = json(&outcome.status)?;
        let code = outcome.error_code.as_ref().map(json).transpose()?;
        if self.db.execute("UPDATE command_journal SET status=?1,error_code=?2,updated_at_ms=?3 WHERE server_identity=?4 AND device_id=?5 AND request_id=?6 AND operation_id=?7 AND status IN('accepted','running')",params![status.trim_matches('"'),code,now_ms,self.identity.as_str(),device.as_str(),request.as_str(),outcome.operation_id.as_str()])? == 0 { return Err(error("stale_state")); }
        Ok(())
    }
    /// Use only when pairing.begin actually issues its ephemeral ready candidate.
    /// Preparation/confirmation results and failures use ordinary finish_command.
    /// This single statement atomically records success and the safe issuance bit;
    /// it persists neither the candidate nor its secret.
    pub fn finish_pairing_issuance(
        &mut self,
        device: &Id,
        request: &Id,
        operation: &Id,
        now_ms: i64,
    ) -> Result<()> {
        if self.db.execute(
            "UPDATE command_journal SET status='succeeded',error_code=NULL,pairing_issuance=1,updated_at_ms=?1 WHERE server_identity=?2 AND device_id=?3 AND request_id=?4 AND operation_id=?5 AND pairing_begin=1 AND status IN('accepted','running')",
            params![now_ms,self.identity.as_str(),device.as_str(),request.as_str(),operation.as_str()],
        )? == 0 {
            return Err(error("stale_state"));
        }
        Ok(())
    }
    /// Called once on startup by the native server for THIS identity, never by
    /// database::open or the borrowed wrapper constructor. Does not replay work.
    pub fn recover_unfinished(&mut self, now_ms: i64) -> Result<usize> {
        Ok(self.db.execute("UPDATE command_journal SET status='outcome_unknown',error_code='\"outcome_unknown\"',updated_at_ms=?1 WHERE server_identity=?2 AND status IN('accepted','running')",params![now_ms,self.identity.as_str()])?)
    }
    pub fn prune(&mut self, now_ms: i64) -> Result<usize> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = prune_rows(&tx, &self.identity, now_ms, CAPACITY)?;
        tx.commit()?;
        Ok(removed)
    }
    pub fn preferences(&self) -> Result<RemotePreferences> {
        let stored = self.db.query_row("SELECT revision,enabled,keep_available,start_at_login FROM remote_preferences WHERE server_identity=?",[self.identity.as_str()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,bool>(1)?,r.get::<_,bool>(2)?,r.get::<_,bool>(3)?))).optional()?;
        let (revision, enabled, keep_available, start_at_login) =
            stored.unwrap_or((1, false, false, false));
        Ok(RemotePreferences {
            revision: (revision as u64).into(),
            enabled,
            keep_available,
            start_at_login,
        })
    }
    pub fn save_preferences(
        &mut self,
        preferences: &RemotePreferences,
    ) -> Result<RemotePreferences> {
        let expected =
            i64::try_from(preferences.revision.value()).map_err(|_| error("stale_state"))?;
        let next = expected
            .checked_add(1)
            .ok_or_else(|| error("revision_exhausted"))?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT OR IGNORE INTO remote_preferences(server_identity) VALUES(?)",
            [self.identity.as_str()],
        )?;
        let before = tx.query_row(
            "SELECT enabled,keep_available,start_at_login FROM remote_preferences WHERE server_identity=?1",
            [self.identity.as_str()], |row| Ok((row.get::<_,bool>(0)?, row.get::<_,bool>(1)?, row.get::<_,bool>(2)?)),
        )?;
        let routine_revision = crate::storage::settings_revision::revision(&tx)?;
        if tx.execute("UPDATE remote_preferences SET revision=?6,enabled=?1,keep_available=?2,start_at_login=?3 WHERE server_identity=?4 AND revision=?5",params![preferences.enabled,preferences.keep_available,preferences.start_at_login,self.identity.as_str(),expected,next])? == 0 { return Err(error("stale_state")); }
        if before
            != (
                preferences.enabled,
                preferences.keep_available,
                preferences.start_at_login,
            )
        {
            crate::storage::settings_revision::advance(&tx, routine_revision)?;
        }
        tx.commit()?;
        self.preferences()
    }
}
struct StoredCommand {
    entry: JournalEntry,
    hash: Vec<u8>,
    pairing: bool,
}
fn read_command(
    db: &Connection,
    identity: &Id,
    device: &Id,
    request: &Id,
) -> Result<Option<StoredCommand>> {
    let row = db.query_row("SELECT parameter_hash,server_epoch,operation_id,status,error_code,created_at_ms,updated_at_ms,pairing_issuance FROM command_journal WHERE server_identity=?1 AND device_id=?2 AND request_id=?3",params![identity.as_str(),device.as_str(),request.as_str()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,i64>(5)?,r.get::<_,i64>(6)?,r.get::<_,bool>(7)?))).optional()?;
    row.map(
        |(hash, epoch, operation, status, code, created, updated, pairing)| {
            Ok(StoredCommand {
                entry: JournalEntry {
                    device_id: device.clone(),
                    request_id: request.clone(),
                    server_epoch: opaque(epoch)?,
                    outcome: JournalOutcome {
                        operation_id: opaque(operation)?,
                        status: decode(&json(&status)?)?,
                        error_code: code.as_deref().map(decode).transpose()?,
                    },
                    created_at_ms: created,
                    updated_at_ms: updated,
                },
                hash,
                pairing,
            })
        },
    )
    .transpose()
}
fn prune_rows(db: &Connection, identity: &Id, now_ms: i64, target: i64) -> Result<usize> {
    let expired = db.execute("DELETE FROM command_journal WHERE server_identity=?1 AND status NOT IN('accepted','running') AND updated_at_ms<=?2",params![identity.as_str(),now_ms.saturating_sub(RETENTION_MS)])?;
    let count: i64 = db.query_row(
        "SELECT count(*) FROM command_journal WHERE server_identity=?",
        [identity.as_str()],
        |r| r.get(0),
    )?;
    let removed = db.execute("DELETE FROM command_journal WHERE rowid IN(SELECT rowid FROM command_journal WHERE server_identity=?1 AND status NOT IN('accepted','running') ORDER BY updated_at_ms,rowid LIMIT ?2)",params![identity.as_str(),(count-target).max(0)])?;
    Ok(expired + removed)
}
#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
