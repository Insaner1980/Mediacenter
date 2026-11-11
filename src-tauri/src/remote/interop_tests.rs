//! Task21 offline exporter: production DTO serializers and Dispatcher, isolated native effects.
//! Only compiled into the Rust test binary; no production RPC or path surface.
use super::{adapters::checked, dispatch::Dispatcher, protocol::*, store::CredentialHash};
use crate::{app_state::AppState, control::ControlServices};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn wire(value: impl Serialize) -> String {
    serde_json::to_string(&value).unwrap()
}
fn reply(method: &str, value: Value) -> Value {
    let method_name = checked(method).unwrap();
    let typed = parse_reply(method_name, &value.to_string())
        .unwrap_or_else(|_| panic!("invalid exporter reply for {method}: {value}"));
    json!({"method":method,"wire":wire(typed)})
}
fn envelope(id: &str, result: Result<Value, RemoteError>) -> String {
    let id = checked(id).unwrap();
    // Same concrete RemoteReply<Value> envelope used by server.rs after dispatch.
    match result {
        Ok(result) => wire(RemoteReply::Success { id, result }),
        Err(error) => wire(RemoteReply::<Value>::Failure { id, error }),
    }
}

async fn dispatcher_trace() -> Value {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().into()).unwrap();
    let identity: Id = checked("interop-server").unwrap();
    let services = ControlServices::native(state.clone(), identity.clone()).unwrap();
    let device: PairedDevice = checked(json!({"deviceId":"interop-phone","label":"Test phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
    let credential: BearerCredential = checked("x".repeat(43)).unwrap();
    super::store::RemoteStore::new(&mut state.db.lock().unwrap(), &identity)
        .add_device(&device, &CredentialHash::from_credential(&credential))
        .unwrap();
    let principal = RemotePrincipal {
        device_id: device.device_id,
        scopes: device.scopes,
    };
    let effects = Arc::new(AtomicUsize::new(0));
    let count = effects.clone();
    let mut dispatcher = Dispatcher::new(services);
    dispatcher.pause_effect = Some(Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    let request = parse_request(&json!({"v":1,"id":"pause-once","method":"player.pause","params":{"sessionId":"test-session","paused":true},"expected":{"serverEpoch":state.event_hub.epoch(),"sessionId":"test-session"}}).to_string()).unwrap();
    let accepted = envelope(
        "pause-once",
        dispatcher.dispatch(&principal, request.clone()).await,
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !state.operations.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Discard the terminal event, then recover by the original request identity.
    let lookup = parse_request(
        r#"{"v":1,"id":"lookup","method":"operations.get","params":{"requestId":"pause-once"}}"#,
    )
    .unwrap();
    let terminal = envelope("lookup", dispatcher.dispatch(&principal, lookup).await);
    let duplicate = envelope(
        "pause-once",
        dispatcher.dispatch(&principal, request.clone()).await,
    );
    assert_eq!(accepted, duplicate);
    let mut conflict = serde_json::to_value(&request).unwrap();
    conflict["params"]["paused"] = json!(false);
    let conflict = envelope(
        "pause-once",
        dispatcher
            .dispatch(&principal, parse_request(&conflict.to_string()).unwrap())
            .await,
    );
    assert_eq!(
        serde_json::from_str::<Value>(&terminal).unwrap()["result"]["status"],
        "succeeded"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&conflict).unwrap()["error"]["code"],
        "request_id_conflict"
    );
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert_eq!(
        state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    json!({"request":wire(request),"accepted":accepted,"duplicate":duplicate,"terminal":terminal,"conflict":conflict,"effectCount":effects.load(Ordering::SeqCst)})
}

#[tokio::test]
async fn task21_export_rust_client_corpus() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    let requests: Vec<_> = fixtures["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let typed = parse_request(&v.to_string()).unwrap();
            json!({"method":v["method"],"wire":wire(typed)})
        })
        .collect();
    assert_eq!(requests.len(), 88);
    let mut replies: Vec<_> = fixtures["replies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| reply(v["method"].as_str().unwrap(), v["reply"].clone()))
        .collect();
    for v in fixtures["pairingBeginOutcomes"].as_array().unwrap() {
        replies.push(reply("pairing.begin", v.clone()));
    }
    for (key, method) in [
        ("confirmationStale", "confirmations.get"),
        ("pairingQrReply", "pairing.begin"),
        ("confirmationAcceptedReply", "activity.prepare"),
        ("photoSelectionTooLarge", "photos.slideshow.start"),
    ] {
        replies.push(reply(method, fixtures[key].clone()));
    }
    for v in fixtures["journalOutcomes"].as_array().unwrap() {
        replies.push(reply(
            "operations.get",
            json!({"v":1,"id":"journal","ok":true,"result":v}),
        ));
    }
    let hello: Hello = checked(fixtures["hello"].clone()).unwrap();
    let mut hello_max = hello.clone();
    hello_max.snapshot_sequence = u64::MAX.into();
    let snapshot: RemoteSnapshot = checked(fixtures["emptySnapshot"].clone()).unwrap();
    let populated: RemoteSnapshot = checked(fixtures["populatedSnapshot"].clone()).unwrap();
    let mut snapshot_max = snapshot.clone();
    snapshot_max.sequence = u64::MAX.into();
    let mut data = snapshot.player.clone();
    data.revision = 9_007_199_254_740_994_u64.into();
    data.phase = PlayerPhase::Paused;
    let event = RemoteEvent::Player {
        server_epoch: snapshot.server_epoch.clone(),
        sequence: 9_007_199_254_740_994_u64.into(),
        data: data.clone(),
    };
    let gap = RemoteEvent::Player {
        server_epoch: snapshot.server_epoch.clone(),
        sequence: 9_007_199_254_740_996_u64.into(),
        data,
    };
    let mut invalid_requests: Vec<String> = fixtures["malformedRequests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    invalid_requests.extend(
        fixtures["invalidRequests"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::to_string),
    );
    invalid_requests.push(
        json!({"v":1,"id":"oversize","method":"state.get","params":{},"extra":"x".repeat(65536)})
            .to_string(),
    );
    for input in &invalid_requests {
        assert!(parse_request(input).is_err());
    }
    let invalid_replies: Vec<_> = fixtures["invalidReplies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let method: MethodName = checked(v["method"].clone()).unwrap();
            assert!(parse_reply(method, &v["reply"].to_string()).is_err());
            json!({"method":v["method"],"wire":v["reply"].to_string()})
        })
        .collect();
    let corpus = json!({"format":"mediacenter-rust-interop-v1","requests":requests,"replies":replies,"hello":wire(&hello),"helloMax":wire(hello_max),"snapshots":[wire(&snapshot),wire(populated)],"snapshotMax":wire(snapshot_max),"invalidRequests":invalid_requests,"invalidReplies":invalid_replies,"reducer":{"hello":wire(hello),"snapshot":wire(snapshot),"event":wire(event),"gap":wire(gap)},"dispatcher":dispatcher_trace().await});
    if let Some(path) = std::env::var_os("MEDIACENTER_TASK21_CORPUS_OUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&corpus).unwrap()).unwrap();
    }
    println!("Task21 exporter: 88 request shapes, {} replies, {} negative requests, fake-native effect count 1", replies.len(), invalid_requests.len());
}
