use super::*;
use crate::{
    control::{drives::dispatch_with, ControlServices},
    remote::{
        dispatch::Dispatcher,
        store::{CredentialHash, RemoteStore},
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Fake {
    devices: Arc<Mutex<Devices>>,
    calls: Arc<Mutex<Vec<String>>>,
    failure: Arc<Mutex<Option<String>>>,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    wait: bool,
    wait_for_discover: Arc<AtomicBool>,
}
impl Fake {
    fn new() -> Self {
        let drive = Drive {
            connection_id: "insertion-1".into(),
            id: "drive:usb".into(),
            path: "/fake/drive".into(),
            name: "USB".into(),
            removable: true,
            safe_to_remove: true,
            can_power_off: true,
            sibling_id: "enclosure".into(),
        };
        let mut sibling = drive.clone();
        sibling.id = "drive:sibling".into();
        sibling.path = "/fake/sibling".into();
        sibling.name = "Sibling".into();
        let volume = |id: &str, drive: &str| Volume {
            id: id.into(),
            path: format!("/fake/{id}"),
            drive_id: drive.into(),
            identity: Some(format!("uuid:{id}")),
            mounts: vec![format!("/fake/media/{id}").into()],
            label: id.into(),
            size: 100,
            removable: true,
        };
        Self {
            devices: Arc::new(Mutex::new(Devices {
                drives: vec![drive, sibling],
                volumes: vec![
                    volume("Movies", "drive:usb"),
                    volume("Music", "drive:sibling"),
                ],
            })),
            calls: Default::default(),
            failure: Default::default(),
            entered: Default::default(),
            release: Default::default(),
            wait: false,
            wait_for_discover: Default::default(),
        }
    }
}
impl DeviceBackend for Fake {
    async fn discover(&self) -> Result<Devices, AppError> {
        if self.wait_for_discover.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(self.devices.lock().unwrap().clone())
    }
    async fn mount(&self, v: &Volume) -> Result<(), AppError> {
        self.calls.lock().unwrap().push("mount".into());
        if let Some(code) = self.failure.lock().unwrap().clone() {
            return Err(AppError::new(&code, "fake refusal"));
        }
        self.devices
            .lock()
            .unwrap()
            .volumes
            .iter_mut()
            .find(|a| a.id == v.id)
            .unwrap()
            .mounts = vec!["/fake/mounted".into()];
        Ok(())
    }
    async fn unmount(&self, v: &Volume) -> Result<(), AppError> {
        self.calls.lock().unwrap().push(format!("unmount:{}", v.id));
        if self.wait {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if let Some(code) = self.failure.lock().unwrap().clone() {
            return Err(AppError::new(&code, "fake refusal"));
        }
        self.devices
            .lock()
            .unwrap()
            .volumes
            .iter_mut()
            .find(|a| a.id == v.id)
            .unwrap()
            .mounts
            .clear();
        Ok(())
    }
    async fn power_off(&self, _: &Drive) -> Result<(), AppError> {
        self.calls.lock().unwrap().push("power_off".into());
        Ok(())
    }
}
#[derive(Clone, Default)]
struct Lifecycle(Arc<Mutex<Vec<String>>>);
impl SourceLifecycle for Lifecycle {
    type Guard = ();
    async fn quiesce(&self, volumes: &[Volume]) -> Result<(), AppError> {
        *self.0.lock().unwrap() = volumes.iter().map(|v| v.id.clone()).collect();
        Ok(())
    }
}
#[tokio::test]
async fn cancellation_during_drive_discovery_blocks_removal() {
    let (_dir, d, _) = fixture();
    let backend = Fake::new();
    let devices = backend.discover().await.unwrap();
    let state = &d.services.state;
    let action = capture(state, &devices, &devices.drives[0].id)
        .await
        .unwrap();
    let lifecycle = Lifecycle::default();
    let cancel = CancellationToken::new();
    backend.wait_for_discover.store(true, Ordering::SeqCst);
    let cancel_while_discovering = async {
        backend.entered.notified().await;
        cancel.cancel();
        backend.release.notify_one();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::join!(
            eject(state, &backend, &lifecycle, &action, &cancel),
            cancel_while_discovering
        )
    })
    .await
    .expect("drive removal did not stop after cancellation");
    assert_eq!(result.unwrap_err().code, "cancelled");
    assert!(lifecycle.0.lock().unwrap().is_empty());
    assert!(backend.calls.lock().unwrap().is_empty());
}
fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().join("data")).unwrap();
    let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
    let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","library_manage"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("a".repeat(43)).unwrap();
    RemoteStore::new(&mut state.db.lock().unwrap(), &services.server_identity)
        .add_device(&device, &CredentialHash::from_credential(&credential))
        .unwrap();
    (
        dir,
        Dispatcher::new(services),
        RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        },
    )
}
fn request(d: &Dispatcher, id: &str, method: &str, params: serde_json::Value) -> RemoteRequest {
    parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch()}}).to_string()).unwrap()
}
async fn prepare(d: &Dispatcher, o: &RemotePrincipal, b: &Fake) -> ConfirmationPreview {
    let handle = drive_handle(&b.devices.lock().unwrap().drives[0]);
    let value = dispatch_with(
        d,
        o,
        request(
            d,
            "prepare",
            "drives.prepareEject",
            serde_json::json!({"driveId":handle}),
        ),
        b.clone(),
        Lifecycle::default(),
    )
    .await
    .unwrap();
    serde_json::from_value::<ActivityPreparation>(value)
        .map(|p| match p {
            ActivityPreparation::ConfirmationRequired { preview } => *preview,
            _ => panic!(),
        })
        .unwrap()
}
async fn terminal(d: &Dispatcher, o: &RemotePrincipal, id: &Id) -> OperationState {
    for _ in 0..100 {
        let op = d
            .services
            .operations()
            .lookup_request(o, id)
            .unwrap()
            .unwrap();
        if !matches!(
            op.status,
            OperationStatus::Accepted | OperationStatus::Running
        ) {
            return op;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("not terminal")
}
#[tokio::test]
async fn preview_covers_sibling_volumes_owned_use_and_never_paths() {
    let (_dir, d, o) = fixture();
    let b = Fake::new();
    d.services.state.db.lock().unwrap().execute("INSERT INTO media_sources(id,source_type,label,root,kinds,availability) VALUES('source','local','Fake',CAST('/fake/media/Movies' AS BLOB),'[\"video\"]','available')",[]).unwrap();
    let _reader = d
        .services
        .state
        .resources
        .acquire("source", CancellationToken::new())
        .unwrap();
    let preview = prepare(&d, &o, &b).await;
    let labels = serde_json::to_value(&preview.affected_labels).unwrap();
    assert_eq!(
        labels,
        serde_json::json!([
            "Sibling",
            "USB",
            "Movies",
            "Music",
            "Open media files or photo presentation"
        ])
    );
    let wire = serde_json::to_string(&preview).unwrap();
    assert!(!wire.contains("/fake/"));
    assert!(b.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn replacement_and_replug_require_fresh_confirmation_without_effect() {
    for replug in [false, true] {
        let (_dir, d, o) = fixture();
        let b = Fake::new();
        let preview = prepare(&d, &o, &b).await;
        if replug {
            b.devices.lock().unwrap().drives[0].connection_id = "insertion-2".into();
        } else {
            b.devices.lock().unwrap().volumes[0].identity = Some("replacement".into());
        }
        let err = dispatch_with(
            &d,
            &o,
            request(
                &d,
                "confirm",
                "drives.confirmEject",
                serde_json::json!({"confirmationId":preview.confirmation_id}),
            ),
            b.clone(),
            Lifecycle::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ConfirmationStale);
        assert_ne!(
            err.replacement_preview.unwrap().confirmation_id,
            preview.confirmation_id
        );
        assert!(b.calls.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn foreign_busy_and_local_authorization_are_terminal_failures() {
    for (native, wire) in [
        ("device_busy", WireErrorCode::Busy),
        (
            "device_permission_denied",
            WireErrorCode::LocalAuthorizationRequired,
        ),
    ] {
        let (_dir, d, o) = fixture();
        let b = Fake::new();
        let preview = prepare(&d, &o, &b).await;
        *b.failure.lock().unwrap() = Some(native.into());
        let req = request(
            &d,
            "confirm",
            "drives.confirmEject",
            serde_json::json!({"confirmationId":preview.confirmation_id}),
        );
        dispatch_with(&d, &o, req.clone(), b.clone(), Lifecycle::default())
            .await
            .unwrap();
        let op = terminal(&d, &o, &req.id).await;
        assert_eq!(op.status, OperationStatus::Failed);
        assert_eq!(op.error_code, Some(wire));
        assert!(!b.calls.lock().unwrap().contains(&"power_off".into()));
    }
}
#[tokio::test]
async fn cancellation_before_effect_does_not_quiesce_or_mutate() {
    let (_dir, d, _o) = fixture();
    let b = Fake::new();
    let action = capture(&d.services.state, &b.discover().await.unwrap(), "drive:usb")
        .await
        .unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let lifecycle = Lifecycle::default();
    assert_eq!(
        eject(&d.services.state, &b, &lifecycle, &action, &token)
            .await
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert!(lifecycle.0.lock().unwrap().is_empty());
    assert!(b.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn accepted_eject_survives_disconnect_and_cancel_waits_for_actual_completion() {
    let (_dir, d, o) = fixture();
    let mut b = Fake::new();
    b.wait = true;
    let preview = prepare(&d, &o, &b).await;
    let lifecycle = Lifecycle::default();
    let req = request(
        &d,
        "confirm",
        "drives.confirmEject",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    let accepted = dispatch_with(&d, &o, req.clone(), b.clone(), lifecycle.clone())
        .await
        .unwrap();
    b.entered.notified().await;
    let cancel = request(
        &d,
        "cancel",
        "operations.cancel",
        serde_json::json!({"requestId":req.id}),
    );
    d.dispatch(&o, cancel).await.unwrap();
    let running = d
        .services
        .operations()
        .lookup_request(&o, &req.id)
        .unwrap()
        .unwrap();
    assert!(running.cancellation_requested);
    assert_eq!(running.status, OperationStatus::Running);
    // A replacement dispatcher models transport disconnect/reconnect; operation ownership is native.
    let reconnect = Dispatcher::new(d.services.clone());
    drop(d);
    b.release.notify_one();
    b.entered.notified().await;
    b.release.notify_one();
    let completed = terminal(&reconnect, &o, &req.id).await;
    assert_eq!(completed.status, OperationStatus::Succeeded);
    assert_eq!(lifecycle.0.lock().unwrap().len(), 2);
    assert_eq!(
        dispatch_with(&reconnect, &o, req, b.clone(), lifecycle)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(
        b.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|v| *v == "power_off")
            .count(),
        1
    );
}
#[test]
fn wire_never_admits_device_paths_force_or_deletion_into_typed_command() {
    let parsed=parse_request(&serde_json::json!({"v":1,"id":"request","method":"drives.mount","params":{"volumeId":"volume","force":true,"delete":true,"devicePath":"/dev/fake"},"expected":{"serverEpoch":"epoch"}}).to_string()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed.command).unwrap(),
        serde_json::json!({"method":"drives.mount","params":{"volumeId":"volume"}})
    );
    assert!(parse_request(&serde_json::json!({"v":1,"id":"request","method":"drives.mount","params":{"volumeId":"/dev/fake"},"expected":{"serverEpoch":"epoch"}}).to_string()).is_err());
}

#[tokio::test]
async fn insertion_bound_handles_and_missing_identity_fail_closed() {
    let (_dir, d, o) = fixture();
    let b = Fake::new();
    let devices = b.discover().await.unwrap();
    let list = inventory(&devices, 1).unwrap();
    let wire = serde_json::to_string(&list).unwrap();
    assert!(!wire.contains("/fake/"));
    assert!(!wire.contains("drive:usb"));
    let handle = drive_handle(&devices.drives[0]);
    assert!(checked::<Id, _>(handle.clone()).is_ok());
    b.devices.lock().unwrap().drives[0].connection_id = "replug".into();
    let request = request(
        &d,
        "old-handle",
        "drives.prepareEject",
        serde_json::json!({"driveId":handle}),
    );
    assert_eq!(
        dispatch_with(&d, &o, request, b.clone(), Lifecycle::default())
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    b.devices.lock().unwrap().drives[0].connection_id.clear();
    assert_eq!(
        capture(&d.services.state, &b.discover().await.unwrap(), "drive:usb")
            .await
            .err()
            .unwrap()
            .code,
        "unavailable"
    );
    assert!(b.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn mount_uses_only_inventory_handle_and_retains_native_authorization_failure() {
    for refusal in [false, true] {
        let (_dir, d, o) = fixture();
        let b = Fake::new();
        b.devices.lock().unwrap().volumes[0].mounts.clear();
        if refusal {
            *b.failure.lock().unwrap() = Some("device_permission_denied".into());
        }
        let devices = b.discover().await.unwrap();
        let handle = volume_handle(&devices.volumes[0], &devices.drives[0]);
        let req = request(
            &d,
            "mount",
            "drives.mount",
            serde_json::json!({"volumeId":handle,"force":true,"delete":true}),
        );
        dispatch_with(&d, &o, req.clone(), b.clone(), Lifecycle::default())
            .await
            .unwrap();
        let op = terminal(&d, &o, &req.id).await;
        assert_eq!(
            op.status,
            if refusal {
                OperationStatus::Failed
            } else {
                OperationStatus::Succeeded
            }
        );
        assert_eq!(
            op.error_code,
            if refusal {
                Some(WireErrorCode::LocalAuthorizationRequired)
            } else {
                None
            }
        );
        assert_eq!(*b.calls.lock().unwrap(), vec!["mount"]);
    }
}
#[tokio::test]
async fn replacement_after_acceptance_is_checked_before_owned_use_stops() {
    let (_dir, d, o) = fixture();
    let b = Fake::new();
    let preview = prepare(&d, &o, &b).await;
    let session_gate = d.services.state.session_gate.lock().await;
    let req = request(
        &d,
        "confirm",
        "drives.confirmEject",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    let lifecycle = Lifecycle::default();
    dispatch_with(&d, &o, req.clone(), b.clone(), lifecycle.clone())
        .await
        .unwrap();
    b.devices.lock().unwrap().drives[0].connection_id = "after-acceptance-replug".into();
    drop(session_gate);
    let op = terminal(&d, &o, &req.id).await;
    assert_eq!(op.error_code, Some(WireErrorCode::ConfirmationStale));
    assert!(lifecycle.0.lock().unwrap().is_empty());
    assert!(b.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn phone_owner_and_cancelled_preview_never_admit_eject() {
    let (_dir, d, o) = fixture();
    let b = Fake::new();
    let preview = prepare(&d, &o, &b).await;
    let other = RemotePrincipal {
        device_id: checked("other").unwrap(),
        scopes: o.scopes.clone(),
    };
    let req = request(
        &d,
        "confirm",
        "drives.confirmEject",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    assert_eq!(
        dispatch_with(&d, &other, req.clone(), b.clone(), Lifecycle::default())
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    d.dispatch(
        &o,
        request(
            &d,
            "cancel-preview",
            "confirmations.cancel",
            serde_json::json!({"confirmationId":preview.confirmation_id}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        dispatch_with(&d, &o, req, b.clone(), Lifecycle::default())
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert!(b.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn timed_out_native_eject_is_unknown_and_lookup_never_replays() {
    let (_dir, d, o) = fixture();
    let b = Fake::new();
    let preview = prepare(&d, &o, &b).await;
    *b.failure.lock().unwrap() = Some("device_timeout".into());
    let req = request(
        &d,
        "confirm",
        "drives.confirmEject",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    dispatch_with(&d, &o, req.clone(), b.clone(), Lifecycle::default())
        .await
        .unwrap();
    let op = terminal(&d, &o, &req.id).await;
    assert_eq!(op.status, OperationStatus::OutcomeUnknown);
    assert_eq!(op.error_code, Some(WireErrorCode::OutcomeUnknown));
    let result = d
        .dispatch(
            &o,
            request(
                &d,
                "lookup",
                "operations.get",
                serde_json::json!({"requestId":req.id}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "outcome_unknown");
    assert_eq!(b.calls.lock().unwrap().len(), 1);
}
