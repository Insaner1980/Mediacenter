//! Explicit native-to-wire translations. Paths and native error text never cross this boundary.
use super::protocol::*;
use crate::contracts as native;
use serde::de::DeserializeOwned;

pub(crate) fn checked<T: DeserializeOwned, S: serde::Serialize>(
    value: S,
) -> Result<T, RemoteError> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| RemoteError::invalid())?)
        .map_err(|_| RemoteError::invalid())
}

/// Native PulseAudio names are backend arguments, never wire identifiers.
pub(crate) fn audio_output_id(native_id: &str) -> Id {
    use sha2::{Digest, Sha256};
    checked(format!("audio-{:x}", Sha256::digest(native_id.as_bytes())))
        .expect("fixed-length hexadecimal audio identifier")
}

pub(crate) fn resolve_audio_output<'a>(
    inventory: &'a [native::AudioSinkDescriptor],
    id: &Id,
) -> Result<&'a native::AudioSinkDescriptor, RemoteError> {
    inventory
        .iter()
        .find(|output| output.available && audio_output_id(&output.id) == *id)
        .ok_or_else(|| {
            RemoteError::new(
                ErrorCode::AudioOutputUnavailable,
                "Audio output unavailable.",
            )
        })
}

impl From<native::MediaKind> for MediaKind {
    fn from(kind: native::MediaKind) -> Self {
        match kind {
            native::MediaKind::Video => Self::Video,
            native::MediaKind::Audio => Self::Music,
            native::MediaKind::Photo => Self::Photo,
        }
    }
}
impl From<MediaKind> for native::MediaKind {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Video => Self::Video,
            MediaKind::Music => Self::Audio,
            MediaKind::Photo => Self::Photo,
        }
    }
}
impl From<Theme> for native::Theme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::Dark,
            Theme::Light => Self::Light,
        }
    }
}
impl TryFrom<&native::MediaItem> for MediaItem {
    type Error = RemoteError;
    fn try_from(item: &native::MediaItem) -> Result<Self, Self::Error> {
        Ok(Self {
            id: checked(&item.id)?,
            source_id: checked(&item.source_id)?,
            kind: item.kind.into(),
            name: checked(&item.display_name)?,
            availability: checked(&item.availability)?,
            duration_seconds: item
                .duration_ms
                .map(|ms| checked(ms as f64 / 1000.0))
                .transpose()?,
            resume_position_seconds: item
                .resume_position_ms
                .map(|ms| checked(ms as f64 / 1000.0))
                .transpose()?,
            thumbnail_id: item.thumbnail_id.as_ref().map(checked).transpose()?,
        })
    }
}

impl TryFrom<&native::PlayerSnapshot> for PlayerTopic {
    type Error = RemoteError;
    fn try_from(player: &native::PlayerSnapshot) -> Result<Self, Self::Error> {
        let track = |track: &native::PlayerTrack| -> Result<Track, RemoteError> {
            Ok(Track {
                id: checked(track.id.to_string())?,
                label: checked(&track.title)?,
                language: None,
                selected: track.selected,
            })
        };
        let tracks = |kind: &str| -> Result<Tracks, RemoteError> {
            checked(
                player
                    .tracks
                    .iter()
                    .filter(|t| t.kind == kind)
                    .map(track)
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };
        Ok(Self {
            revision: player.revision.into(),
            session_id: (!player.session_id.is_empty())
                .then(|| checked(&player.session_id))
                .transpose()?,
            item_id: (!player.media_id.is_empty())
                .then(|| checked(&player.media_id))
                .transpose()?,
            phase: checked(if player.paused && player.phase == "playing" {
                "paused"
            } else {
                match player.phase.as_str() {
                    "ready" if player.paused => "paused",
                    "ready" => "opening",
                    "ended" => "stopped",
                    "error" => "failed",
                    phase => phase,
                }
            })?,
            position_seconds: checked(player.position_seconds)?,
            duration_seconds: player.duration_seconds.map(checked).transpose()?,
            volume: checked(player.volume)?,
            muted: player.muted,
            audio_tracks: tracks("audio")?,
            subtitle_tracks: tracks("sub")?,
        })
    }
}
impl TryFrom<PlayerAction> for native::PlayerAction {
    type Error = RemoteError;
    fn try_from(action: PlayerAction) -> Result<Self, Self::Error> {
        fn track_id(id: Id) -> Result<i32, RemoteError> {
            let value = id
                .as_str()
                .parse::<i32>()
                .map_err(|_| RemoteError::invalid())?;
            if value < 0 || value.to_string() != id.as_str() {
                return Err(RemoteError::invalid());
            }
            Ok(value)
        }
        Ok(match action {
            PlayerAction::PlayerPause { params } => Self::Pause {
                paused: params.paused,
            },
            PlayerAction::PlayerSeek { params } => Self::Seek {
                seconds: params.position_seconds.value(),
            },
            PlayerAction::PlayerVolume { params } => Self::Volume {
                value: params.volume.value(),
            },
            PlayerAction::PlayerMute { params } => Self::Mute {
                muted: params.muted,
            },
            PlayerAction::PlayerAudioTrack { params } => Self::AudioTrack {
                id: track_id(params.track_id)?,
            },
            PlayerAction::PlayerSubtitleTrack { params } => Self::SubtitleTrack {
                id: params.track_id.map(track_id).transpose()?,
            },
            PlayerAction::PlayerStop { .. } => Self::Stop,
        })
    }
}

pub fn queue_entry_id(value: u32) -> Id {
    checked(format!("queue-entry-{value}")).expect("bounded native queue ID")
}
pub fn native_queue_entry_id(id: &Id) -> Result<u32, RemoteError> {
    let value = id
        .as_str()
        .strip_prefix("queue-entry-")
        .ok_or_else(RemoteError::invalid)?;
    let parsed = value.parse::<u32>().map_err(|_| RemoteError::invalid())?;
    if parsed.to_string() != value {
        return Err(RemoteError::invalid());
    }
    Ok(parsed)
}
pub fn native_queue_revision(value: &Decimal) -> Result<u32, RemoteError> {
    value.value().try_into().map_err(|_| RemoteError::invalid())
}

impl TryFrom<&crate::music::queue::MusicQueue> for QueueSummary {
    type Error = RemoteError;
    fn try_from(queue: &crate::music::queue::MusicQueue) -> Result<Self, Self::Error> {
        Ok(Self {
            revision: u64::from(queue.revision).into(),
            count: (queue.items.len() as u64).into(),
            session_id: queue.session_id.as_ref().map(checked).transpose()?,
            current_entry_id: queue
                .current_index
                .and_then(|i| queue.items.get(i))
                .map(|entry| queue_entry_id(entry.entry_id)),
            shuffle: queue.shuffle,
            repeat: match queue.repeat {
                crate::music::queue::RepeatMode::Off => RepeatMode::Off,
                crate::music::queue::RepeatMode::One => RepeatMode::One,
                crate::music::queue::RepeatMode::All => RepeatMode::All,
            },
        })
    }
}

impl From<Fit> for crate::photos::presenter::SlideshowAction {
    fn from(fit: Fit) -> Self {
        Self::Fit {
            actual: matches!(fit, Fit::Actual),
        }
    }
}

pub fn slideshow_topic(
    snapshot: &crate::photos::presenter::SlideshowSnapshot,
    revision: Decimal,
) -> Result<SlideshowTopic, RemoteError> {
    Ok(SlideshowTopic {
        revision,
        session_id: snapshot.session_id.as_ref().map(checked).transpose()?,
        current_item_id: snapshot.media_id.as_ref().map(checked).transpose()?,
        paused: snapshot.paused,
        interval_seconds: checked(snapshot.interval_seconds)?,
        shuffle: snapshot.shuffle,
        fit: checked(&snapshot.fit)?,
    })
}

#[cfg(test)]
mod audio_tests {
    use super::*;
    #[test]
    fn opaque_audio_ids_resolve_only_to_the_current_available_native_sink() {
        let native_id = "alsa_output.pci-0000_00_1f.3.hdmi-stereo";
        let mut sinks = vec![native::AudioSinkDescriptor {
            id: native_id.into(),
            backend_name: native_id.into(),
            name: "TV HDMI".into(),
            available: true,
        }];
        let wire_id = audio_output_id(native_id);
        assert_eq!(
            resolve_audio_output(&sinks, &wire_id).unwrap().id,
            native_id
        );
        assert!(resolve_audio_output(&sinks, &checked("unknown-output").unwrap()).is_err());
        assert!(resolve_audio_output(&sinks, &audio_output_id("other-native-sink")).is_err());
        sinks[0].available = false;
        assert_eq!(
            resolve_audio_output(&sinks, &wire_id).unwrap_err().code,
            ErrorCode::AudioOutputUnavailable
        );
        assert!(resolve_audio_output(&[], &wire_id).is_err());
        assert_ne!(audio_output_id(wire_id.as_str()), wire_id);
    }
}
