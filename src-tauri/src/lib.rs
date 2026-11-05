use tauri::Manager;
pub mod app_state;
pub mod commands;
pub mod contracts;
pub mod events;
pub mod operations;
pub mod process;
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
            commands::get_home_snapshot,
            commands::set_theme,
            commands::refresh_vpn,
            commands::list_vpn_countries,
            commands::connect_vpn,
            commands::disconnect_vpn,
            commands::enable_kill_switch,
            commands::cancel_operation,
            commands::set_vpn_requirement
        ])
        .setup(|app| {
            let state = app_state::AppState::open(app.path().app_data_dir()?)
                .map_err(|error| std::io::Error::other(error.message))?;
            app.manage(state);
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let _ = vpn::nordvpn::refresh(&handle, &state).await;
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            });
            use tauri::menu::{Menu, MenuItem, Submenu};
            let dark = MenuItem::with_id(app, "theme-dark", "Dark", true, None::<&str>)?;
            let light = MenuItem::with_id(app, "theme-light", "Light", true, None::<&str>)?;
            let view = Submenu::with_items(app, "View", true, &[&dark, &light])?;
            app.set_menu(Menu::with_items(app, &[&view])?)?;
            let window = app.get_webview_window("main").expect("main window");
            let monitors = window.available_monitors()?;
            let monitor = monitors
                .into_iter()
                .find(|m| {
                    m.name()
                        .is_some_and(|name| name.starts_with("eDP") || name.starts_with("LVDS"))
                })
                .or(window.primary_monitor()?);
            if let Some(monitor) = monitor {
                let scale = monitor.scale_factor();
                let size = monitor.work_area().size.to_logical::<f64>(scale);
                let width = 1280_f64.min(size.width);
                let height = 800_f64.min(size.height);
                window.set_min_size(Some(tauri::LogicalSize::new(
                    800_f64.min(width),
                    600_f64.min(height),
                )))?;
                window.set_size(tauri::LogicalSize::new(width, height))?;
                window.set_position(monitor.work_area().position)?;
            }
            Ok(())
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
