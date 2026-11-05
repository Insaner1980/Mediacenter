use crate::contracts::{AppError, ServiceId};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
pub fn prepare_profile(data: &Path, service: ServiceId) -> Result<PathBuf, AppError> {
    use std::os::unix::fs::PermissionsExt;
    let parent = data.join("browser-profiles");
    if parent
        .symlink_metadata()
        .is_ok_and(|metadata| !metadata.is_dir())
    {
        return Err(AppError::new(
            "profile_invalid",
            "The browser profile directory must be an application-owned directory.",
        ));
    }
    std::fs::create_dir_all(&parent)?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
    let profile = parent.join(super::catalog::key(service));
    if profile.join("SingletonLock").symlink_metadata().is_ok() {
        return Err(AppError::new("profile_busy","This service profile is already open or has an unresolved lock. Close its browser and retry; its files were not changed."));
    }
    if profile
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(AppError::new(
            "profile_invalid",
            "The service profile must be an application-owned directory.",
        ));
    }
    std::fs::create_dir_all(&profile)?;
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700))?;
    Ok(profile)
}
pub fn arguments(profile: &Path, service: ServiceId) -> Vec<OsString> {
    let mut profile_arg = OsString::from("--user-data-dir=");
    profile_arg.push(profile);
    vec![
        profile_arg,
        format!("--app={}", super::catalog::url(service)).into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
    ]
}
pub struct BrowserSession {
    pub routing_target: Option<crate::contracts::PlaybackTarget>,
    pub session: crate::contracts::ManagedSession,
    pub child: tokio::process::Child,
    pub identity: crate::process::registry::ProcessIdentity,
    pub profile: PathBuf,
    pub window: crate::outputs::x11::WindowInfo,
}
impl BrowserSession {
    pub async fn verify(&self) -> Result<crate::outputs::x11::WindowInfo, AppError> {
        if !self.identity.still_owned() {
            return Err(AppError::new(
                "session_unavailable",
                "This service process is no longer available.",
            ));
        }
        let lock = std::fs::read_link(self.profile.join("SingletonLock"))?;
        if lock.to_string_lossy().rsplit('-').next() != Some(self.identity.pid.to_string().as_str())
        {
            return Err(AppError::new(
                "ownership_uncertain",
                "The service profile owner could not be verified. Use the browser manually.",
            ));
        }
        let listing = crate::outputs::x11::wmctrl(&["-lpG"]).await?;
        let window = crate::outputs::x11::owned_window(&listing, self.identity.pid)
            .filter(|window| window.id == self.window.id)
            .ok_or_else(|| {
                AppError::new(
                    "ownership_uncertain",
                    "The service window could not be identified safely. Use the browser manually.",
                )
            })?;
        Ok(window)
    }
    pub async fn focus(&self) -> Result<(), AppError> {
        let window = self.verify().await?;
        crate::outputs::x11::wmctrl(&["-ia", &window.id]).await?;
        Ok(())
    }
    pub async fn close(&mut self) -> Result<(), AppError> {
        let window = self.verify().await?;
        crate::outputs::x11::wmctrl(&["-ic", &window.id]).await?;
        if tokio::time::timeout(std::time::Duration::from_secs(3), self.child.wait())
            .await
            .is_err()
        {
            if !self.identity.still_owned() {
                return Err(AppError::new(
                    "ownership_uncertain",
                    "The service process changed while closing. It was not terminated.",
                ));
            }
            self.child.kill().await?;
            self.child.wait().await?;
        }
        Ok(())
    }
}
pub async fn launch(
    data: &Path,
    service: ServiceId,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<BrowserSession, AppError> {
    use crate::{contracts::*, outputs::x11, process::registry::ProcessIdentity};
    use std::{os::unix::fs::PermissionsExt, process::Stdio, time::Duration};
    if std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("x11") {
        return Err(AppError::new("manual_browser_required","Managed browser placement requires X11. Open the service manually in this desktop session."));
    }
    let metadata = std::fs::metadata("/usr/bin/google-chrome").map_err(|_| {
        AppError::new(
            "chrome_unavailable",
            "Install Google Chrome to open streaming services.",
        )
    })?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(AppError::new(
            "chrome_unavailable",
            "Install Google Chrome to open streaming services.",
        ));
    }
    let profile = prepare_profile(data, service)?;
    let mut child = tokio::process::Command::new("/usr/bin/google-chrome")
        .args(arguments(&profile, service))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id().ok_or_else(|| {
        AppError::new(
            "browser_start_failed",
            "Chrome did not provide a process identity.",
        )
    })?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut captured = None;
    while tokio::time::Instant::now() < deadline && !cancel.is_cancelled() {
        if let Some(identity) = ProcessIdentity::capture(pid) {
            if identity.executable == Path::new("/opt/google/chrome/chrome") {
                captured = Some(identity.clone());
                if let Ok(list) = x11::wmctrl(&["-lpG"]).await {
                    if let Some(window) = x11::owned_window(&list, pid) {
                        let session = ManagedSession {
                            id: uuid::Uuid::new_v4().to_string(),
                            kind: "streaming".into(),
                            service_id: Some(service),
                            media_id: None,
                            phase: "ready".into(),
                            target: PlaybackTarget::default(),
                        };
                        let mut browser = BrowserSession {
                            routing_target: None,
                            session,
                            child,
                            identity,
                            profile,
                            window,
                        };
                        if let Err(error) = browser.verify().await {
                            if browser.identity.still_owned() {
                                let _ = browser.child.kill().await;
                                let _ = browser.child.wait().await;
                            }
                            return Err(error);
                        }
                        return Ok(browser);
                    }
                }
            }
        }
        if child.try_wait()?.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if captured.as_ref().is_some_and(ProcessIdentity::still_owned) {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    Err(AppError::new(if cancel.is_cancelled(){"cancelled"}else{"browser_ownership_unconfirmed"},"A managed Chrome window could not be established. A launcher exit does not prove that every browser window closed; inspect the service profile before retrying."))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_chrome_profile_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let normal = dir.path().join("normal");
        std::fs::create_dir(&normal).unwrap();
        std::fs::write(normal.join("sentinel"), b"keep").unwrap();
        let profile = prepare_profile(dir.path(), ServiceId::Youtube).unwrap();
        assert_eq!(profile, dir.path().join("browser-profiles/youtube"));
        let args = arguments(&profile, ServiceId::Youtube);
        assert!(args
            .iter()
            .any(|arg| arg == "--app=https://www.youtube.com/"));
        assert!(args
            .iter()
            .any(|arg| arg.to_string_lossy().starts_with("--user-data-dir=")));
        assert!(!args.iter().any(|arg| arg == "--no-sandbox"));
        assert_eq!(std::fs::read(normal.join("sentinel")).unwrap(), b"keep");
    }
    #[test]
    fn locked_profile_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("browser-profiles/netflix");
        std::fs::create_dir_all(&profile).unwrap();
        std::os::unix::fs::symlink("host-999999", profile.join("SingletonLock")).unwrap();
        assert!(prepare_profile(dir.path(), ServiceId::Netflix).is_err());
        assert!(profile.join("SingletonLock").symlink_metadata().is_ok());
    }
    #[test]
    fn linked_profile_parent_does_not_modify_external_directories() {
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(external.path(), dir.path().join("browser-profiles")).unwrap();
        assert!(prepare_profile(dir.path(), ServiceId::Youtube).is_err());
        assert!(!external.path().join("youtube").exists());
    }
    #[test]
    fn unknown_service_id_is_rejected() {
        assert!(serde_json::from_str::<ServiceId>("\"injected; command\"").is_err());
    }
}
