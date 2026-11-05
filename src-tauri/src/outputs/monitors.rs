use crate::contracts::{AppError, MonitorDescriptor};
pub fn resolve<'a>(
    monitors: &'a [MonitorDescriptor],
    id: Option<&str>,
    computer: Option<&str>,
) -> Result<&'a MonitorDescriptor, AppError> {
    let id = id
        .or(computer)
        .ok_or_else(|| AppError::new("monitor_unavailable", "Choose an available display."))?;
    let mut matches = monitors.iter().filter(|m| m.connected && m.id == id);
    let first = matches.next().ok_or_else(|| {
        AppError::new(
            "monitor_unavailable",
            "The selected display is disconnected.",
        )
    })?;
    if matches.next().is_some() {
        return Err(AppError::new(
            "monitor_ambiguous",
            "The display identity is ambiguous. Choose another display.",
        ));
    }
    Ok(first)
}
pub fn reachable(monitor: &MonitorDescriptor, x: i32, y: i32, width: u32, height: u32) -> bool {
    let b = &monitor.bounds;
    monitor.connected
        && width > 0
        && height > 0
        && i64::from(x) >= i64::from(b.x)
        && i64::from(y) >= i64::from(b.y)
        && i64::from(x) + i64::from(width) <= i64::from(b.x) + i64::from(b.width)
        && i64::from(y) + i64::from(height) <= i64::from(b.y) + i64::from(b.height)
}
pub fn discover(
    app: &tauri::AppHandle,
) -> Result<(Vec<MonitorDescriptor>, Option<String>), AppError> {
    use tauri::Manager;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::new("window_unavailable", "The control window is unavailable."))?;
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned());
    let monitors = window
        .available_monitors()
        .map_err(|_| AppError::new("monitor_unavailable", "Display discovery is unavailable."))?
        .into_iter()
        .filter_map(|monitor| {
            let connector = monitor.name()?.clone();
            Some(MonitorDescriptor {
                id: connector.clone(),
                name: connector.clone(),
                primary: Some(&connector) == primary.as_ref(),
                connector,
                connected: true,
                scale_factor: monitor.scale_factor(),
                bounds: crate::contracts::Bounds {
                    x: monitor.position().x,
                    y: monitor.position().y,
                    width: monitor.size().width,
                    height: monitor.size().height,
                },
            })
        })
        .collect::<Vec<_>>();
    let preferred = app
        .try_state::<crate::app_state::AppState>()
        .and_then(|state| {
            state.db.lock().ok().and_then(|db| {
                db.query_row(
                    "SELECT value_json FROM settings WHERE key='control_monitor'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok()
            })
        })
        .and_then(|s| serde_json::from_str::<String>(&s).ok());
    let current = window
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned());
    let computer = monitors
        .iter()
        .find(|m| Some(&m.id) == preferred.as_ref())
        .or_else(|| {
            monitors
                .iter()
                .find(|m| m.connector.starts_with("eDP") || m.connector.starts_with("LVDS"))
        })
        .or_else(|| monitors.iter().find(|m| Some(&m.id) == current.as_ref()))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
        .map(|m| m.id.clone());
    Ok((monitors, computer))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Bounds;
    fn monitor(id: &str, x: i32, scale: f64) -> MonitorDescriptor {
        MonitorDescriptor {
            id: id.into(),
            name: "Same name".into(),
            connector: id.into(),
            primary: false,
            connected: true,
            scale_factor: scale,
            bounds: Bounds {
                x,
                y: -100,
                width: 1920,
                height: 1080,
            },
        }
    }
    #[test]
    fn reordered_outputs_and_duplicate_names_use_identity() {
        let monitors = vec![monitor("HDMI-1", -1920, 1.5), monitor("eDP-1", 0, 1.)];
        assert_eq!(
            resolve(&monitors, Some("eDP-1"), Some("eDP-1")).unwrap().id,
            "eDP-1"
        );
        assert_eq!(resolve(&monitors, None, Some("eDP-1")).unwrap().id, "eDP-1");
        assert!(resolve(&monitors, Some("removed"), Some("eDP-1")).is_err());
    }
    #[test]
    fn negative_geometry_and_scaling_preserve_reachability() {
        let monitor = monitor("HDMI-1", -1920, 1.5);
        assert!(reachable(&monitor, -1800, 0, 800, 600));
        assert!(!reachable(&monitor, 100, 100, 800, 600));
    }
}
