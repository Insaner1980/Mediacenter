//! Retained native outcomes and identity/owner-scoped durable operation execution.
use super::{
    adapters::checked,
    protocol::*,
    store::{CommandAcceptance, JournalOutcome, RemoteStore},
};
use crate::{
    app_state::{now_ms, AppState},
    contracts::AppError,
};
use std::collections::HashMap;
use tauri::Emitter;
use tokio_util::sync::CancellationToken;
#[derive(Default)]
pub struct RetainedOperations {
    records: HashMap<String, (OperationStatus, u64)>,
    // Only completed native work whose terminal journal write failed. Never a
    // replay queue; no parameters, result text or credentials are retained.
    unknown: HashMap<(String, String, String), (OperationState, u64)>,
}
#[derive(Clone)]
pub struct OperationRegistry {
    state: AppState,
    identity: Id,
    app: Option<tauri::AppHandle>,
}
impl RetainedOperations {
    fn remember_unknown(&mut self, identity: &Id, ctx: &CommandContext, operation: &Id) {
        let now = now_ms();
        self.unknown
            .retain(|_, (_, at)| now.saturating_sub(*at) < 86_400_000);
        if self.unknown.len() >= 10_000 {
            if let Some(oldest) = self
                .unknown
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(key, _)| key.clone())
            {
                self.unknown.remove(&oldest);
            }
        }
        self.unknown.insert(
            (
                identity.as_str().into(),
                ctx.principal.device_id.as_str().into(),
                ctx.request_id.as_str().into(),
            ),
            (
                OperationState {
                    operation_id: operation.clone(),
                    request_id: ctx.request_id.clone(),
                    status: OperationStatus::OutcomeUnknown,
                    error_code: Some(WireErrorCode::OutcomeUnknown),
                    cancellation_requested: false,
                },
                now,
            ),
        );
    }
    fn unknown_items(&self, identity: &Id, owner: &Id) -> Vec<OperationState> {
        let now = now_ms();
        let mut items = self
            .unknown
            .iter()
            .filter(|((server, device, _), (_, at))| {
                server == identity.as_str()
                    && device == owner.as_str()
                    && now.saturating_sub(*at) < 86_400_000
            })
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        items.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
        items.into_iter().map(|(item, _)| item.clone()).collect()
    }
    fn unknown_request(&self, identity: &Id, owner: &Id, request: &Id) -> Option<OperationState> {
        self.unknown
            .get(&(
                identity.as_str().into(),
                owner.as_str().into(),
                request.as_str().into(),
            ))
            .filter(|(_, at)| now_ms().saturating_sub(*at) < 86_400_000)
            .map(|(item, _)| item.clone())
    }
    pub fn record(&mut self, id: &str, status: OperationStatus) {
        let now = now_ms();
        self.records.retain(|_, (status, time)| {
            matches!(status, OperationStatus::Accepted | OperationStatus::Running)
                || now.saturating_sub(*time) < 86_400_000
        });
        if self.records.len() >= 10_000 {
            if let Some(oldest) = self
                .records
                .iter()
                .filter(|(_, (status, _))| {
                    !matches!(status, OperationStatus::Accepted | OperationStatus::Running)
                })
                .min_by_key(|(_, (_, time))| *time)
                .map(|(id, _)| id.clone())
            {
                self.records.remove(&oldest);
            }
        }
        self.records.insert(id.into(), (status, now));
    }
    pub fn status(&self, id: &str) -> Option<OperationStatus> {
        self.records
            .get(id)
            .filter(|(status, time)| {
                matches!(status, OperationStatus::Accepted | OperationStatus::Running)
                    || now_ms().saturating_sub(*time) < 86_400_000
            })
            .map(|(status, _)| status.clone())
    }
}
fn failure(code: &str) -> AppError {
    AppError::new(code, "The operation could not be completed.")
}
fn wire(entry: super::store::JournalEntry, cancellation_requested: bool) -> OperationState {
    OperationState {
        operation_id: entry.outcome.operation_id,
        request_id: entry.request_id,
        status: entry.outcome.status,
        error_code: entry.outcome.error_code,
        cancellation_requested,
    }
}
impl OperationRegistry {
    pub fn new(state: AppState, identity: Id) -> Self {
        Self {
            state,
            identity,
            app: None,
        }
    }
    pub fn with_app(mut self, app: Option<tauri::AppHandle>) -> Self {
        self.app = app;
        self
    }
    /// Explicit startup boundary: construction and lookup NEVER recover live work.
    pub fn recover_unfinished(&self) -> Result<usize, AppError> {
        {
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            RemoteStore::new(&mut db, &self.identity).recover_unfinished(now_ms() as i64)
        }
    }
    pub fn lookup(
        &self,
        principal: &RemotePrincipal,
        operation: &Id,
    ) -> Result<Option<OperationState>, AppError> {
        if !principal.scopes.as_slice().contains(&Scope::Browse) {
            return Err(failure("forbidden"));
        }
        if let Some(unknown) = self
            .state
            .retained_operations
            .lock()
            .map_err(|_| failure("state_error"))?
            .unknown_items(&self.identity, &principal.device_id)
            .into_iter()
            .find(|item| &item.operation_id == operation)
        {
            return Ok(Some(unknown));
        }
        let entry = {
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            RemoteStore::new(&mut db, &self.identity)
                .get_operation(&principal.device_id, operation)?
        };
        let cancelled = self
            .state
            .operations
            .lock()
            .map_err(|_| failure("state_error"))?
            .get(operation.as_str())
            .is_some_and(CancellationToken::is_cancelled);
        Ok(entry.map(|entry| wire(entry, cancelled)))
    }
    pub fn lookup_request(
        &self,
        principal: &RemotePrincipal,
        request: &Id,
    ) -> Result<Option<OperationState>, AppError> {
        if !principal.scopes.as_slice().contains(&Scope::Browse) {
            return Err(failure("forbidden"));
        }
        if let Some(unknown) = self
            .state
            .retained_operations
            .lock()
            .map_err(|_| failure("state_error"))?
            .unknown_request(&self.identity, &principal.device_id, request)
        {
            return Ok(Some(unknown));
        }
        let entry = {
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            RemoteStore::new(&mut db, &self.identity).get_command(&principal.device_id, request)?
        };
        match entry {
            Some(entry) => self.lookup(principal, &entry.outcome.operation_id),
            None => Ok(None),
        }
    }
    fn project(
        &self,
        owner: &Id,
        entries: Vec<super::store::JournalEntry>,
        active: &HashMap<String, CancellationToken>,
    ) -> Result<Vec<OperationState>, AppError> {
        let retained = self
            .state
            .retained_operations
            .lock()
            .map_err(|_| failure("state_error"))?;
        let mut items = entries
            .into_iter()
            .map(|entry| {
                retained
                    .unknown_request(&self.identity, owner, &entry.request_id)
                    .unwrap_or_else(|| {
                        let cancelled = active
                            .get(entry.outcome.operation_id.as_str())
                            .is_some_and(CancellationToken::is_cancelled);
                        wire(entry, cancelled)
                    })
            })
            .collect::<Vec<_>>();
        for unknown in retained.unknown_items(&self.identity, owner) {
            if !items
                .iter()
                .any(|item| item.operation_id == unknown.operation_id)
            {
                items.push(unknown);
            }
        }
        // Keep current native work visible even after many terminal results.
        items.sort_by_key(|item| match item.status {
            OperationStatus::Accepted | OperationStatus::Running => 0,
            OperationStatus::OutcomeUnknown => 1,
            _ => 2,
        });
        items.truncate(100);
        Ok(items)
    }
    pub fn recent(&self, principal: &RemotePrincipal) -> Result<OperationsTopic, AppError> {
        self.capture(principal)
    }
    fn capture(&self, principal: &RemotePrincipal) -> Result<OperationsTopic, AppError> {
        if !principal.scopes.as_slice().contains(&Scope::Browse) {
            return Err(failure("forbidden"));
        }
        let (entries, revision) = {
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            let entries =
                RemoteStore::new(&mut db, &self.identity).recent_commands(&principal.device_id)?;
            (entries, self.state.event_hub.revision(Topic::Operations))
        };
        let active = self
            .state
            .operations
            .lock()
            .map_err(|_| failure("state_error"))?;
        let items = self.project(&principal.device_id, entries, &active)?;
        Ok(OperationsTopic {
            revision: revision.into(),
            items: checked(items).map_err(|_| failure("state_error"))?,
        })
    }
    pub fn cancel(
        &self,
        principal: &RemotePrincipal,
        operation: &Id,
    ) -> Result<Option<OperationState>, AppError> {
        if self.lookup(principal, operation)?.is_none() {
            return Ok(None);
        }
        {
            let active = self
                .state
                .operations
                .lock()
                .map_err(|_| failure("state_error"))?;
            if let Some(token) = active.get(operation.as_str()) {
                token.cancel();
            }
        }
        // A cancellation request is observable, but is not a terminal outcome.
        self.publish(principal);
        self.lookup(principal, operation)
    }
    fn publish(&self, _principal: &RemotePrincipal) {
        // Capture ALL owner projections at the same DB boundary. A later B update
        // includes A's latest state, so rejecting delayed A cannot lose A's outcome.
        let captured = (|| -> Result<_, AppError> {
            let active = self
                .state
                .operations
                .lock()
                .map_err(|_| failure("state_error"))?;
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            let store = RemoteStore::new(&mut db, &self.identity);
            let mut topics = Vec::new();
            for owner in store.active_device_ids()? {
                let items = self.project(&owner, store.recent_commands(&owner)?, &active)?;
                topics.push((owner, checked(items).map_err(|_| failure("state_error"))?));
            }
            Ok((self.state.event_hub.reserve(Topic::Operations), topics))
        })();
        if let Ok((stamp, topics)) = captured {
            self.state
                .event_hub
                .publish_projected(stamp, |owner, server_epoch, sequence| {
                    RemoteEvent::Operations {
                        server_epoch,
                        sequence,
                        data: OperationsTopic {
                            revision: stamp.revision.into(),
                            items: topics
                                .iter()
                                .find(|(id, _)| Some(id) == owner)
                                .map(|(_, items): &(Id, OperationStates)| items.clone())
                                .unwrap_or_else(|| checked(Vec::<OperationState>::new()).unwrap()),
                        },
                    }
                });
        }
    }
    pub fn start<F, Fut>(
        &self,
        ctx: CommandContext,
        request: RemoteRequest,
        work: F,
    ) -> Result<JournalOutcome, AppError>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), AppError>> + Send + 'static,
    {
        if ctx.request_id != request.id
            || ctx.server_epoch != self.state.event_hub.epoch()
            || request
                .expected
                .as_ref()
                .and_then(|expected| expected.server_epoch.as_ref())
                .is_some_and(|epoch| epoch != &ctx.server_epoch)
        {
            return Err(failure("stale_state"));
        }
        if !ctx
            .principal
            .scopes
            .as_slice()
            .contains(&request.command.scope())
        {
            return Err(failure("forbidden"));
        }
        // Pairing issuance needs its own ephemeral result boundary (Task 4).
        if matches!(request.command, RemoteCommand::PairingBegin(_)) {
            return Err(failure("unsupported"));
        }
        let id: Id = checked(uuid::Uuid::new_v4().to_string()).expect("UUID");
        let token = CancellationToken::new();
        let outcome = {
            let mut active = self
                .state
                .operations
                .lock()
                .map_err(|_| failure("state_error"))?;
            let mut db = self
                .state
                .db
                .lock()
                .map_err(|_| failure("database_error"))?;
            let mut store = RemoteStore::new(&mut db, &self.identity);
            // Duplicates are returned even when native admission is full.
            if active.len() >= 32
                && store
                    .get_command(&ctx.principal.device_id, &request.id)?
                    .is_none()
            {
                return Err(failure("busy"));
            }
            match store.accept_command(
                &ctx.principal.device_id,
                &request,
                &ctx.server_epoch,
                &id,
                now_ms() as i64,
            )? {
                CommandAcceptance::Existing(entry) => {
                    if let Some(unknown) = self
                        .state
                        .retained_operations
                        .lock()
                        .map_err(|_| failure("state_error"))?
                        .unknown_request(&self.identity, &ctx.principal.device_id, &ctx.request_id)
                    {
                        return Ok(JournalOutcome {
                            operation_id: unknown.operation_id,
                            status: unknown.status,
                            error_code: unknown.error_code,
                        });
                    }
                    return Ok(entry.outcome);
                }
                CommandAcceptance::AlreadyIssued { .. } => return Err(failure("outcome_unknown")),
                CommandAcceptance::New(entry) => {
                    active.insert(id.as_str().into(), token.clone());
                    entry.outcome
                }
            }
        };
        self.publish(&ctx.principal);
        let registry = self.clone();
        tauri::async_runtime::spawn(async move {
            let running = {
                let mut db = registry.state.db.lock().unwrap();
                RemoteStore::new(&mut db, &registry.identity).mark_running(
                    &ctx.principal.device_id,
                    &ctx.request_id,
                    now_ms() as i64,
                )
            };
            let result = if running.is_ok() {
                registry.publish(&ctx.principal);
                match tauri::async_runtime::spawn(work(token)).await {
                    Ok(result) => result,
                    Err(_) => Err(failure("operation_failed")),
                }
            } else {
                Err(failure("database_error"))
            };
            let status = match &result {
                Ok(()) => OperationStatus::Succeeded,
                Err(error) if error.code == "cancelled" => OperationStatus::Cancelled,
                Err(_) => OperationStatus::Failed,
            };
            let error_code = result
                .as_ref()
                .err()
                .map(|e| checked(e.code.as_str()).unwrap_or(WireErrorCode::Internal));
            let terminal = JournalOutcome {
                operation_id: id.clone(),
                status,
                error_code,
            };
            // Persist terminal before forgetting the active token. If persistence fails,
            // retain a private, bounded unknown projection; never claim durable success.
            let persisted = {
                let mut db = registry.state.db.lock().unwrap();
                RemoteStore::new(&mut db, &registry.identity).finish_command(
                    &ctx.principal.device_id,
                    &ctx.request_id,
                    &terminal,
                    now_ms() as i64,
                )
            };
            if persisted.is_err() {
                registry
                    .state
                    .retained_operations
                    .lock()
                    .unwrap()
                    .remember_unknown(&registry.identity, &ctx, &id);
            }
            registry
                .state
                .operations
                .lock()
                .unwrap()
                .remove(id.as_str());
            registry.publish(&ctx.principal);
            if let Some(app) = registry.app {
                let mut error = if persisted.is_err() {
                    let mut error=AppError::new("outcome_unknown","The operation finished, but its outcome could not be recorded. Check native state before retrying.");
                    error.retryable = false;
                    Some(error)
                } else {
                    result.err()
                };
                if let Some(error) = &mut error {
                    error.operation_id = Some(id.as_str().into());
                }
                let _ = app.emit_to(
                    "main",
                    "operation_finished",
                    crate::contracts::OperationFinished {
                        operation_id: id.as_str().into(),
                        error,
                    },
                );
            }
        });
        Ok(outcome)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    fn fixture() -> (
        tempfile::TempDir,
        AppState,
        OperationRegistry,
        CommandContext,
        RemoteRequest,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let identity: Id = checked("server").unwrap();
        let principal: RemotePrincipal =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse","playback"]}))
                .unwrap();
        let device=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
        let credential = checked("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        RemoteStore::new(&mut state.db.lock().unwrap(), &identity)
            .add_device(
                &device,
                &super::super::store::CredentialHash::from_credential(&credential),
            )
            .unwrap();
        let registry = OperationRegistry::new(state.clone(), identity);
        let ctx = CommandContext {
            principal: principal.clone(),
            request_id: checked("request").unwrap(),
            server_epoch: state.event_hub.epoch(),
        };
        let mut request=super::super::protocol::parse_request(r#"{"v":1,"id":"request","method":"player.pause","params":{"sessionId":"player","paused":true},"expected":{"serverEpoch":"epoch","sessionId":"player"}}"#).unwrap();
        request.expected.as_mut().unwrap().server_epoch = Some(ctx.server_epoch.clone());
        (dir, state, registry, ctx, request)
    }
    #[tokio::test]
    async fn lost_reply_has_one_effect_and_terminal_lookup_survives_disconnect() {
        let (_dir, state, registry, ctx, request) = fixture();
        let principal = ctx.principal.clone();
        let listener = state
            .event_hub
            .subscribe_owner(Some(principal.device_id.clone()));
        let effects = Arc::new(AtomicUsize::new(0));
        let counter = effects.clone();
        let first = registry
            .start(ctx.clone(), request.clone(), move |_| async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        assert_eq!(first.status, OperationStatus::Accepted);
        drop(listener);
        let counter = effects.clone();
        let retry = registry
            .start(ctx, request, move |_| async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        assert_eq!(first.operation_id, retry.operation_id);
        for _ in 0..100 {
            if registry
                .lookup(&principal, &first.operation_id)
                .unwrap()
                .is_some_and(|op| op.status == OperationStatus::Succeeded)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        assert_eq!(
            registry
                .lookup(&principal, &first.operation_id)
                .unwrap()
                .unwrap()
                .status,
            OperationStatus::Succeeded
        );
        assert!(state.operations.lock().unwrap().is_empty());
        assert_eq!(
            registry
                .lookup_request(&principal, &checked("request").unwrap())
                .unwrap()
                .unwrap()
                .operation_id,
            first.operation_id
        );
        let other = RemotePrincipal {
            device_id: checked("other").unwrap(),
            scopes: principal.scopes,
        };
        assert!(registry
            .lookup(&other, &first.operation_id)
            .unwrap()
            .is_none());
    }
    #[test]
    fn operation_events_are_complete_owner_projections_with_contiguous_sequences() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let identity: Id = checked("server").unwrap();
        let registry = OperationRegistry::new(state.clone(), identity.clone());
        let a: Id = checked("a").unwrap();
        let b: Id = checked("b").unwrap();
        let suba = state.event_hub.subscribe_owner(Some(a.clone()));
        let subb = state.event_hub.subscribe_owner(Some(b.clone()));
        let scopes = checked(vec![Scope::Browse, Scope::Playback]).unwrap();
        let principal = RemotePrincipal {
            device_id: a.clone(),
            scopes,
        };
        {
            let mut db = state.db.lock().unwrap();
            let mut store = RemoteStore::new(&mut db, &identity);
            for owner in [&a, &b] {
                let device=checked(serde_json::json!({"deviceId":owner,"label":"Phone","scopes":["browse","playback"],"createdAtMs":1,"revoked":false})).unwrap();
                let credential = checked(owner.as_str().repeat(43)).unwrap();
                store
                    .add_device(
                        &device,
                        &super::super::store::CredentialHash::from_credential(&credential),
                    )
                    .unwrap();
                let request=super::super::protocol::parse_request(&serde_json::json!({"v":1,"id":format!("request-{}",owner.as_str()),"method":"player.pause","params":{"sessionId":"session","paused":true},"expected":{"serverEpoch":"epoch","sessionId":"session"}}).to_string()).unwrap();
                store
                    .accept_command(
                        owner,
                        &request,
                        &checked("epoch").unwrap(),
                        &checked(format!("operation-{}", owner.as_str())).unwrap(),
                        now_ms() as i64,
                    )
                    .unwrap();
            }
        }
        registry.publish(&principal);
        registry.publish(&RemotePrincipal {
            device_id: b,
            scopes: principal.scopes.clone(),
        });
        for (mut sub, expected) in [(suba, "operation-a"), (subb, "operation-b")] {
            let events = sub.drain().unwrap();
            assert_eq!(events.len(), 2);
            for (i, event) in events.iter().enumerate() {
                if let RemoteEvent::Operations { sequence, data, .. } = event {
                    assert_eq!(sequence.value(), i as u64 + 1);
                    assert_eq!(data.items.as_slice().len(), 1);
                    assert_eq!(data.items.as_slice()[0].operation_id.as_str(), expected);
                } else {
                    panic!()
                }
            }
        }
    }
    #[test]
    fn terminal_local_history_does_not_populate_active_shutdown_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        state
            .retained_operations
            .lock()
            .unwrap()
            .record("local", OperationStatus::Succeeded);
        assert_eq!(
            state.retained_operations.lock().unwrap().status("local"),
            Some(OperationStatus::Succeeded)
        );
        assert!(state.operations.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn cancellation_is_requested_until_native_completion_is_observed() {
        let (_dir, _state, registry, ctx, request) = fixture();
        let principal = ctx.principal.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let accepted = registry
            .start(ctx, request, move |_| async move {
                let _ = started_tx.send(());
                let _ = finish_rx.await;
                Ok(())
            })
            .unwrap();
        assert_eq!(accepted.status, OperationStatus::Accepted);
        started_rx.await.unwrap();
        let requested = registry
            .cancel(&principal, &accepted.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(requested.status, OperationStatus::Running);
        assert!(requested.cancellation_requested);
        finish_tx.send(()).unwrap();
        for _ in 0..100 {
            if registry
                .lookup(&principal, &accepted.operation_id)
                .unwrap()
                .unwrap()
                .status
                == OperationStatus::Succeeded
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("native success was not retained");
    }
    #[tokio::test]
    async fn failed_terminal_write_keeps_private_unknown_outcome_without_reexecution() {
        let (_dir, state, registry, ctx, request) = fixture();
        let principal = ctx.principal.clone();
        state.db.lock().unwrap().execute_batch("CREATE TRIGGER fail_terminal BEFORE UPDATE ON command_journal WHEN NEW.status IN ('succeeded','failed','cancelled') BEGIN SELECT RAISE(FAIL,'injected terminal failure'); END;").unwrap();
        let mut own = state
            .event_hub
            .subscribe_owner(Some(principal.device_id.clone()));
        let mut other = state
            .event_hub
            .subscribe_owner(Some(checked("other").unwrap()));
        let effects = Arc::new(AtomicUsize::new(0));
        let counter = effects.clone();
        let accepted = registry
            .start(ctx.clone(), request.clone(), move |_| async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        for _ in 0..100 {
            if state.operations.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(state.operations.lock().unwrap().is_empty());
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        let actual = registry
            .lookup(&principal, &accepted.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(actual.status, OperationStatus::OutcomeUnknown);
        assert_eq!(actual.error_code, Some(WireErrorCode::OutcomeUnknown));
        assert_eq!(
            registry
                .lookup_request(&principal, &ctx.request_id)
                .unwrap()
                .unwrap(),
            actual
        );
        assert_eq!(
            registry.recent(&principal).unwrap().items.as_slice()[0],
            actual
        );
        let counter = effects.clone();
        let retry = registry
            .start(ctx, request, move |_| async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        assert_eq!(retry.operation_id, accepted.operation_id);
        assert_eq!(retry.status, OperationStatus::OutcomeUnknown);
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        let other_principal = RemotePrincipal {
            device_id: checked("other").unwrap(),
            scopes: principal.scopes.clone(),
        };
        assert!(registry
            .lookup(&other_principal, &accepted.operation_id)
            .unwrap()
            .is_none());
        let other_identity =
            OperationRegistry::new(state.clone(), checked("other-server").unwrap());
        assert!(other_identity
            .lookup(&principal, &accepted.operation_id)
            .unwrap()
            .is_none());
        let events = own.drain().unwrap();
        assert!(
            matches!(events.last(),Some(RemoteEvent::Operations{data,..}) if data.items.as_slice()[0].status==OperationStatus::OutcomeUnknown)
        );
        assert!(other.drain().unwrap().iter().all(|event|matches!(event,RemoteEvent::Operations{data,..} if data.items.as_slice().is_empty())));
        let status: String = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT status FROM command_journal", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "running");
    }
}
