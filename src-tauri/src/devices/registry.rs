use crate::contracts::AppError;
use std::path::PathBuf;
#[derive(Clone, Debug)]
pub struct Volume {
    pub path: String,
    pub id: String,
    pub drive_id: String,
    pub identity: Option<String>,
    pub mounts: Vec<PathBuf>,
    pub label: String,
    pub size: u64,
    pub removable: bool,
}
pub fn resolve<'a>(volumes: &'a [Volume], identity: &str) -> Result<&'a Volume, AppError> {
    let mut matches = volumes
        .iter()
        .filter(|v| v.identity.as_deref() == Some(identity));
    let first = matches
        .next()
        .ok_or_else(|| AppError::new("volume_offline", "The source drive is offline."))?;
    if matches.next().is_some() {
        return Err(AppError::new("volume_ambiguous","Multiple connected volumes have the same identity. Disconnect the unwanted copy before using this source."));
    }
    Ok(first)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn volume(path: &str) -> Volume {
        Volume {
            path: "/test".into(),
            id: "opaque".into(),
            drive_id: "drive".into(),
            identity: Some("uuid:abc".into()),
            mounts: vec![path.into()],
            label: "test".into(),
            size: 1024,
            removable: true,
        }
    }
    #[test]
    fn replug_at_new_mount_path_preserves_source_id() {
        let volumes = vec![volume("/media/new")];
        assert_eq!(
            resolve(&volumes, "uuid:abc").unwrap().mounts[0],
            PathBuf::from("/media/new")
        );
    }
    #[test]
    fn duplicate_uuid_requires_disambiguation() {
        assert_eq!(
            resolve(&[volume("/a"), volume("/b")], "uuid:abc")
                .unwrap_err()
                .code,
            "volume_ambiguous"
        );
    }
    #[test]
    fn usb_c_does_not_require_separate_media_logic() {
        assert!(resolve(&[volume("/media/usb-c")], "uuid:abc").is_ok());
    }
}
