use crate::{
    contracts::*,
    devices::{registry, udisks::Devices},
    library::sources,
};
use rusqlite::{params, Connection};
use std::{
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct SourceRecord {
    pub id: String,
    pub root: PathBuf,
    pub volume: Option<String>,
    pub relative: Option<PathBuf>,
    pub label: String,
    pub kinds: Vec<MediaKind>,
    pub availability: String,
}
fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    let root: Vec<u8> = row.get(1)?;
    let relative: Option<Vec<u8>> = row.get(3)?;
    let kinds: String = row.get(5)?;
    Ok(SourceRecord {
        id: row.get(0)?,
        root: PathBuf::from(std::ffi::OsString::from_vec(root)),
        volume: row.get(2)?,
        relative: relative.map(|v| PathBuf::from(std::ffi::OsString::from_vec(v))),
        label: row.get(4)?,
        kinds: serde_json::from_str(&kinds).unwrap_or_default(),
        availability: row.get(6)?,
    })
}
pub fn records(db: &Connection) -> Result<Vec<SourceRecord>, AppError> {
    let mut query=db.prepare("SELECT id,root,volume_identity,relative_root,label,kinds,availability FROM media_sources ORDER BY label,id")?;
    let result = query
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}
pub fn get(db: &Connection, id: &str) -> Result<SourceRecord, AppError> {
    db.query_row("SELECT id,root,volume_identity,relative_root,label,kinds,availability FROM media_sources WHERE id=?",[id],from_row).map_err(|_|AppError::new("source_missing","This library source is unavailable."))
}
pub fn root(record: &SourceRecord, devices: &Devices) -> Result<PathBuf, AppError> {
    if let Some(identity) = &record.volume {
        let volume = registry::resolve(&devices.volumes, identity)?;
        let mount = volume.mounts.first().ok_or_else(|| {
            AppError::new("volume_unmounted", "Mount this drive to use its library.")
        })?;
        let relative = record.relative.as_deref().unwrap_or(Path::new(""));
        if relative.as_os_str().is_empty() {
            return Ok(mount.clone());
        }
        return sources::resolve_local(mount, relative);
    }
    if !record.root.is_dir() {
        return Err(AppError::new(
            "source_offline",
            "The selected folder is unavailable.",
        ));
    }
    Ok(record.root.clone())
}
pub fn add(
    db: &Connection,
    root: &Path,
    kinds: &[MediaKind],
    devices: &Devices,
) -> Result<String, AppError> {
    if kinds.is_empty() || kinds.len() > 3 {
        return Err(AppError::new(
            "invalid_kinds",
            "Select at least one media kind.",
        ));
    }
    let root = root.canonicalize()?;
    add_resolved(db, &root, kinds, devices)
}
/// The remote adapter has already pinned and revalidated this exact root.
pub(crate) fn add_resolved(
    db: &Connection,
    root: &Path,
    kinds: &[MediaKind],
    devices: &Devices,
) -> Result<String, AppError> {
    if kinds.is_empty() || kinds.len() > 3 {
        return Err(AppError::new(
            "invalid_kinds",
            "Select at least one media kind.",
        ));
    }
    let root = root.to_path_buf();
    if !root.is_dir() {
        return Err(AppError::new(
            "source_offline",
            "The selected folder is unavailable.",
        ));
    }
    let mut identity = None;
    let mut relative = None;
    let selected = devices
        .volumes
        .iter()
        .flat_map(|v| v.mounts.iter().map(move |mount| (v, mount)))
        .filter(|(_, mount)| root.starts_with(mount))
        .max_by_key(|(_, mount)| mount.components().count());
    if let Some((volume, mount)) = selected {
        if volume.removable {
            let id=volume.identity.clone().ok_or_else(||AppError::new("volume_identity_missing","This removable volume has no stable filesystem identity. It cannot yet be saved as a source."))?;
            registry::resolve(&devices.volumes, &id)?;
            relative = Some(root.strip_prefix(mount).unwrap().to_path_buf());
            identity = Some(id);
        }
    }
    for record in records(db)? {
        let same_volume = record.volume == identity;
        let existing = root_for_comparison(&record, devices);
        if same_volume && existing == root {
            let mut merged = record.kinds;
            for kind in kinds {
                if !merged.contains(kind) {
                    merged.push(*kind);
                }
            }
            db.execute(
                "UPDATE media_sources SET kinds=?1 WHERE id=?2",
                params![serde_json::to_string(&merged).unwrap(), record.id],
            )?;
            return Ok(record.id);
        }
        if same_volume && sources::overlaps(&existing, &root) {
            return Err(AppError::new("source_overlap","This folder overlaps an existing source. Use that source or remove its library association first."));
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let label = root
        .file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned();
    db.execute("INSERT INTO media_sources(id,source_type,root,volume_identity,relative_root,label,kinds,availability,last_seen_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,'available',?8)",params![id,if identity.is_some(){"volume"}else{"local"},root.as_os_str().as_bytes(),identity,relative.as_ref().map(|p|p.as_os_str().as_bytes()),label,serde_json::to_string(kinds).unwrap(),crate::app_state::now_ms() as i64])?;
    Ok(id)
}
fn root_for_comparison(record: &SourceRecord, devices: &Devices) -> PathBuf {
    root(record, devices).unwrap_or_else(|_| record.root.clone())
}
pub fn list(db: &Connection, devices: &Devices) -> Result<Vec<MediaSource>, AppError> {
    let mut result = vec![];
    for record in records(db)? {
        let resolved = root(&record, devices);
        let availability = match &resolved {
            Ok(_) => "available",
            Err(e) if e.code == "volume_ambiguous" => "ambiguous",
            Err(e) if e.code == "volume_unmounted" => "unmounted",
            _ => "offline",
        };
        let display = resolved
            .as_deref()
            .unwrap_or(&record.root)
            .to_string_lossy()
            .into_owned();
        db.execute(
            "UPDATE media_sources SET availability=?1 WHERE id=?2",
            params![availability, record.id],
        )?;
        result.push(MediaSource {
            id: record.id,
            label: record.label,
            display_path: display,
            kinds: record.kinds,
            availability: availability.into(),
            removable: record.volume.is_some(),
        });
    }
    Ok(result)
}
pub fn remove(db: &Connection, id: &str) -> Result<(), AppError> {
    db.execute("DELETE FROM media_sources WHERE id=?", [id])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removing_source_does_not_delete_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        std::fs::create_dir(&root).unwrap();
        let file = root.join("sample.mp4");
        std::fs::write(&file, b"original").unwrap();
        let db = crate::storage::database::open(&dir.path().join("db")).unwrap();
        let id = add(&db, &root, &[MediaKind::Video], &Devices::default()).unwrap();
        remove(&db, &id).unwrap();
        assert_eq!(std::fs::read(file).unwrap(), b"original");
        assert!(records(&db).unwrap().is_empty());
    }
    #[test]
    fn duplicate_root_merges_kinds_without_duplicate_index() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::database::open(&dir.path().join("db")).unwrap();
        let id = add(&db, dir.path(), &[MediaKind::Video], &Devices::default()).unwrap();
        assert_eq!(
            id,
            add(&db, dir.path(), &[MediaKind::Audio], &Devices::default()).unwrap()
        );
        assert_eq!(records(&db).unwrap().len(), 1);
        assert_eq!(get(&db, &id).unwrap().kinds.len(), 2);
    }
}
