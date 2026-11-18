use crate::{
    contracts::{AppError, MediaKind},
    process::runner::{self, CommandSpec, Executable},
};
use std::{ffi::OsString, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
pub const FORMATS: &str = "mov,matroska,avi,mp3,flac,wav,ogg,aac,png_pipe,jpeg_pipe,webp_pipe";
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub duration_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub tags: serde_json::Value,
}
pub fn candidate(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" => Some(MediaKind::Video),
        "mp3" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wav" => Some(MediaKind::Audio),
        "jpg" | "jpeg" | "png" | "webp" | "avif" | "heif" | "heic" => Some(MediaKind::Photo),
        _ => None,
    }
}
pub fn local_args() -> Vec<OsString> {
    [
        "-v",
        "error",
        "-max_alloc",
        "67108864",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        FORMATS,
    ]
    .iter()
    .map(OsString::from)
    .collect()
}
pub async fn inspect(
    file: &std::fs::File,
    kind: MediaKind,
    cancel: CancellationToken,
) -> Result<Metadata, AppError> {
    let mut args = local_args();
    args.extend(
        [
            "-show_entries",
            "format=duration:format_tags=title,artist,album:stream=codec_type,width,height",
            "-of",
            "json",
            "-i",
        ]
        .iter()
        .map(OsString::from),
    );
    args.push(super::files::descriptor_path(file).into_os_string());
    let output = runner::run(
        CommandSpec {
            executable: Executable::Ffprobe,
            args,
            timeout: Duration::from_secs(15),
            output_limit: 256 * 1024,
        },
        cancel,
    )
    .await
    .map_err(|code| AppError::new(&code, "Media inspection failed or timed out."))?;
    if !output.success {
        return Err(AppError::new(
            "unsupported_media",
            "This file could not be decoded with the installed media tools.",
        ));
    }
    parse(&output.stdout, kind)
}
pub fn parse(bytes: &[u8], kind: MediaKind) -> Result<Metadata, AppError> {
    let data: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::new("invalid_metadata", "Media metadata could not be read."))?;
    let required = if kind == MediaKind::Audio {
        "audio"
    } else {
        "video"
    };
    let stream = data["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"] == required))
        .ok_or_else(|| {
            AppError::new(
                "unsupported_media",
                "The file contains no supported media stream.",
            )
        })?;
    let width = stream["width"].as_u64().and_then(|v| u32::try_from(v).ok());
    let height = stream["height"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok());
    if width
        .zip(height)
        .is_some_and(|(w, h)| u64::from(w) * u64::from(h) > 40_000_000)
    {
        return Err(AppError::new(
            "image_too_large",
            "The image exceeds the supported decoding size.",
        ));
    }
    let duration_ms = data["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0. && *v < 1e9)
        .map(|v| (v * 1000.) as u64);
    let mut tags = serde_json::Map::new();
    for key in ["title", "artist", "album"] {
        if let Some(value) = data["format"]["tags"][key].as_str() {
            tags.insert(
                key.into(),
                value.chars().take(500).collect::<String>().into(),
            );
        }
    }
    Ok(Metadata {
        duration_ms,
        width,
        height,
        tags: tags.into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn local_only_probe_uses_open_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let mut data = vec![];
        data.extend(b"RIFF");
        data.extend(2036u32.to_le_bytes());
        data.extend(b"WAVEfmt ");
        data.extend(16u32.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(8000u32.to_le_bytes());
        data.extend(16000u32.to_le_bytes());
        data.extend(2u16.to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(b"data");
        data.extend(2000u32.to_le_bytes());
        data.resize(2044, 0);
        std::fs::write(&path, data).unwrap();
        let file = super::super::files::open(dir.path(), Path::new("tone.wav")).unwrap();
        let meta = inspect(&file, MediaKind::Audio, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(meta.duration_ms, Some(125));
    }
    #[tokio::test]
    async fn external_reference_never_fetches_http() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        server.set_nonblocking(true).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("video.mp4"),
            format!(
                "ffconcat version 1.0\nfile 'http://127.0.0.1:{}/media'\n",
                server.local_addr().unwrap().port()
            ),
        )
        .unwrap();
        let file = super::super::files::open(dir.path(), Path::new("video.mp4")).unwrap();
        assert!(inspect(&file, MediaKind::Video, CancellationToken::new())
            .await
            .is_err());
        assert_eq!(
            server.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
