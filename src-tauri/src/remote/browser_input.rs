//! Native authority for browser launch, confirmation and transient tab input.
use super::{
    adapters::checked,
    browser_runtime::{self, Session},
    dispatch::{native_error, unavailable, value, Dispatcher},
    protocol::*,
};
use crate::{
    app_state::{now_ms, AppState},
    contracts::{AppError, ManagedSession, PlayerAction, ServiceId},
    control::{
        activity,
        confirmations::{Action, Pending},
    },
    streaming::catalog,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub enum BrowserApproval {
    Launch {
        service: ServiceId,
        profile: String,
    },
    Origin {
        session: Id,
        generation: Decimal,
        hostname: String,
    },
}

const METHODS: &[&str] = &[
    "browser.open",
    "browser.state",
    "browser.pointer",
    "browser.click",
    "browser.scroll",
    "browser.key",
    "browser.text",
    "browser.back",
    "browser.reload",
    "browser.close",
];

fn controls(available: bool) -> BrowserCapabilities {
    BrowserCapabilities {
        pointer: available,
        keyboard: available,
        text: available,
        semantic_pause: false,
        semantic_seek: false,
        semantic_volume: false,
    }
}

pub fn capabilities(mut base: Capabilities, state: &AppState) -> Capabilities {
    let mut methods = base.methods.as_slice().to_vec();
    methods.extend(
        METHODS
            .iter()
            .map(|name| checked(name).expect("browser method")),
    );
    base.methods = checked(methods).expect("bounded methods");
    base.services = checked(
        catalog::entries()
            .into_iter()
            .map(|entry| {
                let launchable =
                    state
                        .browser_runtime
                        .launchable(&state.data_dir, entry.id, "auto");
                ServiceCapability {
                    service_id: checked(catalog::key(entry.id)).expect("catalog key"),
                    support: if launchable {
                        SupportedState::Partial
                    } else {
                        SupportedState::Unsupported
                    },
                    browser: controls(launchable),
                    limitation: checked(if launchable {
                        "Website controls only. Provider playback and HDMI audio need verification."
                    } else {
                        "Set up the Chrome profile for this service."
                    })
                    .ok(),
                }
            })
            .collect::<Vec<_>>(),
    )
    .expect("bounded catalog");
    base.browser_bridge = base
        .services
        .as_slice()
        .iter()
        .any(|service| service.support == SupportedState::Partial);
    base
}

pub fn handles(command: &RemoteCommand) -> bool {
    METHODS.contains(&command.method())
}

pub fn motion(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::BrowserPointer(_) | RemoteCommand::BrowserScroll(_)
    )
}

pub async fn dispatch(
    d: &Dispatcher,
    principal: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<Value, RemoteError> {
    if matches!(&request.command, RemoteCommand::BrowserState(_)) {
        return value(state(d, principal).await?);
    }
    if request
        .expected
        .as_ref()
        .and_then(|e| e.server_epoch.as_ref())
        != Some(&d.services.state.event_hub.epoch())
    {
        return Err(browser_runtime::stale());
    }
    if matches!(&request.command, RemoteCommand::BrowserOpen(_)) {
        let result = open(d, principal, request).await;
        d.present_confirmations();
        return result;
    }
    input(d, principal, request).await
}

async fn state(d: &Dispatcher, principal: &RemotePrincipal) -> Result<BrowserState, RemoteError> {
    let runtime = &d.services.state.browser_runtime;
    let session = runtime
        .session()
        .filter(|session| session.owner == principal.device_id);
    let mut result = BrowserState {
        browser_session_id: None,
        target_generation: None,
        service_id: None,
        profile_id: None,
        enrolled: runtime.available(),
        capabilities: controls(false),
        origin_label: None,
        pending_confirmation: None,
    };
    let Some(session) = session else {
        return Ok(result);
    };
    result.browser_session_id = Some(session.id.clone());
    result.target_generation = Some(session.generation.clone());
    result.service_id = Some(checked(catalog::key(session.service))?);
    result.profile_id = Some(session.profile.clone());
    result.capabilities = controls(session.ready);
    result.origin_label = session
        .pending_origin
        .as_ref()
        .or(session.origin.as_ref())
        .map(checked)
        .transpose()?;
    if let Some(hostname) = session.pending_origin.as_ref() {
        let observed = activity::identity(&d.services.state).await;
        let mut confirmations = d.services.confirmations.lock().map_err(|_| unavailable())?;
        if let Some(existing) = confirmations.pending.values().find(|pending| {
            pending.preview.owner_device_id == principal.device_id
                && matches!(&pending.action, Action::Browser(BrowserApproval::Origin { session: id, generation, hostname: host }) if id == &session.id && generation == &session.generation && host == hostname)
        }) {
            // Dismissed/expired redirects must not silently re-open approval every poll.
            if existing.operation.is_none() && existing.expires_at > now_ms() {
                result.pending_confirmation = Some(confirmations.get(&principal.device_id, &existing.preview.confirmation_id, now_ms())?.preview);
            }
        } else if runtime.claim_origin_preview(&session) {
            let preview = preview(d, principal, ActionKind::ApproveBrowserOrigin, vec![hostname.clone()], Some(&session))?;
            let request = preview.confirmation_id.clone();
            confirmations.insert(Pending { preview: preview.clone(), action: Action::Browser(BrowserApproval::Origin {
                session: session.id, generation: session.generation, hostname: hostname.clone(),
            }), observed, request, expires_at: now_ms() + 60_000, operation: None }, now_ms())?;
            result.pending_confirmation = Some(preview);
        }
    }
    Ok(result)
}

fn preview(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    action: ActionKind,
    labels: Vec<String>,
    session: Option<&Session>,
) -> Result<ConfirmationPreview, RemoteError> {
    let browser_context = session
        .map(|session| -> Result<_, RemoteError> {
            Ok(BrowserApprovalContext {
                browser_session_id: session.id.clone(),
                target_generation: session.generation.clone(),
                hostname: checked(
                    session
                        .pending_origin
                        .as_deref()
                        .ok_or_else(browser_runtime::stale)?,
                )?,
            })
        })
        .transpose()?;
    Ok(ConfirmationPreview {
        confirmation_id: checked(uuid::Uuid::new_v4().to_string())?,
        owner_device_id: owner.device_id.clone(),
        action,
        affected_labels: checked(labels)?,
        expected: checked(json!({"serverEpoch":d.services.state.event_hub.epoch()}))?,
        expires_in_ms: checked(60_000)?,
        browser_context,
        confirm_method: ConfirmMethod::BrowserOpen,
    })
}

fn authorize_effect(
    services: &crate::control::ControlServices,
    owner: &RemotePrincipal,
) -> Result<(), AppError> {
    let mut db = services
        .state
        .db
        .lock()
        .map_err(|_| app_error(unavailable()))?;
    let devices =
        super::store::RemoteStore::new(&mut db, &services.server_identity).active_devices()?;
    if devices.iter().any(|device| {
        device.device_id == owner.device_id
            && device.scopes.as_slice().contains(&Scope::BrowserControl)
    }) {
        Ok(())
    } else {
        Err(AppError::new(
            "forbidden",
            "The phone no longer has browser control permission.",
        ))
    }
}

async fn preflight(d: &Dispatcher, service: ServiceId) -> Result<(), RemoteError> {
    let state = &d.services.state;
    let app = d.services.app().ok_or_else(unavailable)?;
    super::idle::require_unlocked().await?;
    if !crate::vpn::policy::requires_vpn(&state.snapshot().map_err(native_error)?.services, service)
    {
        return Ok(());
    }
    let observation = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        crate::vpn::nordvpn::refresh(app, state),
    )
    .await
    .map_err(|_| {
        RemoteError::new(
            ErrorCode::Unavailable,
            "VPN status could not be verified before opening this service.",
        )
    })?
    .map_err(native_error)?;
    crate::vpn::policy::evaluate_launch(true, &observation, now_ms()).map_err(native_error)
}

async fn open(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<Value, RemoteError> {
    let RemoteCommand::BrowserOpen(params) = &request.command else {
        unreachable!()
    };
    let state = &d.services.state;
    let confirmation = params.confirmation_id.clone();
    let music = state.music_gate.clone().lock_owned().await;
    let session = state.session_gate.clone().lock_owned().await;
    if let Some(replayed) = replay(d, owner, &request)? {
        return Ok(replayed);
    }
    let observed = activity::identity(state).await;
    let Some(confirmation) = confirmation else {
        let (Some(service), Some(requested)) = (&params.service_id, &params.profile_id) else {
            return Err(RemoteError::invalid());
        };
        let entry = catalog::entries()
            .into_iter()
            .find(|entry| catalog::key(entry.id) == service.as_str())
            .ok_or_else(|| {
                RemoteError::new(
                    ErrorCode::NotFound,
                    "This streaming service is unavailable.",
                )
            })?;
        let profile = match state.browser_runtime.profile(entry.id, requested.as_str()) {
            Ok(profile) => profile,
            Err(error)
                if !state.browser_runtime.launchable(
                    &state.data_dir,
                    entry.id,
                    requested.as_str(),
                ) =>
            {
                return Err(error)
            }
            Err(_) => requested.as_str().to_owned(),
        };
        preflight(d, entry.id).await?;
        authorize_effect(&d.services, owner).map_err(native_error)?;
        let mut labels = vec![entry.name];
        if observed.player.is_some() {
            labels.push("Current local playback".into());
        }
        if observed.photos.is_some() {
            labels.push("Current photo presentation".into());
        }
        if observed.browser.is_some() {
            labels.push("Current streaming session".into());
        }
        let preview = preview(d, owner, ActionKind::OpenBrowser, labels, None)?;
        let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
        registry.insert(
            Pending {
                preview: preview.clone(),
                action: Action::Browser(BrowserApproval::Launch {
                    service: entry.id,
                    profile,
                }),
                observed,
                request: request.id.clone(),
                expires_at: now_ms() + 60_000,
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
        return value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        });
    };
    let pending = d
        .services
        .confirmations
        .lock()
        .map_err(|_| unavailable())?
        .get(&owner.device_id, &confirmation, now_ms())?;
    let Action::Browser(action) = pending.action else {
        return Err(RemoteError::invalid());
    };
    if let Some(operation_id) = pending.operation {
        return accepted(operation_id);
    }
    if pending.observed != observed {
        return Err(browser_runtime::stale());
    }
    let services = d.services.clone();
    let principal = owner.clone();
    let context = d.context(owner, &request);
    let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
    let current = registry.get(&owner.device_id, &confirmation, now_ms())?;
    if let Some(operation_id) = current.operation {
        return accepted(operation_id);
    }
    let operation = services
        .operations()
        .start(context, request, move |cancel| async move {
            let (_music, _session) = (music, session);
            let app = services.app().ok_or_else(|| app_error(unavailable()))?;
            let state = &services.state;
            if cancel.is_cancelled() {
                return Err(AppError::new("cancelled", "Browser activity cancelled."));
            }
            if activity::identity(state).await != observed {
                return Err(app_error(browser_runtime::stale()));
            }
            authorize_effect(&services, &principal)?;
            match action {
                BrowserApproval::Launch { service, profile } => {
                    let d = Dispatcher::new(services.clone());
                    preflight(&d, service).await.map_err(app_error)?;
                    let profile = state
                        .browser_runtime
                        .prepare_profile(&state.data_dir, service, &profile, &cancel)
                        .await
                        .map_err(app_error)?;
                    if cancel.is_cancelled() {
                        return Err(AppError::new("cancelled", "Browser activity cancelled."));
                    }
                    activity::validate_identity(&observed, &activity::identity(state).await)?;
                    authorize_effect(&services, &principal)?;
                    stop_previous(app, state, &observed).await?;
                    let session = crate::vpn::policy::launch_after_preflight(
                        state,
                        service,
                        &cancel,
                        async {
                            tokio::time::timeout(
                                std::time::Duration::from_secs(3),
                                crate::vpn::nordvpn::refresh_locked(app, state),
                            )
                            .await
                            .map_err(|_| {
                                AppError::new(
                                    "unavailable",
                                    "VPN status could not be verified before opening this service.",
                                )
                            })?
                        },
                        async {
                            authorize_effect(&services, &principal)?;
                            state
                                .browser_runtime
                                .launch(principal.device_id, service, &profile, &cancel)
                                .await
                                .map_err(app_error)
                        },
                    )
                    .await?;
                    let id = session.id.as_str().to_owned();
                    crate::events::publish(app, state, |home| {
                        home.sessions.push(ManagedSession {
                            id,
                            kind: "streaming".into(),
                            service_id: Some(service),
                            media_id: None,
                            phase: "playing".into(),
                            target: Default::default(),
                        })
                    })?;
                    state.idle.begin_viewing();
                    Ok(())
                }
                BrowserApproval::Origin {
                    session,
                    generation,
                    hostname,
                } => {
                    super::idle::require_unlocked().await.map_err(app_error)?;
                    let target = state
                        .browser_runtime
                        .session()
                        .filter(|s| {
                            s.owner == principal.device_id
                                && s.id == session
                                && s.generation == generation
                                && s.pending_origin.as_deref() == Some(&hostname)
                        })
                        .ok_or_else(|| app_error(browser_runtime::stale()))?;
                    state
                        .browser_runtime
                        .command(
                            &target,
                            json!({"type":"approve","session":session,"generation":generation}),
                            &cancel,
                        )
                        .await
                        .map_err(app_error)
                }
            }
        })
        .map_err(native_error)?;
    registry
        .pending
        .get_mut(confirmation.as_str())
        .expect("confirmation held by registry guard")
        .operation = Some(operation.operation_id.clone());
    accepted(operation.operation_id)
}

fn replay(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: &RemoteRequest,
) -> Result<Option<Value>, RemoteError> {
    if d.services
        .operations()
        .lookup_request(owner, &request.id)
        .map_err(native_error)?
        .is_none()
    {
        return Ok(None);
    }
    let operation = d
        .services
        .operations()
        .start(d.context(owner, request), request.clone(), |_| async {
            Ok(())
        })
        .map_err(native_error)?;
    let RemoteCommand::BrowserOpen(params) = &request.command else {
        unreachable!()
    };
    if params.confirmation_id.is_some() {
        return accepted(operation.operation_id).map(Some);
    }
    let registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
    let previous = registry
        .pending
        .values()
        .find(|p| p.request == request.id && p.preview.owner_device_id == owner.device_id)
        .ok_or_else(|| RemoteError::new(ErrorCode::OutcomeUnknown, "Prepare the service again."))?;
    if let Some(operation) = previous.operation.clone() {
        return accepted(operation).map(Some);
    }
    value(ActivityPreparation::ConfirmationRequired {
        preview: Box::new(
            registry
                .get(
                    &owner.device_id,
                    &previous.preview.confirmation_id,
                    now_ms(),
                )?
                .preview,
        ),
    })
    .map(Some)
}

fn accepted(operation_id: Id) -> Result<Value, RemoteError> {
    value(ActivityPreparation::Accepted {
        operation: OperationRef { operation_id },
    })
}

async fn stop_previous(
    app: &tauri::AppHandle,
    state: &AppState,
    observed: &activity::Identity,
) -> Result<(), AppError> {
    if let Some(id) = &observed.player {
        crate::player::mpv::control(app, state, id, PlayerAction::Stop).await?;
    }
    if let Some(id) = &observed.photos {
        crate::photos::presenter::control_session(
            app,
            state,
            Some(id),
            crate::photos::presenter::SlideshowAction::Stop,
        )
        .await?;
    }
    if let Some(id) = &observed.browser {
        crate::streaming::sessions::close(app, state, id).await?;
    }
    Ok(())
}

fn wire(command: &RemoteCommand) -> Option<(&Id, &Decimal, Value)> {
    let (id, generation, input) = match command {
        RemoteCommand::BrowserPointer(p) => (
            &p.browser_session_id,
            &p.target_generation,
            json!({"type":"pointer","x":p.x,"y":p.y,"inputSequence":p.input_sequence}),
        ),
        RemoteCommand::BrowserClick(p) => (
            &p.browser_session_id,
            &p.target_generation,
            json!({"type":"click","x":p.x,"y":p.y,"button":p.button}),
        ),
        RemoteCommand::BrowserScroll(p) => (
            &p.browser_session_id,
            &p.target_generation,
            json!({"type":"scroll","deltaX":p.delta_x,"deltaY":p.delta_y,"inputSequence":p.input_sequence}),
        ),
        RemoteCommand::BrowserKey(p) => (
            &p.browser_session_id,
            &p.target_generation,
            json!({"type":"key","key":p.key}),
        ),
        RemoteCommand::BrowserText(p) => (
            &p.browser_session_id,
            &p.target_generation,
            json!({"type":"text","text":p.text}),
        ),
        RemoteCommand::BrowserBack(p) => {
            return Some((
                &p.browser_session_id,
                &p.target_generation,
                json!({"type":"back","session":p.browser_session_id,"generation":p.target_generation}),
            ))
        }
        RemoteCommand::BrowserReload(p) => {
            return Some((
                &p.browser_session_id,
                &p.target_generation,
                json!({"type":"reload","session":p.browser_session_id,"generation":p.target_generation}),
            ))
        }
        RemoteCommand::BrowserClose(p) => {
            return Some((
                &p.browser_session_id,
                &p.target_generation,
                json!({"type":"close","session":p.browser_session_id,"generation":p.target_generation}),
            ))
        }
        _ => return None,
    };
    Some((
        id,
        generation,
        json!({"type":"input","session":id,"generation":generation,"input":input}),
    ))
}

async fn input(
    d: &Dispatcher,
    principal: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<Value, RemoteError> {
    let state = &d.services.state;
    let (id, generation, command) = wire(&request.command).ok_or_else(RemoteError::invalid)?;
    let closing = matches!(&request.command, RemoteCommand::BrowserClose(_));
    let target = state
        .browser_runtime
        .session()
        .filter(|s| {
            s.owner == principal.device_id
                && &s.id == id
                && &s.generation == generation
                && (s.ready || closing)
        })
        .ok_or_else(browser_runtime::stale)?;
    super::idle::require_unlocked().await?;
    authorize_effect(&d.services, principal).map_err(native_error)?;
    if closing {
        let _session = state.session_gate.try_lock().map_err(|_| {
            RemoteError::new(ErrorCode::Busy, "Another playback change is in progress.")
        })?;
        let app = d.services.app().ok_or_else(unavailable)?;
        state
            .browser_runtime
            .command(&target, command, &CancellationToken::new())
            .await?;
        retire(app, state, &target).map_err(native_error)?;
    } else {
        state
            .browser_runtime
            .command(&target, command, &CancellationToken::new())
            .await?;
    }
    value(BrowserAck {
        request_id: request.id,
        browser_session_id: id.clone(),
        target_generation: generation.clone(),
    })
}

pub async fn close(app: &tauri::AppHandle, state: &AppState, id: &str) -> Result<(), AppError> {
    let session = state
        .browser_runtime
        .session()
        .filter(|s| s.id.as_str() == id)
        .ok_or_else(|| app_error(browser_runtime::stale()))?;
    state
        .browser_runtime
        .command(
            &session,
            json!({"type":"close","session":session.id,"generation":session.generation}),
            &CancellationToken::new(),
        )
        .await
        .map_err(app_error)?;
    retire(app, state, &session)
}

fn retire(app: &tauri::AppHandle, state: &AppState, session: &Session) -> Result<(), AppError> {
    state.browser_runtime.retire(session);
    crate::events::publish(app, state, |home| {
        home.sessions
            .retain(|current| current.id != session.id.as_str())
    })
}

pub async fn focus(state: &AppState, id: &str) -> Result<(), AppError> {
    let session = state
        .browser_runtime
        .session()
        .filter(|s| s.id.as_str() == id)
        .ok_or_else(|| app_error(browser_runtime::stale()))?;
    super::idle::require_unlocked().await.map_err(app_error)?;
    state
        .browser_runtime
        .command(
            &session,
            json!({"type":"focus","session":session.id,"generation":session.generation}),
            &CancellationToken::new(),
        )
        .await
        .map_err(app_error)
}

fn app_error(error: RemoteError) -> AppError {
    let code = serde_json::to_value(&error.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unavailable".into());
    AppError::new(&code, error.message.as_str())
}
