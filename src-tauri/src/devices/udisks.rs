use crate::{contracts::AppError, devices::registry::Volume};
use std::{collections::HashMap, os::unix::ffi::OsStringExt, path::PathBuf};
use zbus::zvariant::OwnedValue;
type Props = HashMap<String, OwnedValue>;
fn string(props: &Props, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or_default()
        .into()
}
fn boolean(props: &Props, key: &str) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn number(props: &Props, key: &str) -> u64 {
    props
        .get(key)
        .and_then(|v| u64::try_from(v).ok())
        .unwrap_or(0)
}
#[derive(Clone, Debug)]
pub struct Drive {
    pub safe_to_remove: bool,
    pub id: String,
    pub path: String,
    pub name: String,
    pub removable: bool,
    pub can_power_off: bool,
    pub sibling_id: String,
}
#[derive(Clone, Debug, Default)]
pub struct Devices {
    pub drives: Vec<Drive>,
    pub volumes: Vec<Volume>,
}
async fn read() -> Result<Devices, zbus::Error> {
    let connection = zbus::Connection::system().await?;
    let proxy = zbus::fdo::ObjectManagerProxy::builder(&connection)
        .destination("org.freedesktop.UDisks2")?
        .path("/org/freedesktop/UDisks2")?
        .build()
        .await?;
    let objects = proxy.get_managed_objects().await?;
    let mut result = Devices::default();
    let mut identities = HashMap::new();
    let mut hardware_ids = HashMap::new();
    for (path, interfaces) in &objects {
        let Some(props) = interfaces.get("org.freedesktop.UDisks2.Drive") else {
            continue;
        };
        let id = format!(
            "drive:{}",
            path.as_str().rsplit('/').next().unwrap_or_default()
        );
        identities.insert(path.to_string(), id.clone());
        let hardware = ["Id", "WWN", "Serial"]
            .iter()
            .map(|key| string(props, key))
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        hardware_ids.insert(path.to_string(), hardware);
        let name = format!("{} {}", string(props, "Vendor"), string(props, "Model"))
            .trim()
            .to_string();
        result.drives.push(Drive {
            safe_to_remove: true,
            id,
            path: path.to_string(),
            name,
            removable: boolean(props, "Removable") || string(props, "ConnectionBus") == "usb",
            can_power_off: boolean(props, "CanPowerOff"),
            sibling_id: string(props, "SiblingId"),
        });
    }
    for interfaces in objects.values() {
        let Some(block) = interfaces.get("org.freedesktop.UDisks2.Block") else {
            continue;
        };
        let drive_path = block
            .get("Drive")
            .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
            .map(|p| p.as_str())
            .unwrap_or_default();
        let crypto = block
            .get("CryptoBackingDevice")
            .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
            .is_some_and(|p| p.as_str() != "/");
        let unsafe_device = boolean(block, "HintSystem")
            || boolean(block, "HintIgnore")
            || crypto
            || interfaces
                .get("org.freedesktop.UDisks2.Swapspace")
                .is_some_and(|p| boolean(p, "Active"));
        if unsafe_device {
            if let Some(drive) = result.drives.iter_mut().find(|d| d.path == drive_path) {
                drive.safe_to_remove = false;
            }
        }
        if let Some(backing) = block
            .get("CryptoBackingDevice")
            .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
            .filter(|p| p.as_str() != "/")
        {
            if let Some(props) = objects
                .iter()
                .find(|(path, _)| path.as_str() == backing.as_str())
                .and_then(|(_, interfaces)| interfaces.get("org.freedesktop.UDisks2.Block"))
            {
                if let Some(parent) = props
                    .get("Drive")
                    .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
                {
                    if let Some(drive) =
                        result.drives.iter_mut().find(|d| d.path == parent.as_str())
                    {
                        drive.safe_to_remove = false;
                    }
                }
            }
        }
        if ["raid", "other", "crypto"].contains(&string(block, "IdUsage").as_str()) {
            if let Some(drive) = result.drives.iter_mut().find(|d| d.path == drive_path) {
                drive.safe_to_remove = false;
            }
        }
    }
    for (path, interfaces) in &objects {
        let Some(block) = interfaces.get("org.freedesktop.UDisks2.Block") else {
            continue;
        };
        let Some(fs) = interfaces.get("org.freedesktop.UDisks2.Filesystem") else {
            continue;
        };
        let drive_path = block
            .get("Drive")
            .and_then(|v| <&zbus::zvariant::ObjectPath>::try_from(v).ok())
            .map(|p| p.as_str())
            .unwrap_or_default();
        let drive_id = identities.get(drive_path).cloned().unwrap_or_default();
        let uuid = string(block, "IdUUID");
        let raw_mounts: Vec<Vec<u8>> = fs
            .get("MountPoints")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| v.try_into().ok())
            .unwrap_or_default();
        let mounts = raw_mounts
            .into_iter()
            .filter_map(|mut bytes| {
                if bytes.last() == Some(&0) {
                    bytes.pop();
                }
                if bytes.is_empty() || bytes.contains(&0) {
                    None
                } else {
                    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
                }
            })
            .collect();
        let removable = result
            .drives
            .iter()
            .any(|d| d.id == drive_id && d.removable);
        result.volumes.push(Volume {
            id: format!(
                "volume:{}",
                path.as_str().rsplit('/').next().unwrap_or_default()
            ),
            path: path.to_string(),
            drive_id,
            identity: if !uuid.is_empty() {
                Some(format!("uuid:{uuid}"))
            } else {
                super::identity::fallback_fingerprint(
                    hardware_ids
                        .get(drive_path)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    interfaces
                        .get("org.freedesktop.UDisks2.Partition")
                        .map_or(0, |p| number(p, "Offset")),
                    number(block, "Size"),
                    &string(block, "IdType"),
                )
            },
            mounts,
            label: string(block, "IdLabel"),
            size: number(block, "Size"),
            removable,
        });
    }
    result.drives.sort_by(|a, b| a.id.cmp(&b.id));
    result.volumes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
pub async fn discover() -> Result<Devices, AppError> {
    tokio::time::timeout(std::time::Duration::from_secs(5), read())
        .await
        .map_err(|_| AppError::new("device_timeout", "Drive discovery timed out."))?
        .map_err(|_| {
            AppError::new(
                "devices_unavailable",
                "UDisks2 drive discovery is unavailable.",
            )
        })
}
pub fn descriptors(devices: &Devices) -> Vec<crate::contracts::DriveDescriptor> {
    devices
        .drives
        .iter()
        .map(|drive| crate::contracts::DriveDescriptor {
            id: drive.id.clone(),
            name: drive.name.clone(),
            removable: drive.removable,
            can_power_off: drive.can_power_off,
            volumes: devices
                .volumes
                .iter()
                .filter(|v| v.drive_id == drive.id)
                .map(|v| crate::contracts::VolumeDescriptor {
                    id: v.id.clone(),
                    drive_id: v.drive_id.clone(),
                    label: v.label.clone(),
                    mounted: !v.mounts.is_empty(),
                    mount_points: v
                        .mounts
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                    size: v.size,
                    ambiguous: v
                        .identity
                        .as_ref()
                        .is_some_and(|id| super::registry::resolve(&devices.volumes, id).is_err()),
                })
                .collect(),
        })
        .collect()
}
#[cfg(test)]
mod native_tests {
    #[tokio::test]
    #[ignore = "read-only host UDisks2 integration probe"]
    async fn host_device_discovery() {
        let devices = super::discover().await.unwrap();
        println!(
            "UDisks2 read-only discovery: {} drives, {} volumes, {} mounted volumes",
            devices.drives.len(),
            devices.volumes.len(),
            devices
                .volumes
                .iter()
                .filter(|v| !v.mounts.is_empty())
                .count()
        );
        for volume in devices.volumes {
            assert!(volume.mounts.iter().all(|path| path.is_absolute()));
        }
    }
}
