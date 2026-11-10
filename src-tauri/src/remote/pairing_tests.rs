use super::*;
use crate::remote::tls::{IdentityNamespace, ServerIdentity};
use rusqlite::Connection;
use std::net::{IpAddr, Ipv4Addr};
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
        .unwrap();
    db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
        .unwrap();
    db
}
fn setup() -> (tempfile::TempDir, Connection, PairingManager) {
    let dir = tempfile::tempdir().unwrap();
    let mut db = db();
    let identity =
        ServerIdentity::load_or_create(dir.path(), IdentityNamespace::Development, &mut db, 100)
            .unwrap();
    (dir, db, PairingManager::new(&identity))
}
fn peer(n: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(192, 168, 1, n))
}
fn request(ready: &PairingReady) -> PairRequest {
    PairRequest {
        attempt_id: ready.candidate.attempt_id.clone(),
        secret: ready.candidate.secret.clone(),
        device_label: checked("Phone ä").unwrap(),
    }
}
fn begin(manager: &mut PairingManager) -> PairingReady {
    manager
        .begin(
            checked(serde_json::json!(["192.168.1.2"])).unwrap(),
            checked(47832).unwrap(),
        )
        .unwrap()
}
#[test]
fn pairing_requires_window_and_is_single_use_with_hash_only_storage() {
    let (_dir, mut db, mut manager) = setup();
    assert!(manager.manual_challenge().is_err());
    let ready = begin(&mut manager);
    let request = request(&ready);
    let paired = manager.pair_qr(peer(1), &request, &mut db, 123).unwrap();
    assert!(manager.pair_qr(peer(1), &request, &mut db, 123).is_err());
    assert_eq!(paired.scopes.as_slice().len(), 7);
    let bytes: Vec<u8> = db
        .query_row("SELECT token_hash FROM paired_devices", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bytes.len(), 32);
    assert_ne!(bytes, paired.credential.as_str().as_bytes());
}
#[test]
fn expiry_is_monotonic_at_120_seconds_and_shared_between_manual_and_qr() {
    let (_dir, mut db, mut manager) = setup();
    let ready = begin(&mut manager);
    manager.advance_for_test(Duration::from_millis(119999));
    assert!(manager.manual_challenge().is_ok());
    manager.advance_for_test(Duration::from_millis(1));
    assert!(manager.manual_challenge().is_err());
    assert!(manager
        .pair_qr(peer(1), &request(&ready), &mut db, 1)
        .is_err());
}
#[test]
fn fifth_failed_secret_closes_attempt_and_peer_limit_survives_new_window() {
    let (_dir, mut db, mut manager) = setup();
    let ready = begin(&mut manager);
    let mut bad = request(&ready);
    bad.secret = checked("A".repeat(43)).unwrap();
    for _ in 0..5 {
        assert!(manager.pair_qr(peer(1), &bad, &mut db, 1).is_err());
    }
    assert!(manager.manual_challenge().is_err());
    let ready = begin(&mut manager);
    assert!(manager
        .pair_qr(peer(1), &request(&ready), &mut db, 1)
        .is_err());
    manager.advance_for_test(Duration::from_secs(60));
    assert!(manager
        .pair_qr(peer(1), &request(&ready), &mut db, 1)
        .is_ok());
}
#[test]
fn global_failure_limit_covers_rotating_peers_and_attempts() {
    let (_dir, mut db, mut manager) = setup();
    for n in 1..=20 {
        let ready = begin(&mut manager);
        let mut bad = request(&ready);
        bad.secret = checked("A".repeat(43)).unwrap();
        assert!(manager.pair_qr(peer(n), &bad, &mut db, 1).is_err());
    }
    let ready = begin(&mut manager);
    assert!(manager
        .pair_qr(peer(21), &request(&ready), &mut db, 1)
        .is_err());
    manager.advance_for_test(Duration::from_secs(60));
    assert!(manager
        .pair_qr(peer(21), &request(&ready), &mut db, 1)
        .is_ok());
}
#[test]
fn wrong_attempt_and_no_window_failures_are_rate_limited() {
    let (_dir, mut db, mut manager) = setup();
    let ready = begin(&mut manager);
    let mut bad = request(&ready);
    bad.attempt_id = checked("other").unwrap();
    for _ in 0..5 {
        assert!(manager.pair_qr(peer(1), &bad, &mut db, 1).is_err());
    }
    assert!(manager
        .pair_qr(peer(1), &request(&ready), &mut db, 1)
        .is_err());
}
#[test]
fn manual_success_consumes_qr_and_device_limit_preserves_attempt() {
    let (_dir, mut db, mut manager) = setup();
    for _ in 0..8 {
        let ready = begin(&mut manager);
        let challenge = manager.manual_challenge().unwrap();
        assert_eq!(challenge.attempt_id, ready.candidate.attempt_id);
        let manual = ManualPairRequest {
            attempt_id: challenge.attempt_id,
            code: ready.manual_code.clone(),
            device_label: checked("Phone").unwrap(),
        };
        assert!(manager.pair_manual(peer(1), &manual, &mut db, 1).is_ok());
        assert!(manager
            .pair_qr(peer(1), &request(&ready), &mut db, 1)
            .is_err());
        manager.advance_for_test(Duration::from_secs(60));
    }
    let ready = begin(&mut manager);
    assert!(manager
        .pair_qr(peer(1), &request(&ready), &mut db, 1)
        .is_err());
    assert!(manager.manual_challenge().is_ok());
}
#[test]
fn endpoint_rejects_ambiguous_or_unbounded_documents() {
    assert!(matches!(
        parse_pair_request(r#"{"v":1,"mode":"manual_challenge"}"#).unwrap(),
        PairEndpointRequest::ManualChallenge(_)
    ));
    for input in [
        r#"{"v":1,"mode":"manual_challenge","secret":"leak"}"#.to_owned(),
        r#"{"v":1,"v":1,"mode":"manual_challenge"}"#.to_owned(),
        r#"{"v":2,"mode":"manual_challenge"}"#.to_owned(),
        " ".repeat(65537),
    ] {
        assert!(parse_pair_request(&input).is_err());
    }
}

#[test]
fn desktop_status_hides_expired_consumed_and_canceled_attempts_without_canceling_a_newer_one() {
    let (_dir, mut db, mut manager) = setup();
    let old = begin(&mut manager);
    let current = begin(&mut manager);
    manager.close_attempt(&old.candidate.attempt_id);
    let (id, remaining) = manager.active_attempt().unwrap();
    assert_eq!(id, current.candidate.attempt_id);
    assert!(remaining > Duration::from_secs(119));
    manager
        .pair_qr(peer(1), &request(&current), &mut db, 100)
        .unwrap();
    assert!(manager.active_attempt().is_none());
    begin(&mut manager);
    manager.advance_for_test(Duration::from_secs(120));
    assert!(manager.active_attempt().is_none());
    let cancel = begin(&mut manager);
    manager.close_attempt(&cancel.candidate.attempt_id);
    assert!(manager.active_attempt().is_none());
}
