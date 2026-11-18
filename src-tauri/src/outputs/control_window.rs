use crate::{app_state::AppState, contracts::AppError};
use serde::{Deserialize, Serialize};
use tauri::Manager;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}
fn clamp(saved: Geometry, area: tauri::PhysicalRect<i32, u32>, scale: f64) -> Geometry {
    let max_width = area.size.width.saturating_sub(16).max(1);
    let max_height = area.size.height.saturating_sub(40).max(1);
    let min_width = ((800. * scale) as u32).min(max_width);
    let min_height = ((600. * scale) as u32).min(max_height);
    let width = saved.width.clamp(min_width, max_width);
    let height = saved.height.clamp(min_height, max_height);
    let x = (saved.x as i64).clamp(
        area.position.x as i64,
        area.position.x as i64 + (max_width - width) as i64,
    ) as i32;
    let y = (saved.y as i64).clamp(
        area.position.y as i64,
        area.position.y as i64 + (max_height - height) as i64,
    ) as i32;
    Geometry {
        x,
        y,
        width,
        height,
    }
}
pub fn restore(app: &tauri::AppHandle, state: &AppState) -> Result<(), AppError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::new("window_unavailable", "The control window is unavailable."))?;
    let (monitors, computer) = super::monitors::discover(app)?;
    let Some(id) = computer else {
        return Ok(());
    };
    let monitor = window
        .available_monitors()
        .map_err(|_| AppError::new("monitor_unavailable", "Display geometry is unavailable."))?
        .into_iter()
        .find(|m| m.name() == Some(&id))
        .ok_or_else(|| {
            AppError::new(
                "monitor_unavailable",
                "The computer display is unavailable.",
            )
        })?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let saved = state
        .db
        .lock()
        .ok()
        .and_then(|db| {
            db.query_row(
                "SELECT value_json FROM settings WHERE key='main_geometry'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .and_then(|value| serde_json::from_str::<Geometry>(&value).ok())
        .unwrap_or(Geometry {
            x: area.position.x,
            y: area.position.y,
            width: (1280. * scale) as u32,
            height: (800. * scale) as u32,
        });
    let geometry = clamp(saved, *area, scale);
    let _ = monitors;
    window
        .set_min_size(Some(tauri::PhysicalSize::new(
            ((800. * scale) as u32).min(geometry.width),
            ((600. * scale) as u32).min(geometry.height),
        )))
        .map_err(|_| {
            AppError::new(
                "window_placement_failed",
                "The control window size could not be set.",
            )
        })?;
    window
        .set_size(tauri::PhysicalSize::new(geometry.width, geometry.height))
        .map_err(|_| {
            AppError::new(
                "window_placement_failed",
                "The control window size could not be set.",
            )
        })?;
    window
        .set_position(tauri::PhysicalPosition::new(geometry.x, geometry.y))
        .map_err(|_| {
            AppError::new(
                "window_placement_failed",
                "The control window could not be placed on the computer.",
            )
        })?;
    Ok(())
}
pub fn select(app: &tauri::AppHandle, state: &AppState, id: &str) -> Result<(), AppError> {
    let (monitors, _) = super::monitors::discover(app)?;
    super::monitors::resolve(&monitors, Some(id), None)?;
    state.db.lock().map_err(|_|AppError::new("database_error","Display preference could not be saved."))?.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('control_monitor',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(id).unwrap(),crate::app_state::now_ms() as i64])?;
    restore(app, state)
}
pub fn recover(app: &tauri::AppHandle, state: &AppState) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(position), Ok(size), Ok(monitors)) = (
        window.outer_position(),
        window.outer_size(),
        window.available_monitors(),
    ) else {
        return;
    };
    let visible = monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        (position.x as i64 + size.width as i64) > p.x as i64
            && (position.y as i64 + size.height as i64) > p.y as i64
            && (position.x as i64) < p.x as i64 + s.width as i64
            && (position.y as i64) < p.y as i64 + s.height as i64
    });
    if !visible {
        if let Err(error) = restore(app, state) {
            crate::diagnostics::record(
                state,
                "control_window",
                "recovery_failed",
                None,
                0,
                Some(&error.code),
            );
        }
    }
}
pub fn remember(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main"
        || !matches!(
            event,
            tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::CloseRequested { .. }
        )
    {
        return;
    }
    let Some(state) = window.app_handle().try_state::<AppState>() else {
        return;
    };
    if let Ok(mut saved) = state.geometry_saved_at.lock() {
        if saved.elapsed() < std::time::Duration::from_secs(1)
            && !matches!(event, tauri::WindowEvent::CloseRequested { .. })
        {
            return;
        }
        *saved = std::time::Instant::now();
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    let geometry = Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(db) = state.db.lock() {
            let _=db.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('main_geometry',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![serde_json::to_string(&geometry).unwrap(),crate::app_state::now_ms() as i64]);
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_is_reachable_on_small_negative_scaled_display() {
        let area = tauri::PhysicalRect {
            position: tauri::PhysicalPosition::new(-1280, -100),
            size: tauri::PhysicalSize::new(1280, 720),
        };
        let result = clamp(
            Geometry {
                x: 4000,
                y: 2000,
                width: 2560,
                height: 1600,
            },
            area,
            2.,
        );
        assert_eq!(result.x, -1280);
        assert_eq!(result.y, -100);
        assert!(result.width <= 1280 && result.height <= 720);
    }
}
