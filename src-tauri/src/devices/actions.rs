//! Device actions only accept opaque IDs resolved against fresh UDisks observations.
use crate::{
    contracts::AppError,
    devices::{
        registry::Volume,
        udisks::{Devices, Drive},
    },
};
use serde::Serialize;
use std::future::Future;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

pub trait DeviceBackend: Sync {
    fn discover(&self) -> impl Future<Output = Result<Devices, AppError>> + Send;
    fn mount(&self, volume: &Volume) -> impl Future<Output = Result<(), AppError>> + Send;
    fn unmount(&self, volume: &Volume) -> impl Future<Output = Result<(), AppError>> + Send;
    fn power_off(&self, drive: &Drive) -> impl Future<Output = Result<(), AppError>> + Send;
}
/// The guard must prevent new source work until dropped; quiesce drains existing readers.
pub trait SourceLifecycle: Sync {
    type Guard: Send;
    fn quiesce<'a>(
        &'a self,
        volumes: &'a [Volume],
    ) -> impl Future<Output = Result<Self::Guard, AppError>> + Send;
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EjectPreview {
    pub drive_ids: Vec<String>,
    pub drive_names: Vec<String>,
    pub volume_ids: Vec<String>,
    pub mounted_volumes: usize,
    pub active_uses: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EjectStatus {
    PoweredOff,
    Unmounted,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EjectOutcome {
    pub drive_ids: Vec<String>,
    pub status: EjectStatus,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MountOutcome {
    pub volume_id: String,
}
fn protected_mount(volume: &Volume) -> bool {
    volume.mounts.iter().any(|mount| {
        mount == std::path::Path::new("/")
            || ["/boot", "/usr", "/etc", "/var", "/home", "/opt"]
                .iter()
                .any(|root| mount.starts_with(root))
    })
}
pub(crate) fn group(
    devices: &Devices,
    drive_id: &str,
) -> Result<(Vec<Drive>, Vec<Volume>), AppError> {
    let selected = devices
        .drives
        .iter()
        .find(|d| d.id == drive_id)
        .ok_or_else(|| AppError::new("drive_offline", "The drive is no longer connected."))?;
    let mut drives: Vec<_> = devices
        .drives
        .iter()
        .filter(|d| {
            d.id == selected.id
                || (!selected.sibling_id.is_empty() && d.sibling_id == selected.sibling_id)
        })
        .cloned()
        .collect();
    drives.sort_by(|a, b| a.id.cmp(&b.id));
    let mut volumes: Vec<_> = devices
        .volumes
        .iter()
        .filter(|v| drives.iter().any(|d| d.id == v.drive_id))
        .cloned()
        .collect();
    volumes.sort_by(|a, b| a.id.cmp(&b.id));
    if drives.iter().any(|d| !d.removable || !d.safe_to_remove)
        || volumes.iter().any(protected_mount)
    {
        return Err(AppError::new("device_protected", "This device contains internal, system or unsupported storage and cannot be removed here."));
    }
    Ok((drives, volumes))
}
pub(crate) fn same_group(
    before: &(Vec<Drive>, Vec<Volume>),
    after: &(Vec<Drive>, Vec<Volume>),
) -> bool {
    before.0.len() == after.0.len()
        && before.1.len() == after.1.len()
        && before.0.iter().zip(&after.0).all(|(a, b)| {
            a.id == b.id
                && a.path == b.path
                && a.sibling_id == b.sibling_id
                && a.connection_id == b.connection_id
        })
        && before.1.iter().zip(&after.1).all(|(a, b)| {
            a.id == b.id
                && a.path == b.path
                && a.drive_id == b.drive_id
                && a.identity == b.identity
                && a.size == b.size
        })
}
fn changed() -> AppError {
    AppError::new(
        "device_confirmation_changed",
        "The device or its volumes changed. Refresh drives and confirm removal again.",
    )
}
pub async fn prepare_eject_drive<B: DeviceBackend>(
    backend: &B,
    drive_id: &str,
) -> Result<EjectPreview, AppError> {
    let (drives, volumes) = group(&backend.discover().await?, drive_id)?;
    Ok(EjectPreview {
        drive_ids: drives.iter().map(|d| d.id.clone()).collect(),
        drive_names: drives
            .iter()
            .map(|d| {
                if d.name.is_empty() {
                    "Storage device".into()
                } else {
                    d.name.clone()
                }
            })
            .collect(),
        volume_ids: volumes.iter().map(|v| v.id.clone()).collect(),
        mounted_volumes: volumes.iter().filter(|v| !v.mounts.is_empty()).count(),
        active_uses: Vec::new(),
    })
}
pub async fn eject_drive<B: DeviceBackend, L: SourceLifecycle>(
    backend: &B,
    lifecycle: &L,
    drive_id: &str,
    confirmed_drive_ids: &[String],
) -> Result<EjectOutcome, AppError> {
    let initial = group(&backend.discover().await?, drive_id)?;
    let ids: Vec<_> = initial.0.iter().map(|d| d.id.clone()).collect();
    let mut confirmed = confirmed_drive_ids.to_vec();
    confirmed.sort();
    if confirmed != ids {
        return Err(changed());
    }
    eject_group(backend, lifecycle, drive_id, &initial, None).await
}
/// Execute only the exact physical identity retained by a remote preview.
pub(crate) async fn eject_group<B: DeviceBackend, L: SourceLifecycle>(
    backend: &B,
    lifecycle: &L,
    drive_id: &str,
    confirmed: &(Vec<Drive>, Vec<Volume>),
    cancel: Option<&CancellationToken>,
) -> Result<EjectOutcome, AppError> {
    let initial = group(&backend.discover().await?, drive_id)?;
    if !same_group(confirmed, &initial) {
        return Err(changed());
    }
    let ids = initial.0.iter().map(|d| d.id.clone()).collect();
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        return Err(AppError::new("cancelled", "Removal cancelled."));
    }
    let _guard = lifecycle.quiesce(&initial.1).await?;
    let fresh = group(&backend.discover().await?, drive_id)?;
    if !same_group(&initial, &fresh) {
        return Err(changed());
    }
    for volume in &fresh.1 {
        if !volume.mounts.is_empty() {
            backend.unmount(volume).await?;
        }
    }
    // Never claim success when another actor remounted a volume or the topology changed.
    let final_group = group(&backend.discover().await?, drive_id)?;
    if !same_group(&fresh, &final_group) {
        return Err(changed());
    }
    if final_group.1.iter().any(|v| !v.mounts.is_empty()) {
        return Err(AppError::new("device_busy", "A volume is still mounted. Another application may be using it. The device is not safe to remove."));
    }
    let selected = final_group
        .0
        .iter()
        .find(|d| d.id == drive_id)
        .ok_or_else(changed)?;
    let status = if selected.can_power_off {
        // A single PowerOff affects the complete sibling group, already confirmed and drained.
        backend.power_off(selected).await?;
        EjectStatus::PoweredOff
    } else {
        EjectStatus::Unmounted
    };
    Ok(EjectOutcome {
        drive_ids: ids,
        status,
    })
}
pub async fn mount_volume<B: DeviceBackend>(
    backend: &B,
    volume_id: &str,
) -> Result<MountOutcome, AppError> {
    let devices = backend.discover().await?;
    let volume = devices
        .volumes
        .iter()
        .find(|v| v.id == volume_id)
        .ok_or_else(|| {
            AppError::new(
                "volume_offline",
                "The selected volume is no longer connected.",
            )
        })?;
    let confirmed = group(&devices, &volume.drive_id)?;
    mount_group(backend, volume_id, &confirmed).await
}
pub(crate) async fn mount_group<B: DeviceBackend>(
    backend: &B,
    volume_id: &str,
    confirmed: &(Vec<Drive>, Vec<Volume>),
) -> Result<MountOutcome, AppError> {
    let expected = confirmed
        .1
        .iter()
        .find(|v| v.id == volume_id)
        .ok_or_else(changed)?;
    let devices = backend.discover().await?;
    let current = group(&devices, &expected.drive_id)?;
    if !same_group(confirmed, &current) {
        return Err(changed());
    }
    let volume = current
        .1
        .iter()
        .find(|v| v.id == volume_id)
        .ok_or_else(changed)?;
    if volume.mounts.is_empty() {
        backend.mount(volume).await?;
    }
    let observed = backend.discover().await?;
    if !same_group(&current, &group(&observed, &volume.drive_id)?) {
        return Err(changed());
    }
    if !observed.volumes.iter().any(|v| {
        v.id == volume.id
            && v.path == volume.path
            && v.identity == volume.identity
            && !v.mounts.is_empty()
    }) {
        return Err(AppError::new(
            "mount_unconfirmed",
            "Mount completion could not be confirmed. Refresh drives.",
        ));
    }
    Ok(MountOutcome {
        volume_id: volume_id.into(),
    })
}

pub struct UDisksBackend;
fn os_error(error: zbus::Error) -> AppError {
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => name.as_str(),
        _ => "",
    };
    if name.ends_with("DeviceBusy") {
        AppError::new("device_busy", "Another application is using this drive. Close its files and try again. The device is not safe to remove.")
    } else if name.contains("NotAuthorized") || name.ends_with("AccessDenied") {
        AppError::new("device_permission_denied", "The operating system did not authorize this drive operation. The device is not safe to remove.")
    } else {
        AppError::new("device_operation_failed", "The operating system could not complete the drive operation. Refresh drives before trying again; safe removal has not been confirmed.")
    }
}
fn operation_options(
    interactive: bool,
) -> std::collections::HashMap<&'static str, zbus::zvariant::Value<'static>> {
    let mut options = std::collections::HashMap::new();
    if !interactive {
        options.insert(
            "auth.no_user_interaction",
            zbus::zvariant::Value::Bool(true),
        );
    }
    options
}
async fn mutate(
    path: &str,
    interface: &str,
    method: &str,
    interactive: bool,
) -> Result<(), AppError> {
    tokio::time::timeout(std::time::Duration::from_secs(30),async {
        let connection = zbus::Connection::system().await?;
        let proxy = zbus::Proxy::new(&connection,"org.freedesktop.UDisks2",path,interface).await?;
        let options = operation_options(interactive);
        if method == "Mount" { let _: String = proxy.call(method,&(options,)).await?; }
        else { let _: () = proxy.call(method,&(options,)).await?; }
        Ok::<(),zbus::Error>(())
    }).await.map_err(|_|AppError::new("device_timeout", "The drive operation timed out and may still be running. Refresh drives; safe removal has not been confirmed."))?.map_err(os_error)
}
impl DeviceBackend for UDisksBackend {
    async fn discover(&self) -> Result<Devices, AppError> {
        super::udisks::discover().await
    }
    async fn mount(&self, volume: &Volume) -> Result<(), AppError> {
        mutate(
            &volume.path,
            "org.freedesktop.UDisks2.Filesystem",
            "Mount",
            true,
        )
        .await
    }
    async fn unmount(&self, volume: &Volume) -> Result<(), AppError> {
        mutate(
            &volume.path,
            "org.freedesktop.UDisks2.Filesystem",
            "Unmount",
            true,
        )
        .await
    }
    async fn power_off(&self, drive: &Drive) -> Result<(), AppError> {
        mutate(
            &drive.path,
            "org.freedesktop.UDisks2.Drive",
            "PowerOff",
            true,
        )
        .await
    }
}

/// Remote requests must return authorization failures without opening an OS password dialog.
#[derive(Clone)]
pub struct RemoteUDisksBackend;
impl DeviceBackend for RemoteUDisksBackend {
    async fn discover(&self) -> Result<Devices, AppError> {
        super::udisks::discover().await
    }
    async fn mount(&self, volume: &Volume) -> Result<(), AppError> {
        mutate(
            &volume.path,
            "org.freedesktop.UDisks2.Filesystem",
            "Mount",
            false,
        )
        .await
    }
    async fn unmount(&self, volume: &Volume) -> Result<(), AppError> {
        mutate(
            &volume.path,
            "org.freedesktop.UDisks2.Filesystem",
            "Unmount",
            false,
        )
        .await
    }
    async fn power_off(&self, drive: &Drive) -> Result<(), AppError> {
        mutate(
            &drive.path,
            "org.freedesktop.UDisks2.Drive",
            "PowerOff",
            false,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake {
        devices: Mutex<Devices>,
        failure: Option<(&'static str, &'static str)>,
        power: Mutex<Vec<String>>,
        remount: bool,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                devices: Mutex::new(Devices {
                    drives: vec![Drive {
                        connection_id: "usb-insertion-1".into(),
                        id: "drive:usb".into(),
                        path: "/drive/usb".into(),
                        name: "USB".into(),
                        removable: true,
                        safe_to_remove: true,
                        can_power_off: true,
                        sibling_id: String::new(),
                    }],
                    volumes: vec![volume("a"), volume("b")],
                }),
                failure: None,
                power: Mutex::new(vec![]),
                remount: false,
            }
        }
    }
    fn volume(id: &str) -> Volume {
        Volume {
            id: format!("volume:{id}"),
            path: format!("/volume/{id}"),
            drive_id: "drive:usb".into(),
            identity: Some(format!("uuid:{id}")),
            mounts: vec![format!("/media/{id}").into()],
            label: id.into(),
            size: 1024,
            removable: true,
        }
    }
    impl DeviceBackend for Fake {
        async fn discover(&self) -> Result<Devices, AppError> {
            Ok(self.devices.lock().unwrap().clone())
        }
        async fn mount(&self, volume: &Volume) -> Result<(), AppError> {
            let mut devices = self.devices.lock().unwrap();
            let v = devices
                .volumes
                .iter_mut()
                .find(|v| v.id == volume.id)
                .unwrap();
            v.mounts = vec!["/media/mounted".into()];
            Ok(())
        }
        async fn unmount(&self, volume: &Volume) -> Result<(), AppError> {
            if let Some((id, code)) = self.failure {
                if id == volume.id {
                    return Err(AppError::new(code, "OS refused removal"));
                }
            }
            if self.remount {
                return Ok(());
            }
            self.devices
                .lock()
                .unwrap()
                .volumes
                .iter_mut()
                .find(|v| v.id == volume.id)
                .unwrap()
                .mounts
                .clear();
            Ok(())
        }
        async fn power_off(&self, drive: &Drive) -> Result<(), AppError> {
            if let Some((id, code)) = self.failure {
                if id == drive.id {
                    return Err(AppError::new(code, "Power off denied"));
                }
            }
            self.power.lock().unwrap().push(drive.id.clone());
            Ok(())
        }
    }
    #[derive(Default)]
    struct Lifecycle {
        closed: Mutex<Vec<String>>,
        fail: bool,
    }
    impl SourceLifecycle for Lifecycle {
        type Guard = ();
        async fn quiesce(&self, volumes: &[Volume]) -> Result<(), AppError> {
            if self.fail {
                return Err(AppError::new("source_busy", "Readers still active"));
            }
            *self.closed.lock().unwrap() = volumes.iter().map(|v| v.id.clone()).collect();
            Ok(())
        }
    }
    async fn remove(fake: &Fake, lifecycle: &Lifecycle) -> Result<EjectOutcome, AppError> {
        eject_drive(fake, lifecycle, "drive:usb", &["drive:usb".into()]).await
    }
    #[test]
    fn remote_os_options_disable_prompts_and_have_no_force_option() {
        let options = operation_options(false);
        assert_eq!(options.len(), 1);
        assert_eq!(
            options.get("auth.no_user_interaction"),
            Some(&zbus::zvariant::Value::Bool(true))
        );
        assert!(!options.contains_key("force"));
        assert!(operation_options(true).is_empty());
    }
    #[test]
    fn preview_identity_rejects_same_drive_replug() {
        let fake = Fake::new();
        let before = group(&fake.devices.lock().unwrap(), "drive:usb").unwrap();
        fake.devices.lock().unwrap().drives[0].connection_id = "usb-insertion-2".into();
        let after = group(&fake.devices.lock().unwrap(), "drive:usb").unwrap();
        assert!(
            !same_group(&before, &after),
            "a replug must invalidate the preview"
        );
    }
    #[tokio::test]
    async fn whole_drive_removal_drains_every_volume_then_unmounts_and_powers_off() {
        let fake = Fake::new();
        let lifecycle = Lifecycle::default();
        let outcome = remove(&fake, &lifecycle).await.unwrap();
        assert_eq!(outcome.status, EjectStatus::PoweredOff);
        assert_eq!(
            *lifecycle.closed.lock().unwrap(),
            vec!["volume:a", "volume:b"]
        );
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| v.mounts.is_empty()));
        assert_eq!(fake.power.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn one_volume_failure_prevents_false_drive_success() {
        for code in ["device_busy", "device_permission_denied"] {
            let mut fake = Fake::new();
            fake.failure = Some(("volume:b", code));
            assert_eq!(
                remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
                code
            );
            assert!(fake.power.lock().unwrap().is_empty());
            assert!(!fake.devices.lock().unwrap().volumes[1].mounts.is_empty());
        }
    }
    #[tokio::test]
    async fn no_power_off_support_reports_unmounted_only() {
        let fake = Fake::new();
        fake.devices.lock().unwrap().drives[0].can_power_off = false;
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap().status,
            EjectStatus::Unmounted
        );
        assert!(fake.power.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn readers_that_do_not_close_prevent_os_mutations() {
        let fake = Fake::new();
        assert!(remove(
            &fake,
            &Lifecycle {
                fail: true,
                ..Default::default()
            }
        )
        .await
        .is_err());
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| !v.mounts.is_empty()));
    }
    #[tokio::test]
    async fn internal_and_system_drives_cannot_be_removed() {
        let fake = Fake::new();
        fake.devices.lock().unwrap().drives[0].removable = false;
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_protected"
        );
        fake.devices.lock().unwrap().drives[0].removable = true;
        fake.devices.lock().unwrap().volumes[1].mounts = vec!["/".into()];
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_protected"
        );
        assert!(fake.power.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn shared_physical_device_requires_explicit_complete_confirmation() {
        let fake = Fake::new();
        {
            let mut d = fake.devices.lock().unwrap();
            d.drives[0].sibling_id = "enclosure".into();
            let mut sibling = d.drives[0].clone();
            sibling.id = "drive:other".into();
            sibling.name = "Other disk".into();
            d.drives.push(sibling);
            let mut v = volume("c");
            v.drive_id = "drive:other".into();
            d.volumes.push(v);
        }
        let preview = prepare_eject_drive(&fake, "drive:usb").await.unwrap();
        assert_eq!(preview.drive_ids.len(), 2);
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_confirmation_changed"
        );
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| !v.mounts.is_empty()));
        let outcome = eject_drive(
            &fake,
            &Lifecycle::default(),
            "drive:usb",
            &preview.drive_ids,
        )
        .await
        .unwrap();
        assert_eq!(outcome.drive_ids.len(), 2);
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| v.mounts.is_empty()));
    }
    #[tokio::test]
    async fn mount_resolves_opaque_volume_id_and_confirms_new_state() {
        let fake = Fake::new();
        fake.devices.lock().unwrap().volumes[0].mounts.clear();
        assert_eq!(
            mount_volume(&fake, "/dev/sda").await.unwrap_err().code,
            "volume_offline"
        );
        assert_eq!(
            mount_volume(&fake, "volume:a").await.unwrap().volume_id,
            "volume:a"
        );
        assert!(!fake.devices.lock().unwrap().volumes[0].mounts.is_empty());
    }
    #[tokio::test]
    async fn remounted_volume_prevents_power_off_and_success() {
        let mut fake = Fake::new();
        fake.remount = true;
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_busy"
        );
        assert!(fake.power.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn power_off_failure_never_reports_safe_removal() {
        let mut fake = Fake::new();
        fake.failure = Some(("drive:usb", "device_permission_denied"));
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_permission_denied"
        );
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| v.mounts.is_empty()));
    }
    #[tokio::test]
    async fn unsafe_block_topology_prevents_eject_even_when_removable() {
        let fake = Fake::new();
        fake.devices.lock().unwrap().drives[0].safe_to_remove = false;
        assert_eq!(
            remove(&fake, &Lifecycle::default()).await.unwrap_err().code,
            "device_protected"
        );
        assert!(fake
            .devices
            .lock()
            .unwrap()
            .volumes
            .iter()
            .all(|v| !v.mounts.is_empty()));
    }
}
