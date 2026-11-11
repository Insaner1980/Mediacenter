//! Explicit native method admission. Never forward names to Tauri.
use super::{
    adapters::checked,
    limits::DeviceLimits,
    protocol::*,
    store::{CommandAcceptance, JournalOutcome, RemoteStore},
};
use crate::{app_state::now_ms, contracts::AppError, control::ControlServices};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Dispatcher {
    pub(crate) services: ControlServices,
    pub(crate) work: Arc<DeviceLimits>,
    cancellation_gate: Arc<Mutex<()>>,
    pub(crate) registration: Option<Arc<()>>,
    #[cfg(test)]
    pub(super) pause_effect: Option<Arc<dyn Fn() -> Result<(), AppError> + Send + Sync>>,
}
pub(crate) fn native_error(error: AppError) -> RemoteError {
    RemoteError::new(
        checked(super::media::normalize(error).code).unwrap_or(ErrorCode::Internal),
        "The native operation could not be completed.",
    )
}
pub(crate) fn unavailable() -> RemoteError {
    RemoteError::new(
        ErrorCode::Unavailable,
        "The requested resource is unavailable.",
    )
}
pub fn capabilities() -> Capabilities {
    checked(serde_json::json!({"methods":["settings.get","settings.theme.set","vpn.state","vpn.countries","vpn.prepareChange","vpn.confirmChange","vpn.requirement.set","devices.list","devices.revoke","pairing.begin","state.get","capabilities.get","operations.get","operations.cancel","player.pause","library.query","library.item","player.play","player.seek","player.volume","player.mute","player.audioTrack","player.subtitleTrack","player.stop","queue.get","queue.enqueue","queue.remove","queue.move","queue.playEntry","queue.next","queue.previous","queue.shuffle","queue.repeat","playlists.list","playlists.get","playlists.save","playlists.delete","playlists.play","photos.show","photos.slideshow.start","photos.next","photos.previous","photos.pause","photos.interval","photos.shuffle","photos.fit","photos.stop","activity.prepare","activity.confirm","confirmations.get","confirmations.cancel","audio.outputs","audio.prepare","audio.confirm","audio.test","activity.showMenu","activity.showPlayback","ui.navigate","ui.moveFocus","ui.activate","ui.back","ui.search","drives.list","drives.mount","drives.prepareEject","drives.confirmEject","sources.list","folders.roots","folders.children","folders.prepareAdd","folders.confirmAdd","sources.scan","sources.cancelScan","sources.prepareRemove","sources.confirmRemove","sources.confirmReattachment"],"services":[],"browserBridge":false})).expect("static capabilities")
}
pub(crate) fn value(value: impl serde::Serialize) -> Result<serde_json::Value, RemoteError> {
    serde_json::to_value(value).map_err(|_| RemoteError::invalid())
}
impl Dispatcher {
    pub fn new(services: ControlServices) -> Self {
        Self {
            services,
            work: Arc::default(),
            cancellation_gate: Arc::default(),
            registration: None,
            #[cfg(test)]
            pause_effect: None,
        }
    }
    pub fn authorize(
        principal: &RemotePrincipal,
        request: &RemoteRequest,
    ) -> Result<(), RemoteError> {
        if !principal
            .scopes
            .as_slice()
            .contains(&request.command.scope())
        {
            return Err(RemoteError::new(
                ErrorCode::Forbidden,
                "Permission is required for this action.",
            ));
        }
        Ok(())
    }
    /// state.get is handled by the transport using its original admitted stream.
    pub async fn dispatch(
        &self,
        principal: &RemotePrincipal,
        request: RemoteRequest,
    ) -> Result<serde_json::Value, RemoteError> {
        Self::authorize(principal, &request)?;
        let confirmations_changed = matches!(
            request.command,
            RemoteCommand::VpnPrepareChange(_)
                | RemoteCommand::VpnConfirmChange(_)
                | RemoteCommand::DevicesRevoke(_)
                | RemoteCommand::PairingBegin(_)
                | RemoteCommand::ActivityPrepare(_)
                | RemoteCommand::ActivityConfirm(_)
                | RemoteCommand::AudioPrepare(_)
                | RemoteCommand::AudioConfirm(_)
                | RemoteCommand::FoldersPrepareAdd(_)
                | RemoteCommand::FoldersConfirmAdd(_)
                | RemoteCommand::DrivesPrepareEject(_)
                | RemoteCommand::SourcesPrepareRemove(_)
                | RemoteCommand::DrivesConfirmEject(_)
                | RemoteCommand::SourcesConfirmRemove(_)
                | RemoteCommand::SourcesConfirmReattachment(_)
                | RemoteCommand::ConfirmationsCancel(_)
        );
        let result = self.dispatch_inner(principal, request).await;
        if confirmations_changed {
            self.present_confirmations();
        }
        result
    }
    async fn dispatch_inner(
        &self,
        principal: &RemotePrincipal,
        request: RemoteRequest,
    ) -> Result<serde_json::Value, RemoteError> {
        if super::vpn::handles(&request.command) {
            return super::vpn::dispatch(self, principal, request).await;
        }
        if crate::control::settings_devices::handles(&request.command) {
            return crate::control::settings_devices::dispatch(self, principal, request).await;
        }
        if crate::control::settings::handles(&request.command) {
            return crate::control::settings::dispatch(self, principal, request).await;
        }
        if crate::control::drives::handles(&request.command) {
            return crate::control::drives::dispatch(self, principal, request).await;
        }
        if crate::control::library::handles(&request.command) {
            return crate::control::library::dispatch(self, principal, request).await;
        }
        match &request.command {
            RemoteCommand::UiNavigate(_)
            | RemoteCommand::UiMoveFocus(_)
            | RemoteCommand::UiActivate(_)
            | RemoteCommand::UiBack(_)
            | RemoteCommand::UiSearch(_) => {
                self.renderer_input(principal, request, principal.device_id.as_str())
                    .await
            }
            RemoteCommand::CapabilitiesGet(_) => value(capabilities()),
            RemoteCommand::AudioOutputs(_)
            | RemoteCommand::AudioPrepare(_)
            | RemoteCommand::AudioConfirm(_)
            | RemoteCommand::AudioTest(_) => self.audio(principal, request).await,
            RemoteCommand::ActivityShowMenu(_) | RemoteCommand::ActivityShowPlayback(_) => {
                let expected = request.expected.as_ref().ok_or_else(RemoteError::invalid)?;
                if expected.server_epoch.as_ref() != Some(&self.services.state.event_hub.epoch()) {
                    return Err(RemoteError::new(
                        ErrorCode::StaleState,
                        "Refresh the current activity.",
                    ));
                }
                let _permit = self
                    .work
                    .acquire(principal.device_id.as_str(), 8)
                    .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Too much queued input."))?;
                match &request.command {
                    RemoteCommand::ActivityShowMenu(_) => crate::control::activity::show_menu(
                        self.services.app().ok_or_else(unavailable)?,
                    )
                    .map_err(native_error)?,
                    _ => crate::control::activity::show_playback(
                        self.services.app(),
                        &self.services.state,
                        expected
                            .session_id
                            .as_ref()
                            .ok_or_else(RemoteError::invalid)?
                            .as_str(),
                    )
                    .await
                    .map_err(native_error)?,
                }
                value(ForegroundAck {
                    request_id: request.id,
                })
            }

            RemoteCommand::ActivityPrepare(_) | RemoteCommand::ActivityConfirm(_) => {
                self.activity(principal, request).await
            }
            RemoteCommand::ConfirmationsGet(params) => {
                let pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?
                    .get(&principal.device_id, &params.confirmation_id, now_ms())?;
                if pending.operation.is_some() {
                    return Err(RemoteError::new(
                        ErrorCode::NotFound,
                        "The confirmation has been resolved.",
                    ));
                }
                value(pending.preview)
            }
            RemoteCommand::ConfirmationsCancel(params) => {
                let mut confirmations = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                if self
                    .services
                    .operations()
                    .lookup_request(principal, &request.id)
                    .map_err(native_error)?
                    .is_none()
                {
                    confirmations.get(&principal.device_id, &params.confirmation_id, now_ms())?;
                }
                self.services
                    .operations()
                    .start(
                        self.context(principal, &request),
                        request.clone(),
                        |_| async { Ok(()) },
                    )
                    .map_err(native_error)?;
                if confirmations
                    .pending
                    .get(params.confirmation_id.as_str())
                    .is_some_and(|p| {
                        p.preview.owner_device_id == principal.device_id && p.operation.is_none()
                    })
                {
                    confirmations
                        .pending
                        .remove(params.confirmation_id.as_str());
                }
                value(CancellationAck {
                    cancellation_requested: true,
                })
            }
            RemoteCommand::LibraryQuery(_)
            | RemoteCommand::LibraryItem(_)
            | RemoteCommand::QueueGet(_)
            | RemoteCommand::PlaylistsList(_)
            | RemoteCommand::PlaylistsGet(_) => {
                super::media::read(&self.services, principal, &request.command).await
            }
            RemoteCommand::OperationsGet(params) => value(
                self.lookup(
                    principal,
                    params.request_id.as_ref(),
                    params.operation_id.as_ref(),
                )?
                .ok_or_else(|| {
                    RemoteError::new(ErrorCode::NotFound, "The operation was not found.")
                })?,
            ),
            RemoteCommand::OperationsCancel(params) => self.cancel(principal, &request, params),
            RemoteCommand::PlayerPause(params) => {
                let permit = self.work.acquire(principal.device_id.as_str(), 8);
                // Existing requests must remain queryable even while admission is full.
                if permit.is_none()
                    && self
                        .services
                        .operations()
                        .lookup_request(principal, &request.id)
                        .map_err(native_error)?
                        .is_none()
                {
                    return Err(RemoteError::new(
                        ErrorCode::Busy,
                        "Too much work is queued for this device.",
                    ));
                }
                let ctx = CommandContext {
                    principal: principal.clone(),
                    request_id: request.id.clone(),
                    server_epoch: self.services.state.event_hub.epoch(),
                };
                let params = params.clone();
                let state = self.services.state.clone();
                let registration = self.registration.clone();
                #[cfg(test)]
                let effect = self.pause_effect.clone();
                let outcome = self
                    .services
                    .operations()
                    .start(ctx, request, move |cancel| async move {
                        let _permit = permit;
                        let _registration = registration;
                        if cancel.is_cancelled() {
                            return Err(AppError::new("cancelled", "Operation cancelled."));
                        }
                        #[cfg(test)]
                        if let Some(effect) = effect {
                            return effect();
                        }
                        crate::player::mpv::pause(&state, params.session_id.as_str(), params.paused)
                            .await
                            .map_err(super::media::normalize)
                    })
                    .map_err(native_error)?;
                // Acceptance remains the stable reply; terminal truth is operations.get.
                value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id: outcome.operation_id,
                })
            }
            command if super::media::mutation(command) => {
                let permit = self.work.acquire(principal.device_id.as_str(), 8);
                let existing = self
                    .services
                    .operations()
                    .lookup_request(principal, &request.id)
                    .map_err(native_error)?;
                if existing.is_none() {
                    if permit.is_none() {
                        return Err(RemoteError::new(ErrorCode::Busy, "Too much queued work."));
                    }
                    super::media::validate(&self.services, principal, &request)
                        .await
                        .map_err(native_error)?;
                }
                let ctx = CommandContext {
                    principal: principal.clone(),
                    request_id: request.id.clone(),
                    server_epoch: self.services.state.event_hub.epoch(),
                };
                let services = self.services.clone();
                let owner = principal.clone();
                let work = request.clone();
                let registration = self.registration.clone();
                let outcome = self
                    .services
                    .operations()
                    .start(ctx, request, move |cancel| async move {
                        let _permit = permit;
                        let _registration = registration;
                        super::media::execute(services, owner, work, cancel)
                            .await
                            .map_err(super::media::normalize)
                    })
                    .map_err(native_error)?;
                value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id: outcome.operation_id,
                })
            }
            _ => Err(RemoteError::new(
                ErrorCode::Unsupported,
                "This method is not available.",
            )),
        }
    }
    fn lookup(
        &self,
        principal: &RemotePrincipal,
        request: Option<&Id>,
        operation: Option<&Id>,
    ) -> Result<Option<OperationState>, RemoteError> {
        match (request, operation) {
            (Some(id), None) => self
                .services
                .operations()
                .lookup_request(principal, id)
                .map_err(native_error),
            (None, Some(id)) => self
                .services
                .operations()
                .lookup(principal, id)
                .map_err(native_error),
            _ => Err(RemoteError::invalid()),
        }
    }
    pub(crate) fn cancel(
        &self,
        principal: &RemotePrincipal,
        request: &RemoteRequest,
        params: &OperationsCancelParams,
    ) -> Result<serde_json::Value, RemoteError> {
        // This short synchronous workflow never awaits. Serialize acceptance and
        // cancellation so a concurrent duplicate cannot report a premature ack.
        if request
            .expected
            .as_ref()
            .and_then(|e| e.server_epoch.as_ref())
            != Some(&self.services.state.event_hub.epoch())
        {
            return Err(RemoteError::new(
                ErrorCode::StaleState,
                "Refresh the native state before cancellation.",
            ));
        }
        let _gate = self.cancellation_gate.lock().map_err(|_| unavailable())?;
        let registry = self.services.operations();
        let existing = registry
            .lookup_request(principal, &request.id)
            .map_err(native_error)?;
        let target = if existing.is_none() {
            Some(
                self.lookup(
                    principal,
                    params.request_id.as_ref(),
                    params.operation_id.as_ref(),
                )?
                .ok_or_else(|| {
                    RemoteError::new(ErrorCode::NotFound, "The operation was not found.")
                })?,
            )
        } else {
            None
        };
        let id: Id = checked(uuid::Uuid::new_v4().to_string())?;
        let acceptance = {
            let mut db = self.services.state.db.lock().map_err(|_| unavailable())?;
            RemoteStore::new(&mut db, &self.services.server_identity)
                .accept_command(
                    &principal.device_id,
                    request,
                    &self.services.state.event_hub.epoch(),
                    &id,
                    now_ms() as i64,
                )
                .map_err(native_error)?
        };
        match acceptance {
            CommandAcceptance::Existing(entry) => {
                if entry.outcome.status != OperationStatus::Succeeded {
                    return Err(RemoteError::new(
                        entry
                            .outcome
                            .error_code
                            .unwrap_or(ErrorCode::OutcomeUnknown),
                        "Inspect the original operation outcome.",
                    ));
                }
            }
            CommandAcceptance::New(entry) => {
                let result = registry
                    .cancel(
                        principal,
                        &target
                            .ok_or_else(|| {
                                RemoteError::new(
                                    ErrorCode::NotFound,
                                    "The operation was not found.",
                                )
                            })?
                            .operation_id,
                    )
                    .map_err(native_error)
                    .and_then(|target| {
                        target.ok_or_else(|| {
                            RemoteError::new(ErrorCode::NotFound, "The operation was not found.")
                        })
                    });
                let outcome = JournalOutcome {
                    operation_id: entry.outcome.operation_id,
                    status: if result.is_ok() {
                        OperationStatus::Succeeded
                    } else {
                        OperationStatus::Failed
                    },
                    error_code: result.as_ref().err().map(|e| e.code.clone()),
                };
                {
                    let mut db = self.services.state.db.lock().map_err(|_| unavailable())?;
                    RemoteStore::new(&mut db, &self.services.server_identity)
                        .finish_command(
                            &principal.device_id,
                            &request.id,
                            &outcome,
                            now_ms() as i64,
                        )
                        .map_err(|_| {
                            RemoteError::new(
                                ErrorCode::OutcomeUnknown,
                                "Inspect the target operation.",
                            )
                        })?;
                }
                registry.publish(principal);
                result?;
            }
            CommandAcceptance::AlreadyIssued { .. } => return Err(RemoteError::invalid()),
        }
        value(CancellationAck {
            cancellation_requested: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_scope_and_future_methods_do_not_create_journal_intents() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let dispatcher = Dispatcher::new(services);
        let principal = checked(
            serde_json::json!({"deviceId":"phone","scopes":["browse","library_manage","settings"]}),
        )
        .unwrap();
        let epoch = state.event_hub.epoch();
        let request = parse_request(&serde_json::json!({"v":1,"id":"pause","method":"player.pause","params":{"sessionId":"session","paused":true},"expected":{"serverEpoch":epoch,"sessionId":"session"}}).to_string()).unwrap();
        assert_eq!(
            dispatcher
                .dispatch(&principal, request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Forbidden
        );
        let request = parse_request(&serde_json::json!({"v":1,"id":"deferred","method":"settings.startAtLogin.set","params":{"enabled":true},"expected":{"serverEpoch":epoch,"settingsRevision":"1"}}).to_string()).unwrap();
        assert_eq!(
            dispatcher
                .dispatch(&principal, request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Unsupported
        );
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(state.operations.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod admission_tests;

impl Dispatcher {
    pub(crate) fn context(
        &self,
        owner: &RemotePrincipal,
        request: &RemoteRequest,
    ) -> CommandContext {
        CommandContext {
            principal: owner.clone(),
            request_id: request.id.clone(),
            server_epoch: self.services.state.event_hub.epoch(),
        }
    }
    fn confirmation_preview(
        &self,
        owner: &RemotePrincipal,
        target: &ActivityPrepareParams,
        observed: &crate::control::activity::Identity,
    ) -> Result<ConfirmationPreview, RemoteError> {
        let db = self.services.state.db.lock().map_err(|_| unavailable())?;
        let item = super::media::item(&db, target.item_id.as_str())?;
        let mut expected: Expected = checked(
            serde_json::json!({"serverEpoch":self.services.state.event_hub.epoch(),"queueRevision":observed.queue_revision.to_string()}),
        )?;
        expected.session_id = observed
            .player
            .as_ref()
            .or(observed.photos.as_ref())
            .map(checked)
            .transpose()?;
        let mut labels = vec![item.name];
        if observed.player.is_some() {
            labels.push(checked("Current local playback")?);
        }
        if observed.photos.is_some() {
            labels.push(checked("Current photo presentation")?);
        }
        Ok(ConfirmationPreview {
            confirmation_id: checked(uuid::Uuid::new_v4().to_string())?,
            owner_device_id: owner.device_id.clone(),
            action: ActionKind::ReplaceActivity,
            affected_labels: checked(labels)?,
            expected,
            expires_in_ms: checked(60000)?,
            browser_context: None,
            confirm_method: ConfirmMethod::ActivityConfirm,
        })
    }
    pub(crate) fn activity_replay(
        &self,
        owner: &RemotePrincipal,
        request: &RemoteRequest,
    ) -> Result<Option<serde_json::Value>, RemoteError> {
        if self
            .services
            .operations()
            .lookup_request(owner, &request.id)
            .map_err(native_error)?
            .is_some()
        {
            let outcome = self
                .services
                .operations()
                .start(self.context(owner, request), request.clone(), |_| async {
                    Ok(())
                })
                .map_err(native_error)?;
            if matches!(
                request.command,
                RemoteCommand::ActivityPrepare(_)
                    | RemoteCommand::VpnPrepareChange(_)
                    | RemoteCommand::AudioPrepare(_)
                    | RemoteCommand::FoldersPrepareAdd(_)
                    | RemoteCommand::DrivesPrepareEject(_)
                    | RemoteCommand::SourcesPrepareRemove(_)
            ) {
                let pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                if let Some(p) = pending.pending.values().find(|p| {
                    p.request == request.id && p.preview.owner_device_id == owner.device_id
                }) {
                    if matches!(
                        request.command,
                        RemoteCommand::AudioPrepare(_)
                            | RemoteCommand::VpnPrepareChange(_)
                            | RemoteCommand::FoldersPrepareAdd(_)
                            | RemoteCommand::DrivesPrepareEject(_)
                            | RemoteCommand::SourcesPrepareRemove(_)
                    ) {
                        if let Some(operation_id) = &p.operation {
                            return value(ActivityPreparation::Accepted {
                                operation: OperationRef {
                                    operation_id: operation_id.clone(),
                                },
                            })
                            .map(Some);
                        }
                    }
                    return value(ActivityPreparation::ConfirmationRequired {
                        preview: Box::new(
                            pending
                                .get(&owner.device_id, &p.preview.confirmation_id, now_ms())?
                                .preview,
                        ),
                    })
                    .map(Some);
                }
                if pending
                    .automatic
                    .contains_key(&(owner.device_id.as_str().into(), request.id.as_str().into()))
                {
                    return value(ActivityPreparation::Accepted {
                        operation: OperationRef {
                            operation_id: outcome.operation_id,
                        },
                    })
                    .map(Some);
                }
                return Err(RemoteError::new(ErrorCode::OutcomeUnknown,"The original confirmation is no longer available. Inspect the activity before preparing again."));
            }
            return value(AcceptedResult {
                status: OperationAccepted::Accepted,
                operation_id: outcome.operation_id,
            })
            .map(Some);
        }
        Ok(None)
    }
    async fn activity(
        &self,
        owner: &RemotePrincipal,
        request: RemoteRequest,
    ) -> Result<serde_json::Value, RemoteError> {
        use crate::control::{activity, confirmations::Pending};
        if request
            .expected
            .as_ref()
            .and_then(|e| e.server_epoch.as_ref())
            != Some(&self.services.state.event_hub.epoch())
        {
            return Err(RemoteError::new(
                ErrorCode::StaleState,
                "Refresh the native activity.",
            ));
        }
        if let Some(replay) = self.activity_replay(owner, &request)? {
            return Ok(replay);
        }
        let permit = self
            .work
            .acquire(owner.device_id.as_str(), 8)
            .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Too much queued work."))?;
        let music = self.services.state.music_gate.clone().lock_owned().await;
        let session = self.services.state.session_gate.clone().lock_owned().await;
        if let Some(replay) = self.activity_replay(owner, &request)? {
            return Ok(replay);
        }
        let observed = activity::identity(&self.services.state).await;
        if observed.browser.is_some() {
            return Err(RemoteError::new(
                ErrorCode::Unsupported,
                "Close the streaming activity before switching local media.",
            ));
        }
        match &request.command {
            RemoteCommand::ActivityPrepare(target) => {
                let media_identity = activity::preflight(&self.services.state, target)
                    .await
                    .map_err(native_error)?;
                if !observed.is_idle() {
                    let mut registry = self
                        .services
                        .confirmations
                        .lock()
                        .map_err(|_| unavailable())?;
                    if let Some(p) = registry.pending.values().find(|p| {
                        p.request == request.id && p.preview.owner_device_id == owner.device_id
                    }) {
                        self.services
                            .operations()
                            .start(self.context(owner, &request), request.clone(), |_| async {
                                Ok(())
                            })
                            .map_err(native_error)?;
                        return value(ActivityPreparation::ConfirmationRequired {
                            preview: Box::new(
                                registry
                                    .get(&owner.device_id, &p.preview.confirmation_id, now_ms())?
                                    .preview,
                            ),
                        });
                    }
                    let preview = self.confirmation_preview(owner, target, &observed)?;
                    let pending = Pending {
                        preview: preview.clone(),
                        action: crate::control::confirmations::Action::Activity {
                            target: target.clone(),
                            media: media_identity,
                        },
                        observed,
                        request: request.id.clone(),
                        expires_at: now_ms() + 60000,
                        operation: None,
                    };
                    registry.insert(pending, now_ms())?;
                    if let Err(error) = self.services.operations().start(
                        self.context(owner, &request),
                        request.clone(),
                        |_| async { Ok(()) },
                    ) {
                        registry.pending.remove(preview.confirmation_id.as_str());
                        return Err(native_error(error));
                    }
                    return value(ActivityPreparation::ConfirmationRequired {
                        preview: Box::new(preview),
                    });
                }
                let key = (
                    owner.device_id.as_str().to_string(),
                    request.id.as_str().to_string(),
                );
                let target = target.clone();
                let services = self.services.clone();
                let registration = self.registration.clone();
                let mut confirmations = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                let outcome = self
                    .services
                    .operations()
                    .start(
                        self.context(owner, &request),
                        request,
                        move |cancel| async move {
                            let _permit = permit;
                            let _music = music;
                            let _session = session;
                            let _registration = registration;
                            let app = services.app().ok_or_else(|| {
                                AppError::new("unavailable", "Native app unavailable.")
                            })?;
                            activity::execute_locked(
                                app,
                                &services.state,
                                target,
                                observed,
                                media_identity,
                                cancel,
                            )
                            .await
                            .map_err(super::media::normalize)
                        },
                    )
                    .map_err(native_error)?;
                confirmations
                    .automatic
                    .retain(|_, at| now_ms().saturating_sub(*at) < 86_400_000);
                if confirmations.automatic.len() < 10000 {
                    confirmations.automatic.insert(key, now_ms());
                }
                value(ActivityPreparation::Accepted {
                    operation: OperationRef {
                        operation_id: outcome.operation_id,
                    },
                })
            }
            RemoteCommand::ActivityConfirm(params) => {
                let pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?
                    .get(&owner.device_id, &params.confirmation_id, now_ms())?;
                if let Some(operation_id) = pending.operation {
                    return value(AcceptedResult {
                        status: OperationAccepted::Accepted,
                        operation_id,
                    });
                }
                let crate::control::confirmations::Action::Activity {
                    target,
                    media: before_media,
                } = &pending.action
                else {
                    return Err(RemoteError::invalid());
                };
                let media_identity = activity::preflight(&self.services.state, target)
                    .await
                    .map_err(native_error)?;
                if pending.observed != observed || *before_media != media_identity {
                    let preview = self.confirmation_preview(owner, target, &observed)?;
                    let fresh = Pending {
                        preview: preview.clone(),
                        observed,
                        action: crate::control::confirmations::Action::Activity {
                            target: target.clone(),
                            media: media_identity.clone(),
                        },
                        expires_at: now_ms() + 60000,
                        ..pending
                    };
                    self.services
                        .confirmations
                        .lock()
                        .map_err(|_| unavailable())?
                        .insert(fresh, now_ms())?;
                    let mut error = RemoteError::new(
                        ErrorCode::ConfirmationStale,
                        "The active media changed. Review this replacement.",
                    );
                    error.replacement_preview = Some(Box::new(preview));
                    return Err(error);
                }
                let mut confirmations = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                let current =
                    confirmations.get(&owner.device_id, &params.confirmation_id, now_ms())?;
                if let Some(operation_id) = current.operation {
                    return value(AcceptedResult {
                        status: OperationAccepted::Accepted,
                        operation_id,
                    });
                }
                let target = target.clone();
                let before = pending.observed;
                let services = self.services.clone();
                let registration = self.registration.clone();
                let id = params.confirmation_id.clone();
                let outcome = self
                    .services
                    .operations()
                    .start(
                        self.context(owner, &request),
                        request,
                        move |cancel| async move {
                            let _permit = permit;
                            let _music = music;
                            let _session = session;
                            let _registration = registration;
                            let app = services.app().ok_or_else(|| {
                                AppError::new("unavailable", "Native app unavailable.")
                            })?;
                            activity::execute_locked(
                                app,
                                &services.state,
                                target,
                                before,
                                media_identity,
                                cancel,
                            )
                            .await
                            .map_err(super::media::normalize)
                        },
                    )
                    .map_err(native_error)?;
                if let Some(p) = confirmations.pending.get_mut(id.as_str()) {
                    p.operation = Some(outcome.operation_id.clone());
                }
                value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id: outcome.operation_id,
                })
            }
            _ => Err(RemoteError::invalid()),
        }
    }
}

impl Dispatcher {
    fn audio_preview(
        &self,
        owner: &RemotePrincipal,
        output: &crate::contracts::AudioSinkDescriptor,
        sessions: &crate::control::audio::Sessions,
    ) -> Result<ConfirmationPreview, RemoteError> {
        let mut labels: Vec<DisplayText> =
            vec![checked(output.name.chars().take(200).collect::<String>())?];
        if sessions.player.is_some() {
            labels.push(checked("Current local playback")?);
        }
        if sessions.browser.is_some() {
            labels.push(checked(
                "Current managed browser audio; browser pausing is unavailable",
            )?);
        }
        if sessions.player.is_none() && sessions.browser.is_none() {
            labels.push(checked("Preference for the next playback stream")?);
        }
        checked(
            serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":"change_audio","affectedLabels":labels,"expected":{"serverEpoch":self.services.state.event_hub.epoch()},"expiresInMs":60000,"confirmMethod":"audio.confirm"}),
        )
    }
    async fn audio(
        &self,
        owner: &RemotePrincipal,
        request: RemoteRequest,
    ) -> Result<serde_json::Value, RemoteError> {
        use crate::control::{
            audio,
            confirmations::{Action, Pending},
        };
        if matches!(request.command, RemoteCommand::AudioOutputs(_)) {
            audio::discover(&self.services.state)
                .await
                .map_err(native_error)?;
            let snapshot = self.services.state.snapshot().map_err(native_error)?;
            return value(super::state::outputs_topic(
                &self.services.state,
                &snapshot,
                self.services.state.event_hub.revision(Topic::Outputs),
            )?);
        }
        if request
            .expected
            .as_ref()
            .and_then(|e| e.server_epoch.as_ref())
            != Some(&self.services.state.event_hub.epoch())
        {
            return Err(RemoteError::new(
                ErrorCode::StaleState,
                "Refresh sound output state.",
            ));
        }
        if let Some(replay) = self.activity_replay(owner, &request)? {
            return Ok(replay);
        }
        let permit = self
            .work
            .acquire(owner.device_id.as_str(), 8)
            .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Too much queued work."))?;
        let session = self.services.state.session_gate.clone().lock_owned().await;
        if let Some(replay) = self.activity_replay(owner, &request)? {
            return Ok(replay);
        }
        let inventory = audio::discover(&self.services.state)
            .await
            .map_err(native_error)?;
        let observed = audio::sessions(&self.services.state).await;
        match &request.command {
            RemoteCommand::AudioPrepare(params) => {
                let output = audio::validate_output(&inventory, params.output_id.as_str())
                    .map_err(native_error)?
                    .clone();
                let preview = self.audio_preview(owner, &output, &observed)?;
                let pending = Pending {
                    preview: preview.clone(),
                    action: Action::Audio {
                        output,
                        sessions: observed,
                    },
                    observed: Default::default(),
                    request: request.id.clone(),
                    expires_at: now_ms() + 60000,
                    operation: None,
                };
                let mut confirmations = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                confirmations.insert(pending, now_ms())?;
                if let Err(error) = self.services.operations().start(
                    self.context(owner, &request),
                    request.clone(),
                    |_| async { Ok(()) },
                ) {
                    confirmations
                        .pending
                        .remove(preview.confirmation_id.as_str());
                    return Err(native_error(error));
                }
                value(ActivityPreparation::ConfirmationRequired {
                    preview: Box::new(preview),
                })
            }
            RemoteCommand::AudioConfirm(params) => {
                let mut pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?
                    .get(&owner.device_id, &params.confirmation_id, now_ms())?;
                let Action::Audio {
                    output,
                    sessions: before,
                } = &pending.action
                else {
                    return Err(RemoteError::invalid());
                };
                if let Some(operation_id) = pending.operation {
                    return value(AcceptedResult {
                        status: OperationAccepted::Accepted,
                        operation_id,
                    });
                }
                let current =
                    audio::validate_output(&inventory, &output.id).map_err(native_error)?;
                if current.id != output.id
                    || current.backend_name != output.backend_name
                    || current.name != output.name
                    || &observed != before
                {
                    let preview = self.audio_preview(owner, current, &observed)?;
                    pending.preview = preview.clone();
                    pending.action = Action::Audio {
                        output: current.clone(),
                        sessions: observed,
                    };
                    pending.expires_at = now_ms() + 60000;
                    self.services
                        .confirmations
                        .lock()
                        .map_err(|_| unavailable())?
                        .insert(pending, now_ms())?;
                    let mut error = RemoteError::new(
                        ErrorCode::ConfirmationStale,
                        "Playback or sound output changed. Review again.",
                    );
                    error.replacement_preview = Some(Box::new(preview));
                    return Err(error);
                }
                let output = output.id.clone();
                let before = before.clone();
                let services = self.services.clone();
                let registration = self.registration.clone();
                let mut confirmations = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                let current =
                    confirmations.get(&owner.device_id, &params.confirmation_id, now_ms())?;
                if let Some(operation_id) = current.operation {
                    return value(AcceptedResult {
                        status: OperationAccepted::Accepted,
                        operation_id,
                    });
                }
                let confirmation = params.confirmation_id.clone();
                let outcome = self
                    .services
                    .operations()
                    .start(
                        self.context(owner, &request),
                        request,
                        move |cancel| async move {
                            let _permit = permit;
                            let _session = session;
                            let _registration = registration;
                            if cancel.is_cancelled() {
                                return Err(AppError::new("cancelled", "Sound change cancelled."));
                            }
                            let app = services.app().ok_or_else(|| {
                                AppError::new("unavailable", "Native app unavailable.")
                            })?;
                            audio::apply_audio_output(app, &services.state, &output, &before)
                                .await
                                .map_err(super::media::normalize)
                        },
                    )
                    .map_err(native_error)?;
                if let Some(p) = confirmations.pending.get_mut(confirmation.as_str()) {
                    p.operation = Some(outcome.operation_id.clone());
                }
                value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id: outcome.operation_id,
                })
            }
            RemoteCommand::AudioTest(params) => {
                audio::validate_output(&inventory, params.output_id.as_str())
                    .map_err(native_error)?;
                let output = params.output_id.as_str().to_string();
                let state = self.services.state.clone();
                let registration = self.registration.clone();
                let outcome = self
                    .services
                    .operations()
                    .start(
                        self.context(owner, &request),
                        request,
                        move |cancel| async move {
                            let _permit = permit;
                            let _session = session;
                            let _registration = registration;
                            audio::test_output(&state, &output, cancel)
                                .await
                                .map_err(super::media::normalize)
                        },
                    )
                    .map_err(native_error)?;
                value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id: outcome.operation_id,
                })
            }
            _ => Err(RemoteError::invalid()),
        }
    }
}

#[cfg(test)]
mod task10_tests {
    use super::*;
    #[tokio::test]
    async fn task10_foreground_input_is_never_journaled_or_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let dispatcher = Dispatcher::new(services);
        let owner =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","navigation"]}))
                .unwrap();
        let before = serde_json::to_value(state.snapshot().unwrap()).unwrap();
        for (method, error) in [
            ("activity.showMenu", ErrorCode::Unavailable),
            ("activity.showPlayback", ErrorCode::StaleState),
        ] {
            for _ in 0..2 {
                let request=parse_request(&serde_json::json!({"v":1,"id":"same-transient-id","method":method,"params":{},"expected":{"serverEpoch":state.event_hub.epoch(),"sessionId":"foreign"}}).to_string()).unwrap();
                assert_eq!(
                    dispatcher.dispatch(&owner, request).await.unwrap_err().code,
                    error
                );
            }
        }
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(state.operations.lock().unwrap().is_empty());
        assert_eq!(
            serde_json::to_value(state.snapshot().unwrap()).unwrap(),
            before
        );
    }
    #[tokio::test]
    async fn task10_audio_preview_replay_uses_the_shared_owner_bound_confirmation() {
        use crate::control::{
            audio::Sessions,
            confirmations::{Action, Pending},
        };
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let dispatcher = Dispatcher::new(services.clone());
        let owner: RemotePrincipal =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","outputs"]})).unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Private fixture phone","scopes":["browse","outputs"],"createdAtMs":1,"revoked":false})).unwrap();
        let credential: BearerCredential = checked("a".repeat(43)).unwrap();
        RemoteStore::new(&mut state.db.lock().unwrap(), &services.server_identity)
            .add_device(
                &device,
                &super::super::store::CredentialHash::from_credential(&credential),
            )
            .unwrap();
        let output = crate::contracts::AudioSinkDescriptor {
            id: "hdmi".into(),
            backend_name: "hdmi".into(),
            name: "TV sound".into(),
            available: true,
        };
        let sessions = Sessions {
            player: Some("owned-player".into()),
            browser: None,
        };
        let preview = dispatcher
            .audio_preview(&owner, &output, &sessions)
            .unwrap();
        assert_eq!(preview.action, ActionKind::ChangeAudio);
        assert_eq!(preview.confirm_method, ConfirmMethod::AudioConfirm);
        assert_eq!(preview.affected_labels.as_slice().len(), 2);
        let request=parse_request(&serde_json::json!({"v":1,"id":"prepare","method":"audio.prepare","params":{"outputId":"hdmi"},"expected":{"serverEpoch":state.event_hub.epoch()}}).to_string()).unwrap();
        services
            .confirmations
            .lock()
            .unwrap()
            .insert(
                Pending {
                    preview: preview.clone(),
                    action: Action::Audio { output, sessions },
                    observed: Default::default(),
                    request: request.id.clone(),
                    expires_at: now_ms() + 60000,
                    operation: None,
                },
                now_ms(),
            )
            .unwrap();
        services
            .operations()
            .start(
                dispatcher.context(&owner, &request),
                request.clone(),
                |_| async { Ok(()) },
            )
            .unwrap();
        let result = dispatcher.dispatch(&owner, request.clone()).await.unwrap();
        assert_eq!(
            result["preview"]["confirmationId"],
            serde_json::to_value(&preview.confirmation_id).unwrap()
        );
        assert_eq!(services.confirmations.lock().unwrap().pending.len(), 1);
        let other = checked("other-phone").unwrap();
        assert_eq!(
            services
                .confirmations
                .lock()
                .unwrap()
                .get(&other, &preview.confirmation_id, now_ms())
                .err()
                .unwrap()
                .code,
            ErrorCode::NotFound
        );
        services
            .confirmations
            .lock()
            .unwrap()
            .pending
            .get_mut(preview.confirmation_id.as_str())
            .unwrap()
            .operation = Some(checked("confirmed-audio-operation").unwrap());
        let consumed = dispatcher.dispatch(&owner, request.clone()).await.unwrap();
        assert_eq!(consumed["status"], "accepted");
        assert_eq!(
            consumed["operation"]["operationId"],
            "confirmed-audio-operation"
        );
        let mut changed = request;
        changed.command = RemoteCommand::AudioPrepare(
            checked(serde_json::json!({"outputId":"other-output"})).unwrap(),
        );
        assert_eq!(
            dispatcher.dispatch(&owner, changed).await.unwrap_err().code,
            ErrorCode::RequestIdConflict
        );
    }
    #[tokio::test]
    async fn task10_remote_play_admission_preserves_mirrored_target_and_stored_main_geometry() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let dispatcher = Dispatcher::new(services.clone());
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Private fixture phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
        let credential: BearerCredential = checked("a".repeat(43)).unwrap();
        RemoteStore::new(&mut state.db.lock().unwrap(), &services.server_identity)
            .add_device(
                &device,
                &super::super::store::CredentialHash::from_credential(&credential),
            )
            .unwrap();
        let owner = RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        };
        state.home.lock().unwrap().outputs.desired = crate::contracts::PlaybackTarget {
            monitor_id: Some("existing-mirrored-desktop".into()),
            audio_sink_id: Some("existing-hdmi".into()),
        };
        {
            let db = state.db.lock().unwrap();
            for (key, value) in [
                (
                    "main_geometry",
                    r#"{"x":40,"y":50,"width":1100,"height":700}"#,
                ),
                ("control_monitor", r#""existing-mirrored-desktop""#),
            ] {
                db.execute(
                    "INSERT INTO settings(key,value_json,updated_at_ms) VALUES(?1,?2,1)",
                    rusqlite::params![key, value],
                )
                .unwrap();
            }
        }
        let before = serde_json::to_value(state.snapshot().unwrap()).unwrap();
        let settings = || {
            state
                .db
                .lock()
                .unwrap()
                .prepare("SELECT key,value_json FROM settings ORDER BY key")
                .unwrap()
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let before_settings = settings();
        let request=parse_request(&serde_json::json!({"v":1,"id":"play","method":"player.play","params":{"itemId":"private-item","start":"beginning","monitorId":"injected-display","resolution":"640x480","mirroring":false},"expected":{"serverEpoch":state.event_hub.epoch()}}).to_string()).unwrap();
        let encoded = serde_json::to_value(&request.command).unwrap();
        assert!(encoded["params"].get("monitorId").is_none());
        assert!(encoded["params"].get("resolution").is_none());
        assert!(encoded["params"].get("mirroring").is_none());
        dispatcher.dispatch(&owner, request).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if state.operations.lock().unwrap().is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(state.snapshot().unwrap()).unwrap(),
            before
        );
        assert_eq!(settings(), before_settings);
        // This headless admission fixture deliberately has no AppHandle/player launch.
        let operation = services
            .operations()
            .lookup_request(&owner, &checked("play").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(operation.status, OperationStatus::Failed);
        assert_eq!(operation.error_code, Some(ErrorCode::Unavailable));
    }
}

#[cfg(test)]
mod task11_tests {
    use super::*;
    #[tokio::test]
    async fn task11_ui_without_renderer_is_unavailable_and_never_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::app_state::AppState::open(dir.path().into()).unwrap();
        let dispatcher = Dispatcher::new(
            ControlServices::native(state.clone(), checked("server").unwrap()).unwrap(),
        );
        let owner =
            checked(serde_json::json!({"deviceId":"phone","scopes":["navigation"]})).unwrap();
        let request = parse_request(&serde_json::json!({"v":1,"id":"move","method":"ui.moveFocus","params":{"rendererGeneration":"1","focusRevision":"1","direction":"down"},"expected":{"serverEpoch":state.event_hub.epoch()}}).to_string()).unwrap();
        assert_eq!(
            dispatcher.dispatch(&owner, request).await.unwrap_err().code,
            ErrorCode::RendererUnavailable
        );
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
