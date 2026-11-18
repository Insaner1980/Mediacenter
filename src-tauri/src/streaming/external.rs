use crate::contracts::{AppError, ServiceId};
use std::{process::Stdio, time::Duration};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

fn hbo_command() -> Command {
    let mut command = Command::new("/usr/bin/google-chrome");
    // Let the user's running Chrome choose its existing profile and tab/window.
    // Never supply a private profile, a debugging port or security overrides.
    command.arg(super::catalog::url(ServiceId::HboMax));
    command
}

pub async fn open_hbo(cancel: CancellationToken) -> Result<(), AppError> {
    handoff(hbo_command(), cancel).await
}

async fn handoff(mut command: Command, cancel: CancellationToken) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Opening HBO Max was cancelled."));
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // This can become the user's ordinary browser. It is never owned playback.
        .kill_on_drop(false)
        .spawn()
        .map_err(|_| AppError::new("chrome_unavailable", "Google Chrome could not be opened."))?;
    match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(_) => Err(AppError::new(
            "chrome_open_failed",
            "Chrome could not accept the HBO Max request. Open Chrome and try again.",
        )),
        Err(_) => {
            // Chrome may remain running when no browser was open. Reap its launcher
            // eventually, but never kill it on timeout, cancellation or app exit.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hbo_handoff_preserves_the_users_chrome_profile() {
        let command = hbo_command();
        assert_eq!(command.as_std().get_program(), "/usr/bin/google-chrome");
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            ["https://www.hbomax.com/"]
        );
    }
    #[tokio::test]
    async fn immediate_launcher_failure_is_not_reported_as_success() {
        let error = handoff(Command::new("/usr/bin/false"), CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.code, "chrome_open_failed");
        assert!(
            handoff(Command::new("/usr/bin/true"), CancellationToken::new())
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    async fn cancelled_handoff_does_not_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("spawned");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            handoff(command, cancel).await.unwrap_err().code,
            "cancelled"
        );
        assert!(!marker.exists());
    }
    #[tokio::test]
    async fn long_lived_browser_survives_handoff_and_later_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("completed");
        let mut command = Command::new("/usr/bin/python3");
        command
            .args([
                "-c",
                "import sys,time;time.sleep(2);open(sys.argv[1],'w').write('survived')",
            ])
            .arg(&marker);
        let cancel = CancellationToken::new();
        handoff(command, cancel.clone()).await.unwrap();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "survived");
    }
}
