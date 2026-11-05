//! A UUID-less fingerprint is a reattachment hint, never an authorization to read media.
//! A native folder choice must confirm the source again after it goes offline.
/// `hardware_id` comes from UDisks Drive.Id or WWN/Serial, never its object path.
/// `partition_offset` is the partition byte offset (zero for a whole-disk filesystem).
pub fn fallback_fingerprint(
    hardware_id: &str,
    partition_offset: u64,
    volume_size: u64,
    filesystem_type: &str,
) -> Option<String> {
    use sha2::{Digest, Sha256};
    if hardware_id.trim().is_empty() || volume_size == 0 || filesystem_type.trim().is_empty() {
        return None;
    }
    let mut hash = Sha256::new();
    for component in [hardware_id.as_bytes(), filesystem_type.as_bytes()] {
        hash.update((component.len() as u64).to_be_bytes());
        hash.update(component);
    }
    hash.update(partition_offset.to_be_bytes());
    hash.update(volume_size.to_be_bytes());
    Some(format!("fingerprint:v1:{:x}", hash.finalize()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_requires_actual_hardware_and_filesystem_evidence() {
        assert!(fallback_fingerprint("", 0, 1024, "ext4").is_none());
        assert!(fallback_fingerprint("serial-123", 0, 0, "ext4").is_none());
        assert!(fallback_fingerprint("serial-123", 0, 1024, "").is_none());
    }
    #[test]
    fn fingerprint_distinguishes_partition_and_device_but_not_mount_path() {
        let original = fallback_fingerprint("wwn-123", 1048576, 1024, "ext4").unwrap();
        assert_eq!(
            original,
            fallback_fingerprint("wwn-123", 1048576, 1024, "ext4").unwrap()
        );
        assert_ne!(
            original,
            fallback_fingerprint("wwn-456", 1048576, 1024, "ext4").unwrap()
        );
        assert_ne!(
            original,
            fallback_fingerprint("wwn-123", 2097152, 1024, "ext4").unwrap()
        );
        assert!(original.starts_with("fingerprint:v1:"));
        assert!(!original.contains("wwn-123"));
    }
}
