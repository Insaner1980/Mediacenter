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
    work: Arc<DeviceLimits>,
    cancellation_gate: Arc<Mutex<()>>,
    pub(super) registration: Option<Arc<()>>,
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
    checked(serde_json::json!({"methods":["state.get","capabilities.get","operations.get","operations.cancel","player.pause","library.query","library.item","player.play","player.seek","player.volume","player.mute","player.audioTrack","player.subtitleTrack","player.stop","queue.get","queue.enqueue","queue.remove","queue.move","queue.playEntry","queue.next","queue.previous","queue.shuffle","queue.repeat","playlists.list","playlists.get","playlists.save","playlists.delete","playlists.play","photos.show","photos.slideshow.start","photos.next","photos.previous","photos.pause","photos.interval","photos.shuffle","photos.fit","photos.stop","activity.prepare","activity.confirm","confirmations.get","confirmations.cancel"],"services":[],"browserBridge":false})).expect("static capabilities")
}
fn value(value: impl serde::Serialize) -> Result<serde_json::Value, RemoteError> {
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
        match &request.command {
            RemoteCommand::CapabilitiesGet(_) => value(capabilities()),
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
    fn cancel(
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
        let principal =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","library_manage"]}))
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
        let request =
            parse_request(r#"{"v":1,"id":"read","method":"sources.list","params":{}}"#).unwrap();
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
    fn context(&self, owner: &RemotePrincipal, request: &RemoteRequest) -> CommandContext {
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
    fn activity_replay(
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
            if matches!(request.command, RemoteCommand::ActivityPrepare(_)) {
                let pending = self
                    .services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?;
                if let Some(p) = pending.pending.values().find(|p| {
                    p.request == request.id && p.preview.owner_device_id == owner.device_id
                }) {
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
                        target: target.clone(),
                        observed,
                        media: media_identity,
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
                let media_identity = activity::preflight(&self.services.state, &pending.target)
                    .await
                    .map_err(native_error)?;
                if pending.observed != observed || pending.media != media_identity {
                    let preview = self.confirmation_preview(owner, &pending.target, &observed)?;
                    let fresh = Pending {
                        preview: preview.clone(),
                        observed,
                        media: media_identity,
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
                let target = pending.target;
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
