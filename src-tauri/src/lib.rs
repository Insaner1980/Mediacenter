use tauri::Manager;
pub mod app_state;
pub mod commands;
pub mod contracts;
pub mod control;
pub mod devices;
pub mod diagnostics;
pub mod events;
pub mod library;
pub mod lifecycle;
pub mod music;
pub mod operations;
pub mod outputs;
pub mod photos;
pub mod player;
pub mod process;
pub mod remote;
pub mod storage;
pub mod streaming;
pub mod vpn;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_outputs,
            commands::select_output,
            commands::move_session,
            commands::choose_media_source,
            commands::add_media_source,
            commands::list_media_sources,
            commands::ensure_default_media_source,
            commands::remove_media_source,
            commands::list_drives,
            commands::rescan_source,
            commands::cancel_source_scan,
            commands::query_media,
            commands::get_thumbnail,
            commands::play_media,
            commands::get_player,
            commands::control_player,
            commands::get_music_queue,
            commands::control_music,
            commands::play_music_queue,
            commands::play_playlist,
            commands::save_playlist,
            commands::list_playlists,
            commands::get_playlist,
            commands::remove_playlist,
            commands::start_slideshow,
            commands::control_slideshow,
            commands::get_slideshow,
            commands::get_presentation_frame,
            commands::get_photo_image,
            commands::prepare_eject_drive,
            commands::mount_volume,
            commands::eject_drive,
            commands::request_exit,
            commands::get_diagnostics,
            commands::export_diagnostics,
            commands::set_control_monitor,
            commands::get_home_snapshot,
            commands::set_theme,
            commands::refresh_vpn,
            commands::list_vpn_countries,
            commands::connect_vpn,
            commands::disconnect_vpn,
            commands::enable_kill_switch,
            commands::cancel_operation,
            commands::set_vpn_requirement,
            commands::open_service,
            commands::focus_session,
            commands::close_session
        ])
        .setup(|app| {
            let mut state = app_state::AppState::open(app.path().app_data_dir()?)
                .map_err(|error| std::io::Error::other(error.message))?;
            state.cache_dir = app.path().app_cache_dir()?;
            app.manage(state);
            // Native setup owns explicit identity/network selection and enablement.
            app.manage(remote::server::RemoteServer::default());
            tauri::async_runtime::spawn(outputs::audio::subscribe(
                app.handle().clone(),
                app.state::<app_state::AppState>().inner().clone(),
            ));
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let mut wall = app_state::now_ms();
                let mut monotonic = std::time::Instant::now();
                loop {
                    let now = app_state::now_ms();
                    let elapsed = monotonic.elapsed().as_millis() as u64;
                    if now.saturating_sub(wall) > elapsed.saturating_add(2000) || elapsed > 15000 {
                        let _ = events::publish(&handle, &state, |home| {
                            home.vpn.state = "unknown".into();
                            home.vpn.kill_switch = "unknown".into();
                            home.vpn.observed_at_ms = 0;
                            home.vpn.error_code = Some("observation_stale".into());
                            home.vpn.revision += 1;
                        });
                    }
                    wall = now;
                    monotonic = std::time::Instant::now();
                    let _ = vpn::nordvpn::refresh(&handle, &state).await;
                    let interval = state
                        .snapshot()
                        .map(|home| {
                            if ["connecting", "disconnecting"].contains(&home.vpn.state.as_str()) {
                                1
                            } else if home.sessions.iter().any(|s| s.kind == "streaming") {
                                2
                            } else {
                                5
                            }
                        })
                        .unwrap_or(5);
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_devices =
                    std::time::Instant::now() - std::time::Duration::from_secs(10);
                loop {
                    outputs::control_window::recover(&handle, &state);
                    streaming::sessions::reconcile(&handle, &state).await;
                    outputs::coordinator::reconcile(&handle, &state).await;
                    if last_devices.elapsed() >= std::time::Duration::from_secs(10) {
                        let _ = lifecycle::reconcile_sources(&handle, &state).await;
                        last_devices = std::time::Instant::now();
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });
            use tauri::menu::{Menu, MenuItem, Submenu};
            let dark = MenuItem::with_id(app, "theme-dark", "Dark", true, None::<&str>)?;
            let light = MenuItem::with_id(app, "theme-light", "Light", true, None::<&str>)?;
            let view = Submenu::with_items(app, "View", true, &[&dark, &light])?;
            app.set_menu(Menu::with_items(app, &[&view])?)?;
            let state = app.state::<app_state::AppState>();
            if let Err(error) = outputs::control_window::restore(app.handle(), &state) {
                diagnostics::record(
                    &state,
                    "control_window",
                    "initial_placement_failed",
                    None,
                    0,
                    Some(&error.code),
                );
            }
            let _ = library::scanner::publish_summaries(app.handle(), &state);
            Ok(())
        })
        .on_window_event(|window, event| {
            outputs::control_window::remember(window, event);
            lifecycle::on_close(window, event);
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "theme-dark" => {
                let _ = commands::save_theme(app, contracts::Theme::Dark);
            }
            "theme-light" => {
                let _ = commands::save_theme(app, contracts::Theme::Light);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("Mediacenter failed to start");
}
