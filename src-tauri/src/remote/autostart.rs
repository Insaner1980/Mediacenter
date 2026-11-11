//! User-session startup. Paths are injected so fixtures never touch the user's login.
use crate::contracts::AppError;
use std::path::{Path, PathBuf};
const OWNER: &str = "# Managed by io.emma.mediacenter remote startup v1\n";
#[derive(Clone)]
pub struct Autostart {
    directory: PathBuf,
    executable: PathBuf,
    #[cfg(test)]
    pub(crate) fail_restore: bool,
}
fn failed() -> AppError {
    AppError::new(
        "unavailable",
        "User login startup could not be changed safely.",
    )
}
impl Autostart {
    pub fn new(config: &Path, executable: &Path) -> Result<Self, AppError> {
        if !config.is_absolute() || !executable.is_absolute() {
            return Err(failed());
        }
        let executable = executable.canonicalize().map_err(|_| failed())?;
        if !executable.is_file() {
            return Err(failed());
        }
        quote_exec(&executable)?;
        Ok(Self {
            directory: config.join("autostart"),
            executable,
            #[cfg(test)]
            fail_restore: false,
        })
    }
    pub fn production() -> Result<Self, AppError> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".config")))
            .ok_or_else(failed)?;
        Self::new(&config, &std::env::current_exe().map_err(|_| failed())?)
    }
    fn path(&self) -> PathBuf {
        self.directory.join("io.emma.mediacenter-remote.desktop")
    }
    pub(crate) fn contents(&self) -> Result<String, AppError> {
        Ok(format!(
            "{OWNER}{}",
            include_str!("autostart.desktop.in")
                .replace("@EXECUTABLE@", &quote_exec(&self.executable)?)
        ))
    }
    pub(crate) fn snapshot(&self) -> Result<Option<Vec<u8>>, AppError> {
        let path = self.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(failed()),
        };
        if !metadata.is_file() || metadata.len() > 65536 {
            return Err(failed());
        }
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| failed())?;
        let mut bytes = vec![];
        file.by_ref()
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| failed())?;
        if bytes.len() > 65536 || !bytes.starts_with(OWNER.as_bytes()) {
            return Err(failed());
        }
        Ok(Some(bytes))
    }
    pub(crate) fn restore(
        &self,
        before: Option<&[u8]>,
        written: Option<&[u8]>,
    ) -> Result<(), AppError> {
        #[cfg(test)]
        if self.fail_restore {
            return Err(failed());
        }
        if self.snapshot()?.as_deref() != written {
            return Err(failed());
        }
        match before {
            Some(bytes) => super::tls::write_private(&self.path(), bytes).map_err(|_| failed()),
            None => match std::fs::remove_file(self.path()) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(failed()),
            },
        }
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<(), AppError> {
        let path = self.path();
        let before = self.snapshot()?;
        if !enabled && before.is_none() {
            return Ok(());
        }
        if !enabled {
            return std::fs::remove_file(path).map_err(|_| failed());
        }
        std::fs::create_dir_all(&self.directory).map_err(|_| failed())?;
        super::tls::write_private(&path, self.contents()?.as_bytes()).map_err(|_| failed())?;
        Ok(())
    }
}
/// Desktop-entry escaping has two layers: Exec quoting then string-value escaping.
fn quote_exec(path: &Path) -> Result<String, AppError> {
    let input = path.to_str().ok_or_else(failed)?;
    if input.contains('=') || input.chars().any(char::is_control) {
        return Err(failed());
    }
    let mut quoted = String::from("\"");
    for c in input.chars() {
        match c {
            '%' => quoted.push_str("%%"),
            '\\' | '"' | '`' | '$' => {
                quoted.push('\\');
                quoted.push(c);
            }
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    Ok(quoted.replace('\\', "\\\\"))
}
// A write-ahead marker survives process crashes and unverified rollback. It is
// deliberately separate from the desired preference: stored intent is not evidence
// that the user's actual login entry was committed coherently.
pub(crate) fn pending_path(
    state: &crate::app_state::AppState,
    identity: &super::protocol::Id,
) -> PathBuf {
    state.data_dir.join(format!(
        "remote-lifetime-pending-{}.json",
        identity.as_str()
    ))
}
fn marker(identity: &super::protocol::Id) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"version":1,"serverIdentity":identity}))
        .expect("bounded identity marker")
}
pub(crate) fn pending(
    state: &crate::app_state::AppState,
    identity: &super::protocol::Id,
) -> Result<bool, AppError> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let path = pending_path(state, identity);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(failed()),
    };
    if !metadata.is_file() || metadata.len() > 1024 {
        return Err(failed());
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| failed())?;
    let mut bytes = vec![];
    file.take(1025)
        .read_to_end(&mut bytes)
        .map_err(|_| failed())?;
    if bytes == marker(identity) {
        Ok(true)
    } else {
        Err(failed())
    }
}
pub(crate) fn begin_change(
    state: &crate::app_state::AppState,
    identity: &super::protocol::Id,
) -> Result<(), AppError> {
    if pending(state, identity)? {
        return Ok(());
    }
    super::tls::write_private(&pending_path(state, identity), &marker(identity))
        .map_err(|_| failed())
}
pub(crate) fn finish_change(
    state: &crate::app_state::AppState,
    identity: &super::protocol::Id,
) -> Result<(), AppError> {
    if !pending(state, identity)? {
        return Err(failed());
    }
    std::fs::remove_file(pending_path(state, identity)).map_err(|_| failed())?;
    std::fs::File::open(&state.data_dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| failed())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversible_user_only_file_and_exec_escaping() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("media $`\"%\\ center");
        std::fs::write(&executable, b"fixture").unwrap();
        let adapter = Autostart::new(dir.path(), &executable).unwrap();
        adapter.set_enabled(true).unwrap();
        let entry = std::fs::read_to_string(adapter.path()).unwrap();
        assert!(entry.contains(" --remote-background\n"));
        assert!(entry.contains("\\\\$\\\\`\\\\\"%%\\\\\\\\ center\""));
        assert!(!entry.contains("sudo"));
        adapter.set_enabled(false).unwrap();
        assert!(!adapter.path().exists());
        adapter.set_enabled(false).unwrap();
    }
    #[test]
    fn task20_invalid_desktop_exec_path_is_rejected_before_file_writes() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("media=center");
        std::fs::write(&executable, b"fixture").unwrap();
        assert!(Autostart::new(dir.path(), &executable).is_err());
        assert!(!dir.path().join("autostart").exists());
    }
    #[test]
    fn unowned_files_and_symlinks_are_never_overwritten_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("app");
        std::fs::write(&exe, b"fixture").unwrap();
        let adapter = Autostart::new(dir.path(), &exe).unwrap();
        std::fs::create_dir_all(&adapter.directory).unwrap();
        std::fs::write(adapter.path(), b"user file").unwrap();
        assert!(adapter.set_enabled(true).is_err());
        assert!(adapter.set_enabled(false).is_err());
        assert_eq!(std::fs::read(adapter.path()).unwrap(), b"user file");
        std::fs::remove_file(adapter.path()).unwrap();
        std::os::unix::fs::symlink(&exe, adapter.path()).unwrap();
        assert!(adapter.set_enabled(true).is_err());
        assert!(adapter.set_enabled(false).is_err());
        assert_eq!(std::fs::read(exe).unwrap(), b"fixture");
    }
}
