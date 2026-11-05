fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_home_snapshot",
            "set_theme",
            "refresh_vpn",
            "list_vpn_countries",
            "connect_vpn",
            "disconnect_vpn",
            "enable_kill_switch",
            "cancel_operation",
            "set_vpn_requirement",
        ]),
    ))
    .expect("build application permissions")
}
