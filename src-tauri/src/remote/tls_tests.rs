use super::*;
use std::os::unix::fs::PermissionsExt;
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
        .unwrap();
    db.execute_batch(include_str!("../../migrations/0002_remote_control.sql"))
        .unwrap();
    db
}
#[test]
fn identity_persists_and_private_permissions_and_namespaces_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let a = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
        .unwrap();
    let b = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 2)
        .unwrap();
    assert_eq!(a.id(), b.id());
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert!(!b.requires_repairing());
    let dir = root.path().join("remote-development");
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dir.join("identity.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let production =
        ServerIdentity::load_or_create(root.path(), IdentityNamespace::Production, &mut db, 2)
            .unwrap();
    assert_ne!(production.id(), a.id());
    assert_ne!(production.fingerprint(), a.fingerprint());
}
#[test]
fn missing_key_keeps_uuid_but_revokes_old_credentials_and_requires_pairing() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let a = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
        .unwrap();
    db.execute("INSERT INTO paired_devices(server_identity,device_id,token_hash,label,scopes_json,created_at_ms) VALUES(?1,'phone',zeroblob(32),'Phone','[]',1)",[a.id().as_str()]).unwrap();
    fs::remove_file(root.path().join("remote-development/identity.json")).unwrap();
    let b = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 2)
        .unwrap();
    assert_eq!(a.id(), b.id());
    assert_ne!(a.fingerprint(), b.fingerprint());
    assert!(b.requires_repairing());
    let revoked: i64 = db
        .query_row("SELECT revoked_at_ms FROM paired_devices", [], |r| r.get(0))
        .unwrap();
    assert_eq!(revoked, 2);
}
#[test]
fn identity_rejects_symlinks_and_corruption_without_silent_rotation() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let _a =
        ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
            .unwrap();
    let path = root.path().join("remote-development/identity.json");
    fs::write(&path, b"bad").unwrap();
    assert!(ServerIdentity::load_or_create(
        root.path(),
        IdentityNamespace::Development,
        &mut db,
        2
    )
    .is_err());
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(root.path().join("outside"), path).unwrap();
    assert!(ServerIdentity::load_or_create(
        root.path(),
        IdentityNamespace::Development,
        &mut db,
        2
    )
    .is_err());
}
#[test]
fn phrase_is_six_11_bit_indices_with_fixed_wordlist_version() {
    let phrase = verification_phrase(&[0; 32]);
    assert_eq!(phrase.wordlist_version.as_str(), "bip39-en-v1");
    assert_eq!(
        phrase
            .words
            .as_slice()
            .iter()
            .map(|w| w.as_str())
            .collect::<Vec<_>>(),
        vec!["abandon"; 6]
    );
    let phrase = verification_phrase(&[255; 32]);
    assert_eq!(
        phrase
            .words
            .as_slice()
            .iter()
            .map(|w| w.as_str())
            .collect::<Vec<_>>(),
        vec!["zoo"; 6]
    );
    let mut value = [0; 32];
    value[8] = 0b0100_0000;
    assert_eq!(
        verification_phrase(&value).words.as_slice()[5].as_str(),
        "ability"
    );
}

/// Runs real rustls handshakes and encrypted records over an in-memory wire.
/// Queueing the secret BEFORE the handshake is deliberate: rustls must not put
/// application plaintext on the server until the custom verifier accepts the pin.
fn exchange(
    server: Arc<ServerConfig>,
    client: Arc<rustls::ClientConfig>,
    secret: &[u8],
) -> (bool, Vec<u8>) {
    let mut server = rustls::ServerConnection::new(server).unwrap();
    let mut client =
        rustls::ClientConnection::new(client, "mediacenter.local".try_into().unwrap()).unwrap();
    client.writer().write_all(secret).unwrap();
    let mut failed = false;
    for _ in 0..8 {
        let mut wire = Vec::new();
        client.write_tls(&mut wire).unwrap();
        server.read_tls(&mut std::io::Cursor::new(wire)).unwrap();
        if server.process_new_packets().is_err() {
            failed = true;
            break;
        }
        let mut wire = Vec::new();
        server.write_tls(&mut wire).unwrap();
        client.read_tls(&mut std::io::Cursor::new(wire)).unwrap();
        if client.process_new_packets().is_err() {
            failed = true;
            break;
        }
    }
    let mut received = Vec::new();
    let _ = server.reader().read_to_end(&mut received);
    (failed, received)
}
#[test]
fn actual_tls_pin_validation_precedes_qr_secret_and_existing_bearer_transmission() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let identity =
        ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
            .unwrap();
    for secret in [
        b"qr-one-use-secret".as_slice(),
        b"Authorization: Bearer existing-credential".as_slice(),
    ] {
        let (failed, received) = exchange(
            identity.server_config(),
            pinned_client_config(checked("0".repeat(64)).unwrap()),
            secret,
        );
        assert!(failed);
        assert!(
            received.is_empty(),
            "wrong certificate must receive no application secret"
        );
        let (failed, received) = exchange(
            identity.server_config(),
            pinned_client_config(identity.fingerprint()),
            secret,
        );
        assert!(!failed);
        assert_eq!(received, secret);
    }
}
#[test]
fn manual_probe_aborts_tls_and_code_requires_explicit_phrase_confirmation() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let identity =
        ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
            .unwrap();
    let probe = CertificateProbe::default();
    assert!(probe.observation().is_none());
    let (failed, received) = exchange(
        identity.server_config(),
        probe.client_config(),
        b"must-never-reach-server",
    );
    assert!(failed);
    assert!(received.is_empty());
    let observation = probe.observation().unwrap();
    assert_eq!(
        observation.phrase(),
        &verification_phrase(&Sha256::digest(identity.certificate().as_ref()).into())
    );
    assert!(probe.observation().unwrap().confirm_phrase(false).is_err());
    let confirmed = observation.confirm_phrase(true).unwrap();
    let (failed, received) = exchange(
        identity.server_config(),
        confirmed.client_config(),
        b"ABCDEF234567",
    );
    assert!(!failed);
    assert_eq!(received, b"ABCDEF234567");
    let other =
        ServerIdentity::load_or_create(root.path(), IdentityNamespace::Production, &mut db, 1)
            .unwrap();
    let (failed, received) = exchange(
        other.server_config(),
        confirmed.client_config(),
        b"ABCDEF234567",
    );
    assert!(failed);
    assert!(received.is_empty());
}
#[test]
fn missing_uuid_does_not_silently_reassign_an_existing_certificate() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
        .unwrap();
    fs::remove_file(root.path().join("remote-development/server-id")).unwrap();
    assert!(ServerIdentity::load_or_create(
        root.path(),
        IdentityNamespace::Development,
        &mut db,
        2
    )
    .is_err());
}

#[test]
fn confirming_displayed_phrase_a_cannot_authorize_later_probe_b() {
    let root = tempfile::tempdir().unwrap();
    let mut db = db();
    let a = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Development, &mut db, 1)
        .unwrap();
    let b = ServerIdentity::load_or_create(root.path(), IdentityNamespace::Production, &mut db, 1)
        .unwrap();
    let probe = CertificateProbe::default();
    let (failed, received) = exchange(
        a.server_config(),
        probe.client_config(),
        b"no-application-data",
    );
    assert!(failed);
    assert!(received.is_empty());
    let displayed_observation = probe.observation().unwrap();
    assert_eq!(
        displayed_observation.phrase(),
        &verification_phrase(&Sha256::digest(a.certificate().as_ref()).into())
    );
    // A retry/parallel handshake completes while A's confirmation is displayed.
    let (failed, received) = exchange(
        b.server_config(),
        probe.client_config(),
        b"no-application-data",
    );
    assert!(failed);
    assert!(received.is_empty());
    assert_eq!(probe.observation().unwrap().fingerprint(), &b.fingerprint());
    let confirmed = displayed_observation.confirm_phrase(true).unwrap();
    assert_eq!(
        confirmed.fingerprint(),
        &a.fingerprint(),
        "confirmation must bind the displayed observation A, never newly observed B"
    );
    let (failed, received) = exchange(
        b.server_config(),
        confirmed.client_config(),
        b"ABCDEF234567",
    );
    assert!(failed);
    assert!(received.is_empty());
    let (failed, received) = exchange(
        a.server_config(),
        confirmed.client_config(),
        b"ABCDEF234567",
    );
    assert!(!failed);
    assert_eq!(received, b"ABCDEF234567");
}
