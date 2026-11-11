use super::*;
use crate::remote::store::CredentialHash;
fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal) {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
    let identity: Id = checked("server").unwrap();
    let services = ControlServices::native(state.clone(), identity.clone()).unwrap();
    let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("a".repeat(43)).unwrap();
    RemoteStore::new(&mut state.db.lock().unwrap(), &identity)
        .add_device(&device, &CredentialHash::from_credential(&credential))
        .unwrap();
    let principal = RemotePrincipal {
        device_id: device.device_id,
        scopes: device.scopes,
    };
    (dir, Dispatcher::new(services), principal)
}
fn pause(dispatcher: &Dispatcher, id: &str, paused: bool) -> RemoteRequest {
    parse_request(&serde_json::json!({"v":1,"id":id,"method":"player.pause","params":{"sessionId":"session","paused":paused},"expected":{"serverEpoch":dispatcher.services.state.event_hub.epoch(),"sessionId":"session"}}).to_string()).unwrap()
}
async fn settled(dispatcher: &Dispatcher) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !dispatcher
            .services
            .state
            .operations
            .lock()
            .unwrap()
            .is_empty()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn eight_queued_native_jobs_per_device_and_duplicates_do_not_take_another_slot() {
    let (_dir, dispatcher, principal) = fixture();
    let player = dispatcher.services.state.player.lock().await;
    let mut first = None;
    for id in 0..8 {
        let result = dispatcher
            .dispatch(
                &principal,
                pause(&dispatcher, &format!("request{id}"), true),
            )
            .await
            .unwrap();
        if id == 0 {
            first = Some(result);
        }
    }
    assert_eq!(
        dispatcher
            .dispatch(&principal, pause(&dispatcher, "ninth", true))
            .await
            .unwrap_err()
            .code,
        ErrorCode::Busy
    );
    assert_eq!(
        dispatcher
            .dispatch(&principal, pause(&dispatcher, "request0", true))
            .await
            .unwrap(),
        first.unwrap()
    );
    assert_eq!(
        dispatcher
            .dispatch(&principal, pause(&dispatcher, "request0", false))
            .await
            .unwrap_err()
            .code,
        ErrorCode::RequestIdConflict
    );
    assert_eq!(
        dispatcher
            .services
            .state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        8
    );
    drop(player);
    settled(&dispatcher).await;
    assert!(dispatcher
        .dispatch(&principal, pause(&dispatcher, "after", true))
        .await
        .is_ok());
    settled(&dispatcher).await;
}
#[tokio::test]
async fn durable_cancellation_ack_replays_without_target_lookup_or_repeated_effect() {
    let (_dir, dispatcher, principal) = fixture();
    let player = dispatcher.services.state.player.lock().await;
    let accepted = dispatcher
        .dispatch(&principal, pause(&dispatcher, "target", true))
        .await
        .unwrap();
    let target: Id = checked(&accepted["operationId"]).unwrap();
    let request=parse_request(&serde_json::json!({"v":1,"id":"cancel","method":"operations.cancel","params":{"operationId":target},"expected":{"serverEpoch":dispatcher.services.state.event_hub.epoch()}}).to_string()).unwrap();
    let result = dispatcher
        .dispatch(&principal, request.clone())
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!({"cancellationRequested":true}));
    let active_cancelled = dispatcher
        .services
        .state
        .operations
        .lock()
        .unwrap()
        .get(target.as_str())
        .map(tokio_util::sync::CancellationToken::is_cancelled);
    if let Some(cancelled) = active_cancelled {
        assert!(cancelled);
    } else {
        // A fast pre-effect cancellation may already have persisted its terminal outcome.
        assert_eq!(
            dispatcher
                .services
                .operations()
                .lookup(&principal, &target)
                .unwrap()
                .unwrap()
                .status,
            OperationStatus::Cancelled
        );
    }
    drop(player);
    settled(&dispatcher).await;
    dispatcher
        .services
        .state
        .db
        .lock()
        .unwrap()
        .execute("DELETE FROM command_journal WHERE request_id='target'", [])
        .unwrap();
    assert_eq!(
        dispatcher.dispatch(&principal, request).await.unwrap(),
        result
    );
    let operation = dispatcher
        .services
        .operations()
        .lookup_request(&principal, &checked("cancel").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Succeeded);
}
#[tokio::test]
async fn transient_input_and_scope_failures_never_reach_the_persistent_journal() {
    let (_dir, dispatcher, mut principal) = fixture();
    let request=parse_request(&serde_json::json!({"v":1,"id":"search","method":"ui.search","params":{"rendererGeneration":"1","focusRevision":"1","route":"home","text":"private-password","editSequence":"1"},"expected":{"serverEpoch":dispatcher.services.state.event_hub.epoch()}}).to_string()).unwrap();
    principal.scopes = checked(vec!["browse", "navigation"]).unwrap();
    assert_eq!(
        dispatcher
            .dispatch(&principal, request)
            .await
            .unwrap_err()
            .code,
        ErrorCode::RendererUnavailable
    );
    principal.scopes = checked(vec!["browse"]).unwrap();
    assert_eq!(
        dispatcher
            .dispatch(&principal, pause(&dispatcher, "forbidden", true))
            .await
            .unwrap_err()
            .code,
        ErrorCode::Forbidden
    );
    assert_eq!(
        dispatcher
            .services
            .state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn stale_epoch_cannot_cancel_an_operation() {
    let (_dir, dispatcher, principal) = fixture();
    let player = dispatcher.services.state.player.lock().await;
    let accepted = dispatcher
        .dispatch(&principal, pause(&dispatcher, "target", true))
        .await
        .unwrap();
    let target: Id = checked(&accepted["operationId"]).unwrap();
    let request=parse_request(&serde_json::json!({"v":1,"id":"stale-cancel","method":"operations.cancel","params":{"operationId":target},"expected":{"serverEpoch":"old-epoch"}}).to_string()).unwrap();
    assert_eq!(
        dispatcher
            .dispatch(&principal, request)
            .await
            .unwrap_err()
            .code,
        ErrorCode::StaleState
    );
    assert!(!dispatcher
        .services
        .state
        .operations
        .lock()
        .unwrap()
        .get(target.as_str())
        .unwrap()
        .is_cancelled());
    drop(player);
    settled(&dispatcher).await;
}

#[tokio::test]
async fn task9_absent_and_foreign_operation_lookup_are_indistinguishable_not_found() {
    let (_dir, dispatcher, principal) = fixture();
    let accepted = dispatcher
        .dispatch(&principal, pause(&dispatcher, "owned", true))
        .await
        .unwrap();
    settled(&dispatcher).await;
    let foreign: RemotePrincipal =
        checked(serde_json::json!({"deviceId":"other","scopes":["browse"]})).unwrap();
    for (caller, params) in [
        (&principal, serde_json::json!({"operationId":"absent"})),
        (
            &foreign,
            serde_json::json!({"operationId":accepted["operationId"]}),
        ),
        (&foreign, serde_json::json!({"requestId":"owned"})),
    ] {
        let request = parse_request(
            &serde_json::json!({"v":1,"id":"lookup","method":"operations.get","params":params})
                .to_string(),
        )
        .unwrap();
        assert_eq!(
            dispatcher.dispatch(caller, request).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }
}

async fn activity_fixture() -> (
    tempfile::TempDir,
    Dispatcher,
    RemotePrincipal,
    std::path::PathBuf,
) {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let (dir, dispatcher, owner) = fixture();
    let media = dir.path().join("target.audio");
    std::fs::write(&media, b"first").unwrap();
    let metadata = std::fs::metadata(&media).unwrap();
    {
        let db = dispatcher.services.state.db.lock().unwrap();
        db.execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('source','local',?,'Test','[]','available')",[dir.path().as_os_str().as_bytes()]).unwrap();
        db.execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('target','source',?,'Target','audio',? ,?,'available')",rusqlite::params![b"target.audio".as_slice(),metadata.len() as i64,format!("{}:{}",metadata.mtime(),metadata.mtime_nsec())]).unwrap();
    }
    crate::photos::presenter::seed_test_session(&dispatcher.services.state, "previewed-photo")
        .await;
    (dir, dispatcher, owner, media)
}
fn activity_request(
    dispatcher: &Dispatcher,
    id: &str,
    method: &str,
    params: serde_json::Value,
) -> RemoteRequest {
    parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":dispatcher.services.state.event_hub.epoch()}}).to_string()).unwrap()
}
#[tokio::test]
async fn task9_activity_preview_binds_target_file_identity_and_returns_fresh_preview() {
    use std::os::unix::fs::MetadataExt;
    let (_dir, dispatcher, owner, media) = activity_fixture().await;
    let preview = dispatcher
        .dispatch(
            &owner,
            activity_request(
                &dispatcher,
                "prepare",
                "activity.prepare",
                serde_json::json!({"kind":"music","itemId":"target"}),
            ),
        )
        .await
        .unwrap();
    let confirmation = preview["preview"]["confirmationId"].clone();
    std::fs::write(&media, b"rescanned replacement media").unwrap();
    let metadata = std::fs::metadata(&media).unwrap();
    dispatcher
        .services
        .state
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE media_items SET size=?,mtime_ns=? WHERE id='target'",
            rusqlite::params![
                metadata.len() as i64,
                format!("{}:{}", metadata.mtime(), metadata.mtime_nsec())
            ],
        )
        .unwrap();
    let result = dispatcher
        .dispatch(
            &owner,
            activity_request(
                &dispatcher,
                "confirm",
                "activity.confirm",
                serde_json::json!({"confirmationId":confirmation}),
            ),
        )
        .await;
    let error = result.unwrap_err();
    assert_eq!(error.code, ErrorCode::ConfirmationStale);
    assert_ne!(
        serde_json::to_value(error.replacement_preview.unwrap().confirmation_id).unwrap(),
        confirmation
    );
    assert_eq!(
        crate::photos::presenter::snapshot(&dispatcher.services.state)
            .await
            .session_id
            .as_deref(),
        Some("previewed-photo")
    );
}
#[tokio::test]
async fn task9_replaced_preview_does_not_replay_as_successful_activity() {
    let (_dir, dispatcher, owner, _media) = activity_fixture().await;
    let first = activity_request(
        &dispatcher,
        "first-prepare",
        "activity.prepare",
        serde_json::json!({"kind":"music","itemId":"target"}),
    );
    dispatcher.dispatch(&owner, first.clone()).await.unwrap();
    settled(&dispatcher).await;
    dispatcher
        .dispatch(
            &owner,
            activity_request(
                &dispatcher,
                "second-prepare",
                "activity.prepare",
                serde_json::json!({"kind":"music","itemId":"target"}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        dispatcher.dispatch(&owner, first).await.unwrap_err().code,
        ErrorCode::OutcomeUnknown
    );
}

#[tokio::test]
async fn task9_native_queue_cas_is_observed_after_acceptance_and_desktop_reorder() {
    let (_dir, dispatcher, owner) = fixture();
    let state = &dispatcher.services.state;
    let items = (0..3)
        .map(|n| crate::music::queue::MusicItem {
            media_id: format!("m{n}"),
            title: format!("M{n}"),
            artist: None,
            album: None,
            availability: "missing".into(),
        })
        .collect();
    state.music.lock().await.replace(items, 0).unwrap();
    let revision = state.music.lock().await.revision;
    let gate = state.music_gate.lock().await;
    let request = |id: &str, rev: u32| {
        parse_request(&serde_json::json!({"v":1,"id":id,"method":"queue.remove","params":{"entryId":"queue-entry-1"},"expected":{"serverEpoch":state.event_hub.epoch(),"queueRevision":rev.to_string()}}).to_string()).unwrap()
    };
    let accepted = dispatcher
        .dispatch(&owner, request("remove", revision))
        .await
        .unwrap();
    assert_eq!(accepted["status"], "accepted");
    state
        .music
        .lock()
        .await
        .edit(
            revision,
            crate::music::queue::MusicAction::MoveItem { from: 1, to: 2 },
            vec![],
            0,
        )
        .unwrap();
    drop(gate);
    settled(&dispatcher).await;
    let failed = dispatcher
        .services
        .operations()
        .lookup_request(&owner, &checked("remove").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, OperationStatus::Failed);
    assert_eq!(failed.error_code, Some(ErrorCode::StaleState));
    let revision = state.music.lock().await.revision;
    dispatcher
        .dispatch(&owner, request("remove-current-revision", revision))
        .await
        .unwrap();
    settled(&dispatcher).await;
    assert_eq!(
        state
            .music
            .lock()
            .await
            .snapshot()
            .items
            .iter()
            .map(|e| e.entry_id)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[tokio::test]
async fn task9_pause_missing_session_finishes_with_typed_stale_state() {
    let (_dir, dispatcher, owner) = fixture();
    dispatcher
        .dispatch(&owner, pause(&dispatcher, "stale-pause", true))
        .await
        .unwrap();
    settled(&dispatcher).await;
    let result = dispatcher
        .services
        .operations()
        .lookup_request(&owner, &checked("stale-pause").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(result.error_code, Some(ErrorCode::StaleState));
}
#[tokio::test]
async fn task9_activity_preview_revalidates_native_queue_revision() {
    let (_dir, dispatcher, owner, _media) = activity_fixture().await;
    let preview = dispatcher
        .dispatch(
            &owner,
            activity_request(
                &dispatcher,
                "queue-prepare",
                "activity.prepare",
                serde_json::json!({"kind":"music","itemId":"target"}),
            ),
        )
        .await
        .unwrap();
    let revision = dispatcher.services.state.music.lock().await.revision;
    dispatcher
        .services
        .state
        .music
        .lock()
        .await
        .edit(
            revision,
            crate::music::queue::MusicAction::SetShuffle { enabled: true },
            vec![],
            0,
        )
        .unwrap();
    let error = dispatcher
        .dispatch(
            &owner,
            activity_request(
                &dispatcher,
                "queue-confirm",
                "activity.confirm",
                serde_json::json!({"confirmationId":preview["preview"]["confirmationId"]}),
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ConfirmationStale);
    assert!(error.replacement_preview.is_some());
}
