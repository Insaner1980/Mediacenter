fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "list_outputs",
            "select_output",
            "move_session",
            "choose_media_source",
            "add_media_source",
            "list_media_sources",
            "remove_media_source",
            "list_drives",
            "get_home_snapshot",
            "set_theme",
            "refresh_vpn",
            "list_vpn_countries",
            "connect_vpn",
            "disconnect_vpn",
            "enable_kill_switch",
            "cancel_operation",
            "set_vpn_requirement",
            "open_service",
            "focus_session",
            "close_session",
        ]),
    ))
    .expect("build application permissions")
}
