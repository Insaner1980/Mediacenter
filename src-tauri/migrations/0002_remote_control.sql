ALTER TABLE playlists ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK(typeof(revision)='integer' AND revision > 0);
-- Cascading library/source deletion changes playlist contents too. Run before
-- the media-item FK removes its playlist occurrences; duplicate survivors keep
-- their existing ordinals. An exhausted revision aborts the whole native delete.
CREATE TRIGGER playlist_revision_on_media_delete BEFORE DELETE ON media_items
BEGIN
 UPDATE playlists SET revision=CASE WHEN revision=9223372036854775807
   THEN RAISE(ABORT,'playlist revision exhausted') ELSE revision+1 END
 WHERE id IN(SELECT playlist_id FROM playlist_items WHERE media_id=OLD.id);
END;
CREATE TABLE paired_devices(
 server_identity TEXT NOT NULL,
 device_id TEXT NOT NULL,
 token_hash BLOB NOT NULL CHECK(length(token_hash)=32),
 label TEXT NOT NULL,
 scopes_json TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL,
 last_seen_at_ms INTEGER,
 revoked_at_ms INTEGER,
 PRIMARY KEY(server_identity,device_id),
 UNIQUE(server_identity,token_hash)
);
CREATE TABLE command_journal(
 server_identity TEXT NOT NULL,
 device_id TEXT NOT NULL,
 request_id TEXT NOT NULL,
 parameter_hash BLOB NOT NULL CHECK(length(parameter_hash)=32),
 server_epoch TEXT NOT NULL,
 operation_id TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN('accepted','running','succeeded','failed','cancelled','outcome_unknown')),
 error_code TEXT,
 pairing_issuance INTEGER NOT NULL DEFAULT 0 CHECK(pairing_issuance IN(0,1)),
 created_at_ms INTEGER NOT NULL,
 updated_at_ms INTEGER NOT NULL,
 PRIMARY KEY(server_identity,device_id,request_id),
 FOREIGN KEY(server_identity,device_id) REFERENCES paired_devices(server_identity,device_id)
);
CREATE INDEX command_journal_retention ON command_journal(server_identity,status,updated_at_ms);
CREATE TABLE remote_preferences(
 server_identity TEXT PRIMARY KEY,
 revision INTEGER NOT NULL DEFAULT 1 CHECK(typeof(revision)='integer' AND revision > 0),
 enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN(0,1)),
 keep_available INTEGER NOT NULL DEFAULT 0 CHECK(keep_available IN(0,1)),
 start_at_login INTEGER NOT NULL DEFAULT 0 CHECK(start_at_login IN(0,1))
);
PRAGMA user_version=2;
