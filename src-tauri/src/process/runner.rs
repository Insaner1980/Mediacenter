use nix::{
    sys::signal::{killpg, Signal},
    unistd::Pid,
};
use std::{ffi::OsString, time::Duration};
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
pub enum Executable {
    NordVpn,
    Ffprobe,
    Ffmpeg,
    Xrandr,
    Wmctrl,
    Pactl,
    #[cfg(test)]
    Test(&'static str),
}
pub struct CommandSpec {
    pub executable: Executable,
    pub args: Vec<OsString>,
    pub timeout: Duration,
    pub output_limit: usize,
}
#[derive(Debug)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}
impl Executable {
    pub fn path(&self) -> &'static str {
        match self {
            Self::NordVpn => "/usr/bin/nordvpn",
            Self::Ffprobe => "/usr/bin/ffprobe",
            Self::Ffmpeg => "/usr/bin/ffmpeg",
            Self::Xrandr => "/usr/bin/xrandr",
            Self::Wmctrl => "/usr/bin/wmctrl",
            Self::Pactl => "/usr/bin/pactl",
            #[cfg(test)]
            Self::Test(path) => path,
        }
    }
}
async fn read_bounded(
    mut input: impl AsyncRead + Unpin,
    used: Arc<AtomicUsize>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let size = input
            .read(&mut buffer)
            .await
            .map_err(|_| "read_failed".to_string())?;
        if size == 0 {
            return Ok(result);
        }
        if used.fetch_add(size, Ordering::Relaxed).saturating_add(size) > limit {
            return Err("output_limit".into());
        }
        result.extend_from_slice(&buffer[..size]);
    }
}
pub async fn run(spec: CommandSpec, cancel: CancellationToken) -> Result<Output, String> {
    if cancel.is_cancelled() {
        return Err("cancelled".into());
    }
    if spec.output_limit == 0
        || spec.output_limit > 4 * 1024 * 1024
        || spec.timeout > Duration::from_secs(60)
    {
        return Err("invalid_command_budget".into());
    }
    let mut command = tokio::process::Command::new(spec.executable.path());
    command
        .args(spec.args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| "spawn_failed".to_string())?;
    let pid = child.id().ok_or("spawn_failed")?;
    let stdout = child.stdout.take().ok_or("pipe_failed")?;
    let stderr = child.stderr.take().ok_or("pipe_failed")?;
    let used = Arc::new(AtomicUsize::new(0));
    let work = async {
        let (stdout, stderr, status) = tokio::try_join!(
            read_bounded(stdout, used.clone(), spec.output_limit),
            read_bounded(stderr, used, spec.output_limit),
            async { child.wait().await.map_err(|_| "wait_failed".to_string()) }
        )?;
        Ok(Output {
            stdout,
            stderr,
            success: status.success(),
        })
    };
    let result = tokio::select! {
        _=cancel.cancelled()=>Err("cancelled".into()),
        result=tokio::time::timeout(spec.timeout,work)=>result.unwrap_or_else(|_|Err("timeout".into())),
    };
    if result.is_err() {
        // The group was created by this runner and cannot target an existing process group.
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
        let _ = child.wait().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(program: &'static str, args: &[&str]) -> CommandSpec {
        CommandSpec {
            executable: Executable::Test(program),
            args: args.iter().map(OsString::from).collect(),
            timeout: Duration::from_secs(2),
            output_limit: 1024,
        }
    }
    #[tokio::test]
    async fn arguments_are_not_shell_interpreted() {
        let out = run(
            spec(
                "/usr/bin/printf",
                &[
                    "%s",
                    "$(touch /tmp/mediacenter-should-never-exist); å -file",
                ],
            ),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            "$(touch /tmp/mediacenter-should-never-exist); å -file"
        );
    }
    #[tokio::test]
    async fn command_timeout_reaps_owned_child() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");
        let mut s = spec(
            "/usr/bin/python3",
            &[
                "-c",
                "import os,time,sys;open(sys.argv[1],'w').write(str(os.getpid()));time.sleep(30)",
            ],
        );
        s.args.push(pidfile.as_os_str().into());
        s.timeout = Duration::from_millis(200);
        assert_eq!(
            run(s, CancellationToken::new()).await.unwrap_err(),
            "timeout"
        );
        let pid = std::fs::read_to_string(pidfile).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    #[tokio::test]
    async fn oversized_output_is_bounded() {
        let result = run(
            spec(
                "/usr/bin/python3",
                &[
                    "-c",
                    "import sys;sys.stdout.write('x'*100000);sys.stderr.write('y'*100000)",
                ],
            ),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(result.unwrap_err(), "output_limit");
    }
    #[tokio::test]
    async fn cancellation_releases_command() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            run(spec("/usr/bin/sleep", &["30"]), cancel)
                .await
                .unwrap_err(),
            "cancelled"
        );
    }
}
