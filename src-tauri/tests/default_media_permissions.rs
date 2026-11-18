#[test]
fn system_folder_opening_is_available_only_to_the_main_window() {
    let main: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    let presentation: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/presentation.json")).unwrap();
    let permission = serde_json::json!("allow-ensure-default-media-source");
    assert!(main["permissions"]
        .as_array()
        .unwrap()
        .contains(&permission));
    assert_eq!(main["windows"], serde_json::json!(["main"]));
    assert!(!presentation["permissions"]
        .as_array()
        .unwrap()
        .contains(&permission));
    assert!(mediacenter_lib::commands::guard("presentation").is_err());
}

#[test]
fn task11_renderer_commands_are_available_only_to_main() {
    let main: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    let presentation: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/presentation.json")).unwrap();
    for permission in [
        "allow-register-remote-renderer",
        "allow-update-remote-renderer",
        "allow-acknowledge-remote-renderer",
        "allow-get-renderer-confirmations",
        "allow-resolve-renderer-confirmation",
    ] {
        assert!(main["permissions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(permission)));
        assert!(!presentation["permissions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(permission)));
    }
}
