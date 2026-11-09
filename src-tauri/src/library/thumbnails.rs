use crate::{
    app_state::AppState,
    contracts::*,
    process::runner::{self, CommandSpec, Executable},
};
use base64::Engine;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, time::Duration};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy)]
struct PreviewProfile {
    tag: &'static str,
    edge: u32,
    bytes: usize,
    scale: &'static str,
    publish_id: bool,
}
const DESKTOP: PreviewProfile = PreviewProfile {
    tag: "png640-v1",
    edge: 640,
    bytes: 2 * 1024 * 1024,
    scale: "scale=640:640:force_original_aspect_ratio=decrease",
    publish_id: true,
};
const REMOTE: PreviewProfile = PreviewProfile {
    tag: "png512-v1",
    edge: 512,
    bytes: 512 * 1024,
    scale: "scale=512:512:force_original_aspect_ratio=decrease",
    publish_id: false,
};
pub async fn get(
    state: &AppState,
    id: &str,
    cancel: CancellationToken,
) -> Result<EncodedImage, AppError> {
    get_profile(state, id, cancel, DESKTOP).await.map(encoded)
}
async fn get_profile(
    state: &AppState,
    id: &str,
    cancel: CancellationToken,
    profile: PreviewProfile,
) -> Result<Vec<u8>, AppError> {
    let _permit = tokio::select! {_=cancel.cancelled()=>return Err(AppError::new("cancelled","Image loading cancelled.")),permit=state.thumbnail_gate.acquire()=>permit.map_err(|_|AppError::new("thumbnail_unavailable","Image decoding is unavailable."))?};
    let media = super::query::open(state, id).await?;
    if media.kind == MediaKind::Audio {
        return Err(AppError::new(
            "thumbnail_unavailable",
            "This audio item has no image preview.",
        ));
    }
    let fingerprint = format!(
        "{}:{id}:{}:{}:{}",
        media.source_id, media.size, media.mtime, profile.tag
    );
    let key = format!("{:x}", Sha256::digest(fingerprint.as_bytes()));
    let directory = state.cache_dir.join("thumbnails");
    tokio::fs::create_dir_all(&directory).await?;
    let path = directory.join(format!("{key}.png"));
    if let Some(bytes) = read_cached(&path, profile.bytes).await {
        if valid_preview(&bytes, profile) {
            if cancel.is_cancelled() || media.lease.cancelled() {
                return Err(AppError::new("cancelled", "Image loading cancelled."));
            }
            if record(
                state,
                &key,
                id,
                &fingerprint,
                bytes.len(),
                profile.publish_id,
            )? {
                evict(state).await?;
            }
            return Ok(bytes);
        }
    }
    let mut args = super::probe::local_args();
    args.extend(["-threads", "1", "-i"].iter().map(OsString::from));
    args.push(super::files::descriptor_path(&media.file).into_os_string());
    args.extend(
        [
            "-vf",
            profile.scale,
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ]
        .iter()
        .map(OsString::from),
    );
    let lease_cancel = media.lease.token();
    let work = runner::run(
        CommandSpec {
            executable: Executable::Ffmpeg,
            args,
            timeout: Duration::from_secs(15),
            output_limit: profile.bytes,
        },
        lease_cancel.clone(),
    );
    tokio::pin!(work);
    let output=tokio::select!{_=cancel.cancelled()=>{lease_cancel.cancel();work.await},result=&mut work=>result}
    .map_err(|_| {
        AppError::new(
            "thumbnail_failed",
            "The image preview could not be decoded.",
        )
    })?;
    if !output.success
        || cancel.is_cancelled()
        || media.lease.cancelled()
        || !valid_preview(&output.stdout, profile)
    {
        return Err(AppError::new(
            "thumbnail_failed",
            "The installed decoder could not create an image preview.",
        ));
    }
    tokio::fs::write(&path, &output.stdout).await?;
    record(
        state,
        &key,
        id,
        &fingerprint,
        output.stdout.len(),
        profile.publish_id,
    )?;
    evict(state).await?;
    Ok(output.stdout)
}
fn encoded(bytes: Vec<u8>) -> EncodedImage {
    EncodedImage {
        mime: "image/png".into(),
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}
/// Validate framing and CRCs before a cached image reaches a webview. A header alone
/// is not a valid image, even when it advertises bounded dimensions.
pub fn valid_png(bytes: &[u8]) -> bool {
    if bytes.len() < 45
        || bytes.len() > 2 * 1024 * 1024
        || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10]
        || bytes[8..12] != 13u32.to_be_bytes()
        || bytes[12..16] != *b"IHDR"
        || !(1..=640).contains(&u32::from_be_bytes(bytes[16..20].try_into().unwrap()))
        || !(1..=640).contains(&u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
    {
        return false;
    }
    let mut offset = 8usize;
    let mut has_pixels = false;
    while offset + 12 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let Some(end) = offset
            .checked_add(12)
            .and_then(|v| v.checked_add(size))
            .filter(|end| *end <= bytes.len())
        else {
            return false;
        };
        let kind = &bytes[offset + 4..offset + 8];
        let expected = u32::from_be_bytes(bytes[end - 4..end].try_into().unwrap());
        if crc32(&bytes[offset + 4..end - 4]) != expected {
            return false;
        }
        if kind == b"IDAT" {
            has_pixels |= size > 0;
        }
        if kind == b"IEND" {
            return has_pixels && size == 0 && end == bytes.len();
        }
        offset = end;
    }
    false
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}
async fn read_cached(path: &std::path::Path, maximum: usize) -> Option<Vec<u8>> {
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    Some(bytes)
}
fn record(
    state: &AppState,
    key: &str,
    id: &str,
    fingerprint: &str,
    bytes: usize,
    publish_id: bool,
) -> Result<bool, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
    let previous_bytes: Option<i64> = db
        .query_row(
            "SELECT bytes FROM thumbnail_entries WHERE cache_key=?",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    db.execute("INSERT INTO thumbnail_entries(cache_key,media_id,fingerprint,filename,bytes,last_access_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(cache_key) DO UPDATE SET media_id=excluded.media_id,fingerprint=excluded.fingerprint,filename=excluded.filename,bytes=excluded.bytes,last_access_ms=excluded.last_access_ms",rusqlite::params![key,id,fingerprint,format!("{key}.png"),bytes as i64,crate::app_state::now_ms() as i64])?;
    if publish_id {
        db.execute(
            "UPDATE media_items SET thumbnail_id=?1 WHERE id=?2",
            rusqlite::params![key, id],
        )?;
    }
    Ok(previous_bytes != Some(bytes as i64))
}
async fn evict(state: &AppState) -> Result<(), AppError> {
    let entries = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        let mut statement = db.prepare(
            "SELECT cache_key,bytes FROM thumbnail_entries ORDER BY last_access_ms DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let known: std::collections::HashSet<_> = entries
        .iter()
        .map(|(key, _)| format!("{key}.png"))
        .collect();
    let directory = state.cache_dir.join("thumbnails");
    if let Ok(mut files) = tokio::fs::read_dir(&directory).await {
        while let Some(entry) = files.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() == 68
                && name.ends_with(".png")
                && name[..64].bytes().all(|b| b.is_ascii_hexdigit())
                && !known.contains(&name)
            {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
    let mut retained = 0i64;
    for (key, bytes) in entries {
        retained = retained.saturating_add(bytes.max(0));
        if retained <= 512 * 1024 * 1024 {
            continue;
        }
        if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let _ = tokio::fs::remove_file(
            state
                .cache_dir
                .join("thumbnails")
                .join(format!("{key}.png")),
        )
        .await;
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "The library is unavailable."))?;
        db.execute("DELETE FROM thumbnail_entries WHERE cache_key=?", [&key])?;
        db.execute(
            "UPDATE media_items SET thumbnail_id=NULL WHERE thumbnail_id=?",
            [&key],
        )?;
    }
    Ok(())
}

fn valid_preview(bytes: &[u8], profile: PreviewProfile) -> bool {
    valid_png(bytes)
        && bytes.len() <= profile.bytes
        && u32::from_be_bytes(bytes[16..20].try_into().unwrap()) <= profile.edge
        && u32::from_be_bytes(bytes[20..24].try_into().unwrap()) <= profile.edge
}
/// Bounded remote preview selected only by trusted opaque cache metadata. Open
/// and source-lease revalidation are shared with desktop previews, even on hits.
pub async fn get_remote(
    state: &AppState,
    opaque: &str,
    cancel: CancellationToken,
) -> Result<Vec<u8>, AppError> {
    if opaque.len() != 64 || !opaque.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::new(
            "thumbnail_unavailable",
            "Preview unavailable.",
        ));
    }
    let id: String = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::new("database_error", "Preview unavailable."))?;
        db.query_row(
            "SELECT media_id FROM thumbnail_entries WHERE cache_key=?",
            [opaque],
            |r| r.get(0),
        )?
    };
    get_profile(state, &id, cancel, REMOTE).await
}
