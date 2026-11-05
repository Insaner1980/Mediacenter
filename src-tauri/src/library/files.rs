use crate::contracts::AppError;
use std::{
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Component, Path},
};
/// Opens relative to an already opened source directory. The kernel enforces the boundary.
pub fn open(root: &Path, relative: &Path) -> Result<File, AppError> {
    open_entry(root, relative, false)
}
pub fn open_directory(root: &Path, relative: &Path) -> Result<File, AppError> {
    open_entry(root, relative, true)
}
fn open_entry(root: &Path, relative: &Path, is_directory: bool) -> Result<File, AppError> {
    if (!is_directory && relative.as_os_str().is_empty())
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(AppError::new(
            "source_escape",
            "The file is outside the selected folder.",
        ));
    }
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
        .open(root)?;
    if is_directory && relative.as_os_str().is_empty() {
        return Ok(directory);
    }
    let relative = std::ffi::CString::new(relative.as_os_str().as_bytes())
        .map_err(|_| AppError::new("invalid_path", "The local filename is invalid."))?;
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = OpenHow {
        flags: (nix::libc::O_RDONLY
            | nix::libc::O_CLOEXEC
            | nix::libc::O_NONBLOCK
            | if is_directory {
                nix::libc::O_DIRECTORY
            } else {
                0
            }) as u64,
        mode: 0,
        resolve: 0x08 | 0x02 | 0x01,
    }; // beneath, no magic links, no mount crossing
       // SAFETY: all pointers remain valid for the syscall; the returned descriptor is uniquely owned.
    let fd = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_openat2,
            directory.as_raw_fd(),
            relative.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(AppError::new(
            "file_unavailable",
            "The file is unavailable or outside the selected source.",
        ));
    }
    let file = unsafe { File::from_raw_fd(fd as i32) };
    if if is_directory {
        !file.metadata()?.is_dir()
    } else {
        !file.metadata()?.is_file()
    } {
        return Err(AppError::new(
            "invalid_media",
            "Only regular local media files can be opened.",
        ));
    }
    Ok(file)
}
pub fn descriptor_path(file: &File) -> std::path::PathBuf {
    format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd()).into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kernel_boundary_rejects_external_symlink() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.path().join("outside")).unwrap();
        assert!(open(dir.path(), Path::new("outside")).is_err());
    }
    #[test]
    fn descriptor_survives_path_replacement() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), b"original").unwrap();
        let mut file = open(dir.path(), Path::new("file")).unwrap();
        std::fs::rename(dir.path().join("file"), dir.path().join("old")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.path().join("file")).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"original");
    }
}
