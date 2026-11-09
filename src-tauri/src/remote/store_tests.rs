use super::*;
use serde_json::json;
fn id(value: &str) -> Id {
    serde_json::from_value(json!(value)).unwrap()
}
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
        .unwrap();
    db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
        .unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db
}
fn device(value: &str) -> PairedDevice {
    serde_json::from_value(json!({"deviceId":value,"label":"Phone ä","scopes":["browse","playback","settings"],"createdAtMs":100,"revoked":false})).unwrap()
}
fn hash(value: char) -> CredentialHash {
    CredentialHash::from_credential(
        &serde_json::from_value(json!(value.to_string().repeat(43))).unwrap(),
    )
}
fn request(request_id: &str, volume: u32) -> RemoteRequest {
    parse_request(&json!({"v":1,"id":request_id,"method":"player.volume","params":{"sessionId":"session","volume":volume},"expected":{"serverEpoch":"epoch","sessionId":"session"}}).to_string()).unwrap()
}
fn add(store: &mut RemoteStore<'_>) {
    store.add_device(&device("phone"), &hash('a')).unwrap();
}
fn accept(store: &mut RemoteStore<'_>, request: &RemoteRequest, now: i64) -> CommandAcceptance {
    store
        .accept_command(&id("phone"), request, &id("epoch"), &id("operation"), now)
        .unwrap()
}
fn finish(store: &mut RemoteStore<'_>, request_id: &str, now: i64) {
    store
        .finish_command(
            &id("phone"),
            &id(request_id),
            &JournalOutcome {
                operation_id: id("operation"),
                status: OperationStatus::Succeeded,
                error_code: None,
            },
            now,
        )
        .unwrap();
}
fn count(db: &Connection) -> i64 {
    db.query_row("SELECT count(*) FROM command_journal", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn credentials_are_hash_only_scoped_and_revocable_without_affecting_other_identity() {
    let mut db = db();
    {
        let mut prod = RemoteStore::new(&mut db, &id("prod"));
        add(&mut prod);
        assert_eq!(
            prod.lookup_credential(&hash('a'))
                .unwrap()
                .unwrap()
                .device_id,
            id("phone")
        );
    }
    let stored: Vec<u8> = db
        .query_row("SELECT token_hash FROM paired_devices", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored.len(), 32);
    assert_ne!(stored, "a".repeat(43).as_bytes());
    {
        let mut dev = RemoteStore::new(&mut db, &id("dev"));
        assert!(dev.lookup_credential(&hash('a')).unwrap().is_none());
        assert!(!dev.revoke_device(&id("phone"), 200).unwrap());
        add(&mut dev);
        assert!(dev.revoke_device(&id("phone"), 201).unwrap());
        assert!(dev.lookup_credential(&hash('a')).unwrap().is_none());
    }
    let prod = RemoteStore::new(&mut db, &id("prod"));
    assert!(prod.lookup_credential(&hash('a')).unwrap().is_some());
}
#[test]
fn ninth_active_device_rejected_and_revocation_frees_slot_per_identity() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    for n in 0..8 {
        store
            .add_device(&device(&format!("phone{n}")), &hash((b'a' + n) as char))
            .unwrap();
    }
    assert_eq!(
        store
            .add_device(&device("ninth"), &hash('z'))
            .unwrap_err()
            .code,
        "busy"
    );
    store.revoke_device(&id("phone0"), 200).unwrap();
    store.add_device(&device("ninth"), &hash('z')).unwrap();
    let mut dev = RemoteStore::new(&mut db, &id("dev"));
    add(&mut dev);
}
#[test]
fn normalized_retry_returns_original_and_changed_hash_conflicts() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    let req = request("r", 25);
    let original = match accept(&mut store, &req, 100) {
        CommandAcceptance::New(e) => e,
        _ => panic!(),
    };
    let reordered = parse_request(r#"{"params":{"sessionId":"session","volume":25},"id":"r","expected":{"sessionId":"session","serverEpoch":"epoch"},"method":"player.volume","v":1}"#).unwrap();
    assert_eq!(
        store
            .accept_command(
                &id("phone"),
                &reordered,
                &id("new-epoch"),
                &id("other-operation"),
                200
            )
            .unwrap(),
        CommandAcceptance::Existing(original)
    );
    assert_eq!(
        store
            .accept_command(
                &id("phone"),
                &request("r", 26),
                &id("epoch"),
                &id("operation"),
                200
            )
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    finish(&mut store, "r", 300);
    assert!(matches!(
        accept(&mut store, &req, 400),
        CommandAcceptance::Existing(JournalEntry {
            outcome: JournalOutcome {
                status: OperationStatus::Succeeded,
                ..
            },
            ..
        })
    ));
    assert_eq!(count(&db), 1);
}
#[test]
fn transient_input_and_safe_reads_never_store_payload_or_hash() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    for json in [
        r#"{"v":1,"id":"text","method":"ui.search","params":{"rendererGeneration":"1","focusRevision":"1","route":"music","text":"secret","editSequence":"1"},"expected":{"serverEpoch":"epoch"}}"#,
        r#"{"v":1,"id":"read","method":"state.get","params":{}}"#,
    ] {
        let req = parse_request(json).unwrap();
        assert_eq!(
            store
                .accept_command(&id("phone"), &req, &id("epoch"), &id("operation"), 100)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    assert_eq!(count(&db), 0);
}
#[test]
fn recovery_is_explicit_scoped_and_persists_unknown_without_replay() {
    let mut db = db();
    for identity in ["prod", "dev"] {
        let mut store = RemoteStore::new(&mut db, &id(identity));
        add(&mut store);
        accept(&mut store, &request("r", 25), 100);
        store.mark_running(&id("phone"), &id("r"), 101).unwrap();
    }
    {
        let prod = RemoteStore::new(&mut db, &id("prod"));
        assert_eq!(
            prod.get_command(&id("phone"), &id("r"))
                .unwrap()
                .unwrap()
                .outcome
                .status,
            OperationStatus::Running
        );
    }
    {
        let mut dev = RemoteStore::new(&mut db, &id("dev"));
        assert_eq!(dev.recover_unfinished(200).unwrap(), 1);
        assert_eq!(dev.recover_unfinished(201).unwrap(), 0);
        assert!(matches!(
            accept(&mut dev, &request("r", 25), 202),
            CommandAcceptance::Existing(JournalEntry {
                outcome: JournalOutcome {
                    status: OperationStatus::OutcomeUnknown,
                    ..
                },
                ..
            })
        ));
    }
    let prod = RemoteStore::new(&mut db, &id("prod"));
    assert_eq!(
        prod.get_command(&id("phone"), &id("r"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Running
    );
}
#[test]
fn terminal_outcome_cannot_be_overwritten_and_wrong_operation_is_rejected() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    accept(&mut store, &request("r", 25), 100);
    let mut outcome = JournalOutcome {
        operation_id: id("wrong"),
        status: OperationStatus::Failed,
        error_code: Some(WireErrorCode::Unavailable),
    };
    assert_eq!(
        store
            .finish_command(&id("phone"), &id("r"), &outcome, 101)
            .unwrap_err()
            .code,
        "stale_state"
    );
    outcome.operation_id = id("operation");
    store
        .finish_command(&id("phone"), &id("r"), &outcome, 102)
        .unwrap();
    assert_eq!(
        store
            .finish_command(&id("phone"), &id("r"), &outcome, 103)
            .unwrap_err()
            .code,
        "stale_state"
    );
    assert_eq!(
        store
            .get_command(&id("phone"), &id("r"))
            .unwrap()
            .unwrap()
            .outcome,
        outcome
    );
}
#[test]
fn retention_expires_completed_after_24h_but_protects_unfinished() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    for name in ["done", "accepted", "running"] {
        accept(&mut store, &request(name, 25), 0);
    }
    finish(&mut store, "done", 0);
    store.mark_running(&id("phone"), &id("running"), 0).unwrap();
    assert_eq!(store.prune(86_399_999).unwrap(), 0);
    assert_eq!(store.prune(86_400_000).unwrap(), 1);
    for name in ["accepted", "running"] {
        assert!(store
            .get_command(&id("phone"), &id(name))
            .unwrap()
            .is_some());
    }
}
#[test]
fn capacity_rejects_new_work_without_evicting_running_but_retries_still_work() {
    let mut db = db();
    {
        let mut store = RemoteStore::new(&mut db, &id("prod"));
        add(&mut store);
        accept(&mut store, &request("original", 25), 0);
    }
    db.execute_batch("WITH RECURSIVE n(x) AS(SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<9999) INSERT INTO command_journal(server_identity,device_id,request_id,parameter_hash,server_epoch,operation_id,status,created_at_ms,updated_at_ms) SELECT 'prod','phone','r'||x,zeroblob(32),'epoch','op'||x,'running',0,0 FROM n;").unwrap();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    assert_eq!(
        store
            .accept_command(
                &id("phone"),
                &request("new", 25),
                &id("epoch"),
                &id("op"),
                86_400_001
            )
            .unwrap_err()
            .code,
        "busy"
    );
    assert!(matches!(
        accept(&mut store, &request("original", 25), 86_400_001),
        CommandAcceptance::Existing(_)
    ));
    assert_eq!(
        store
            .accept_command(
                &id("phone"),
                &request("original", 26),
                &id("epoch"),
                &id("op"),
                0
            )
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    assert_eq!(count(&db), 10_000);
    db.execute(
        "UPDATE command_journal SET status='succeeded' WHERE request_id='r1'",
        [],
    )
    .unwrap();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    assert!(matches!(
        accept(&mut store, &request("new", 25), 1),
        CommandAcceptance::New(_)
    ));
    assert_eq!(count(&db), 10_000);
}
#[test]
fn paired_device_ownership_is_required_for_journal_and_revocation_blocks_acceptance() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    assert_eq!(
        store
            .accept_command(&id("phone"), &request("r", 25), &id("epoch"), &id("op"), 0)
            .unwrap_err()
            .code,
        "unauthorized"
    );
    add(&mut store);
    store.revoke_device(&id("phone"), 1).unwrap();
    assert_eq!(
        store
            .accept_command(&id("phone"), &request("r", 25), &id("epoch"), &id("op"), 2)
            .unwrap_err()
            .code,
        "unauthorized"
    );
}
#[test]
fn pairing_completed_duplicate_is_already_issued_and_stores_only_safe_metadata() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    let req = parse_request(r#"{"v":1,"id":"pair","method":"pairing.begin","params":{},"expected":{"serverEpoch":"epoch"}}"#).unwrap();
    accept(&mut store, &req, 0);
    store
        .finish_pairing_issuance(&id("phone"), &id("pair"), &id("operation"), 1)
        .unwrap();
    assert_eq!(
        accept(&mut store, &req, 2),
        CommandAcceptance::AlreadyIssued {
            operation_id: id("operation")
        }
    );
    assert_eq!(
        store
            .get_command(&id("phone"), &id("pair"))
            .unwrap()
            .unwrap()
            .outcome
            .error_code,
        None
    );
}
#[test]
fn preferences_default_disabled_and_cas_is_identity_scoped() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    let mut preferences = store.preferences().unwrap();
    assert!(!preferences.enabled);
    assert_eq!(preferences.revision.value(), 1);
    preferences.enabled = true;
    assert_eq!(
        store
            .save_preferences(&preferences)
            .unwrap()
            .revision
            .value(),
        2
    );
    assert_eq!(
        store.save_preferences(&preferences).unwrap_err().code,
        "stale_state"
    );
    let dev = RemoteStore::new(&mut db, &id("dev"));
    assert!(!dev.preferences().unwrap().enabled);
}

#[test]
fn revision_exhaustion_does_not_commit_invalid_preferences() {
    let mut db = db();
    db.execute("INSERT INTO remote_preferences(server_identity,revision) VALUES('prod',9223372036854775807)",[]).unwrap();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    let mut preferences = store.preferences().unwrap();
    preferences.enabled = true;
    assert!(store.save_preferences(&preferences).is_err());
    let (kind, enabled): (String, bool) = db
        .query_row(
            "SELECT typeof(revision),enabled FROM remote_preferences",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "integer");
    assert!(!enabled);
}

#[test]
fn credentials_and_accepted_intent_survive_reopen_then_startup_recovers_without_execution() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    {
        let mut db = crate::storage::database::open(&path).unwrap();
        let mut store = RemoteStore::new(&mut db, &id("prod"));
        add(&mut store);
        accept(&mut store, &request("persisted", 25), 0);
    }
    let mut db = crate::storage::database::open(&path).unwrap();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    assert!(store.lookup_credential(&hash('a')).unwrap().is_some());
    assert_eq!(
        store
            .get_command(&id("phone"), &id("persisted"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Accepted
    );
    assert_eq!(store.recover_unfinished(100).unwrap(), 1);
    assert!(matches!(
        accept(&mut store, &request("persisted", 25), 101),
        CommandAcceptance::Existing(JournalEntry {
            outcome: JournalOutcome {
                status: OperationStatus::OutcomeUnknown,
                ..
            },
            ..
        })
    ));
}

#[test]
fn journal_ids_hashes_results_and_pruning_are_isolated_between_server_identities() {
    let mut db = db();
    for (identity, volume) in [("prod", 10), ("dev", 90)] {
        let mut store = RemoteStore::new(&mut db, &id(identity));
        add(&mut store);
        assert!(matches!(
            accept(&mut store, &request("same-id", volume), 0),
            CommandAcceptance::New(_)
        ));
        finish(&mut store, "same-id", 0);
    }
    {
        let mut prod = RemoteStore::new(&mut db, &id("prod"));
        assert!(matches!(
            accept(&mut prod, &request("same-id", 10), 1),
            CommandAcceptance::Existing(_)
        ));
        assert_eq!(prod.prune(86_400_000).unwrap(), 1);
    }
    let dev = RemoteStore::new(&mut db, &id("dev"));
    assert_eq!(
        dev.get_command(&id("phone"), &id("same-id"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Succeeded
    );
}

fn assert_pairing_without_issuance(status: OperationStatus) {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    let req = parse_request(r#"{"v":1,"id":"pair","method":"pairing.begin","params":{},"expected":{"serverEpoch":"epoch"}}"#).unwrap();
    accept(&mut store, &req, 0);
    let outcome = JournalOutcome {
        operation_id: id("operation"),
        status,
        error_code: None,
    };
    store
        .finish_command(&id("phone"), &id("pair"), &outcome, 1)
        .unwrap();
    let CommandAcceptance::Existing(entry) = accept(&mut store, &req, 2) else {
        panic!("Pairing without issuance must retain its original safe result");
    };
    assert_eq!(entry.outcome, outcome);
}
#[test]
fn pairing_preparation_success_is_not_secret_issuance() {
    assert_pairing_without_issuance(OperationStatus::Succeeded);
}
#[test]
fn failed_pairing_is_not_secret_issuance() {
    assert_pairing_without_issuance(OperationStatus::Failed);
}
#[test]
fn cancelled_pairing_is_not_secret_issuance() {
    assert_pairing_without_issuance(OperationStatus::Cancelled);
}
#[test]
fn unknown_pairing_is_not_secret_issuance() {
    assert_pairing_without_issuance(OperationStatus::OutcomeUnknown);
}

#[test]
fn successful_pairing_issuance_marker_is_atomic_and_rejects_non_pairing_work() {
    let mut db = db();
    let mut store = RemoteStore::new(&mut db, &id("prod"));
    add(&mut store);
    accept(&mut store, &request("volume", 25), 0);
    assert_eq!(
        store
            .finish_pairing_issuance(&id("phone"), &id("volume"), &id("operation"), 1)
            .unwrap_err()
            .code,
        "stale_state"
    );
    assert_eq!(
        store
            .get_command(&id("phone"), &id("volume"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Accepted
    );
    let req = parse_request(r#"{"v":1,"id":"pair","method":"pairing.begin","params":{},"expected":{"serverEpoch":"epoch"}}"#).unwrap();
    accept(&mut store, &req, 0);
    assert_eq!(
        store
            .finish_pairing_issuance(&id("phone"), &id("pair"), &id("wrong"), 1)
            .unwrap_err()
            .code,
        "stale_state"
    );
    assert_eq!(
        store
            .get_command(&id("phone"), &id("pair"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Accepted
    );
    store
        .finish_pairing_issuance(&id("phone"), &id("pair"), &id("operation"), 2)
        .unwrap();
    assert_eq!(
        accept(&mut store, &req, 3),
        CommandAcceptance::AlreadyIssued {
            operation_id: id("operation")
        }
    );
    assert_eq!(
        store
            .get_command(&id("phone"), &id("pair"))
            .unwrap()
            .unwrap()
            .outcome
            .status,
        OperationStatus::Succeeded
    );
}
