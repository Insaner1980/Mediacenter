use crate::{app_state::AppState, contracts::AppError, library::repository::SourceRecord};
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};
#[derive(Clone, Debug, PartialEq)]
pub struct MountConfirmation {
    mount_id: u64,
    device: u64,
    inode: u64,
}
pub fn capture(path: &Path) -> Result<MountConfirmation, AppError> {
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::new("source_invalid", "The source path is invalid."))?;
    let mut stat = std::mem::MaybeUninit::<nix::libc::statx>::zeroed();
    // SAFETY: valid nul-terminated path and writable statx buffer of the correct size.
    let result = unsafe {
        nix::libc::statx(
            nix::libc::AT_FDCWD,
            name.as_ptr(),
            nix::libc::AT_NO_AUTOMOUNT,
            nix::libc::STATX_MNT_ID,
            stat.as_mut_ptr(),
        )
    };
    if result != 0 {
        return Err(AppError::new(
            "source_identity_unavailable",
            "This source's current mount identity could not be verified.",
        ));
    }
    let stat = unsafe { stat.assume_init() };
    if stat.stx_mask & nix::libc::STATX_MNT_ID == 0 {
        return Err(AppError::new(
            "source_identity_unavailable",
            "This system cannot verify the source mount identity.",
        ));
    }
    let metadata = std::fs::metadata(path)?;
    Ok(MountConfirmation {
        mount_id: stat.stx_mnt_id,
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}
pub fn check(state: &AppState, source: &SourceRecord, root: &Path) -> Result<(), AppError> {
    if !source
        .volume
        .as_deref()
        .is_some_and(|id| id.starts_with("fingerprint:"))
    {
        return Ok(());
    }
    let current = capture(root)?;
    let mut confirmed = state
        .source_confirmations
        .lock()
        .map_err(|_| AppError::new("source_state_error", "Source confirmation is unavailable."))?;
    if confirmed.get(&source.id) == Some(&current) {
        return Ok(());
    }
    confirmed.remove(&source.id);
    Err(AppError::new(
        "source_reattach_required",
        "This volume has no filesystem UUID. Choose its folder again to confirm this attachment.",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uuidless_source_requires_native_confirmation_after_restart_or_remount() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let source = SourceRecord {
            id: "source".into(),
            root: dir.path().into(),
            volume: Some("fingerprint:v1:test".into()),
            relative: None,
            label: "test".into(),
            kinds: vec![],
            availability: "available".into(),
        };
        assert!(check(&state, &source, dir.path()).is_err());
        let mut identity = capture(dir.path()).unwrap();
        state
            .source_confirmations
            .lock()
            .unwrap()
            .insert(source.id.clone(), identity.clone());
        assert!(check(&state, &source, dir.path()).is_ok());
        identity.mount_id += 1;
        state
            .source_confirmations
            .lock()
            .unwrap()
            .insert(source.id.clone(), identity);
        assert!(check(&state, &source, dir.path()).is_err());
    }
}
