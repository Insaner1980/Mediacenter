use crate::contracts::AppError;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Grants {
    entries: HashMap<String, (PathBuf, Instant)>,
}
impl Grants {
    pub fn issue(&mut self, path: PathBuf) -> Result<String, AppError> {
        self.entries
            .retain(|_, (_, issued)| issued.elapsed() < Duration::from_secs(600));
        if self.entries.len() >= 16 {
            return Err(AppError::new(
                "grant_limit",
                "Too many pending folder choices. Complete a selection first.",
            ));
        }
        let root = path.canonicalize()?;
        if !root.is_dir() {
            return Err(AppError::new("invalid_source", "Choose a readable folder."));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.entries.insert(id.clone(), (root, Instant::now()));
        Ok(id)
    }
    pub fn consume(&mut self, id: &str) -> Result<PathBuf, AppError> {
        let (path, issued) = self
            .entries
            .remove(id)
            .ok_or_else(|| AppError::new("grant_invalid", "Choose the folder again."))?;
        if issued.elapsed() >= Duration::from_secs(600) {
            return Err(AppError::new(
                "grant_expired",
                "The folder choice expired. Choose the folder again.",
            ));
        }
        Ok(path)
    }
}
pub fn resolve_local(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(AppError::new(
            "source_escape",
            "The file is outside the selected folder.",
        ));
    }
    let root = root.canonicalize()?;
    let path = root.join(relative).canonicalize()?;
    if !path.starts_with(&root) {
        return Err(AppError::new(
            "source_escape",
            "The file is outside the selected folder.",
        ));
    }
    Ok(path)
}
pub fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[test]
    fn grant_is_single_use_and_expires() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = Grants::default();
        let id = g.issue(dir.path().into()).unwrap();
        assert_eq!(g.consume(&id).unwrap(), dir.path());
        assert!(g.consume(&id).is_err());
        let id = g.issue(dir.path().into()).unwrap();
        g.entries.get_mut(&id).unwrap().1 = Instant::now() - Duration::from_secs(601);
        assert!(g.consume(&id).is_err());
    }
    #[test]
    fn source_root_validation_rejects_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"x").unwrap();
        assert!(Grants::default().issue(path).is_err());
    }
    #[test]
    fn overlapping_roots_are_detected() {
        assert!(overlaps(Path::new("/media/a"), Path::new("/media/a/b")));
        assert!(!overlaps(Path::new("/media/a"), Path::new("/media/ab")));
    }
    #[test]
    fn external_symlink_and_cycles_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.path().join("outside")).unwrap();
        std::os::unix::fs::symlink("cycle", dir.path().join("cycle")).unwrap();
        assert!(resolve_local(dir.path(), Path::new("outside")).is_err());
        assert!(resolve_local(dir.path(), Path::new("cycle")).is_err());
        assert!(resolve_local(dir.path(), Path::new("../etc/passwd")).is_err());
    }
    #[test]
    fn non_utf8_path_identity_is_lossless() {
        let dir = tempfile::tempdir().unwrap();
        let name = std::ffi::OsString::from_vec(vec![0xff, b'.', b'm', b'p', b'4']);
        let path = dir.path().join(&name);
        std::fs::write(&path, b"test").unwrap();
        assert_eq!(resolve_local(dir.path(), Path::new(&name)).unwrap(), path);
    }
}
