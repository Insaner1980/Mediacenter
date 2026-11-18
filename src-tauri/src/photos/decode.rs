use crate::{
    app_state::AppState,
    contracts::{AppError, EncodedImage, MediaKind},
    process::runner::{self, CommandSpec, Executable},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, os::unix::fs::FileExt, time::Duration};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PhotoImage {
    pub image: EncodedImage,
    pub width: u32,
    pub height: u32,
    #[ts(type = "number | null")]
    pub modified_at_ms: Option<i64>,
}
fn failed() -> AppError {
    AppError::new(
        "photo_decode_failed",
        "This photo is corrupt or uses an unsupported image format.",
    )
}
fn raster_format(file: &std::fs::File) -> Result<&'static str, AppError> {
    if file.metadata()?.len() > 64 * 1024 * 1024 {
        return Err(AppError::new(
            "image_too_large",
            "This photo exceeds the 64 MiB image limit.",
        ));
    }
    let mut header = [0u8; 32];
    let n = file.read_at(&mut header, 0)?;
    let header = &header[..n];
    if header.starts_with(&[255, 216, 255]) {
        Ok("jpeg_pipe")
    } else if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("png_pipe")
    } else if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") {
        Ok("webp_pipe")
    } else if header.get(4..8) == Some(b"ftyp")
        && matches!(
            header.get(8..12),
            Some(b"avif" | b"avis" | b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1")
        )
    {
        Ok("mov")
    } else {
        Err(AppError::new(
            "unsupported_photo",
            "This file is not a supported raster photo.",
        ))
    }
}
fn input_args(file: &std::fs::File, format: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-v",
        "error",
        "-max_alloc",
        "67108864",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        "jpeg_pipe,png_pipe,webp_pipe,mov",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    if format == "mov" {
        args.extend(
            ["-enable_drefs", "0", "-use_absolute_path", "0"]
                .iter()
                .map(OsString::from),
        );
    }
    args.extend(
        ["-threads", "1", "-f", format, "-i"]
            .iter()
            .map(OsString::from),
    );
    args.push(crate::library::files::descriptor_path(file).into_os_string());
    args
}
pub async fn decode_file(
    file: &std::fs::File,
    cancel: CancellationToken,
) -> Result<PhotoImage, AppError> {
    if cancel.is_cancelled() {
        return Err(AppError::new("cancelled", "Photo loading stopped."));
    }
    let format = raster_format(file)?;
    let mut probe = input_args(file, format);
    probe.extend(
        ["-show_entries", "stream=width,height", "-of", "json"]
            .iter()
            .map(OsString::from),
    );
    let result = runner::run(
        CommandSpec {
            executable: Executable::Ffprobe,
            args: probe,
            timeout: Duration::from_secs(15),
            output_limit: 64 * 1024,
        },
        cancel.clone(),
    )
    .await
    .map_err(|_| failed())?;
    if !result.success {
        return Err(failed());
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&result.stdout).map_err(|_| failed())?;
    let width = metadata["streams"][0]["width"]
        .as_u64()
        .ok_or_else(failed)?;
    let height = metadata["streams"][0]["height"]
        .as_u64()
        .ok_or_else(failed)?;
    if width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || width.saturating_mul(height) > 40_000_000
    {
        return Err(AppError::new(
            "image_too_large",
            "This photo exceeds the supported 40 megapixel decoding size.",
        ));
    }
    let mut args = input_args(file, format);
    // ffmpeg applies the image display matrix / EXIF orientation before the scale filter.
    args.extend(
        [
            "-map",
            "0:v:0",
            "-vf",
            "scale=w='min(1920,iw)':h='min(1080,ih)':force_original_aspect_ratio=decrease",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "-q:v",
            "3",
            "pipe:1",
        ]
        .iter()
        .map(OsString::from),
    );
    let result = runner::run(
        CommandSpec {
            executable: Executable::Ffmpeg,
            args,
            timeout: Duration::from_secs(15),
            output_limit: 4 * 1024 * 1024,
        },
        cancel.clone(),
    )
    .await
    .map_err(|_| failed())?;
    if !result.success || cancel.is_cancelled() {
        return Err(failed());
    }
    let (width, height) = jpeg_dimensions(&result.stdout).ok_or_else(failed)?;
    if width > 1920 || height > 1080 {
        return Err(failed());
    }
    Ok(PhotoImage {
        image: EncodedImage {
            mime: "image/jpeg".into(),
            base64: base64::engine::general_purpose::STANDARD.encode(result.stdout),
        },
        width,
        height,
        modified_at_ms: None,
    })
}
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[255, 216]) || !bytes.ends_with(&[255, 217]) {
        return None;
    }
    let mut offset = 2;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 255 {
            return None;
        }
        let marker = bytes[offset + 1];
        let length =
            u16::from_be_bytes(bytes.get(offset + 2..offset + 4)?.try_into().ok()?) as usize;
        if length < 2 || offset + 2 + length > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3) {
            let height =
                u16::from_be_bytes(bytes.get(offset + 5..offset + 7)?.try_into().ok()?) as u32;
            let width =
                u16::from_be_bytes(bytes.get(offset + 7..offset + 9)?.try_into().ok()?) as u32;
            return (width > 0 && height > 0).then_some((width, height));
        }
        offset += length + 2;
    }
    None
}
pub async fn from_media(
    media: &crate::library::query::OpenMedia,
    cancel: CancellationToken,
) -> Result<PhotoImage, AppError> {
    if media.kind != MediaKind::Photo {
        return Err(AppError::new(
            "not_photo",
            "Choose an image from your photo library.",
        ));
    }
    // Relay both cancellation owners, then await decoder shutdown before releasing its lease.
    let decode_cancel = media.lease.token().child_token();
    let decoding = decode_file(&media.file, decode_cancel.clone());
    tokio::pin!(decoding);
    let mut image = tokio::select! {
        _=cancel.cancelled()=>{ decode_cancel.cancel(); decoding.await? },
        image=&mut decoding=>image?,
    };
    image.modified_at_ms = media
        .mtime
        .split(':')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|s| s.checked_mul(1000));
    Ok(image)
}
pub async fn get(
    state: &AppState,
    id: &str,
    cancel: CancellationToken,
) -> Result<PhotoImage, AppError> {
    let _permit = tokio::select! { _=cancel.cancelled()=>return Err(failed()), permit=state.thumbnail_gate.acquire()=>permit.map_err(|_|failed())? };
    let _probe = tokio::select! { _=cancel.cancelled()=>return Err(failed()), permit=state.probes.acquire()=>permit.map_err(|_|failed())? };
    let media = crate::library::query::open(state, id).await?;
    from_media(&media, cancel).await
}
