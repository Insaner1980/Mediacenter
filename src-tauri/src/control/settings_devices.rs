//! Confirmed phone administration using the existing secret-free journal boundary.
use super::confirmations::{Action, Pending};
use crate::{
    app_state::now_ms,
    remote::{
        adapters::checked,
        dispatch::{native_error, unavailable, value, Dispatcher},
        protocol::*,
        store::{CommandAcceptance, JournalOutcome, RemoteStore},
    },
};

#[derive(Clone)]
pub struct AdminAction {
    pub target: Option<Id>,
    pub devices: Vec<PairedDevice>,
    pub generation: u64,
}
fn same_devices(a: &[PairedDevice], b: &[PairedDevice]) -> bool {
    let normalized = |items: &[PairedDevice]| {
        items
            .iter()
            .map(|d| serde_json::json!([d.device_id, d.label, d.scopes, d.created_at_ms]))
            .collect::<Vec<_>>()
    };
    normalized(a) == normalized(b)
}
fn preview(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    action: &AdminAction,
) -> Result<ConfirmationPreview, RemoteError> {
    let (kind, method, labels) = if let Some(target) = &action.target {
        let device = action
            .devices
            .iter()
            .find(|v| &v.device_id == target)
            .ok_or_else(|| RemoteError::new(ErrorCode::NotFound, "Phone no longer paired."))?;
        (
            "revoke_device",
            "devices.revoke",
            vec![
                device.label.as_str().to_owned(),
                if target == &owner.device_id {
                    "This phone will disconnect and must pair again."
                } else {
                    "This phone loses access immediately."
                }
                .into(),
            ],
        )
    } else {
        (
            "begin_pairing",
            "pairing.begin",
            vec!["Allow one additional phone to pair within two minutes.".into()],
        )
    };
    checked(
        serde_json::json!({"confirmationId":uuid::Uuid::new_v4().to_string(),"ownerDeviceId":owner.device_id,"action":kind,"affectedLabels":labels,"expected":{"serverEpoch":d.services.state.event_hub.epoch()},"expiresInMs":60000,"confirmMethod":method}),
    )
}
pub fn handles(c: &RemoteCommand) -> bool {
    matches!(
        c,
        RemoteCommand::DevicesList(_)
            | RemoteCommand::DevicesRevoke(_)
            | RemoteCommand::PairingBegin(_)
    )
}
pub async fn dispatch(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    Dispatcher::authorize(owner, &request)?;
    let admin = d.services.admin.as_ref().ok_or_else(unavailable)?;
    let _gate = admin.gate.lock().map_err(|_| unavailable())?;
    let mut pairing = admin.pairing.lock().map_err(|_| unavailable())?;
    let devices = admin.authorize(owner)?;
    if matches!(request.command, RemoteCommand::DevicesList(_)) {
        return value(DeviceList {
            items: checked(devices)?,
        });
    }
    super::settings::check_epoch(d, &request)?;
    let generation = admin.generation()?;
    let (target, confirmation, is_pairing) = match &request.command {
        RemoteCommand::DevicesRevoke(p) => (p.device_id.clone(), p.confirmation_id.clone(), false),
        RemoteCommand::PairingBegin(p) => (None, p.confirmation_id.clone(), true),
        _ => return Err(RemoteError::invalid()),
    };
    let mut registry = d.services.confirmations.lock().map_err(|_| unavailable())?;
    let mut db = admin.state.db.lock().map_err(|_| unavailable())?;
    let mut store = RemoteStore::new(&mut db, &admin.identity);
    // Validate request identity even when returning a consumed confirmation.
    let id: Id = checked(uuid::Uuid::new_v4().to_string())?;
    let accepted = store
        .accept_command(
            &owner.device_id,
            &request,
            &d.services.state.event_hub.epoch(),
            &id,
            now_ms() as i64,
        )
        .map_err(native_error)?;
    let outcome = match accepted {
        CommandAcceptance::AlreadyIssued { operation_id } => {
            return value(PairingPreparation::AlreadyIssued { operation_id })
        }
        CommandAcceptance::Existing(entry) => {
            let original = registry
                .pending
                .values()
                .find(|p| p.request == request.id && p.preview.owner_device_id == owner.device_id);
            if matches!(
                entry.outcome.status,
                OperationStatus::Failed
                    | OperationStatus::Cancelled
                    | OperationStatus::OutcomeUnknown
            ) {
                let code = entry
                    .outcome
                    .error_code
                    .unwrap_or(ErrorCode::OutcomeUnknown);
                if code == ErrorCode::ConfirmationStale {
                    if let Some(p) = original {
                        let preview = registry
                            .get(&owner.device_id, &p.preview.confirmation_id, now_ms())?
                            .preview;
                        let mut error =
                            RemoteError::new(code, "Review the replacement confirmation.");
                        error.replacement_preview = Some(Box::new(preview));
                        return Err(error);
                    }
                    return Err(RemoteError::new(
                        ErrorCode::OutcomeUnknown,
                        "The replacement confirmation is unavailable.",
                    ));
                }
                return Err(RemoteError::new(
                    code,
                    "The original action did not complete. Inspect current state.",
                ));
            }
            if let Some(p) = original.filter(|p| p.operation.is_none()) {
                return preparation(
                    is_pairing,
                    registry
                        .get(&owner.device_id, &p.preview.confirmation_id, now_ms())?
                        .preview,
                );
            }
            if is_pairing {
                let operation = original.and_then(|p| p.operation.clone()).or_else(|| {
                    confirmation
                        .as_ref()
                        .and_then(|id| registry.get(&owner.device_id, id, now_ms()).ok())
                        .and_then(|p| p.operation)
                });
                return if let Some(operation_id) = operation {
                    value(PairingPreparation::AlreadyIssued { operation_id })
                } else {
                    Err(RemoteError::new(
                        ErrorCode::OutcomeUnknown,
                        "The original pairing confirmation is unavailable.",
                    ))
                };
            }
            return value(ActivityPreparation::Accepted {
                operation: OperationRef {
                    operation_id: entry.outcome.operation_id,
                },
            });
        }
        CommandAcceptance::New(entry) => entry.outcome,
    };
    let work = (|| -> Result<serde_json::Value, RemoteError> {
        let fresh = AdminAction {
            target,
            devices,
            generation,
        };
        if let Some(confirmation) = confirmation {
            let pending = registry.get(&owner.device_id, &confirmation, now_ms())?;
            let Action::Admin(mut saved) = pending.action else {
                return Err(RemoteError::invalid());
            };
            if pending.preview.expected.server_epoch.as_ref()
                != Some(&d.services.state.event_hub.epoch())
                || saved.target.is_none() != is_pairing
            {
                return Err(RemoteError::invalid());
            }
            if let Some(operation_id) = pending.operation {
                return if is_pairing {
                    value(PairingPreparation::AlreadyIssued { operation_id })
                } else {
                    value(ActivityPreparation::Accepted {
                        operation: OperationRef { operation_id },
                    })
                };
            }
            if saved.generation != fresh.generation || !same_devices(&saved.devices, &fresh.devices)
            {
                saved.devices = fresh.devices;
                saved.generation = fresh.generation;
                let replacement = preview(d, owner, &saved)?;
                registry.insert(
                    Pending {
                        preview: replacement.clone(),
                        action: Action::Admin(saved),
                        observed: Default::default(),
                        request: request.id.clone(),
                        expires_at: now_ms() + 60000,
                        operation: None,
                    },
                    now_ms(),
                )?;
                return Err(RemoteError {
                    code: ErrorCode::ConfirmationStale,
                    message: checked("Phones or connection changed. Review again.")?,
                    retryable: false,
                    refresh: checked(Vec::<Topic>::new())?,
                    replacement_preview: Some(Box::new(replacement)),
                });
            }
            let response = if let Some(target) = saved.target {
                let mut issued = admin.issued.lock().map_err(|_| unavailable())?;
                if issued.as_ref().is_some_and(|(id, _)| id == &target) {
                    pairing.close();
                    *issued = None;
                }
                // Store is borrowed from this same DB; end its borrow before the auth gate.
                admin
                    .auth
                    .revoke_device(&mut db, &target, now_ms() as i64)?;
                value(ActivityPreparation::Accepted {
                    operation: OperationRef {
                        operation_id: outcome.operation_id.clone(),
                    },
                })?
            } else {
                if saved.devices.len() >= 8 {
                    return Err(RemoteError::new(
                        ErrorCode::Busy,
                        "Remove a paired phone before adding another.",
                    ));
                }
                let (addresses, port) = admin.addresses()?;
                let ready = pairing.begin(addresses, port)?;
                let qr_rows = match crate::remote::pairing::qr_rows(&ready.candidate) {
                    Ok(rows) => rows,
                    Err(error) => {
                        pairing.close();
                        return Err(error);
                    }
                };
                if RemoteStore::new(&mut db, &admin.identity)
                    .finish_pairing_issuance(
                        &owner.device_id,
                        &request.id,
                        &outcome.operation_id,
                        now_ms() as i64,
                    )
                    .is_err()
                {
                    pairing.close();
                    return Err(RemoteError::new(
                        ErrorCode::OutcomeUnknown,
                        "Pairing could not be recorded. No candidate remains active.",
                    ));
                }
                *admin.issued.lock().map_err(|_| unavailable())? =
                    Some((owner.device_id.clone(), ready.candidate.attempt_id.clone()));
                value(PairingPreparation::Ready {
                    operation_id: outcome.operation_id.clone(),
                    candidate: ready.candidate,
                    qr_rows: Some(qr_rows),
                })?
            };
            registry
                .pending
                .get_mut(confirmation.as_str())
                .unwrap()
                .operation = Some(outcome.operation_id.clone());
            Ok(response)
        } else {
            if is_pairing && fresh.devices.len() >= 8 {
                return Err(RemoteError::new(
                    ErrorCode::Busy,
                    "Remove a paired phone before adding another.",
                ));
            }
            let preview = preview(d, owner, &fresh)?;
            registry.insert(
                Pending {
                    preview: preview.clone(),
                    action: Action::Admin(fresh),
                    observed: Default::default(),
                    request: request.id.clone(),
                    expires_at: now_ms() + 60000,
                    operation: None,
                },
                now_ms(),
            )?;
            preparation(is_pairing, preview)
        }
    })();
    let issued = work.as_ref().is_ok_and(|v| v["status"] == "ready");
    if !issued {
        let terminal = JournalOutcome {
            operation_id: outcome.operation_id,
            status: if work.is_ok() {
                OperationStatus::Succeeded
            } else {
                OperationStatus::Failed
            },
            error_code: work.as_ref().err().map(|e| e.code.clone()),
        };
        RemoteStore::new(&mut db, &admin.identity)
            .finish_command(&owner.device_id, &request.id, &terminal, now_ms() as i64)
            .map_err(native_error)?;
    }
    drop(db);
    drop(registry);
    drop(pairing);
    drop(_gate);
    d.services.operations().publish(owner);
    work
}
fn preparation(
    pairing: bool,
    preview: ConfirmationPreview,
) -> Result<serde_json::Value, RemoteError> {
    if pairing {
        value(PairingPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        })
    } else {
        value(ActivityPreparation::ConfirmationRequired {
            preview: Box::new(preview),
        })
    }
}

#[cfg(test)]
mod tests;
