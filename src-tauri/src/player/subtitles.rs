use crate::contracts::AppError;
use std::{fs::File, path::Path};
pub struct Sidecar {
    pub file: File,
    pub title: String,
}
pub fn adjacent(root: &Path, video: &Path) -> Result<Vec<Sidecar>, AppError> {
    use std::{os::unix::ffi::OsStrExt, path::Component};
    if video.as_os_str().is_empty()
        || video
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(AppError::new(
            "source_escape",
            "Subtitles must remain within the selected source.",
        ));
    }
    let parent = video.parent().unwrap_or_else(|| Path::new(""));
    let directory = crate::library::files::open_directory(root, parent)?;
    let stem = video
        .file_stem()
        .ok_or_else(|| AppError::new("invalid_media", "The video filename is unavailable."))?
        .as_bytes();
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(crate::library::files::descriptor_path(&directory))?.take(4096) {
        let entry = entry?;
        let name = entry.file_name();
        let path = Path::new(&name);
        let Some(extension) = path.extension() else {
            continue;
        };
        if ![b"srt".as_slice(), b"ass", b"ssa", b"vtt"]
            .contains(&extension.as_bytes().to_ascii_lowercase().as_slice())
        {
            continue;
        }
        let Some(candidate) = path.file_stem() else {
            continue;
        };
        let candidate = candidate.as_bytes();
        if candidate != stem
            && !(candidate.starts_with(stem) && candidate.get(stem.len()) == Some(&b'.'))
        {
            continue;
        }
        candidates.push(name);
    }
    candidates.sort();
    let mut result = Vec::new();
    for name in candidates {
        if result.len() >= 16 {
            break;
        }
        let Ok(file) = crate::library::files::open(root, &parent.join(&name)) else {
            continue;
        };
        if !file
            .metadata()
            .is_ok_and(|m| m.len() > 0 && m.len() <= 8 * 1024 * 1024)
        {
            continue;
        }
        result.push(Sidecar {
            file,
            title: name.to_string_lossy().chars().take(200).collect(),
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, os::unix::ffi::OsStringExt};
    #[test]
    fn authorized_adjacent_subtitles_preserve_raw_names_and_descriptors() {
        let dir = tempfile::tempdir().unwrap();
        let stem = std::ffi::OsString::from_vec(b"elokuva\xff".to_vec());
        let video = Path::new(&stem).with_extension("mp4");
        let subtitle = Path::new(&stem).with_extension("fi.srt");
        std::fs::write(dir.path().join(&video), b"video").unwrap();
        std::fs::write(
            dir.path().join(&subtitle),
            b"1\n00:00:00,000 --> 00:00:01,000\nHei\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("unrelated.srt"), b"unrelated").unwrap();
        let mut tracks = adjacent(dir.path(), &video).unwrap();
        assert_eq!(tracks.len(), 1);
        std::fs::remove_file(dir.path().join(subtitle)).unwrap();
        let mut text = String::new();
        tracks[0].file.read_to_string(&mut text).unwrap();
        assert!(text.contains("Hei"));
    }
    #[tokio::test]
    async fn real_mpv_loads_authorized_sidecar_with_production_restrictions() {
        use crate::{
            contracts::MediaKind,
            library::{query::OpenMedia, resources::Resources},
        };
        use serde_json::json;
        use std::{os::fd::OwnedFd, process::Stdio, time::Duration};
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("film.wav");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&16036_u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8000_u32.to_le_bytes());
        bytes.extend_from_slice(&16000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&16000_u32.to_le_bytes());
        bytes.resize(16044, 0);
        std::fs::write(&wav, bytes).unwrap();
        std::fs::write(
            dir.path().join("film.fi.srt"),
            b"1\n00:00:00,000 --> 00:00:00,900\nHei\n",
        )
        .unwrap();
        let tracks = adjacent(dir.path(), Path::new("film.wav")).unwrap();
        assert_eq!(tracks.len(), 1);
        let resources = Resources::default();
        let media = OpenMedia {
            lease: resources
                .acquire("s", tokio_util::sync::CancellationToken::new())
                .unwrap(),
            root: dir.path().into(),
            relative: "film.wav".into(),
            root_device: 0,
            root_inode: 0,
            file: File::open(wav).unwrap(),
            kind: MediaKind::Audio,
            source_id: "s".into(),
            size: 16044,
            mtime: "0:0".into(),
        };
        let (parent, socket) = std::os::unix::net::UnixStream::pair().unwrap();
        parent.set_nonblocking(true).unwrap();
        let client = super::super::ipc::Client::new(
            tokio::net::UnixStream::from_std(parent).unwrap(),
            Duration::from_secs(5),
        );
        let mut events = client.subscribe();
        let socket: OwnedFd = socket.into();
        let mut child = tokio::process::Command::new("/usr/bin/mpv")
            .args(["--vo=null", "--ao=null"])
            .args(super::super::mpv::arguments(&media))
            .stdin(Stdio::from(socket))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if events.recv().await.unwrap()["event"] == "file-loaded" {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let loaded = client
            .command(json!([
                "sub-add",
                crate::library::files::descriptor_path(&tracks[0].file),
                "auto",
                tracks[0].title
            ]))
            .await;
        let listed = client
            .command(json!(["get_property", "track-list"]))
            .await
            .unwrap();
        let _ = client.command(json!(["quit"])).await;
        tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap();
        loaded.unwrap();
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(|track| track["type"] == "sub" && track["external"] == true));
    }
    #[test]
    fn external_sidecar_links_and_parent_escape_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("film.srt"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            external.path().join("film.srt"),
            dir.path().join("film.srt"),
        )
        .unwrap();
        assert!(adjacent(dir.path(), Path::new("film.mp4"))
            .unwrap()
            .is_empty());
        assert!(adjacent(dir.path(), Path::new("../film.mp4")).is_err());
    }
}
