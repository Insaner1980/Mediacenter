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
}
