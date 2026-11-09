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
    assert!(dispatcher
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
    let request=parse_request(r#"{"v":1,"id":"search","method":"ui.search","params":{"rendererGeneration":"1","focusRevision":"1","route":"home","text":"private-password","editSequence":"1"},"expected":{"serverEpoch":"epoch"}}"#).unwrap();
    principal.scopes = checked(vec!["browse", "navigation"]).unwrap();
    assert_eq!(
        dispatcher
            .dispatch(&principal, request)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Unsupported
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
