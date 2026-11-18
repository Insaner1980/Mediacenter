use crate::{
    app_state::AppState,
    contracts::*,
    library::{confirmation, repository},
};
pub async fn add(
    state: AppState,
    grant_id: String,
    kinds: Vec<MediaKind>,
) -> Result<String, AppError> {
    add_selected(state, Selection::Picker(grant_id), kinds).await
}

pub async fn add_default(
    state: AppState,
    root: std::path::PathBuf,
    kind: MediaKind,
) -> Result<String, AppError> {
    add_selected(state, Selection::Default(root), vec![kind]).await
}

enum Selection {
    Picker(String),
    Default(std::path::PathBuf),
}

async fn add_selected(
    state: AppState,
    selection: Selection,
    kinds: Vec<MediaKind>,
) -> Result<String, AppError> {
    let gate = state.device_gate.clone().try_lock_owned().map_err(|_| {
        AppError::new(
            "device_busy",
            "A device operation is running. Wait before adding a source.",
        )
    })?;
    let devices = crate::devices::udisks::discover().await?;
    tokio::task::spawn_blocking(move || {
        let _gate = gate;
        let automatic = matches!(selection, Selection::Default(_));
        let root = match selection {
            Selection::Picker(grant_id) => state
                .grants
                .lock()
                .map_err(|_| AppError::new("state_error", "Folder selection is unavailable."))?
                .consume(&grant_id)?,
            Selection::Default(root) => root,
        };
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        let root = if automatic {
            let root = super::defaults::registration_root(&db, &root, &devices)?;
            super::defaults::check_automatic_attachment(&state, &db, &root, &devices)?;
            root
        } else {
            root
        };
        let id = repository::add(&db, &root, &kinds, &devices)?;
        let record = repository::get(&db, &id)?;
        if !automatic
            && record
                .volume
                .as_deref()
                .is_some_and(|v| v.starts_with("fingerprint:"))
        {
            state
                .source_confirmations
                .lock()
                .map_err(|_| {
                    AppError::new("source_state_error", "Source confirmation is unavailable.")
                })?
                .insert(id.clone(), confirmation::capture(&root)?);
        }
        Ok(id)
    })
    .await
    .map_err(|_| AppError::new("source_error", "The source could not be added."))?
}
pub fn list(
    state: &AppState,
    devices: &crate::devices::udisks::Devices,
) -> Result<Vec<MediaSource>, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
    let mut sources = repository::list(&db, devices)?;
    for source in &mut sources {
        let record = repository::get(&db, &source.id)?;
        if record
            .volume
            .as_deref()
            .is_some_and(|id| id.starts_with("fingerprint:"))
        {
            let verified = repository::root(&record, devices)
                .and_then(|root| confirmation::check(state, &record, &root));
            if verified.is_err() {
                state
                    .source_confirmations
                    .lock()
                    .map_err(|_| {
                        AppError::new("source_state_error", "Source confirmation is unavailable.")
                    })?
                    .remove(&source.id);
                if source.availability == "available" {
                    source.availability = "reattach_required".into();
                    db.execute(
                        "UPDATE media_sources SET availability='reattach_required' WHERE id=?",
                        [&source.id],
                    )?;
                }
            }
        }
    }
    Ok(sources)
}
/// The same root policy is used for browsing, confirmation, and final registration.
pub fn remote_roots(
    state: &AppState,
    db: &rusqlite::Connection,
    devices: &crate::devices::udisks::Devices,
    defaults: Vec<std::path::PathBuf>,
) -> Result<Vec<std::path::PathBuf>, AppError> {
    let mut roots = repository::records(db)?
        .iter()
        .filter_map(|source| {
            let root = repository::root(source, devices).ok()?;
            confirmation::check(state, source, &root).ok()?;
            Some(root)
        })
        .collect::<Vec<_>>();
    roots.extend(defaults);
    roots.extend(
        devices
            .volumes
            .iter()
            .filter(|v| {
                v.removable
                    && v.identity.as_ref().is_some_and(|id| {
                        crate::devices::registry::resolve(&devices.volumes, id).is_ok()
                    })
            })
            .flat_map(|v| v.mounts.clone()),
    );
    Ok(roots)
}

pub fn authorize_remote_root(
    state: &AppState,
    db: &rusqlite::Connection,
    selection: &crate::remote::folders::Selection,
    devices: &crate::devices::udisks::Devices,
    defaults: Vec<std::path::PathBuf>,
) -> Result<(), AppError> {
    if !remote_roots(state, db, devices, defaults)?
        .iter()
        .any(|root| root == selection.root())
    {
        return Err(AppError::new(
            "not_found",
            "The folder is no longer authorized. Refresh folders.",
        ));
    }
    Ok(())
}

/// Remote selections are reopened beneath their held directory descriptor immediately
/// before producing the native grant, and again before the repository effect.
pub async fn add_remote(
    state: AppState,
    selection: crate::remote::folders::Selection,
    kinds: Vec<MediaKind>,
    defaults: Vec<std::path::PathBuf>,
) -> Result<String, AppError> {
    let _gate = state
        .device_gate
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::new("busy", "A device operation is running."))?;
    let devices = crate::devices::udisks::discover().await?;
    tokio::task::spawn_blocking(move || {
        let _gate = _gate;
        register_remote(&state, &selection, &kinds, &devices, defaults)
    })
    .await
    .map_err(|_| AppError::new("source_error", "Source could not be added."))?
}

fn register_remote(
    state: &AppState,
    selection: &crate::remote::folders::Selection,
    kinds: &[MediaKind],
    devices: &crate::devices::udisks::Devices,
    defaults: Vec<std::path::PathBuf>,
) -> Result<String, AppError> {
    selection.check_expiry(crate::app_state::now_ms())?;
    let _descriptor = selection.revalidate()?;
    // Keep root authorization and the effect in one transaction/DB guard. Both local
    // and remote removal take this same guard, so revocation cannot race the grant.
    let mut db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Library unavailable."))?;
    let transaction = db.transaction()?;
    authorize_remote_root(state, &transaction, selection, devices, defaults)?;
    let grant = state
        .grants
        .lock()
        .map_err(|_| AppError::new("state_error", "Folder selection unavailable."))?
        .issue(selection.path())?;
    let root = state
        .grants
        .lock()
        .map_err(|_| AppError::new("state_error", "Folder selection unavailable."))?
        .consume(&grant)?;
    selection.revalidate()?;
    if root != selection.path() {
        return Err(AppError::new("stale_state", "Folder changed."));
    }
    let id = repository::add_resolved(&transaction, &root, kinds, devices)?;
    selection.revalidate()?;
    let record = repository::get(&transaction, &id)?;
    if record
        .volume
        .as_deref()
        .is_some_and(|v| v.starts_with("fingerprint:"))
    {
        state
            .source_confirmations
            .lock()
            .map_err(|_| AppError::new("state_error", "Source confirmation unavailable."))?
            .insert(
                id.clone(),
                confirmation::capture(&crate::library::files::descriptor_path(&_descriptor))?,
            );
    }
    transaction.commit()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_source_requires_confirmation_to_contribute_a_remote_root() {
        let dir = tempfile::tempdir().unwrap();
        let mount = dir.path().join("drive");
        let root = mount.join("Music");
        std::fs::create_dir_all(&root).unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let devices = crate::devices::udisks::Devices {
            drives: vec![],
            volumes: vec![crate::devices::registry::Volume {
                path: "/test".into(),
                id: "drive".into(),
                drive_id: "drive".into(),
                identity: Some("fingerprint:v1:test".into()),
                mounts: vec![mount.clone()],
                label: "Drive".into(),
                size: 1024,
                removable: true,
            }],
        };
        let db = state.db.lock().unwrap();
        let source = repository::add(&db, &root, &[MediaKind::Audio], &devices).unwrap();
        assert_eq!(
            remote_roots(&state, &db, &devices, vec![]).unwrap(),
            vec![mount.clone()]
        );
        state
            .source_confirmations
            .lock()
            .unwrap()
            .insert(source, confirmation::capture(&root).unwrap());
        assert_eq!(
            remote_roots(&state, &db, &devices, vec![]).unwrap(),
            vec![root, mount]
        );
    }
    #[tokio::test]
    async fn source_add_cannot_bypass_active_device_gate() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let id = state
            .grants
            .lock()
            .unwrap()
            .issue(dir.path().into())
            .unwrap();
        let _gate = state.device_gate.lock().await;
        assert_eq!(
            add(state.clone(), id.clone(), vec![MediaKind::Video])
                .await
                .unwrap_err()
                .code,
            "device_busy"
        );
        assert!(state.grants.lock().unwrap().consume(&id).is_ok());
        assert!(repository::records(&state.db.lock().unwrap())
            .unwrap()
            .is_empty());
    }
    #[test]
    fn remote_registration_rechecks_root_after_serialized_removal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("LocalGrant");
        std::fs::create_dir_all(root.join("Albums")).unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let source = repository::add(
            &state.db.lock().unwrap(),
            &root,
            &[MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let owner = crate::remote::adapters::checked("phone").unwrap();
        let mut folders = crate::remote::folders::Folders::default();
        let node = folders
            .roots(&owner, vec![root.clone()], crate::app_state::now_ms())
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let child = folders
            .children(&owner, &node, None, None, crate::app_state::now_ms())
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let selection = folders
            .selection(&owner, &child, crate::app_state::now_ms())
            .unwrap();
        let db = state.db.lock().unwrap();
        let worker_state = state.clone();
        let (started, waiting) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            register_remote(
                &worker_state,
                &selection,
                &[MediaKind::Audio],
                &Default::default(),
                vec![],
            )
        });
        waiting.recv().unwrap();
        repository::remove(&db, &source).unwrap();
        drop(db);
        assert_eq!(worker.join().unwrap().unwrap_err().code, "not_found");
        assert!(repository::records(&state.db.lock().unwrap())
            .unwrap()
            .is_empty());
        assert!(root.join("Albums").is_dir());
    }
    #[test]
    fn default_and_removable_roots_survive_association_removal() {
        for removable in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("Music");
            let child = root.join("Albums");
            std::fs::create_dir_all(&child).unwrap();
            let state = AppState::open(dir.path().join("data")).unwrap();
            let devices = if removable {
                crate::devices::udisks::Devices {
                    drives: vec![],
                    volumes: vec![crate::devices::registry::Volume {
                        path: "/test".into(),
                        id: "usb".into(),
                        drive_id: "drive".into(),
                        identity: Some("uuid:test".into()),
                        mounts: vec![root.clone()],
                        label: "USB".into(),
                        size: 1024,
                        removable: true,
                    }],
                }
            } else {
                Default::default()
            };
            let defaults = if removable {
                vec![]
            } else {
                vec![root.clone()]
            };
            let source = repository::add(
                &state.db.lock().unwrap(),
                &root,
                &[MediaKind::Audio],
                &devices,
            )
            .unwrap();
            let owner = crate::remote::adapters::checked("phone").unwrap();
            let mut folders = crate::remote::folders::Folders::default();
            let node = folders
                .roots(&owner, vec![root.clone()], crate::app_state::now_ms())
                .unwrap()
                .items
                .as_slice()[0]
                .node_id
                .clone();
            let node = folders
                .children(&owner, &node, None, None, crate::app_state::now_ms())
                .unwrap()
                .items
                .as_slice()[0]
                .node_id
                .clone();
            let selection = folders
                .selection(&owner, &node, crate::app_state::now_ms())
                .unwrap();
            repository::remove(&state.db.lock().unwrap(), &source).unwrap();
            let added =
                register_remote(&state, &selection, &[MediaKind::Audio], &devices, defaults)
                    .unwrap();
            let record = repository::get(&state.db.lock().unwrap(), &added).unwrap();
            assert_eq!(repository::root(&record, &devices).unwrap(), child);
            assert_eq!(record.kinds, vec![MediaKind::Audio]);
        }
    }
    #[test]
    fn remote_grant_revalidates_before_registration_and_preserves_actual_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Music");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("Albums")).unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let owner = crate::remote::adapters::checked("phone").unwrap();
        let mut folders = crate::remote::folders::Folders::default();
        let root_node = folders
            .roots(&owner, vec![root.clone()], crate::app_state::now_ms())
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let node = folders
            .children(&owner, &root_node, None, None, crate::app_state::now_ms())
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let selected = folders
            .selection(&owner, &node, crate::app_state::now_ms())
            .unwrap();
        std::fs::rename(root.join("Albums"), root.join("original")).unwrap();
        std::os::unix::fs::symlink("/etc", root.join("Albums")).unwrap();
        assert!(register_remote(
            &state,
            &selected,
            &[MediaKind::Audio],
            &Default::default(),
            vec![root.clone()]
        )
        .is_err());
        assert!(repository::records(&state.db.lock().unwrap())
            .unwrap()
            .is_empty());
        let selection = folders
            .selection(&owner, &root_node, crate::app_state::now_ms())
            .unwrap();
        let id = register_remote(
            &state,
            &selection,
            &[MediaKind::Audio],
            &Default::default(),
            vec![root.clone()],
        )
        .unwrap();
        let record = repository::get(&state.db.lock().unwrap(), &id).unwrap();
        assert_eq!(record.root, root);
        assert_eq!(record.kinds, vec![MediaKind::Audio]);
    }
}
