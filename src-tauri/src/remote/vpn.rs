//! VPN remote admission keeps the native provider and launch policy authoritative.
use super::{
    adapters::checked,
    dispatch::{native_error, unavailable, value, Dispatcher},
    protocol::*,
};
use crate::{
    app_state::now_ms,
    contracts::{AppError, CountryOption, VpnSnapshot},
    control::{
        activity,
        confirmations::{Action, Pending},
        ControlServices,
    },
};
use std::future::Future;
use tokio_util::sync::CancellationToken;

pub(crate) type EffectCheck = std::sync::Arc<dyn Fn() -> Result<(), AppError> + Send + Sync>;

pub(crate) trait VpnBackend: Clone + Send + Sync + 'static {
    fn observe(
        &self,
        services: &ControlServices,
    ) -> impl Future<Output = Result<VpnSnapshot, AppError>> + Send;
    fn countries(&self) -> impl Future<Output = Result<Vec<CountryOption>, AppError>> + Send;
    fn change(
        &self,
        services: &ControlServices,
        action: VpnPrepareChangeParams,
        cancel: CancellationToken,
        authorize: EffectCheck,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}
#[derive(Clone)]
struct Native;
impl VpnBackend for Native {
    async fn observe(&self, services: &ControlServices) -> Result<VpnSnapshot, AppError> {
        let observed = crate::vpn::nordvpn::observe_locked(&services.state).await?;
        let publication =
            crate::events::commit(&services.state, |home| home.vpn = observed.clone())?;
        if let Some(app) = services.app() {
            publication.emit(app, &services.state);
        }
        Ok(observed)
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        crate::vpn::nordvpn::countries_locked().await
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        c: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        let action = match p.action {
            VpnAction::Connect => {
                crate::vpn::nordvpn::Action::Connect(p.country_id.map(|id| id.as_str().into()))
            }
            VpnAction::Disconnect => crate::vpn::nordvpn::Action::Disconnect,
        };
        crate::vpn::nordvpn::change_locked_checked(
            s.app()
                .ok_or_else(|| AppError::new("unavailable", "Native provider unavailable."))?,
            &s.state,
            action,
            c,
            move || authorize(),
        )
        .await
        .map_err(normalize)
    }
}
fn normalize(mut error: AppError) -> AppError {
    error.code = match error.code.as_str() {
        "vpn_busy" => "busy",
        "invalid_country" => "invalid_request",
        "cancelled" | "timeout" | "vpn_not_confirmed" => "outcome_unknown",
        _ => "unavailable",
    }
    .into();
    error
}
#[derive(Clone)]
pub struct VpnChange {
    pub params: VpnPrepareChangeParams,
    observed: VpnSnapshot,
    activity: activity::Identity,
    preferences: serde_json::Value,
    generation: (u64, String),
}
fn preferences(s: &ControlServices) -> Result<serde_json::Value, RemoteError> {
    let home = s.state.snapshot().map_err(native_error)?;
    Ok(serde_json::json!([
        home.theme,
        home.services
            .iter()
            .map(|s| serde_json::json!([s.id, s.vpn_required]))
            .collect::<Vec<_>>()
    ]))
}
fn same(a: &VpnChange, b: &VpnChange) -> bool {
    a.observed.state == b.observed.state
        && a.observed.country == b.observed.country
        && a.observed.kill_switch == b.observed.kill_switch
        && a.observed.error_code == b.observed.error_code
        && a.activity == b.activity
        && a.preferences == b.preferences
        && a.generation == b.generation
}
pub(crate) fn fresh(v: &VpnSnapshot, now: u64) -> bool {
    v.observed_at_ms > 0
        && now >= v.observed_at_ms
        && now - v.observed_at_ms <= 10000
        && v.error_code.is_none()
}
fn route(s: &ControlServices, owner: &Id) -> Result<(u64, String), RemoteError> {
    let net = s.network.lock().map_err(|_| unavailable())?;
    if !net.reachable(owner) {
        return Err(RemoteError::new(
            ErrorCode::Unavailable,
            "Refresh the phone connection before changing VPN.",
        ));
    }
    Ok((
        net.generation,
        net.connection(owner).ok_or_else(unavailable)?.into(),
    ))
}
async fn capture<B: VpnBackend>(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    b: &B,
    params: VpnPrepareChangeParams,
) -> Result<VpnChange, RemoteError> {
    crate::control::settings::authorize_active(d, owner)?;
    let generation = route(&d.services, &owner.device_id)?;
    let observed = b
        .observe(&d.services)
        .await
        .map_err(|e| native_error(normalize(e)))?;
    if !fresh(&observed, now_ms()) {
        return Err(unavailable());
    }
    if params.action == VpnAction::Disconnect && params.country_id.is_some() {
        return Err(RemoteError::invalid());
    }
    if let Some(country) = &params.country_id {
        if !b
            .countries()
            .await
            .map_err(|e| native_error(normalize(e)))?
            .iter()
            .any(|c| c.id == country.as_str())
        {
            return Err(RemoteError::invalid());
        }
    }
    let activity = activity::identity(&d.services.state).await;
    crate::control::settings::authorize_active(d, owner)?;
    if route(&d.services, &owner.device_id)? != generation {
        return Err(unavailable());
    }
    Ok(VpnChange {
        params,
        observed,
        activity,
        preferences: preferences(&d.services)?,
        generation,
    })
}
fn preview(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    action: &VpnChange,
) -> Result<ConfirmationPreview, RemoteError> {
    let change = match action.params.action {
        VpnAction::Disconnect => "Disconnect NordVPN".to_owned(),
        VpnAction::Connect => format!(
            "Connect NordVPN: {}",
            action
                .params
                .country_id
                .as_ref()
                .map(|id| id.as_str().replace('_', " "))
                .unwrap_or_else(|| "recommended country".into())
        ),
    };
    checked(
        serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":"change_vpn","affectedLabels":[change,"Streaming and the phone connection may be interrupted.","Kill Switch is preserved. Reconnect to inspect this operation; do not repeat the change."],"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":d.services.state.snapshot().map_err(native_error)?.revision.to_string()},"expiresInMs":60000,"confirmMethod":"vpn.confirmChange"}),
    )
}
fn pending(preview: ConfirmationPreview, action: VpnChange, request: Id) -> Pending {
    Pending {
        preview,
        action: Action::Vpn(action),
        observed: Default::default(),
        request,
        expires_at: now_ms() + 60000,
        operation: None,
    }
}
pub fn handles(c: &RemoteCommand) -> bool {
    matches!(
        c,
        RemoteCommand::VpnState(_)
            | RemoteCommand::VpnCountries(_)
            | RemoteCommand::VpnPrepareChange(_)
            | RemoteCommand::VpnConfirmChange(_)
    )
}
pub async fn dispatch(
    d: &Dispatcher,
    o: &RemotePrincipal,
    r: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    if matches!(r.command, RemoteCommand::VpnState(_)) && d.services.app().is_some() {
        probe_version(d.services.clone());
    }
    dispatch_with(d, o, r, Native).await
}
fn probe_version(services: ControlServices) {
    {
        let Ok(mut last) = services.vpn_version_probe.try_lock() else {
            return;
        };
        if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
            return;
        }
        *last = Some(std::time::Instant::now());
    }
    // Optional metadata is independently bounded and cannot delay a valid observation.
    tauri::async_runtime::spawn(async move {
        let version = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            crate::vpn::nordvpn::version(),
        )
        .await
        .ok()
        .flatten();
        if let Ok(mut network) = services.network.lock() {
            network.diagnostics.vpn_version = version.and_then(|v| checked(v).ok());
        }
    });
}
pub(crate) async fn dispatch_with<B: VpnBackend>(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
    backend: B,
) -> Result<serde_json::Value, RemoteError> {
    tokio::time::timeout(std::time::Duration::from_secs(3),dispatch_admitted(d,owner,request,backend)).await
        .map_err(|_|RemoteError::new(ErrorCode::Unavailable,"VPN observation is taking too long. Routine settings remain available; refresh VPN later."))?
}
async fn dispatch_admitted<B: VpnBackend>(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
    backend: B,
) -> Result<serde_json::Value, RemoteError> {
    Dispatcher::authorize(owner, &request)?;
    let _permit = d
        .work
        .acquire(owner.device_id.as_str(), 8)
        .ok_or_else(unavailable)?;
    let _settings = d.services.settings_gate.lock().await;
    if matches!(
        request.command,
        RemoteCommand::VpnState(_) | RemoteCommand::VpnCountries(_)
    ) {
        let _vpn = d.services.state.vpn_gate.lock().await;
        return if matches!(request.command, RemoteCommand::VpnCountries(_)) {
            let items = backend
                .countries()
                .await
                .map_err(|e| native_error(normalize(e)))?;
            value(checked::<VpnCountryList, _>(
                serde_json::json!({"items":items.iter().map(|c|serde_json::json!({"id":c.id,"label":c.name})).collect::<Vec<_>>()}),
            )?)
        } else {
            backend
                .observe(&d.services)
                .await
                .map_err(|e| native_error(normalize(e)))?;
            value(super::state::vpn_topic(
                &d.services.state.snapshot().map_err(native_error)?,
                d.services.state.event_hub.revision(Topic::Vpn),
            )?)
        };
    }
    crate::control::settings::check_epoch(d, &request)?;
    if let Some(replay) = d.activity_replay(owner, &request)? {
        return Ok(replay);
    }
    let _sessions = d.services.state.session_gate.lock().await;
    let _vpn = d.services.state.vpn_gate.lock().await;
    if let RemoteCommand::VpnPrepareChange(params) = &request.command {
        crate::control::settings::check_revision(d, &request)?;
        let action = capture(d, owner, &backend, params.clone()).await?;
        let preview = preview(d, owner, &action)?;
        let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
        registry.insert(
            pending(preview.clone(), action, request.id.clone()),
            now_ms(),
        )?;
        if let Err(e) =
            d.services
                .operations()
                .start(d.context(owner, &request), request, |_| async { Ok(()) })
        {
            registry.pending.remove(preview.confirmation_id.as_str());
            return Err(native_error(e));
        }
        return value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        });
    }
    let RemoteCommand::VpnConfirmChange(params) = &request.command else {
        return Err(RemoteError::invalid());
    };
    let old = d
        .services
        .confirmations
        .lock()
        .map_err(|_| unavailable())?
        .get(&owner.device_id, &params.confirmation_id, now_ms())?;
    if old.preview.expected.server_epoch.as_ref() != Some(&d.services.state.event_hub.epoch()) {
        return Err(RemoteError::invalid());
    }
    if let Some(operation_id) = old.operation {
        return value(AcceptedResult {
            status: OperationAccepted::Accepted,
            operation_id,
        });
    }
    let Action::Vpn(saved) = old.action else {
        return Err(RemoteError::invalid());
    };
    let current = capture(d, owner, &backend, saved.params.clone()).await?;
    if !same(&saved, &current) {
        let replacement = preview(d, owner, &current)?;
        d.services
            .confirmations
            .lock()
            .map_err(|_| unavailable())?
            .insert(
                pending(replacement.clone(), current, request.id.clone()),
                now_ms(),
            )?;
        return Err(RemoteError {
            code: ErrorCode::ConfirmationStale,
            message: checked("VPN, playback or connection changed. Review again.")?,
            retryable: false,
            refresh: checked(Vec::<Topic>::new())?,
            replacement_preview: Some(Box::new(replacement)),
        });
    }
    let native = d.clone();
    let actor = owner.clone();
    let confirmation = params.confirmation_id.clone();
    let outcome = d
        .services
        .operations()
        .start(
            d.context(owner, &request),
            request,
            move |cancel| async move {
                let _settings = native.services.settings_gate.lock().await;
                let _sessions = native.services.state.session_gate.lock().await;
                let _vpn = native.services.state.vpn_gate.lock().await;
                if cancel.is_cancelled() {
                    return Err(AppError::new(
                        "cancelled",
                        "Cancelled before provider effect.",
                    ));
                }
                let current = capture(&native, &actor, &backend, saved.params.clone())
                    .await
                    .map_err(|_| AppError::new("stale_state", "VPN authorization changed."))?;
                if !same(&saved, &current) {
                    return Err(AppError::new(
                        "stale_state",
                        "VPN state changed before effect.",
                    ));
                }
                if route(&native.services, &actor.device_id)
                    .map_err(|_| AppError::new("unavailable", "Phone route expired."))?
                    != saved.generation
                {
                    return Err(AppError::new("stale_state", "Phone connection changed."));
                }
                let authority = native.clone();
                let generation = saved.generation;
                let authorize: EffectCheck = std::sync::Arc::new(move || {
                    crate::control::settings::authorize_active(&authority, &actor)
                        .map_err(|_| AppError::new("stale_state", "VPN authorization changed."))?;
                    if route(&authority.services, &actor.device_id)
                        .map_err(|_| AppError::new("unavailable", "Phone route expired."))?
                        != generation
                    {
                        return Err(AppError::new("stale_state", "Phone connection changed."));
                    }
                    Ok(())
                });
                backend
                    .change(&native.services, saved.params, cancel, authorize)
                    .await
            },
        )
        .map_err(native_error)?;
    d.services
        .confirmations
        .lock()
        .map_err(|_| unavailable())?
        .pending
        .get_mut(confirmation.as_str())
        .unwrap()
        .operation = Some(outcome.operation_id.clone());
    value(AcceptedResult {
        status: OperationAccepted::Accepted,
        operation_id: outcome.operation_id,
    })
}

#[cfg(test)]
mod tests;
