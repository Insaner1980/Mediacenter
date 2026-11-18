use super::*;
use crate::{
    app_state::AppState,
    control::ControlServices,
    remote::{
        auth::AuthManager,
        pairing::PairingManager,
        settings_admin::RemoteAdmin,
        store::CredentialHash,
        tls::{IdentityNamespace, ServerIdentity},
    },
};
use std::sync::{Arc, Mutex};
fn add(d: &Dispatcher, id: &str) {
    let device:PairedDevice=checked(serde_json::json!({"deviceId":id,"label":id,"scopes":["browse","settings"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked(format!("{id:0<43}")).unwrap();
    RemoteStore::new(
        &mut d.services.state.db.lock().unwrap(),
        &d.services.server_identity,
    )
    .add_device(&device, &CredentialHash::from_credential(&credential))
    .unwrap();
}
fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().join("app")).unwrap();
    let identity = ServerIdentity::load_or_create(
        dir.path(),
        IdentityNamespace::Development,
        &mut state.db.lock().unwrap(),
        1,
    )
    .unwrap();
    let mut s = ControlServices::native(state.clone(), identity.id().clone()).unwrap();
    s.admin = Some(Arc::new(RemoteAdmin::new(
        Arc::new(AuthManager::new(identity.id(), state.event_hub.clone()).unwrap()),
        Arc::new(Mutex::new(PairingManager::new(&identity))),
        &s,
    )));
    s.admin
        .as_ref()
        .unwrap()
        .replace_network(vec!["192.168.1.2:47832".parse().unwrap()])
        .unwrap();
    let d = Dispatcher::new(s);
    add(&d, "phone");
    add(&d, "other");
    let owner =
        checked(serde_json::json!({"deviceId":"phone","scopes":["browse","settings"]})).unwrap();
    (dir, d, owner)
}
fn req(d: &Dispatcher, id: &str, method: &str, params: serde_json::Value) -> RemoteRequest {
    parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch()}}).to_string()).unwrap()
}
async fn prepare(
    d: &Dispatcher,
    o: &RemotePrincipal,
    method: &str,
    params: serde_json::Value,
) -> Id {
    let value = dispatch(d, o, req(d, "prepare", method, params))
        .await
        .unwrap();
    checked(value["preview"]["confirmationId"].clone()).unwrap()
}
#[tokio::test]
async fn pairing_is_confirmed_once_and_only_secret_free_issuance_is_retained() {
    let (_dir, d, o) = fixture();
    let id = prepare(&d, &o, "pairing.begin", serde_json::json!({})).await;
    let r = req(
        &d,
        "issue",
        "pairing.begin",
        serde_json::json!({"confirmationId":id}),
    );
    let first = dispatch(&d, &o, r.clone()).await.unwrap();
    assert_eq!(first["status"], "ready");
    let second = dispatch(&d, &o, r).await.unwrap();
    assert_eq!(second["status"], "already_issued");
    assert!(second.get("candidate").is_none());
    assert_eq!(first["operationId"], second["operationId"]);
    let candidate: PairingCandidate = checked(first["candidate"].clone()).unwrap();
    let rows: QrRows = checked(first["qrRows"].clone()).unwrap();
    assert_eq!(rows, crate::remote::pairing::qr_rows(&candidate).unwrap());
    assert!(serde_json::to_vec(&first).unwrap().len() < 65536);
    assert!(!format!("{rows:?}").contains(rows.as_slice()[0].as_str()));
    let new_id = dispatch(
        &d,
        &o,
        req(
            &d,
            "again",
            "pairing.begin",
            serde_json::json!({"confirmationId":id}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(new_id["status"], "already_issued");
    let db = d.services.state.db.lock().unwrap();
    let row: (String, bool) = db
        .query_row(
            "SELECT status,pairing_issuance FROM command_journal WHERE request_id='issue'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, ("succeeded".into(), true));
    let schema: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='command_journal'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!schema.contains("secret"));
    assert!(!schema.contains("candidate"));
}

#[test]
fn qr_rows_reject_non_square_non_binary_and_out_of_bounds() {
    for rows in [
        vec!["0".repeat(20); 20],
        vec!["0".repeat(22); 21],
        vec!["x".repeat(21); 21],
        vec!["0".repeat(178); 178],
    ] {
        assert!(checked::<QrRows, _>(rows).is_err());
    }
}

#[tokio::test]
async fn concurrent_confirmation_issues_once_and_last_seen_is_not_a_resource_change() {
    let (_dir, d, o) = fixture();
    let id = prepare(&d, &o, "pairing.begin", serde_json::json!({})).await;
    d.services
        .state
        .db
        .lock()
        .unwrap()
        .execute("UPDATE paired_devices SET last_seen_at_ms=100", [])
        .unwrap();
    let first = req(
        &d,
        "race-a",
        "pairing.begin",
        serde_json::json!({"confirmationId":id}),
    );
    let second = req(
        &d,
        "race-b",
        "pairing.begin",
        serde_json::json!({"confirmationId":id}),
    );
    let (a, b) = tokio::join!(dispatch(&d, &o, first), dispatch(&d, &o, second));
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(results.iter().filter(|v| v["status"] == "ready").count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|v| v["status"] == "already_issued")
            .count(),
        1
    );
}

#[tokio::test]
async fn changed_owner_scope_and_changed_target_identity_cannot_revoke() {
    let (_dir, d, o) = fixture();
    let id = prepare(
        &d,
        &o,
        "devices.revoke",
        serde_json::json!({"deviceId":"other"}),
    )
    .await;
    d.services
        .state
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE paired_devices SET label='Renamed' WHERE device_id='other'",
            [],
        )
        .unwrap();
    let stale = dispatch(
        &d,
        &o,
        req(
            &d,
            "stale-revoke",
            "devices.revoke",
            serde_json::json!({"confirmationId":id}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, ErrorCode::ConfirmationStale);
    let replay = dispatch(
        &d,
        &o,
        req(
            &d,
            "stale-revoke",
            "devices.revoke",
            serde_json::json!({"confirmationId":id}),
        ),
    )
    .await
    .unwrap_err();
    assert!(serde_json::to_string(&replay).is_ok());
    let replacement = stale.replacement_preview.unwrap().confirmation_id;
    d.services
        .state
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE paired_devices SET scopes_json='[\"browse\"]' WHERE device_id='phone'",
            [],
        )
        .unwrap();
    assert_eq!(
        dispatch(
            &d,
            &o,
            req(
                &d,
                "scope-revoke",
                "devices.revoke",
                serde_json::json!({"confirmationId":replacement})
            )
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Forbidden
    );
    assert_eq!(
        d.services
            .admin
            .as_ref()
            .unwrap()
            .state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM paired_devices WHERE revoked_at_ms IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
}
#[tokio::test]
async fn self_revocation_closes_socket_and_owner_issued_candidate() {
    let (_dir, d, o) = fixture();
    let p = prepare(&d, &o, "pairing.begin", serde_json::json!({})).await;
    dispatch(
        &d,
        &o,
        req(
            &d,
            "issue",
            "pairing.begin",
            serde_json::json!({"confirmationId":p}),
        ),
    )
    .await
    .unwrap();
    let subscription = d
        .services
        .state
        .event_hub
        .subscribe_owner(Some(o.device_id.clone()));
    let prepared = dispatch(
        &d,
        &o,
        req(
            &d,
            "revoke-prepare",
            "devices.revoke",
            serde_json::json!({"deviceId":o.device_id}),
        ),
    )
    .await
    .unwrap();
    let value = dispatch(
        &d,
        &o,
        req(
            &d,
            "revoke",
            "devices.revoke",
            serde_json::json!({"confirmationId":prepared["preview"]["confirmationId"]}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(value["status"], "accepted");
    assert!(subscription.revocation().is_cancelled());
    let admin = d.services.admin.as_ref().unwrap();
    assert!(admin.pairing.lock().unwrap().active_attempt().is_none());
    assert!(admin.authorize(&o).is_err());
}
#[tokio::test]
async fn wrong_owner_expiry_and_listener_rebind_cannot_issue_a_candidate() {
    let (_dir, d, o) = fixture();
    let id = prepare(&d, &o, "pairing.begin", serde_json::json!({})).await;
    let other = RemotePrincipal {
        device_id: checked("other").unwrap(),
        scopes: o.scopes.clone(),
    };
    let r = req(
        &d,
        "wrong",
        "pairing.begin",
        serde_json::json!({"confirmationId":id}),
    );
    assert_eq!(
        dispatch(&d, &other, r).await.unwrap_err().code,
        ErrorCode::NotFound
    );
    d.services
        .admin
        .as_ref()
        .unwrap()
        .replace_network(vec!["192.168.1.3:47832".parse().unwrap()])
        .unwrap();
    let stale = dispatch(
        &d,
        &o,
        req(
            &d,
            "stale",
            "pairing.begin",
            serde_json::json!({"confirmationId":id}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, ErrorCode::ConfirmationStale);
    let fresh = stale.replacement_preview.unwrap().confirmation_id;
    d.services
        .confirmations
        .lock()
        .unwrap()
        .pending
        .get_mut(fresh.as_str())
        .unwrap()
        .expires_at = 0;
    assert_eq!(
        dispatch(
            &d,
            &o,
            req(
                &d,
                "expired",
                "pairing.begin",
                serde_json::json!({"confirmationId":fresh})
            )
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::ConfirmationExpired
    );
    assert!(d
        .services
        .admin
        .as_ref()
        .unwrap()
        .pairing
        .lock()
        .unwrap()
        .active_attempt()
        .is_none());
}
#[tokio::test]
async fn capacity_scope_disable_and_owner_revocation_fail_closed() {
    let (_dir, d, o) = fixture();
    let limited = RemotePrincipal {
        device_id: o.device_id.clone(),
        scopes: checked(vec!["browse"]).unwrap(),
    };
    assert_eq!(
        dispatch(
            &d,
            &limited,
            req(&d, "scope", "devices.list", serde_json::json!({}))
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Forbidden
    );
    for i in 0..6 {
        add(&d, &format!("extra-{i}"));
    }
    assert_eq!(
        dispatch(
            &d,
            &o,
            req(&d, "full", "pairing.begin", serde_json::json!({}))
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Busy
    );
    let admin = d.services.admin.as_ref().unwrap();
    admin.replace_network(Vec::new()).unwrap();
    assert_eq!(
        dispatch(
            &d,
            &o,
            req(&d, "disabled", "pairing.begin", serde_json::json!({}))
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Unavailable
    );
    admin.revoke(&o.device_id).unwrap();
    assert_eq!(
        dispatch(
            &d,
            &o,
            req(&d, "revoked", "devices.list", serde_json::json!({}))
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Forbidden
    );
}
#[tokio::test]
async fn failed_issuance_journal_write_cancels_candidate() {
    let (_dir, d, o) = fixture();
    let id = prepare(&d, &o, "pairing.begin", serde_json::json!({})).await;
    d.services.state.db.lock().unwrap().execute_batch("CREATE TRIGGER deny_issue BEFORE UPDATE ON command_journal WHEN NEW.pairing_issuance=1 BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
    let error = dispatch(
        &d,
        &o,
        req(
            &d,
            "issue",
            "pairing.begin",
            serde_json::json!({"confirmationId":id}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::OutcomeUnknown);
    assert!(d
        .services
        .admin
        .as_ref()
        .unwrap()
        .pairing
        .lock()
        .unwrap()
        .active_attempt()
        .is_none());
}
