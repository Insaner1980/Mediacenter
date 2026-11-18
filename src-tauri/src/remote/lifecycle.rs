//! Confirmed owned-session teardown. Availability and operating-system power stay untouched.
use super::{
    adapters::checked,
    dispatch::{native_error, unavailable, value, Dispatcher},
    protocol::*,
};
use crate::{
    app_state::now_ms,
    contracts::AppError,
    control::{
        activity,
        confirmations::{Action, Pending},
        ControlServices,
    },
};
use std::{collections::BTreeSet, future::Future, time::Duration};

#[derive(Clone, Debug)]
pub struct EndSession {
    activity: activity::Identity,
    work: BTreeSet<String>,
    scans: BTreeSet<String>,
    // Bind effects to the admitted scan tokens, never a later scan at the same source ID.
    scan_tokens: Vec<tokio_util::sync::CancellationToken>,
}
impl PartialEq for EndSession {
    fn eq(&self, other: &Self) -> bool {
        self.activity == other.activity && self.work == other.work && self.scans == other.scans
    }
}
impl Eq for EndSession {}
async fn capture(s: &ControlServices, excluded: &[String]) -> Result<EndSession, RemoteError> {
    let activity = activity::identity(&s.state).await;
    let (scans, scan_tokens) = {
        let registry = s.state.scans.lock().map_err(|_| unavailable())?;
        (
            registry.keys().cloned().collect(),
            registry.values().cloned().collect(),
        )
    };
    Ok(EndSession {
        activity,
        work: s
            .state
            .operations
            .lock()
            .map_err(|_| unavailable())?
            .keys()
            .filter(|id| !excluded.contains(id))
            .cloned()
            .collect(),
        scans,
        scan_tokens,
    })
}
fn preview(d: &Dispatcher, owner: &RemotePrincipal) -> Result<ConfirmationPreview, RemoteError> {
    checked(
        serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":"end_session","affectedLabels":["Stop Mediacenter playback and cancel its pending work. Remote stays available."],"expected":{"serverEpoch":d.services.state.event_hub.epoch()},"expiresInMs":60000,"confirmMethod":"settings.endSession"}),
    )
}
fn cancel_captured_work(s: &ControlServices, saved: &EndSession) -> Result<(), AppError> {
    {
        let operations = s
            .state
            .operations
            .lock()
            .map_err(|_| AppError::new("unavailable", "Owned work unavailable."))?;
        for id in &saved.work {
            if let Some(token) = operations.get(id) {
                token.cancel();
            }
        }
    }
    for token in &saved.scan_tokens {
        token.cancel();
    }
    Ok(())
}
type EffectCheck = std::sync::Arc<dyn Fn() -> Result<(), AppError> + Send + Sync>;
pub(crate) trait SessionBackend: Clone + Send + Sync + 'static {
    fn stop(
        &self,
        services: &ControlServices,
        saved: EndSession,
        authorize: EffectCheck,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}
#[derive(Clone)]
struct Native;
impl SessionBackend for Native {
    async fn stop(
        &self,
        s: &ControlServices,
        saved: EndSession,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        let app = s
            .app()
            .ok_or_else(|| AppError::new("unavailable", "Native app unavailable."))?;
        authorize()?;
        cancel_captured_work(s, &saved)?;
        let stop = async {
            if let Some(id) = saved.activity.player {
                authorize()?;
                crate::player::mpv::control(
                    app,
                    &s.state,
                    &id,
                    crate::contracts::PlayerAction::Stop,
                )
                .await?;
            }
            if let Some(id) = saved.activity.photos {
                authorize()?;
                crate::photos::presenter::control_session(
                    app,
                    &s.state,
                    Some(&id),
                    crate::photos::presenter::SlideshowAction::Stop,
                )
                .await?;
            }
            if let Some(id) = saved.activity.browser {
                authorize()?;
                crate::streaming::sessions::close(app, &s.state, &id).await?;
            }
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let done = s
                        .state
                        .operations
                        .lock()
                        .is_ok_and(|ops| saved.work.iter().all(|id| !ops.contains_key(id)))
                        && s.state.scans.lock().is_ok_and(|scans| {
                            saved.scans.iter().all(|id| !scans.contains_key(id))
                        });
                    if done {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .map_err(|_| AppError::new("busy", "Owned work is still stopping."))?;
            Ok(())
        }
        .await;
        stop
    }
}
pub async fn dispatch(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    dispatch_with(d, owner, request, Native).await
}
pub(crate) async fn dispatch_with<B: SessionBackend>(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
    backend: B,
) -> Result<serde_json::Value, RemoteError> {
    Dispatcher::authorize(owner, &request)?;
    crate::control::settings::check_epoch(d, &request)?;
    let RemoteCommand::SettingsEndSession(params) = &request.command else {
        return Err(RemoteError::invalid());
    };
    let _music = d.services.state.music_gate.clone().lock_owned().await;
    let _session = d.services.state.session_gate.clone().lock_owned().await;
    // A request query/replay never re-executes native teardown, including after process restart.
    if let Some(existing) = d
        .services
        .operations()
        .lookup_request(owner, &request.id)
        .map_err(native_error)?
    {
        d.services
            .operations()
            .start(d.context(owner, &request), request.clone(), |_| async {
                Ok(())
            })
            .map_err(native_error)?;
        let registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
        if params.confirmation_id.is_none() {
            if let Some(p) = registry
                .pending
                .values()
                .find(|p| p.request == request.id && p.preview.owner_device_id == owner.device_id)
            {
                if let Some(operation_id) = &p.operation {
                    return accepted(operation_id.clone());
                }
                return value(ActivityPreparation::ConfirmationRequired {
                    preview: Box::new(
                        registry
                            .get(&owner.device_id, &p.preview.confirmation_id, now_ms())?
                            .preview,
                    ),
                });
            }
            return Err(RemoteError::new(
                ErrorCode::OutcomeUnknown,
                "The original end-session preview is unavailable. Refresh activity.",
            ));
        }
        return accepted(existing.operation_id);
    }
    let pending = params
        .confirmation_id
        .as_ref()
        .map(|id| {
            d.services
                .confirmations
                .lock()
                .map_err(|_| unavailable())?
                .get(&owner.device_id, id, now_ms())
        })
        .transpose()?;
    let mut excluded = vec![];
    if let Some(p) = &pending {
        if let Some(op) = d
            .services
            .operations()
            .lookup_request(owner, &p.request)
            .map_err(native_error)?
        {
            excluded.push(op.operation_id.as_str().into());
        }
    }
    let current = capture(&d.services, &excluded).await?;
    if let Some(pending) = pending {
        let Action::EndSession(saved) = pending.action else {
            return Err(RemoteError::invalid());
        };
        if pending.preview.expected.server_epoch.as_ref()
            != Some(&d.services.state.event_hub.epoch())
        {
            return Err(RemoteError::new(
                ErrorCode::ConfirmationStale,
                "Review the session again.",
            ));
        }
        if let Some(id) = pending.operation {
            return accepted(id);
        }
        if current != saved {
            let replacement = preview(d, owner)?;
            d.services
                .confirmations
                .lock()
                .map_err(|_| unavailable())?
                .insert(
                    Pending {
                        preview: replacement.clone(),
                        action: Action::EndSession(current.clone()),
                        observed: current.activity,
                        request: request.id.clone(),
                        expires_at: now_ms() + 60000,
                        operation: None,
                    },
                    now_ms(),
                )?;
            let mut error = RemoteError::new(
                ErrorCode::ConfirmationStale,
                "Owned playback or work changed. Review End session again.",
            );
            error.replacement_preview = Some(Box::new(replacement));
            return Err(error);
        }
        crate::control::settings::authorize_active(d, owner)?;
        let native = d.clone();
        let actor = owner.clone();
        let expected = request.clone();
        let id = params.confirmation_id.clone().unwrap();
        let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
        let current_pending = registry.get(&owner.device_id, &id, now_ms())?;
        if let Some(operation) = current_pending.operation {
            return accepted(operation);
        }
        let outcome = d
            .services
            .operations()
            .start(
                d.context(owner, &request),
                request,
                move |cancel| async move {
                    let _music = _music;
                    let _session = _session;
                    let idle = native.services.state.idle.clone();
                    let result = async {
                        if cancel.is_cancelled() {
                            return Err(AppError::new("cancelled", "End session cancelled."));
                        }
                        crate::control::settings::authorize_active(&native, &actor).map_err(
                            |_| AppError::new("forbidden", "Phone authorization changed."),
                        )?;
                        crate::control::settings::check_epoch(&native, &expected)
                            .map_err(|_| AppError::new("stale_state", "Session changed."))?;
                        if let Some(op) = native
                            .services
                            .operations()
                            .lookup_request(&actor, &expected.id)?
                        {
                            excluded.push(op.operation_id.as_str().into());
                        }
                        if capture(&native.services, &excluded).await.map_err(|_| {
                            AppError::new("unavailable", "Owned activity unavailable.")
                        })? != saved
                        {
                            return Err(AppError::new(
                                "stale_state",
                                "Owned playback or work changed.",
                            ));
                        }
                        let services = native.services.clone();
                        // Pending sheets authorize admission only. Accepted immutable work outlives
                        // preview expiry/removal; cancellation, epoch and live phone authority do not.
                        let authorize: EffectCheck = std::sync::Arc::new(move || {
                            if cancel.is_cancelled() {
                                return Err(AppError::new("cancelled", "End session cancelled."));
                            }
                            crate::control::settings::authorize_active(&native, &actor).map_err(
                                |_| AppError::new("forbidden", "Phone authorization changed."),
                            )?;
                            crate::control::settings::check_epoch(&native, &expected)
                                .map_err(|_| AppError::new("stale_state", "Session changed."))
                        });
                        authorize()?;
                        backend.stop(&services, saved, authorize).await
                    }
                    .await;
                    let released = idle.end().await;
                    result.and(released)
                },
            )
            .map_err(native_error)?;
        registry.pending.get_mut(id.as_str()).unwrap().operation =
            Some(outcome.operation_id.clone());
        accepted(outcome.operation_id)
    } else {
        let preview = preview(d, owner)?;
        let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
        registry.insert(
            Pending {
                preview: preview.clone(),
                action: Action::EndSession(current.clone()),
                observed: current.activity,
                request: request.id.clone(),
                expires_at: now_ms() + 60000,
                operation: None,
            },
            now_ms(),
        )?;
        if let Err(error) =
            d.services
                .operations()
                .start(d.context(owner, &request), request, |_| async { Ok(()) })
        {
            registry.pending.remove(preview.confirmation_id.as_str());
            return Err(native_error(error));
        }
        value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        })
    }
}
fn accepted(operation_id: Id) -> Result<serde_json::Value, RemoteError> {
    value(ActivityPreparation::Accepted {
        operation: OperationRef { operation_id },
    })
}

pub async fn set_start_at_login(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    use tauri::Manager;
    Dispatcher::authorize(owner, &request)?;
    crate::control::settings::check_epoch(d, &request)?;
    if let Some(replay) = d.activity_replay(owner, &request)? {
        return Ok(replay);
    }
    let RemoteCommand::SettingsStartAtLoginSet(params) = &request.command else {
        return Err(RemoteError::invalid());
    };
    let setup = d
        .services
        .app()
        .and_then(|app| app.try_state::<super::setup::RemoteSetup>())
        .ok_or_else(unavailable)?
        .inner()
        .clone();
    let enabled = params.enabled;
    let expected = request
        .expected
        .as_ref()
        .and_then(|v| v.settings_revision.as_ref())
        .map(Decimal::value);
    let actor = owner.clone();
    let outcome = d
        .services
        .operations()
        .start(d.context(owner, &request), request, move |_| async move {
            setup
                .set_lifetime_preference(Some(enabled), None, expected, Some(&actor))
                .await
                .map_err(|e| {
                    AppError::new(
                        match e.code {
                            ErrorCode::OutcomeUnknown => "outcome_unknown",
                            ErrorCode::StaleState => "stale_state",
                            ErrorCode::Forbidden => "forbidden",
                            _ => "unavailable",
                        },
                        "Remote startup preference could not be saved.",
                    )
                })
        })
        .map_err(native_error)?;
    value(AcceptedResult {
        status: OperationAccepted::Accepted,
        operation_id: outcome.operation_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    #[derive(Clone, Default)]
    struct Fake(Arc<AtomicUsize>);
    impl SessionBackend for Fake {
        async fn stop(
            &self,
            s: &ControlServices,
            _: EndSession,
            authorize: EffectCheck,
        ) -> Result<(), AppError> {
            authorize()?;
            self.0.fetch_add(1, Ordering::SeqCst);
            s.state.idle.end().await
        }
    }
    fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal) {
        let dir = tempfile::tempdir().unwrap();
        let services = ControlServices::native(
            crate::app_state::AppState::open(dir.path().into()).unwrap(),
            checked("server").unwrap(),
        )
        .unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["settings","browse"],"createdAtMs":1,"revoked":false})).unwrap();
        super::super::store::RemoteStore::new(
            &mut services.state.db.lock().unwrap(),
            &services.server_identity,
        )
        .add_device(
            &device,
            &super::super::store::CredentialHash::from_credential(
                &checked("a".repeat(43)).unwrap(),
            ),
        )
        .unwrap();
        (
            dir,
            Dispatcher::new(services),
            RemotePrincipal {
                device_id: device.device_id,
                scopes: device.scopes,
            },
        )
    }
    fn request(d: &Dispatcher, id: &str, confirm: Option<&str>) -> RemoteRequest {
        parse_request(&serde_json::json!({"v":1,"id":id,"method":"settings.endSession","params":confirm.map(|id|serde_json::json!({"confirmationId":id})).unwrap_or_else(||serde_json::json!({})),"expected":{"serverEpoch":d.services.state.event_hub.epoch()}}).to_string()).unwrap()
    }
    async fn terminal(d: &Dispatcher, owner: &RemotePrincipal, id: &str) -> OperationState {
        for _ in 0..100 {
            if let Some(op) = d
                .services
                .operations()
                .lookup_request(owner, &checked(id).unwrap())
                .unwrap()
            {
                if !matches!(
                    op.status,
                    OperationStatus::Accepted | OperationStatus::Running
                ) {
                    return op;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("operation did not settle")
    }
    #[tokio::test]
    async fn end_session_is_owned_confirmed_journaled_and_replay_never_reexecutes() {
        let (_dir, d, owner) = fixture();
        let fake = Fake::default();
        let prepared = dispatch_with(&d, &owner, request(&d, "prepare", None), fake.clone())
            .await
            .unwrap();
        let id = prepared["preview"]["confirmationId"].as_str().unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 0);
        terminal(&d, &owner, "prepare").await;
        let other = checked(serde_json::json!({"deviceId":"other","scopes":["browse","settings"]}))
            .unwrap();
        assert_eq!(
            dispatch_with(&d, &other, request(&d, "other", Some(id)), fake.clone())
                .await
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
        let confirm = request(&d, "confirm", Some(id));
        let accepted = dispatch_with(&d, &owner, confirm.clone(), fake.clone())
            .await
            .unwrap();
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(
            terminal(&d, &owner, "confirm").await.status,
            OperationStatus::Succeeded
        );
        assert_eq!(
            dispatch_with(&d, &owner, confirm, fake.clone())
                .await
                .unwrap(),
            accepted
        );
        assert_eq!(fake.0.load(Ordering::SeqCst), 1);
        assert!(!d.services.state.exit_pending.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn changed_owned_work_invalidates_confirmation_without_effect() {
        let (_dir, d, owner) = fixture();
        let fake = Fake::default();
        let p = dispatch_with(&d, &owner, request(&d, "prepare", None), fake.clone())
            .await
            .unwrap();
        terminal(&d, &owner, "prepare").await;
        d.services
            .state
            .scans
            .lock()
            .unwrap()
            .insert("new-scan".into(), Default::default());
        let error = dispatch_with(
            &d,
            &owner,
            request(&d, "confirm", p["preview"]["confirmationId"].as_str()),
            fake.clone(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ConfirmationStale);
        assert_eq!(fake.0.load(Ordering::SeqCst), 0);
    }
    #[derive(Default)]
    struct PausedState {
        entered: tokio::sync::Notify,
        resume: tokio::sync::Notify,
        effects: AtomicUsize,
    }
    #[derive(Clone)]
    struct Paused(Arc<PausedState>);
    impl SessionBackend for Paused {
        async fn stop(
            &self,
            _: &ControlServices,
            _: EndSession,
            authorize: EffectCheck,
        ) -> Result<(), AppError> {
            self.0.entered.notify_one();
            self.0.resume.notified().await;
            authorize()?;
            self.0.effects.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[tokio::test]
    async fn task20_revocation_during_native_wait_blocks_end_session_effect() {
        let (_dir, d, owner) = fixture();
        let paused = Arc::new(PausedState::default());
        let fake = Paused(paused.clone());
        let p = dispatch_with(&d, &owner, request(&d, "prepare", None), fake.clone())
            .await
            .unwrap();
        terminal(&d, &owner, "prepare").await;
        dispatch_with(
            &d,
            &owner,
            request(&d, "confirm", p["preview"]["confirmationId"].as_str()),
            fake,
        )
        .await
        .unwrap();
        paused.entered.notified().await;
        super::super::store::RemoteStore::new(
            &mut d.services.state.db.lock().unwrap(),
            &d.services.server_identity,
        )
        .revoke_device(&owner.device_id, now_ms() as i64)
        .unwrap();
        paused.resume.notify_one();
        let result = terminal(&d, &owner, "confirm").await;
        assert_eq!(result.status, OperationStatus::Failed);
        assert_eq!(result.error_code, Some(ErrorCode::Forbidden));
        assert_eq!(paused.effects.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn task20_accepted_end_session_outlives_expired_or_removed_preview_without_replay() {
        let (_dir, d, owner) = fixture();
        let paused = Arc::new(PausedState::default());
        let fake = Paused(paused.clone());
        let p = dispatch_with(&d, &owner, request(&d, "prepare", None), fake.clone())
            .await
            .unwrap();
        terminal(&d, &owner, "prepare").await;
        let confirm = request(&d, "confirm", p["preview"]["confirmationId"].as_str());
        let accepted = dispatch_with(&d, &owner, confirm.clone(), fake.clone())
            .await
            .unwrap();
        paused.entered.notified().await;
        d.services.confirmations.lock().unwrap().pending.clear();
        paused.resume.notify_one();
        assert_eq!(
            terminal(&d, &owner, "confirm").await.status,
            OperationStatus::Succeeded
        );
        assert_eq!(
            dispatch_with(&d, &owner, confirm, fake).await.unwrap(),
            accepted
        );
        assert_eq!(paused.effects.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn task20_unaccepted_sheet_cancellation_expiry_and_replacement_refuse_before_intent() {
        for mode in ["cancel", "expire", "replace"] {
            let (_dir, d, owner) = fixture();
            let fake = Fake::default();
            let p = dispatch_with(&d, &owner, request(&d, "prepare", None), fake.clone())
                .await
                .unwrap();
            terminal(&d, &owner, "prepare").await;
            let id = p["preview"]["confirmationId"].as_str().unwrap();
            let request = request(&d, "confirm", Some(id));
            let music = d.services.state.music.lock().await;
            let native = d.clone();
            let actor = owner.clone();
            let backend = fake.clone();
            let operation =
                tokio::spawn(async move { dispatch_with(&native, &actor, request, backend).await });
            tokio::time::timeout(Duration::from_secs(1), async {
                while d.services.state.session_gate.try_lock().is_ok() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            {
                let mut registry = d.services.confirmations.lock().unwrap();
                match mode {
                    "expire" => registry.pending.get_mut(id).unwrap().expires_at = now_ms(),
                    "replace" => {
                        let mut replacement = registry.pending.get(id).unwrap().clone();
                        replacement.preview.confirmation_id = checked("replacement").unwrap();
                        registry.insert(replacement, now_ms()).unwrap();
                    }
                    _ => {
                        registry.pending.remove(id);
                    }
                }
            }
            drop(music);
            let error = operation.await.unwrap().unwrap_err();
            assert_eq!(
                error.code,
                if mode == "expire" {
                    ErrorCode::ConfirmationExpired
                } else {
                    ErrorCode::NotFound
                }
            );
            assert_eq!(fake.0.load(Ordering::SeqCst), 0);
            assert!(d
                .services
                .operations()
                .lookup_request(&owner, &checked("confirm").unwrap())
                .unwrap()
                .is_none());
        }
    }
    #[tokio::test]
    async fn task20_replacement_scan_is_not_cancelled_by_captured_end_session() {
        let (_dir, d, _owner) = fixture();
        let original = tokio_util::sync::CancellationToken::new();
        d.services
            .state
            .scans
            .lock()
            .unwrap()
            .insert("source".into(), original.clone());
        let saved = capture(&d.services, &[]).await.unwrap();
        let replacement = tokio_util::sync::CancellationToken::new();
        d.services
            .state
            .scans
            .lock()
            .unwrap()
            .insert("source".into(), replacement.clone());
        cancel_captured_work(&d.services, &saved).unwrap();
        assert!(original.is_cancelled());
        assert!(!replacement.is_cancelled());
    }
    #[tokio::test]
    async fn absent_application_cannot_write_real_login_startup() {
        let (_dir, d, owner) = fixture();
        let r=parse_request(&serde_json::json!({"v":1,"id":"start","method":"settings.startAtLogin.set","params":{"enabled":true},"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":"1"}}).to_string()).unwrap();
        assert_eq!(
            set_start_at_login(&d, &owner, r).await.unwrap_err().code,
            ErrorCode::Unavailable
        );
        assert!(!crate::control::settings::read(&d).unwrap().start_at_login);
    }
}

/// The desktop launcher reveals the existing window. A repeated autostart invocation
/// must not reveal it, create services, or acquire a second listener registration.
pub fn background_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == "--remote-background")
}
#[cfg(test)]
mod launch_tests {
    #[test]
    fn task20_only_exact_background_flag_hides_initial_and_existing_ui() {
        assert!(super::background_requested([
            "mediacenter",
            "--remote-background"
        ]));
        assert!(!super::background_requested(["mediacenter"]));
        assert!(!super::background_requested([
            "mediacenter",
            "--remote-background=false"
        ]));
    }
}
