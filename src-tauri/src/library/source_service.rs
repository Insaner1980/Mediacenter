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
/// Remote selections are reopened beneath their held directory descriptor immediately
/// before producing the native grant, and again before the repository effect.
pub async fn add_remote(
    state: AppState,
    selection: crate::remote::folders::Selection,
    kinds: Vec<MediaKind>,
) -> Result<String, AppError> {
    let _gate = state
        .device_gate
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::new("busy", "A device operation is running."))?;
    let devices = crate::devices::udisks::discover().await?;
    tokio::task::spawn_blocking(move || {
        let _gate = _gate;
        register_remote(&state, &selection, &kinds, &devices)
    })
    .await
    .map_err(|_| AppError::new("source_error", "Source could not be added."))?
}

fn register_remote(
    state: &AppState,
    selection: &crate::remote::folders::Selection,
    kinds: &[MediaKind],
    devices: &crate::devices::udisks::Devices,
) -> Result<String, AppError> {
    selection.check_expiry(crate::app_state::now_ms())?;
    let _descriptor = selection.revalidate()?;
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
    let mut db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "Library unavailable."))?;
    let transaction = db.transaction()?;
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
        assert!(
            register_remote(&state, &selected, &[MediaKind::Audio], &Default::default()).is_err()
        );
        assert!(repository::records(&state.db.lock().unwrap())
            .unwrap()
            .is_empty());
        let selection = folders
            .selection(&owner, &root_node, crate::app_state::now_ms())
            .unwrap();
        let id =
            register_remote(&state, &selection, &[MediaKind::Audio], &Default::default()).unwrap();
        let record = repository::get(&state.db.lock().unwrap(), &id).unwrap();
        assert_eq!(record.root, root);
        assert_eq!(record.kinds, vec![MediaKind::Audio]);
    }
}
