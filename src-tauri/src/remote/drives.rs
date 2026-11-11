//! Path-free wire observations and exact physical-drive impact retained by confirmations.
use super::{adapters::checked, protocol::*};
use crate::{
    app_state::AppState,
    contracts::AppError,
    control::activity::Identity,
    devices::{
        actions::{self, DeviceBackend, SourceLifecycle},
        registry::Volume,
        udisks::{Devices, Drive},
    },
};

#[derive(Clone)]
pub struct EjectAction {
    pub drive_id: String,
    pub group: (Vec<Drive>, Vec<Volume>),
    pub activity: Identity,
    pub sources: Vec<String>,
    pub labels: Vec<String>,
}
fn label(value: &str, fallback: &str) -> String {
    if value.is_empty() {
        fallback.into()
    } else {
        value.chars().take(256).collect()
    }
}
pub fn drive_handle(drive: &Drive) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "drive-{:x}",
        Sha256::digest(format!("{}\0{}", drive.id, drive.connection_id).as_bytes())
    )
}
pub fn volume_handle(volume: &Volume, drive: &Drive) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "volume-{:x}",
        Sha256::digest(
            format!(
                "{}\0{}\0{:?}",
                drive_handle(drive),
                volume.id,
                volume.identity
            )
            .as_bytes()
        )
    )
}
pub fn inventory(devices: &Devices, revision: u64) -> Result<DriveList, RemoteError> {
    let items = devices.drives.iter().filter(|d| actions::group(devices, &d.id).is_ok()).map(|d| {
        serde_json::json!({"id":drive_handle(d),"label":label(&d.name,"USB drive"),"removable":d.removable,
            "volumes":devices.volumes.iter().filter(|v|v.drive_id==d.id).map(|v|serde_json::json!({
                "id":volume_handle(v,d),"label":label(&v.label,"Volume"),"mounted":!v.mounts.is_empty(),"availability":if v.mounts.is_empty(){"offline"}else{"available"}
            })).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    checked(serde_json::json!({"revision":revision.to_string(),"items":items}))
}
pub async fn capture(
    state: &AppState,
    devices: &Devices,
    drive_id: &str,
) -> Result<EjectAction, AppError> {
    let group = actions::group(devices, drive_id)?;
    if group.0.iter().any(|d| d.connection_id.is_empty()) {
        return Err(AppError::new(
            "unavailable",
            "Drive insertion identity unavailable. Use the computer.",
        ));
    }
    let sources = crate::lifecycle::affected_sources(state, &group.1)?;
    let mut labels = group
        .0
        .iter()
        .map(|d| label(&d.name, "USB drive"))
        .collect::<Vec<_>>();
    labels.extend(group.1.iter().map(|v| label(&v.label, "Volume")));
    if state
        .scans
        .lock()
        .map_err(|_| AppError::new("unavailable", "Scan state unavailable"))?
        .keys()
        .any(|id| sources.contains(id))
    {
        labels.push("Media scanning".into());
    }
    let player = state
        .player
        .lock()
        .await
        .as_ref()
        .filter(|p| sources.contains(&p.media.source_id))
        .map(|p| p.session.id.clone());
    let photos = state.photos.lock().await.affected_session(&sources);
    if player.is_some() {
        labels.push("Local playback".into());
    }
    if photos.is_some() {
        labels.push("Photo presentation".into());
    }
    // Resource ownership includes photo decoders/presentation readers; foreign processes never enter this registry.
    if state.resources.active_count(&sources)? > 0 {
        labels.push("Open media files or photo presentation".into());
    }
    Ok(EjectAction {
        drive_id: drive_id.into(),
        group,
        activity: Identity {
            player,
            photos,
            ..Default::default()
        },
        sources,
        labels,
    })
}
pub fn same(before: &EjectAction, after: &EjectAction) -> bool {
    actions::same_group(&before.group, &after.group)
        && before
            .group
            .1
            .iter()
            .zip(&after.group.1)
            .all(|(a, b)| a.mounts == b.mounts)
        && before.activity == after.activity
        && before.sources == after.sources
        && before.labels == after.labels
}
pub fn normalize(mut error: AppError) -> AppError {
    error.code = match error.code.as_str() {
        "device_permission_denied" => "local_authorization_required",
        "device_busy" | "source_busy" => "busy",
        "device_confirmation_changed" => "confirmation_stale",
        "device_protected" => "forbidden",
        "drive_offline" | "volume_offline" => "not_found",
        "device_timeout" | "mount_unconfirmed" => "outcome_unknown",
        "device_operation_failed" | "device_state_refresh_failed" | "devices_unavailable" => {
            "unavailable"
        }
        code => code,
    }
    .into();
    error
}
pub async fn eject<B: DeviceBackend, L: SourceLifecycle>(
    state: &AppState,
    backend: &B,
    lifecycle: &L,
    action: &EjectAction,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Removal cancelled."));
    }
    let current = capture(state, &backend.discover().await?, &action.drive_id).await?;
    if !same(action, &current) {
        return Err(AppError::new(
            "confirmation_stale",
            "Drive or active use changed. Review removal again.",
        ));
    }
    // Once native removal starts, observe completion; a cancellation request never drops an OS call.
    actions::eject_group(backend, lifecycle, &action.drive_id, &action.group)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests;
