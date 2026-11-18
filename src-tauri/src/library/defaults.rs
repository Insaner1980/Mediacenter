use crate::{contracts::AppError, devices::udisks::Devices, library::repository};
use std::path::{Path, PathBuf};

/// System defaults are trusted roots, but a disabled XDG directory can resolve
/// to HOME. Never interpret that as permission to index the whole home folder.
pub fn resolve(home: &Path, directory: Option<PathBuf>) -> Result<Option<PathBuf>, AppError> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    if !directory.is_dir() {
        return Ok(None);
    }
    let root = directory.canonicalize()?;
    let home = home.canonicalize()?;
    if root == home || root.parent().is_none() || home.starts_with(&root) {
        return Ok(None);
    }
    Ok(Some(root))
}

/// Keep an already granted ancestor's identity instead of creating duplicates.
pub fn registration_root(
    db: &rusqlite::Connection,
    root: &Path,
    devices: &Devices,
) -> Result<PathBuf, AppError> {
    for record in repository::records(db)? {
        if let Ok(existing) = repository::root(&record, devices) {
            if root.starts_with(&existing) {
                return Ok(existing);
            }
        }
    }
    Ok(root.to_path_buf())
}

/// A configured default path is not a new picker confirmation. In particular,
/// UUID-less removable volumes must keep their per-attachment user grant.
pub fn check_automatic_attachment(
    state: &crate::app_state::AppState,
    db: &rusqlite::Connection,
    root: &Path,
    devices: &Devices,
) -> Result<(), AppError> {
    let selected = devices
        .volumes
        .iter()
        .flat_map(|volume| volume.mounts.iter().map(move |mount| (volume, mount)))
        .filter(|(_, mount)| root.starts_with(mount))
        .max_by_key(|(_, mount)| mount.components().count());
    if let Some((volume, _)) = selected {
        if volume.removable
            && volume
                .identity
                .as_deref()
                .is_some_and(|id| id.starts_with("fingerprint:"))
        {
            for source in repository::records(db)? {
                if source.volume == volume.identity
                    && repository::root(&source, devices).is_ok_and(|path| path == root)
                {
                    return super::confirmation::check(state, &source, root);
                }
            }
            return Err(AppError::new(
                "source_reattach_required",
                "Open this folder to confirm the current USB attachment.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_state::AppState, contracts::MediaKind, devices::udisks::Devices, library::repository,
    };

    #[test]
    fn missing_or_disabled_default_does_not_grant_home_or_root() {
        let home = tempfile::tempdir().unwrap();
        assert!(resolve(home.path(), None).unwrap().is_none());
        assert!(resolve(home.path(), Some(home.path().join("missing")))
            .unwrap()
            .is_none());
        assert!(resolve(home.path(), Some(home.path().into()))
            .unwrap()
            .is_none());
        assert!(resolve(home.path(), Some("/".into())).unwrap().is_none());
    }

    #[test]
    fn configured_folder_is_registered_idempotently_and_existing_parent_reused() {
        let home = tempfile::tempdir().unwrap();
        let pictures = home.path().join("Kuvat");
        std::fs::create_dir(&pictures).unwrap();
        let state = AppState::open(home.path().join("data")).unwrap();
        let db = state.db.lock().unwrap();
        let devices = Devices::default();
        let root = resolve(home.path(), Some(pictures.clone()))
            .unwrap()
            .unwrap();
        let id = repository::add(
            &db,
            &registration_root(&db, &root, &devices).unwrap(),
            &[MediaKind::Photo],
            &devices,
        )
        .unwrap();
        let again = repository::add(
            &db,
            &registration_root(&db, &root, &devices).unwrap(),
            &[MediaKind::Photo],
            &devices,
        )
        .unwrap();
        assert_eq!(id, again);
        let child = pictures.join("Trips");
        std::fs::create_dir(&child).unwrap();
        assert_eq!(registration_root(&db, &child, &devices).unwrap(), pictures);
        assert_eq!(repository::records(&db).unwrap().len(), 1);
    }

    #[test]
    fn existing_child_is_not_silently_duplicated_by_default_registration() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("Pictures");
        let child = root.join("Trips");
        std::fs::create_dir_all(&child).unwrap();
        let state = AppState::open(home.path().join("data")).unwrap();
        let db = state.db.lock().unwrap();
        let devices = Devices::default();
        repository::add(&db, &child, &[MediaKind::Photo], &devices).unwrap();
        let error = repository::add(
            &db,
            &registration_root(&db, &root, &devices).unwrap(),
            &[MediaKind::Photo],
            &devices,
        )
        .unwrap_err();
        assert_eq!(error.code, "source_overlap");
        assert_eq!(repository::records(&db).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use crate::{
        app_state::AppState, contracts::MediaKind, devices::registry::Volume, library::confirmation,
    };
    #[test]
    fn automatic_registration_cannot_confirm_a_uuidless_attachment() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("USB");
        std::fs::create_dir(&root).unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let db = state.db.lock().unwrap();
        let devices = Devices {
            drives: vec![],
            volumes: vec![Volume {
                path: "/test".into(),
                id: "usb".into(),
                drive_id: "drive".into(),
                identity: Some("fingerprint:test".into()),
                mounts: vec![root.clone()],
                label: "USB".into(),
                size: 1024,
                removable: true,
            }],
        };
        assert_eq!(
            check_automatic_attachment(&state, &db, &root, &devices)
                .unwrap_err()
                .code,
            "source_reattach_required"
        );
        let id = repository::add(&db, &root, &[MediaKind::Photo], &devices).unwrap();
        assert_eq!(
            check_automatic_attachment(&state, &db, &root, &devices)
                .unwrap_err()
                .code,
            "source_reattach_required"
        );
        state
            .source_confirmations
            .lock()
            .unwrap()
            .insert(id.clone(), confirmation::capture(&root).unwrap());
        assert!(check_automatic_attachment(&state, &db, &root, &devices).is_ok());
        state.source_confirmations.lock().unwrap().clear();
        assert_eq!(
            check_automatic_attachment(&state, &db, &root, &devices)
                .unwrap_err()
                .code,
            "source_reattach_required"
        );
        assert!(state.source_confirmations.lock().unwrap().is_empty());
    }
}
