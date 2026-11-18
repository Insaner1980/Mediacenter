//! Native folder/source workflows. Server-held confirmations own all resource identities.
use crate::{
    app_state::{now_ms, AppState},
    contracts::AppError,
    library::{confirmation, repository},
    remote::{
        adapters::checked,
        dispatch::{native_error, unavailable, value, Dispatcher},
        folders::Selection,
        protocol::*,
    },
};
use std::path::PathBuf;
use tauri::Manager;
#[derive(Clone)]
pub enum LibraryAction {
    Add {
        selection: Selection,
        kinds: Vec<crate::contracts::MediaKind>,
    },
    Reattach {
        selection: Selection,
        source: repository::SourceRecord,
    },
    Remove {
        source: repository::SourceRecord,
    },
}
pub fn handles(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::FoldersRoots(_)
            | RemoteCommand::FoldersChildren(_)
            | RemoteCommand::FoldersPrepareAdd(_)
            | RemoteCommand::FoldersConfirmAdd(_)
            | RemoteCommand::SourcesList(_)
            | RemoteCommand::SourcesScan(_)
            | RemoteCommand::SourcesCancelScan(_)
            | RemoteCommand::SourcesPrepareRemove(_)
            | RemoteCommand::SourcesConfirmRemove(_)
            | RemoteCommand::SourcesConfirmReattachment(_)
    )
}
fn native_kind(kind: &MediaKind) -> crate::contracts::MediaKind {
    match kind {
        MediaKind::Video => crate::contracts::MediaKind::Video,
        MediaKind::Music => crate::contracts::MediaKind::Audio,
        MediaKind::Photo => crate::contracts::MediaKind::Photo,
    }
}
fn state_error() -> AppError {
    AppError::new("unavailable", "Library unavailable.")
}
fn stale() -> AppError {
    AppError::new("stale_state", "Source changed. Review it again.")
}
fn same_record(a: &repository::SourceRecord, b: &repository::SourceRecord) -> bool {
    a.id == b.id
        && a.root == b.root
        && a.relative == b.relative
        && a.volume == b.volume
        && a.kinds == b.kinds
        && a.label == b.label
}
fn verify_source(state: &AppState, source: &repository::SourceRecord) -> Result<(), AppError> {
    let db = state.db.lock().map_err(|_| state_error())?;
    if !same_record(&repository::get(&db, &source.id)?, source) {
        return Err(stale());
    }
    Ok(())
}
pub fn roots(
    state: &AppState,
    devices: &crate::devices::udisks::Devices,
    defaults: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, AppError> {
    let db = state.db.lock().map_err(|_| state_error())?;
    crate::library::source_service::remote_roots(state, &db, devices, defaults)
}
async fn inventory(_d: &Dispatcher) -> Result<crate::devices::udisks::Devices, RemoteError> {
    crate::devices::udisks::discover()
        .await
        .map_err(native_error)
        .or_else(|_| {
            // Local grants remain usable when removable-device discovery is unavailable.
            Ok(crate::devices::udisks::Devices::default())
        })
}
fn defaults(services: &super::ControlServices) -> Vec<PathBuf> {
    let Some(app) = services.app() else {
        return vec![];
    };
    let Ok(home) = app.path().home_dir() else {
        return vec![];
    };
    [
        app.path().audio_dir().ok(),
        app.path().video_dir().ok(),
        app.path().picture_dir().ok(),
    ]
    .into_iter()
    .filter_map(|path| {
        crate::library::defaults::resolve(&home, path)
            .ok()
            .flatten()
    })
    .collect()
}
async fn authorized_selection(
    d: &Dispatcher,
    owner: &Id,
    node: &Id,
) -> Result<Selection, RemoteError> {
    let selection = d
        .services
        .folders
        .lock()
        .map_err(|_| unavailable())?
        .selection(owner, node, now_ms())
        .map_err(native_error)?;
    let devices = inventory(d).await?;
    let current =
        roots(&d.services.state, &devices, defaults(&d.services)).map_err(native_error)?;
    if !current.iter().any(|root| root == selection.root()) {
        return Err(RemoteError::new(
            ErrorCode::NotFound,
            "The folder is no longer authorized. Refresh folders.",
        ));
    }
    Ok(selection)
}
async fn read(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    command: &RemoteCommand,
) -> Result<serde_json::Value, RemoteError> {
    match command {
        RemoteCommand::FoldersChildren(p) => {
            authorized_selection(d, &owner.device_id, &p.node_id).await?;
            let folders = d.services.folders.clone();
            let owner = owner.device_id.clone();
            let params = p.clone();
            let page = tokio::task::spawn_blocking(move || {
                folders.lock().map_err(|_| state_error())?.children(
                    &owner,
                    &params.node_id,
                    params.cursor.as_ref(),
                    params.limit.as_ref(),
                    now_ms(),
                )
            })
            .await
            .map_err(|_| unavailable())?
            .map_err(native_error)?;
            value(page)
        }
        RemoteCommand::FoldersRoots(_) => {
            let devices = inventory(d).await?;
            let roots =
                roots(&d.services.state, &devices, defaults(&d.services)).map_err(native_error)?;
            value(
                d.services
                    .folders
                    .lock()
                    .map_err(|_| unavailable())?
                    .roots(&owner.device_id, roots, now_ms())
                    .map_err(native_error)?,
            )
        }
        RemoteCommand::SourcesList(p) => {
            let devices = inventory(d).await?;
            let items = crate::library::source_service::list(&d.services.state, &devices)
                .map_err(native_error)?;
            let revision = d.services.state.event_hub.revision(Topic::SourcesSummary);
            let (start, end, next_cursor) = crate::remote::media::bounds(
                p.cursor.as_ref(),
                "sources",
                revision,
                items.len(),
                p.limit.as_ref(),
            )?;
            let scans = d.services.state.scans.lock().map_err(|_| unavailable())?;
            let items = items[start..end].iter().map(|s| checked(serde_json::json!({"id":s.id,"label":s.label.chars().take(256).collect::<String>(),"kinds":s.kinds.iter().map(|k|match k { crate::contracts::MediaKind::Audio=>"music",crate::contracts::MediaKind::Photo=>"photo",_=>"video"}).collect::<Vec<_>>(),"availability":if s.availability=="available" {"available"}else{"offline"},"scanning":scans.contains_key(&s.id)}))).collect::<Result<Vec<Source>,_>>()?;
            value(SourcePage {
                revision: revision.into(),
                items: checked(items)?,
                next_cursor,
            })
        }
        _ => Err(RemoteError::invalid()),
    }
}
fn action_label(action: &LibraryAction) -> String {
    match action {
        LibraryAction::Add { selection, .. } | LibraryAction::Reattach { selection, .. } => {
            selection
                .path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(256)
                .collect()
        }
        LibraryAction::Remove { source } => source.label.chars().take(256).collect(),
    }
}
fn preview(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    action: &LibraryAction,
) -> Result<ConfirmationPreview, RemoteError> {
    let (kind, method) = match action {
        LibraryAction::Add { .. } => (ActionKind::AddSource, ConfirmMethod::FoldersConfirmAdd),
        LibraryAction::Reattach { .. } => (
            ActionKind::ReattachSource,
            ConfirmMethod::SourcesConfirmReattachment,
        ),
        LibraryAction::Remove { .. } => (
            ActionKind::RemoveSource,
            ConfirmMethod::SourcesConfirmRemove,
        ),
    };
    let kinds = match action {
        LibraryAction::Add { kinds, .. } => kinds,
        LibraryAction::Reattach { source, .. } | LibraryAction::Remove { source } => &source.kinds,
    };
    let kinds = kinds
        .iter()
        .map(|k| match k {
            crate::contracts::MediaKind::Video => "Video",
            crate::contracts::MediaKind::Audio => "Music",
            crate::contracts::MediaKind::Photo => "Photos",
        })
        .collect::<Vec<_>>()
        .join(", ");
    checked(
        serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":kind,"affectedLabels":[action_label(action),kinds],"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"sourcesRevision":d.services.state.event_hub.revision(Topic::SourcesSummary).to_string()},"expiresInMs":60000,"confirmMethod":method}),
    )
}
async fn prepare_action(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    command: &RemoteCommand,
) -> Result<LibraryAction, RemoteError> {
    match command {
        RemoteCommand::FoldersPrepareAdd(p) => {
            let selection = authorized_selection(d, &owner.device_id, &p.node_id).await?;
            let devices = inventory(d).await?;
            folder_action(&d.services.state, selection, p.kinds.as_slice(), &devices)
                .map_err(native_error)
        }
        RemoteCommand::SourcesPrepareRemove(p) => Ok(LibraryAction::Remove {
            source: repository::get(
                &*d.services.state.db.lock().map_err(|_| unavailable())?,
                p.source_id.as_str(),
            )
            .map_err(native_error)?,
        }),
        _ => Err(RemoteError::invalid()),
    }
}
fn folder_action(
    state: &AppState,
    selection: Selection,
    kinds: &[MediaKind],
    devices: &crate::devices::udisks::Devices,
) -> Result<LibraryAction, AppError> {
    let db = state.db.lock().map_err(|_| state_error())?;
    for source in repository::records(&db)? {
        if source
            .volume
            .as_deref()
            .is_some_and(|v| v.starts_with("fingerprint:"))
            && repository::root(&source, devices).is_ok_and(|root| root == selection.path())
        {
            return Ok(LibraryAction::Reattach { selection, source });
        }
    }
    Ok(LibraryAction::Add {
        selection,
        kinds: kinds.iter().map(native_kind).collect(),
    })
}
fn reattach(
    state: &AppState,
    selection: &Selection,
    source: &repository::SourceRecord,
    devices: &crate::devices::udisks::Devices,
) -> Result<(), AppError> {
    verify_source(state, source)?;
    selection.check_expiry(now_ms())?;
    if repository::root(source, devices)? != selection.path() {
        return Err(stale());
    }
    let descriptor = selection.revalidate()?;
    state
        .source_confirmations
        .lock()
        .map_err(|_| state_error())?
        .insert(
            source.id.clone(),
            confirmation::capture(&crate::library::files::descriptor_path(&descriptor))?,
        );
    Ok(())
}
fn validate(action: &LibraryAction, state: &AppState) -> Result<(), AppError> {
    match action {
        LibraryAction::Add { selection, .. } => {
            selection.check_expiry(now_ms())?;
            selection.revalidate()?;
        }
        LibraryAction::Reattach { selection, source } => {
            selection.check_expiry(now_ms())?;
            selection.revalidate()?;
            verify_source(state, source)?;
        }
        LibraryAction::Remove { source } => verify_source(state, source)?,
    }
    Ok(())
}
async fn apply(
    services: super::ControlServices,
    action: LibraryAction,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Source operation cancelled."));
    }
    validate(&action, &services.state)?;
    let app = services.app().ok_or_else(state_error)?.clone();
    match action {
        LibraryAction::Add { selection, kinds } => {
            let source = crate::library::source_service::add_remote(
                services.state.clone(),
                selection,
                kinds,
                defaults(&services),
            )
            .await?;
            crate::library::scanner::publish_summaries(&app, &services.state)?;
            crate::library::scanner::scan(app, services.state, source, cancel).await
        }
        LibraryAction::Reattach { selection, source } => {
            let _gate = services
                .state
                .device_gate
                .try_lock()
                .map_err(|_| AppError::new("busy", "Device operation running."))?;
            let devices = crate::devices::udisks::discover().await?;
            reattach(&services.state, &selection, &source, &devices)?;
            crate::library::scanner::publish_summaries(&app, &services.state)
        }
        LibraryAction::Remove { source } => {
            let _gate = services
                .state
                .device_gate
                .try_lock()
                .map_err(|_| AppError::new("busy", "Device operation running."))?;
            let _blocked = crate::lifecycle::stop_sources(
                &app,
                &services.state,
                std::slice::from_ref(&source.id),
            )
            .await?;
            verify_source(&services.state, &source)?;
            repository::remove(
                &*services.state.db.lock().map_err(|_| state_error())?,
                &source.id,
            )?;
            services
                .state
                .source_confirmations
                .lock()
                .map_err(|_| state_error())?
                .remove(&source.id);
            crate::library::scanner::publish_summaries(&app, &services.state)
        }
    }
}
pub async fn dispatch(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    if matches!(
        request.command,
        RemoteCommand::SourcesList(_)
            | RemoteCommand::FoldersRoots(_)
            | RemoteCommand::FoldersChildren(_)
    ) {
        let _permit = d
            .work
            .acquire(owner.device_id.as_str(), 8)
            .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Too much queued folder work."))?;
        return read(d, owner, &request.command).await;
    }
    if request
        .expected
        .as_ref()
        .and_then(|e| e.server_epoch.as_ref())
        != Some(&d.services.state.event_hub.epoch())
    {
        return Err(RemoteError::new(
            ErrorCode::StaleState,
            "Refresh library state.",
        ));
    }
    if let RemoteCommand::SourcesCancelScan(p) = &request.command {
        let scans = d.services.source_scans.lock().map_err(|_| unavailable())?;
        if d.services
            .operations()
            .lookup_request(owner, &request.id)
            .map_err(native_error)?
            .is_none()
            && !scans
                .get(p.operation_id.as_str())
                .is_some_and(|(device, _)| device == &owner.device_id)
        {
            return Err(RemoteError::new(
                ErrorCode::NotFound,
                "Scan operation unavailable.",
            ));
        }
        return d.cancel(
            owner,
            &request,
            &OperationsCancelParams {
                request_id: None,
                operation_id: Some(p.operation_id.clone()),
            },
        );
    }
    let permit = d.work.acquire(owner.device_id.as_str(), 8);
    if permit.is_none()
        && d.services
            .operations()
            .lookup_request(owner, &request.id)
            .map_err(native_error)?
            .is_none()
    {
        return Err(RemoteError::new(
            ErrorCode::Busy,
            "Too many source operations.",
        ));
    }
    let _admission = d.services.source_gate.lock().await;
    if let Some(replay) = d.activity_replay(owner, &request)? {
        return Ok(replay);
    }
    if matches!(
        request.command,
        RemoteCommand::FoldersPrepareAdd(_)
            | RemoteCommand::SourcesPrepareRemove(_)
            | RemoteCommand::SourcesScan(_)
    ) && request
        .expected
        .as_ref()
        .and_then(|e| e.sources_revision.as_ref())
        .map(|r| r.value())
        != Some(d.services.state.event_hub.revision(Topic::SourcesSummary))
    {
        return Err(RemoteError::new(
            ErrorCode::StaleState,
            "Refresh the current sources.",
        ));
    }
    let permit =
        permit.ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Too many source operations."))?;
    if matches!(
        request.command,
        RemoteCommand::FoldersPrepareAdd(_) | RemoteCommand::SourcesPrepareRemove(_)
    ) {
        let action = prepare_action(d, owner, &request.command).await?;
        let preview = preview(d, owner, &action)?;
        let pending = super::confirmations::Pending {
            preview: preview.clone(),
            action: super::confirmations::Action::Library(action),
            observed: Default::default(),
            request: request.id.clone(),
            expires_at: now_ms() + 60000,
            operation: None,
        };
        let mut confirmations = d.services.confirmations.lock().map_err(|_| unavailable())?;
        confirmations.insert(pending, now_ms())?;
        if let Err(error) =
            d.services
                .operations()
                .start(d.context(owner, &request), request, |_| async { Ok(()) })
        {
            confirmations
                .pending
                .remove(preview.confirmation_id.as_str());
            return Err(native_error(error));
        }
        return value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        });
    }
    if let RemoteCommand::SourcesScan(p) = &request.command {
        let source = p.source_id.as_str().to_string();
        let record = repository::get(
            &*d.services.state.db.lock().map_err(|_| unavailable())?,
            &source,
        )
        .map_err(native_error)?;
        let services = d.services.clone();
        let registration = d.registration.clone();
        let outcome = d
            .services
            .operations()
            .start(
                d.context(owner, &request),
                request,
                move |cancel| async move {
                    let _permit = permit;
                    let _registration = registration;
                    verify_source(&services.state, &record)?;
                    crate::library::scanner::scan(
                        services.app().ok_or_else(state_error)?.clone(),
                        services.state,
                        source,
                        cancel,
                    )
                    .await
                    .map_err(crate::remote::media::normalize)
                },
            )
            .map_err(native_error)?;
        remember_scan(d, &owner.device_id, &outcome.operation_id)?;
        return value(AcceptedResult {
            status: OperationAccepted::Accepted,
            operation_id: outcome.operation_id,
        });
    }
    confirm(d, owner, request, permit).await
}
fn remember_scan(d: &Dispatcher, owner: &Id, id: &Id) -> Result<(), RemoteError> {
    let mut scans = d.services.source_scans.lock().map_err(|_| unavailable())?;
    let active = d
        .services
        .state
        .operations
        .lock()
        .map_err(|_| unavailable())?;
    scans.retain(|id, _| active.contains_key(id));
    scans.insert(id.as_str().into(), (owner.clone(), now_ms()));
    Ok(())
}
fn refresh_action(
    action: LibraryAction,
    state: &AppState,
    devices: &crate::devices::udisks::Devices,
    defaults: Vec<PathBuf>,
) -> Result<(LibraryAction, bool), RemoteError> {
    let source = match &action {
        LibraryAction::Remove { source } | LibraryAction::Reattach { source, .. } => source,
        LibraryAction::Add { selection, .. } => {
            crate::library::source_service::authorize_remote_root(
                state,
                &*state.db.lock().map_err(|_| unavailable())?,
                selection,
                devices,
                defaults,
            )
            .map_err(native_error)?;
            return Ok((action, false));
        }
    };
    let current = repository::get(&*state.db.lock().map_err(|_| unavailable())?, &source.id)
        .map_err(|_| {
            RemoteError::new(
                ErrorCode::NotFound,
                "The source association no longer exists.",
            )
        })?;
    if same_record(source, &current) {
        return Ok((action, false));
    }
    match action {
        LibraryAction::Remove { .. } => Ok((LibraryAction::Remove { source: current }, true)),
        LibraryAction::Reattach { selection, source }
            if source.root == current.root
                && source.volume == current.volume
                && source.relative == current.relative =>
        {
            Ok((
                LibraryAction::Reattach {
                    selection,
                    source: current,
                },
                true,
            ))
        }
        _ => Err(RemoteError::new(
            ErrorCode::NotFound,
            "The folder identity changed. Browse and select it again.",
        )),
    }
}
async fn confirm(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<serde_json::Value, RemoteError> {
    let (id, method) = match &request.command {
        RemoteCommand::FoldersConfirmAdd(p) => {
            (&p.confirmation_id, ConfirmMethod::FoldersConfirmAdd)
        }
        RemoteCommand::SourcesConfirmRemove(p) => {
            (&p.confirmation_id, ConfirmMethod::SourcesConfirmRemove)
        }
        RemoteCommand::SourcesConfirmReattachment(p) => (
            &p.confirmation_id,
            ConfirmMethod::SourcesConfirmReattachment,
        ),
        _ => return Err(RemoteError::invalid()),
    };
    let devices = inventory(d).await?;
    let mut confirmations = d.services.confirmations.lock().map_err(|_| unavailable())?;
    let pending = confirmations.get(&owner.device_id, id, now_ms())?;
    if pending.preview.confirm_method != method
        || pending.preview.expected.server_epoch.as_ref()
            != Some(&d.services.state.event_hub.epoch())
    {
        return Err(RemoteError::invalid());
    }
    if let Some(operation_id) = pending.operation {
        return value(AcceptedResult {
            status: OperationAccepted::Accepted,
            operation_id,
        });
    }
    let super::confirmations::Action::Library(action) = pending.action else {
        return Err(RemoteError::invalid());
    };
    let (action, resource_changed) =
        refresh_action(action, &d.services.state, &devices, defaults(&d.services)).inspect_err(
            |_| {
                confirmations.pending.remove(id.as_str());
            },
        )?;
    validate(&action, &d.services.state).map_err(|_| {
        RemoteError::new(
            ErrorCode::NotFound,
            "The selected folder or source changed. Refresh sources and browse again.",
        )
    })?;
    if resource_changed
        || pending.preview.expected.sources_revision
            != Some(
                d.services
                    .state
                    .event_hub
                    .revision(Topic::SourcesSummary)
                    .into(),
            )
    {
        let fresh = preview(d, owner, &action)?;
        confirmations.insert(
            super::confirmations::Pending {
                preview: fresh.clone(),
                action: super::confirmations::Action::Library(action),
                observed: Default::default(),
                request: pending.request,
                expires_at: now_ms() + 60000,
                operation: None,
            },
            now_ms(),
        )?;
        let mut error = RemoteError::new(
            ErrorCode::ConfirmationStale,
            "Library state changed. Review the current folder or source again.",
        );
        error.replacement_preview = Some(Box::new(fresh));
        return Err(error);
    }
    let scans = matches!(action, LibraryAction::Add { .. });
    let services = d.services.clone();
    let registration = d.registration.clone();
    let confirmation = id.clone();
    let outcome = d
        .services
        .operations()
        .start(
            d.context(owner, &request),
            request,
            move |cancel| async move {
                let _permit = permit;
                let _registration = registration;
                apply(services, action, cancel)
                    .await
                    .map_err(crate::remote::media::normalize)
            },
        )
        .map_err(native_error)?;
    if let Some(p) = confirmations.pending.get_mut(confirmation.as_str()) {
        p.operation = Some(outcome.operation_id.clone());
    }
    if scans {
        remember_scan(d, &owner.device_id, &outcome.operation_id)?;
    }
    value(AcceptedResult {
        status: OperationAccepted::Accepted,
        operation_id: outcome.operation_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let services =
            super::super::ControlServices::native(state.clone(), checked("server").unwrap())
                .unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","library_manage"],"createdAtMs":1,"revoked":false})).unwrap();
        let credential: BearerCredential = checked("a".repeat(43)).unwrap();
        crate::remote::store::RemoteStore::new(
            &mut state.db.lock().unwrap(),
            &services.server_identity,
        )
        .add_device(
            &device,
            &crate::remote::store::CredentialHash::from_credential(&credential),
        )
        .unwrap();
        let owner = RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        };
        (dir, Dispatcher::new(services), owner)
    }
    fn request(d: &Dispatcher, id: &str, method: &str, params: serde_json::Value) -> RemoteRequest {
        parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"sourcesRevision":d.services.state.event_hub.revision(Topic::SourcesSummary).to_string()}}).to_string()).unwrap()
    }
    #[tokio::test]
    async fn phone_roots_are_authorized_only_and_paths_cannot_be_supplied() {
        let (dir, d, owner) = fixture();
        let music = dir.path().join("Music");
        std::fs::create_dir(&music).unwrap();
        repository::add(
            &d.services.state.db.lock().unwrap(),
            &music,
            &[crate::contracts::MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let page = d
            .dispatch(
                &owner,
                request(&d, "roots", "folders.roots", serde_json::json!({})),
            )
            .await
            .unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert!(!page
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string()));
        for node in ["/etc", "../Music"] {
            assert!(parse_request(&serde_json::json!({"v":1,"id":"bad","method":"folders.children","params":{"nodeId":node}}).to_string()).is_err());
        }
        let forbidden: RemotePrincipal =
            checked(serde_json::json!({"deviceId":"other","scopes":["browse"]})).unwrap();
        assert_eq!(
            d.dispatch(
                &forbidden,
                request(&d, "forbidden", "folders.roots", serde_json::json!({}))
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Forbidden
        );
    }
    #[tokio::test]
    async fn remove_preview_is_bound_to_source_owner_and_never_removes_on_prepare() {
        let (dir, d, owner) = fixture();
        let music = dir.path().join("Music");
        std::fs::create_dir(&music).unwrap();
        std::fs::write(music.join("song"), b"keep").unwrap();
        let source = repository::add(
            &d.services.state.db.lock().unwrap(),
            &music,
            &[crate::contracts::MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let req = request(
            &d,
            "prepare",
            "sources.prepareRemove",
            serde_json::json!({"sourceId":source}),
        );
        let preview = d.dispatch(&owner, req.clone()).await.unwrap();
        assert_eq!(preview["preview"]["action"], "remove_source");
        assert!(repository::get(&d.services.state.db.lock().unwrap(), &source).is_ok());
        assert!(music.join("song").is_file());
        let replay = d.dispatch(&owner, req).await.unwrap();
        assert_eq!(
            preview["preview"]["confirmationId"],
            replay["preview"]["confirmationId"]
        );
        let other: RemotePrincipal =
            checked(serde_json::json!({"deviceId":"other","scopes":["browse","library_manage"]}))
                .unwrap();
        let confirm = request(
            &d,
            "confirm",
            "sources.confirmRemove",
            serde_json::json!({"confirmationId":preview["preview"]["confirmationId"]}),
        );
        assert_eq!(
            d.dispatch(&other, confirm).await.unwrap_err().code,
            ErrorCode::NotFound
        );
    }
    #[test]
    fn default_and_removable_candidates_do_not_expose_parent_home() {
        let (dir, d, _) = fixture();
        let pictures = dir.path().join("Pictures");
        std::fs::create_dir(&pictures).unwrap();
        let allowed = roots(
            &d.services.state,
            &Default::default(),
            vec![pictures.clone()],
        )
        .unwrap();
        assert_eq!(allowed, vec![pictures]);
        assert!(!allowed.contains(&dir.path().into()));
    }
    #[test]
    fn source_identity_checks_detect_changed_association() {
        let (dir, d, _) = fixture();
        let music = dir.path().join("Music");
        std::fs::create_dir(&music).unwrap();
        let id = repository::add(
            &d.services.state.db.lock().unwrap(),
            &music,
            &[crate::contracts::MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let source = repository::get(&d.services.state.db.lock().unwrap(), &id).unwrap();
        repository::add(
            &d.services.state.db.lock().unwrap(),
            &music,
            &[crate::contracts::MediaKind::Photo],
            &Default::default(),
        )
        .unwrap();
        assert_eq!(
            verify_source(&d.services.state, &source).unwrap_err().code,
            "stale_state"
        );
    }
    #[tokio::test]
    async fn stale_source_revision_requires_fresh_explicit_confirmation() {
        let (dir, d, owner) = fixture();
        let root = dir.path().join("Music");
        std::fs::create_dir(&root).unwrap();
        let id = repository::add(
            &d.services.state.db.lock().unwrap(),
            &root,
            &[crate::contracts::MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let prepared = d
            .dispatch(
                &owner,
                request(
                    &d,
                    "prepare-stale",
                    "sources.prepareRemove",
                    serde_json::json!({"sourceId":id}),
                ),
            )
            .await
            .unwrap();
        let original = prepared["preview"]["confirmationId"].clone();
        crate::remote::state::sources_changed(&d.services.state);
        let error = d
            .dispatch(
                &owner,
                request(
                    &d,
                    "confirm-stale",
                    "sources.confirmRemove",
                    serde_json::json!({"confirmationId":original}),
                ),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ConfirmationStale);
        let replacement = error.replacement_preview.unwrap();
        assert_eq!(replacement.owner_device_id, owner.device_id);
        assert_ne!(
            serde_json::to_value(&replacement.confirmation_id).unwrap(),
            original
        );
        assert!(repository::get(&d.services.state.db.lock().unwrap(), &id).is_ok());
        repository::add(
            &d.services.state.db.lock().unwrap(),
            &root,
            &[crate::contracts::MediaKind::Photo],
            &Default::default(),
        )
        .unwrap();
        let changed = d
            .dispatch(
                &owner,
                request(
                    &d,
                    "changed-resource",
                    "sources.confirmRemove",
                    serde_json::json!({"confirmationId":replacement.confirmation_id}),
                ),
            )
            .await
            .unwrap_err();
        assert_eq!(changed.code, ErrorCode::ConfirmationStale);
        assert!(
            serde_json::to_string(&changed.replacement_preview.unwrap().affected_labels)
                .unwrap()
                .contains("Photos")
        );
        assert!(repository::get(&d.services.state.db.lock().unwrap(), &id).is_ok());
    }
    #[tokio::test]
    async fn revoked_root_invalidates_add_and_stale_preview_retry() {
        for (refresh_before_removal, publish_removal) in
            [(false, false), (false, true), (true, true)]
        {
            let (dir, d, owner) = fixture();
            let root = dir.path().join("LocalGrant");
            let child = root.join("Albums");
            std::fs::create_dir_all(&child).unwrap();
            let source = repository::add(
                &d.services.state.db.lock().unwrap(),
                &root,
                &[crate::contracts::MediaKind::Audio],
                &Default::default(),
            )
            .unwrap();
            let node = {
                let mut folders = d.services.folders.lock().unwrap();
                let root_node = folders
                    .roots(&owner.device_id, vec![root.clone()], now_ms())
                    .unwrap()
                    .items
                    .as_slice()[0]
                    .node_id
                    .clone();
                folders
                    .children(&owner.device_id, &root_node, None, None, now_ms())
                    .unwrap()
                    .items
                    .as_slice()[0]
                    .node_id
                    .clone()
            };
            let prepared = d
                .dispatch(
                    &owner,
                    request(
                        &d,
                        "prepare-add",
                        "folders.prepareAdd",
                        serde_json::json!({"nodeId":node,"kinds":["music"]}),
                    ),
                )
                .await
                .unwrap();
            let mut preview_id = prepared["preview"]["confirmationId"].clone();
            if refresh_before_removal {
                crate::remote::state::sources_changed(&d.services.state);
                let error = d
                    .dispatch(
                        &owner,
                        request(
                            &d,
                            "refresh-add",
                            "folders.confirmAdd",
                            serde_json::json!({"confirmationId":preview_id}),
                        ),
                    )
                    .await
                    .unwrap_err();
                assert_eq!(error.code, ErrorCode::ConfirmationStale);
                preview_id =
                    serde_json::to_value(error.replacement_preview.unwrap().confirmation_id)
                        .unwrap();
            }
            repository::remove(&d.services.state.db.lock().unwrap(), &source).unwrap();
            if publish_removal {
                crate::remote::state::sources_changed(&d.services.state);
            }
            assert!(!roots(&d.services.state, &Default::default(), vec![])
                .unwrap()
                .contains(&root));
            for attempt in ["confirm-revoked", "retry-revoked"] {
                let error = d
                    .dispatch(
                        &owner,
                        request(
                            &d,
                            attempt,
                            "folders.confirmAdd",
                            serde_json::json!({"confirmationId":preview_id}),
                        ),
                    )
                    .await
                    .unwrap_err();
                assert_eq!(error.code, ErrorCode::NotFound);
                assert!(error.replacement_preview.is_none());
            }
            assert!(d
                .services
                .confirmations
                .lock()
                .unwrap()
                .previews(now_ms())
                .is_empty());
            assert!(repository::records(&d.services.state.db.lock().unwrap())
                .unwrap()
                .is_empty());
            assert!(child.is_dir());
        }
    }
    #[test]
    fn default_root_add_can_refresh_after_association_removal() {
        let (dir, d, owner) = fixture();
        let root = dir.path().join("Music");
        std::fs::create_dir_all(root.join("Albums")).unwrap();
        let source = repository::add(
            &d.services.state.db.lock().unwrap(),
            &root,
            &[crate::contracts::MediaKind::Audio],
            &Default::default(),
        )
        .unwrap();
        let selection = {
            let mut folders = d.services.folders.lock().unwrap();
            let node = folders
                .roots(&owner.device_id, vec![root.clone()], now_ms())
                .unwrap()
                .items
                .as_slice()[0]
                .node_id
                .clone();
            let child = folders
                .children(&owner.device_id, &node, None, None, now_ms())
                .unwrap()
                .items
                .as_slice()[0]
                .node_id
                .clone();
            folders
                .selection(&owner.device_id, &child, now_ms())
                .unwrap()
        };
        repository::remove(&d.services.state.db.lock().unwrap(), &source).unwrap();
        let action = LibraryAction::Add {
            selection,
            kinds: vec![crate::contracts::MediaKind::Audio],
        };
        let (refreshed, changed) =
            refresh_action(action, &d.services.state, &Default::default(), vec![root]).unwrap();
        assert!(!changed);
        validate(&refreshed, &d.services.state).unwrap();
        assert_eq!(
            preview(&d, &owner, &refreshed).unwrap().action,
            ActionKind::AddSource
        );
    }
    #[tokio::test]
    async fn source_cancel_is_owner_scoped_and_waits_for_actual_completion() {
        let (_dir, d, owner) = fixture();
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let completed = release.clone();
        let scan = request(
            &d,
            "controlled-scan",
            "sources.scan",
            serde_json::json!({"sourceId":"source"}),
        );
        let outcome = d
            .services
            .operations()
            .start(d.context(&owner, &scan), scan, move |cancel| async move {
                cancel.cancelled().await;
                completed.notified().await;
                Err(AppError::new("cancelled", "Native scan stopped."))
            })
            .unwrap();
        remember_scan(&d, &owner.device_id, &outcome.operation_id).unwrap();
        let other: RemotePrincipal =
            checked(serde_json::json!({"deviceId":"other","scopes":["browse","library_manage"]}))
                .unwrap();
        let cancel = request(
            &d,
            "cancel-scan",
            "sources.cancelScan",
            serde_json::json!({"operationId":outcome.operation_id}),
        );
        assert_eq!(
            d.dispatch(&other, cancel.clone()).await.unwrap_err().code,
            ErrorCode::NotFound
        );
        let ack = d.dispatch(&owner, cancel.clone()).await.unwrap();
        assert_eq!(ack["cancellationRequested"], true);
        let pending = d
            .services
            .operations()
            .lookup(&owner, &outcome.operation_id)
            .unwrap()
            .unwrap();
        assert!(pending.cancellation_requested);
        assert!(matches!(
            pending.status,
            OperationStatus::Accepted | OperationStatus::Running
        ));
        release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if d.services
                    .operations()
                    .lookup(&owner, &outcome.operation_id)
                    .unwrap()
                    .unwrap()
                    .status
                    == OperationStatus::Cancelled
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(d.dispatch(&owner, cancel).await.unwrap(), ack);
    }
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    #[test]
    fn explicit_uuidless_confirmation_preserves_association_and_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("USB");
        std::fs::create_dir(&root).unwrap();
        let state = AppState::open(dir.path().join("data")).unwrap();
        let devices = crate::devices::udisks::Devices {
            drives: vec![],
            volumes: vec![crate::devices::registry::Volume {
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
        let id = repository::add(
            &state.db.lock().unwrap(),
            &root,
            &[crate::contracts::MediaKind::Audio],
            &devices,
        )
        .unwrap();
        let record = repository::get(&state.db.lock().unwrap(), &id).unwrap();
        assert!(confirmation::check(&state, &record, &root).is_err());
        let mut folders = crate::remote::folders::Folders::default();
        let owner: Id = checked("phone").unwrap();
        let node = folders
            .roots(&owner, vec![root.clone()], now_ms())
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let selection = folders.selection(&owner, &node, now_ms()).unwrap();
        let LibraryAction::Reattach { selection, source } =
            folder_action(&state, selection, &[MediaKind::Video], &devices).unwrap()
        else {
            panic!("must require reattachment")
        };
        assert!(state.source_confirmations.lock().unwrap().is_empty());
        reattach(&state, &selection, &source, &devices).unwrap();
        assert!(confirmation::check(&state, &record, &root).is_ok());
        let records = repository::records(&state.db.lock().unwrap()).unwrap();
        assert_eq!(records.len(), 1);
        assert!(same_record(&records[0], &record));
        std::fs::rename(&root, dir.path().join("oldUSB")).unwrap();
        std::fs::create_dir(&root).unwrap();
        assert!(reattach(&state, &selection, &source, &devices).is_err());
        assert!(confirmation::check(&state, &record, &root).is_err());
    }
}
