#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub id: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
pub fn owned_window(listing: &str, pid: u32) -> Option<WindowInfo> {
    let mut matches = listing.lines().filter_map(|line| {
        let fields: Vec<_> = line.split_whitespace().take(7).collect();
        if fields.len() != 7 {
            return None;
        }
        let id = fields[0];
        if !id.starts_with("0x") || u64::from_str_radix(&id[2..], 16).is_err() {
            return None;
        }
        let found = WindowInfo {
            id: id.into(),
            pid: fields[2].parse().ok()?,
            x: fields[3].parse().ok()?,
            y: fields[4].parse().ok()?,
            width: fields[5].parse().ok()?,
            height: fields[6].parse().ok()?,
        };
        (found.pid == pid).then_some(found)
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}
pub async fn wmctrl(args: &[&str]) -> Result<String, crate::contracts::AppError> {
    use crate::{
        contracts::AppError,
        process::runner::{self, CommandSpec, Executable},
    };
    let output = runner::run(
        CommandSpec {
            executable: Executable::Wmctrl,
            args: args.iter().map(std::ffi::OsString::from).collect(),
            timeout: std::time::Duration::from_secs(5),
            output_limit: 1024 * 1024,
        },
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .map_err(|_| {
        AppError::new(
            "window_control_failed",
            "Window control is unavailable. Use the browser's own window controls.",
        )
    })?;
    if !output.success {
        return Err(AppError::new(
            "window_control_failed",
            "The desktop did not accept the window request.",
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        AppError::new(
            "window_state_invalid",
            "The desktop returned unrecognized window information.",
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn title_cannot_claim_unrelated_window() {
        let list = "0x01 0 11 -800 20 640 480 host YouTube\n0x02 0 22 100 30 800 600 host YouTube";
        assert_eq!(owned_window(list, 22).unwrap().id, "0x02");
        assert_eq!(owned_window(list, 11).unwrap().x, -800);
    }
    #[test]
    fn ambiguous_windows_are_not_guessed() {
        assert!(owned_window(
            "0x01 0 11 0 0 100 100 host A\n0x02 0 11 0 0 100 100 host B",
            11
        )
        .is_none());
    }
}

pub fn placement_request(
    session_type: &str,
    kind: &str,
    verified: bool,
    monitor: &crate::contracts::MonitorDescriptor,
) -> Result<(i32, i32, u32, u32), crate::contracts::AppError> {
    use crate::contracts::AppError;
    if session_type != "x11" {
        return Err(AppError::new(
            "manual_placement",
            "Automatic placement requires X11. Move the playback window manually.",
        ));
    }
    if !verified || !matches!(kind, "streaming" | "local-video" | "local-audio") {
        return Err(AppError::new(
            "ownership_uncertain",
            "This window is not a verified playback window.",
        ));
    }
    if !monitor.connected || !monitor.scale_factor.is_finite() || monitor.scale_factor <= 0. {
        return Err(AppError::new(
            "monitor_unavailable",
            "The display geometry is unavailable.",
        ));
    }
    Ok((
        monitor.bounds.x,
        monitor.bounds.y,
        monitor.bounds.width,
        monitor.bounds.height,
    ))
}
#[cfg(test)]
mod placement_tests {
    use super::*;
    use crate::contracts::{Bounds, MonitorDescriptor};
    fn monitor() -> MonitorDescriptor {
        MonitorDescriptor {
            id: "HDMI-1".into(),
            name: "TV".into(),
            connector: "HDMI-1".into(),
            primary: false,
            connected: true,
            scale_factor: 2.,
            bounds: Bounds {
                x: -3840,
                y: 0,
                width: 3840,
                height: 2160,
            },
        }
    }
    #[test]
    fn visible_music_window_uses_verified_playback_placement() {
        assert_eq!(
            placement_request("x11", "local-audio", true, &monitor()).unwrap(),
            (-3840, 0, 3840, 2160)
        );
        assert!(placement_request("x11", "local-audio", false, &monitor()).is_err());
    }
    #[test]
    fn move_playback_never_moves_main_window() {
        assert!(placement_request("x11", "main", true, &monitor()).is_err());
    }
    #[test]
    fn unverified_window_is_not_moved() {
        assert!(placement_request("x11", "streaming", false, &monitor()).is_err());
    }
    #[test]
    fn wayland_reports_manual_placement() {
        assert_eq!(
            placement_request("wayland", "streaming", true, &monitor())
                .unwrap_err()
                .code,
            "manual_placement"
        );
    }
}
