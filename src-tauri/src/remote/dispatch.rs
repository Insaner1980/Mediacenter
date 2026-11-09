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
        checked(error.code).unwrap_or(ErrorCode::Internal),
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
    checked(serde_json::json!({"methods":["state.get","capabilities.get","operations.get","operations.cancel","player.pause"],"services":[],"browserBridge":false})).expect("static capabilities")
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
            RemoteCommand::OperationsGet(params) => value(
                self.lookup(
                    principal,
                    params.request_id.as_ref(),
                    params.operation_id.as_ref(),
                )?
                .ok_or_else(unavailable)?,
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
                    })
                    .map_err(native_error)?;
                // Acceptance remains the stable reply; terminal truth is operations.get.
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
                .ok_or_else(unavailable)?,
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
                    .cancel(principal, &target.ok_or_else(unavailable)?.operation_id)
                    .map_err(native_error)
                    .and_then(|target| target.ok_or_else(unavailable));
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
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse"]})).unwrap();
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
        let request = parse_request(
            r#"{"v":1,"id":"read","method":"library.query","params":{"kind":"video"}}"#,
        )
        .unwrap();
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
