use super::protocol::parse_request;
use serde_json::{json, Value};

fn pause() -> Value {
    json!({"v":1,"id":"request-1","method":"player.pause",
        "params":{"sessionId":"session-1","paused":true},
        "expected":{"serverEpoch":"epoch-1","sessionId":"session-1"}})
}

#[test]
fn valid_pause_and_additive_optional_fields_are_accepted() {
    let mut request = pause();
    request["futureOptional"] = json!("ignored");
    request["params"]["futureOptional"] = json!(true);
    assert!(parse_request(&request.to_string()).is_ok());
}

#[test]
fn unknown_versions_and_methods_are_rejected() {
    for version in [json!(2), json!(0), json!("1"), json!(1.5)] {
        let mut request = pause();
        request["v"] = version;
        assert!(parse_request(&request.to_string()).is_err());
    }
    let mut request = pause();
    for method in ["control_player", "shell.exec", "player.toggle", ""] {
        request["method"] = json!(method);
        assert!(parse_request(&request.to_string()).is_err());
    }
}

#[test]
fn session_commands_require_matching_session_and_epoch() {
    for field in ["serverEpoch", "sessionId"] {
        let mut request = pause();
        request["expected"].as_object_mut().unwrap().remove(field);
        assert!(parse_request(&request.to_string()).is_err(), "{field}");
    }
    let mut request = pause();
    request["expected"]["sessionId"] = json!("different-session");
    assert!(parse_request(&request.to_string()).is_err());
    request["expected"]["sessionId"] = json!("session-1");
    request["params"]
        .as_object_mut()
        .unwrap()
        .remove("sessionId");
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn volume_and_seek_have_finite_nonnegative_ranges() {
    let mut request = pause();
    request["method"] = json!("player.volume");
    for volume in [json!(-1), json!(101), json!("50"), Value::Null] {
        request["params"] = json!({"sessionId":"session-1","volume":volume});
        assert!(parse_request(&request.to_string()).is_err(), "{volume}");
    }
    for volume in [0, 50, 100] {
        request["params"] = json!({"sessionId":"session-1","volume":volume});
        assert!(parse_request(&request.to_string()).is_ok());
    }
    request["method"] = json!("player.seek");
    request["params"] = json!({"sessionId":"session-1","positionSeconds":-0.1});
    assert!(parse_request(&request.to_string()).is_err());
    request["params"]["positionSeconds"] = json!(12.5);
    assert!(parse_request(&request.to_string()).is_ok());
    for value in ["NaN", "Infinity", "1e999"] {
        let input = request.to_string().replace("12.5", value);
        assert!(parse_request(&input).is_err());
    }
}

#[test]
fn revisions_are_canonical_decimal_strings_not_json_numbers() {
    let mut request = json!({"v":1,"id":"request-2","method":"queue.remove",
        "params":{"entryId":"entry-1"},
        "expected":{"serverEpoch":"epoch-1","sessionId":"session-1","queueRevision":"9007199254740993"}});
    assert!(parse_request(&request.to_string()).is_ok());
    for revision in [
        json!(12),
        json!(""),
        json!("-1"),
        json!("01"),
        json!("1.0"),
        json!("1e3"),
        json!(" 1"),
        json!("18446744073709551616"),
    ] {
        request["expected"]["queueRevision"] = revision;
        assert!(parse_request(&request.to_string()).is_err());
    }
    request["expected"]
        .as_object_mut()
        .unwrap()
        .remove("queueRevision");
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn unknown_action_enums_never_select_a_default() {
    let request = json!({"v":1,"id":"r","method":"queue.repeat",
        "params":{"mode":"surprise"},
        "expected":{"serverEpoch":"e","sessionId":"s","queueRevision":"1"}});
    assert!(parse_request(&request.to_string()).is_err());
    let request = json!({"v":1,"id":"r","method":"ui.moveFocus",
        "params":{"direction":"shell","rendererGeneration":"1","focusRevision":"1"},
        "expected":{"serverEpoch":"e"}});
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn query_page_and_unicode_limits_are_enforced() {
    let mut request = json!({"v":1,"id":"r","method":"library.query",
        "params":{"kind":"video","query":"ä".repeat(200),"limit":100}});
    assert!(parse_request(&request.to_string()).is_ok());
    request["params"]["query"] = json!("ä".repeat(201));
    assert!(parse_request(&request.to_string()).is_err());
    request["params"]["query"] = json!("");
    for limit in [0, 101] {
        request["params"]["limit"] = json!(limit);
        assert!(parse_request(&request.to_string()).is_err());
    }
    request["params"]["limit"] = json!(50);
    request["params"]["cursor"] = json!("a".repeat(2049));
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn browser_input_requires_target_generation_and_bounds_text() {
    let mut request = json!({"v":1,"id":"r","method":"browser.text",
        "params":{"browserSessionId":"browser-1","targetGeneration":"2","text":"😀".repeat(4096)},
        "expected":{"serverEpoch":"e"}});
    assert!(parse_request(&request.to_string()).is_ok());
    request["params"]["text"] = json!("😀".repeat(4097));
    assert!(parse_request(&request.to_string()).is_err());
    request["params"]["text"] = json!("private text");
    request["params"]
        .as_object_mut()
        .unwrap()
        .remove("targetGeneration");
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn oversized_complete_messages_are_rejected_before_parsing() {
    let mut request = pause();
    request["extension"] = json!("x".repeat(65536));
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn opaque_identifiers_cannot_be_paths_or_urls() {
    let mut request = json!({"v":1,"id":"r","method":"library.item","params":{"itemId":"media-1"}});
    assert!(parse_request(&request.to_string()).is_ok());
    for id in ["", "../secret", "/home/emma", "https://example.com"] {
        request["params"]["itemId"] = json!(id);
        assert!(parse_request(&request.to_string()).is_err());
    }
}

#[test]
fn operation_lookup_requires_exactly_one_identity() {
    let mut request = json!({"v":1,"id":"r","method":"operations.get","params":{}});
    assert!(parse_request(&request.to_string()).is_err());
    request["params"] = json!({"requestId":"original"});
    assert!(parse_request(&request.to_string()).is_ok());
    request["params"]["operationId"] = json!("op-1");
    assert!(parse_request(&request.to_string()).is_err());
}

#[test]
fn text_is_redacted_and_transient_input_is_never_a_durable_command() {
    use super::protocol::Delivery;
    let request = parse_request(r#"{"v":1,"id":"r","method":"browser.text","params":{"browserSessionId":"b","targetGeneration":"1","text":"private-password"},"expected":{"serverEpoch":"e"}}"#).unwrap();
    assert!(!format!("{request:?}").contains("private-password"));
    assert_eq!(request.command.delivery(), Delivery::Transient);
}

#[test]
fn every_method_fixture_has_a_stable_typed_roundtrip() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for fixture in fixtures["requests"].as_array().unwrap() {
        let request = parse_request(&fixture.to_string())
            .unwrap_or_else(|_| panic!("fixture {} rejected", fixture["method"]));
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(&encoded, fixture, "{}", fixture["method"]);
    }
}

#[test]
fn unsupported_methods_are_distinguished_from_invalid_arguments() {
    use super::protocol::ErrorCode;
    let mut request = pause();
    request["method"] = json!("system.execute");
    assert_eq!(
        parse_request(&request.to_string()).unwrap_err().code,
        ErrorCode::Unsupported
    );
    request["method"] = json!("player.pause");
    request["params"]["paused"] = json!("true");
    assert_eq!(
        parse_request(&request.to_string()).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
}

#[test]
fn hello_and_snapshots_use_bounded_typed_state_and_decimal_sequences() {
    use super::protocol::{Hello, RemoteSnapshot};
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    let hello: Hello = serde_json::from_value(fixtures["hello"].clone()).unwrap();
    assert_eq!(hello.snapshot_sequence.value(), 9_007_199_254_740_993);
    for name in ["emptySnapshot", "populatedSnapshot"] {
        let snapshot: RemoteSnapshot = serde_json::from_value(fixtures[name].clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), fixtures[name]);
    }
    let mut snapshot = fixtures["emptySnapshot"].clone();
    snapshot["sequence"] = json!(42);
    assert!(serde_json::from_value::<RemoteSnapshot>(snapshot).is_err());
}

#[test]
fn reply_success_and_error_are_exclusive_and_versioned() {
    use super::protocol::{AcceptedResult, RemoteReply};
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for name in ["accepted", "error"] {
        let reply: RemoteReply<AcceptedResult> =
            serde_json::from_value(fixtures[name].clone()).unwrap();
        assert_eq!(serde_json::to_value(reply).unwrap(), fixtures[name]);
    }
    let mut mixed = fixtures["accepted"].clone();
    mixed["error"] = fixtures["error"]["error"].clone();
    assert!(serde_json::from_value::<RemoteReply<AcceptedResult>>(mixed).is_err());
    let mut version = fixtures["accepted"].clone();
    version["v"] = json!(2);
    assert!(serde_json::from_value::<RemoteReply<AcceptedResult>>(version).is_err());
}

#[test]
fn shared_malformed_and_boundary_corpus_is_rejected() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for input in fixtures["malformedRequests"].as_array().unwrap() {
        assert!(
            parse_request(input.as_str().unwrap()).is_err(),
            "malformed fixture accepted"
        );
    }
    for input in fixtures["invalidRequests"].as_array().unwrap() {
        assert!(
            parse_request(&input.to_string()).is_err(),
            "invalid {} accepted",
            input["method"]
        );
    }
}

#[test]
fn all_confirmation_phases_are_reachable() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for input in fixtures["confirmationPhases"].as_array().unwrap() {
        assert!(
            parse_request(&input.to_string()).is_ok(),
            "{} phase rejected",
            input["method"]
        );
    }
}

#[test]
fn method_correlated_replies_roundtrip_without_shape_guessing() {
    use super::protocol::{parse_reply, MethodName};
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for entry in fixtures["replies"].as_array().unwrap() {
        let method: MethodName = serde_json::from_value(entry["method"].clone()).unwrap();
        let reply = parse_reply(method, &entry["reply"].to_string())
            .unwrap_or_else(|_| panic!("invalid reply fixture {}", entry["method"]));
        assert_eq!(serde_json::to_value(reply).unwrap(), entry["reply"]);
    }
    let mut mixed = fixtures["accepted"].clone();
    mixed["error"] = Value::Null;
    assert!(parse_reply(MethodName::PlayerPause, &mixed.to_string()).is_err());
    assert!(parse_reply(MethodName::LibraryItem, &fixtures["accepted"].to_string()).is_err());
}

#[test]
fn schema_hash_matches_the_shared_manifest() {
    use sha2::{Digest, Sha256};
    let schema: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/schema.json")).unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/manifest.json")).unwrap();
    let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&schema).unwrap()));
    assert_eq!(hash, manifest["schemaSha256"]);
}

#[test]
fn idle_queue_building_and_bounded_saved_playlist_edits_are_valid() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for key in ["idleQueueEdits", "playlistEdits"] {
        for input in fixtures[key].as_array().unwrap() {
            let request = parse_request(&input.to_string()).unwrap();
            assert_eq!(serde_json::to_value(request).unwrap(), *input);
        }
    }
}

#[test]
fn pairing_dtos_roundtrip_and_secrets_are_redacted() {
    use super::protocol::*;
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    macro_rules! roundtrip {
        ($key:literal, $typ:ty) => {{
            let dto: $typ = serde_json::from_value(fixtures[$key].clone()).unwrap();
            assert_eq!(serde_json::to_value(&dto).unwrap(), fixtures[$key]);
            assert!(!format!("{dto:?}").contains(&"A".repeat(43)));
            assert!(!format!("{dto:?}").contains(&"B".repeat(43)));
        }};
    }
    roundtrip!("pairingCandidate", PairingCandidate);
    roundtrip!("manualPairingCandidate", ManualPairingCandidate);
    roundtrip!("pairRequest", PairRequest);
    roundtrip!("manualPairRequest", ManualPairRequest);
    roundtrip!("pairedMediacenter", PairedMediacenter);
}

#[test]
fn adapters_translate_native_kinds_units_fields_and_bounded_revisions() {
    use super::{adapters::*, protocol::*};
    let native = crate::contracts::MediaItem {
        id: "media-1".into(),
        source_id: "source-1".into(),
        kind: crate::contracts::MediaKind::Audio,
        display_name: "Album title".into(),
        availability: "available".into(),
        duration_ms: Some(12500),
        resume_position_ms: Some(1500),
        thumbnail_id: None,
        modified_at_ms: None,
    };
    let wire = MediaItem::try_from(&native).unwrap();
    let json = serde_json::to_value(wire).unwrap();
    assert_eq!(json["kind"], "music");
    assert_eq!(json["sourceId"], "source-1");
    assert_eq!(json["durationSeconds"], 12.5);
    assert_eq!(json["resumePositionSeconds"], 1.5);
    assert!(json.get("durationMs").is_none());
    assert_eq!(
        native_queue_entry_id(&queue_entry_id(u32::MAX)).unwrap(),
        u32::MAX
    );
    assert_eq!(
        native_queue_revision(&Decimal::from(u64::from(u32::MAX))).unwrap(),
        u32::MAX
    );
    assert!(native_queue_revision(&Decimal::from(u64::from(u32::MAX) + 1)).is_err());
    let action: PlayerAction = serde_json::from_value(
        json!({"method":"player.seek","params":{"sessionId":"s","positionSeconds":12.5}}),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(crate::contracts::PlayerAction::try_from(action).unwrap()).unwrap(),
        json!({"type":"seek","seconds":12.5})
    );
}

#[test]
fn photo_fit_preserves_native_fit_and_actual_semantics() {
    use super::{adapters::slideshow_topic, protocol::*};
    let snapshot = crate::photos::presenter::SlideshowSnapshot::default();
    let topic = slideshow_topic(&snapshot, Decimal::from(1)).unwrap();
    assert_eq!(serde_json::to_value(topic).unwrap()["fit"], "fit");
    let action = crate::photos::presenter::SlideshowAction::from(Fit::Actual);
    assert_eq!(
        serde_json::to_value(action).unwrap(),
        json!({"type":"fit","actual":true})
    );
}

#[test]
fn pairing_begin_delivers_candidate_once_and_redacts_it() {
    use super::protocol::*;
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../protocol/remote/v1/fixtures.json")).unwrap();
    for fixture in fixtures["pairingBeginOutcomes"].as_array().unwrap() {
        let reply = parse_reply(MethodName::PairingBegin, &fixture.to_string()).unwrap();
        assert_eq!(serde_json::to_value(&reply).unwrap(), *fixture);
        assert!(!format!("{reply:?}").contains(&"A".repeat(43)));
        if fixture["result"]["status"] == "already_issued" {
            let serialized = serde_json::to_string(&reply).unwrap();
            assert!(!serialized.contains("candidate"));
            assert!(!serialized.contains("secret"));
        }
    }
    let mut missing = fixtures["pairingBeginOutcomes"][1].clone();
    missing["result"]
        .as_object_mut()
        .unwrap()
        .remove("candidate");
    assert!(parse_reply(MethodName::PairingBegin, &missing.to_string()).is_err());
}
