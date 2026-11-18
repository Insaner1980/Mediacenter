//! Phone drive admission; physical effects reuse device actions and SourceLifecycle.
use super::confirmations::{Action, Pending};
use crate::{
    app_state::now_ms,
    contracts::AppError,
    devices::actions::{DeviceBackend, RemoteUDisksBackend, SourceLifecycle},
    remote::{
        adapters::checked,
        dispatch::{native_error, unavailable, value, Dispatcher},
        drives::{self, EjectAction},
        protocol::*,
    },
};

pub fn handles(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::DrivesList(_)
            | RemoteCommand::DrivesMount(_)
            | RemoteCommand::DrivesPrepareEject(_)
            | RemoteCommand::DrivesConfirmEject(_)
    )
}
#[derive(Clone)]
struct NativeLifecycle(super::ControlServices);
impl SourceLifecycle for NativeLifecycle {
    type Guard = crate::library::resources::ResourceBlock;
    async fn quiesce(
        &self,
        volumes: &[crate::devices::registry::Volume],
    ) -> Result<Self::Guard, AppError> {
        let app = self
            .0
            .app()
            .ok_or_else(|| AppError::new("unavailable", "Native lifecycle unavailable."))?;
        let sources = crate::lifecycle::affected_sources(&self.0.state, volumes)?;
        crate::lifecycle::stop_sources(app, &self.0.state, &sources).await
    }
}
pub async fn dispatch(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    dispatch_with(
        d,
        owner,
        request,
        RemoteUDisksBackend,
        NativeLifecycle(d.services.clone()),
    )
    .await
}
fn preview(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    action: &EjectAction,
) -> Result<ConfirmationPreview, RemoteError> {
    checked(
        serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":"eject_drive","affectedLabels":action.labels,"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"sourcesRevision":d.services.state.event_hub.revision(Topic::SourcesSummary).to_string()},"expiresInMs":60000,"confirmMethod":"drives.confirmEject"}),
    )
}
fn pending(preview: ConfirmationPreview, action: EjectAction, request: Id) -> Pending {
    Pending {
        preview,
        action: Action::Eject(action),
        observed: Default::default(),
        request,
        expires_at: now_ms() + 60000,
        operation: None,
    }
}
fn error(error: AppError) -> RemoteError {
    native_error(drives::normalize(error))
}

pub(crate) async fn dispatch_with<B, L>(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
    backend: B,
    lifecycle: L,
) -> Result<serde_json::Value, RemoteError>
where
    B: DeviceBackend + Clone + Send + 'static,
    L: SourceLifecycle + Clone + Send + 'static,
{
    Dispatcher::authorize(owner, &request)?;
    if matches!(request.command, RemoteCommand::DrivesList(_)) {
        let _permit = d
            .work
            .acquire(owner.device_id.as_str(), 8)
            .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Drive discovery is busy."))?;
        return value(drives::inventory(
            &backend.discover().await.map_err(error)?,
            d.services.state.event_hub.revision(Topic::SourcesSummary),
        )?);
    }
    if request
        .expected
        .as_ref()
        .and_then(|e| e.server_epoch.as_ref())
        != Some(&d.services.state.event_hub.epoch())
    {
        return Err(RemoteError::new(
            ErrorCode::StaleState,
            "Refresh drive state.",
        ));
    }
    let _admission = d.services.source_gate.lock().await;
    if let Some(replay) = d.activity_replay(owner, &request)? {
        return Ok(replay);
    }
    let permit = d
        .work
        .acquire(owner.device_id.as_str(), 8)
        .ok_or_else(|| RemoteError::new(ErrorCode::Busy, "Drive operations are busy."))?;
    let devices = backend.discover().await.map_err(error)?;
    if let RemoteCommand::DrivesPrepareEject(p) = &request.command {
        let action = drives::capture(
            &d.services.state,
            &devices,
            &devices
                .drives
                .iter()
                .find(|drive| drives::drive_handle(drive) == p.drive_id.as_str())
                .ok_or_else(|| {
                    RemoteError::new(ErrorCode::NotFound, "Drive unavailable. Refresh drives.")
                })?
                .id,
        )
        .await
        .map_err(error)?;
        let preview = preview(d, owner, &action)?;
        let mut confirmations = d.services.confirmations.lock().map_err(|_| unavailable())?;
        confirmations.insert(
            pending(preview.clone(), action, request.id.clone()),
            now_ms(),
        )?;
        if let Err(e) =
            d.services
                .operations()
                .start(d.context(owner, &request), request, |_| async { Ok(()) })
        {
            confirmations
                .pending
                .remove(preview.confirmation_id.as_str());
            return Err(native_error(e));
        }
        return value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        });
    }
    let (action, confirmation, mount) = match &request.command {
        RemoteCommand::DrivesMount(p) => {
            let volume = devices
                .volumes
                .iter()
                .find(|v| {
                    devices.drives.iter().any(|drive| {
                        drive.id == v.drive_id
                            && drives::volume_handle(v, drive) == p.volume_id.as_str()
                    })
                })
                .ok_or_else(|| RemoteError::new(ErrorCode::NotFound, "Volume unavailable."))?;
            (
                drives::capture(&d.services.state, &devices, &volume.drive_id)
                    .await
                    .map_err(error)?,
                None,
                Some(volume.id.clone()),
            )
        }
        RemoteCommand::DrivesConfirmEject(p) => {
            let old = d
                .services
                .confirmations
                .lock()
                .map_err(|_| unavailable())?
                .get(&owner.device_id, &p.confirmation_id, now_ms())?;
            if old.preview.confirm_method != ConfirmMethod::DrivesConfirmEject
                || old.preview.expected.server_epoch.as_ref()
                    != Some(&d.services.state.event_hub.epoch())
            {
                return Err(RemoteError::invalid());
            }
            if let Some(operation_id) = old.operation {
                return value(AcceptedResult {
                    status: OperationAccepted::Accepted,
                    operation_id,
                });
            }
            let Action::Eject(action) = old.action else {
                return Err(RemoteError::invalid());
            };
            let fresh = drives::capture(&d.services.state, &devices, &action.drive_id)
                .await
                .map_err(error)?;
            if !drives::same(&action, &fresh) {
                let preview = preview(d, owner, &fresh)?;
                d.services
                    .confirmations
                    .lock()
                    .map_err(|_| unavailable())?
                    .insert(pending(preview.clone(), fresh, old.request), now_ms())?;
                let mut stale = RemoteError::new(
                    ErrorCode::ConfirmationStale,
                    "The drive or its active use changed. Review removal again.",
                );
                stale.replacement_preview = Some(Box::new(preview));
                return Err(stale);
            }
            (action, Some(p.confirmation_id.clone()), None)
        }
        _ => return Err(RemoteError::invalid()),
    };
    let services = d.services.clone();
    let registration = d.registration.clone();
    // Keep the confirmation mutex through admission so desktop/phone cancellation cannot win after validation.
    let mut confirmations = d.services.confirmations.lock().map_err(|_| unavailable())?;
    if let Some(id) = &confirmation {
        confirmations.get(&owner.device_id, id, now_ms())?;
    }
    let outcome = d
        .services
        .operations()
        .start(
            d.context(owner, &request),
            request,
            move |cancel| async move {
                let _permit = permit;
                let _registration = registration;
                let _gate = services
                    .state
                    .device_gate
                    .try_lock()
                    .map_err(|_| AppError::new("busy", "Device operation already running."))?;
                let _sessions = services.state.session_gate.lock().await;
                if cancel.is_cancelled() {
                    return Err(AppError::new("cancelled", "Drive operation cancelled."));
                }
                let result = match mount {
                    Some(volume) => {
                        let fresh = drives::capture(
                            &services.state,
                            &backend.discover().await.map_err(drives::normalize)?,
                            &action.drive_id,
                        )
                        .await
                        .map_err(drives::normalize)?;
                        if !drives::same(&action, &fresh) {
                            Err(AppError::new(
                                "confirmation_stale",
                                "Drive changed. Refresh drives.",
                            ))
                        } else {
                            crate::devices::actions::mount_group(&backend, &volume, &action.group)
                                .await
                                .map(|_| ())
                        }
                    }
                    _ => {
                        drives::eject(&services.state, &backend, &lifecycle, &action, &cancel).await
                    }
                };
                let result = if let Some(app) = services.app() {
                    crate::lifecycle::with_reconciliation(
                        result,
                        crate::lifecycle::reconcile_sources(app, &services.state).await,
                    )
                } else {
                    result
                };
                result.map_err(drives::normalize)
            },
        )
        .map_err(native_error)?;
    if let Some(id) = confirmation {
        if let Some(p) = confirmations.pending.get_mut(id.as_str()) {
            p.operation = Some(outcome.operation_id.clone());
        }
    }
    value(AcceptedResult {
        status: OperationAccepted::Accepted,
        operation_id: outcome.operation_id,
    })
}
