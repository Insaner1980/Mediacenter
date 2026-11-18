use crate::contracts::AppError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MusicItem {
    pub media_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub availability: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    pub entry_id: u32,
    pub item: MusicItem,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    One,
    All,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MusicQueue {
    pub revision: u32,
    pub items: Vec<QueueEntry>,
    pub original_media_ids: Vec<String>,
    pub current_index: Option<usize>,
    pub session_id: Option<String>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MusicAction {
    Next,
    Previous,
    PlayAt { index: usize },
    SetShuffle { enabled: bool },
    SetRepeat { mode: RepeatMode },
    Enqueue { media_ids: Vec<String> },
    RemoveAt { index: usize },
    MoveItem { from: usize, to: usize },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Queue {
    pub(crate) original: Vec<QueueEntry>,
    pub(crate) order: Vec<u32>,
    pub(crate) current: Option<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) revision: u32,
    pub(crate) shuffle: bool,
    pub(crate) repeat: RepeatMode,
    pub(crate) message: Option<String>,
    next_id: u32,
}
impl Queue {
    pub fn load(db: &rusqlite::Connection) -> Result<Self, AppError> {
        use rusqlite::OptionalExtension;
        let saved: Option<String> = db
            .query_row(
                "SELECT value_json FROM settings WHERE key='music_queue'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let Some(saved) = saved else {
            return Ok(Self::default());
        };
        let mut q: Self = serde_json::from_str(&saved).map_err(|_| {
            AppError::new("queue_invalid", "The saved music queue could not be read.")
        })?;
        let ids: std::collections::HashSet<_> = q.original.iter().map(|e| e.entry_id).collect();
        if q.original.len() > 10000
            || ids.len() != q.original.len()
            || q.order.len() != ids.len()
            || q.order
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                != ids
            || q.current.is_some_and(|id| !ids.contains(&id))
        {
            return Err(AppError::new(
                "queue_invalid",
                "The saved music queue is invalid.",
            ));
        }
        q.session_id = None;
        q.revision = q.revision.wrapping_add(1);
        Ok(q)
    }
    pub fn replace(&mut self, items: Vec<MusicItem>, index: usize) -> Result<(), AppError> {
        if items.len() > 10000 {
            return Err(limit());
        }
        if (!items.is_empty() && index >= items.len()) || (items.is_empty() && index != 0) {
            return Err(index_error());
        }
        let revision = self.revision.wrapping_add(1);
        *self = Self {
            original: items
                .into_iter()
                .enumerate()
                .map(|(i, item)| QueueEntry {
                    entry_id: i as u32,
                    item,
                })
                .collect(),
            revision,
            ..Self::default()
        };
        self.order = self.original.iter().map(|e| e.entry_id).collect();
        self.next_id = self.order.len() as u32;
        self.select_from(index, false);
        Ok(())
    }
    pub fn snapshot(&self) -> MusicQueue {
        let entries: std::collections::HashMap<_, _> =
            self.original.iter().map(|e| (e.entry_id, e)).collect();
        MusicQueue {
            revision: self.revision,
            items: self
                .order
                .iter()
                .filter_map(|id| entries.get(id).map(|e| (*e).clone()))
                .collect(),
            original_media_ids: self
                .original
                .iter()
                .map(|e| e.item.media_id.clone())
                .collect(),
            current_index: self
                .current
                .and_then(|id| self.order.iter().position(|v| *v == id)),
            session_id: self.session_id.clone(),
            shuffle: self.shuffle,
            repeat: self.repeat,
            message: self.message.clone(),
        }
    }
    pub fn matches_session(&self, id: Option<&str>) -> bool {
        self.session_id.as_deref() == id
    }
    pub fn can_advance(&self, session: &str, active: Option<&str>) -> bool {
        self.matches_session(Some(session)) && active == Some(session)
    }
    pub fn validate(&self, revision: u32) -> Result<(), AppError> {
        if revision != self.revision {
            Err(AppError::new(
                "queue_stale",
                "The music queue changed. Refresh it before editing.",
            ))
        } else {
            Ok(())
        }
    }
    pub fn edit(
        &mut self,
        revision: u32,
        action: MusicAction,
        items: Vec<MusicItem>,
        seed: u64,
    ) -> Result<bool, AppError> {
        self.validate(revision)?;
        let mut next = self.clone();
        let mut playback = false;
        match action {
            MusicAction::Next => {
                next.advance(false, false);
                playback = true;
            }
            MusicAction::Previous => {
                next.advance(false, true);
                playback = true;
            }
            MusicAction::PlayAt { index } => {
                if index >= next.order.len() {
                    return Err(index_error());
                }
                next.select_from(index, false);
                playback = true;
            }
            MusicAction::SetRepeat { mode } => next.repeat = mode,
            MusicAction::SetShuffle { enabled } => {
                if enabled != next.shuffle {
                    next.shuffle = enabled;
                    if enabled {
                        let mut rng = seed.max(1);
                        for i in (1..next.order.len()).rev() {
                            rng ^= rng << 13;
                            rng ^= rng >> 7;
                            rng ^= rng << 17;
                            next.order.swap(i, (rng as usize) % (i + 1));
                        }
                    } else {
                        next.order = next.original.iter().map(|e| e.entry_id).collect();
                    }
                }
            }
            MusicAction::Enqueue { .. } => {
                if next.original.len() + items.len() > 10000 {
                    return Err(limit());
                }
                for item in items {
                    let entry_id = next.next_id;
                    next.next_id = next.next_id.checked_add(1).ok_or_else(limit)?;
                    next.original.push(QueueEntry { entry_id, item });
                    next.order.push(entry_id);
                }
            }
            MusicAction::RemoveAt { index } => {
                if index >= next.order.len() {
                    return Err(index_error());
                }
                let id = next.order.remove(index);
                next.original.retain(|e| e.entry_id != id);
                if next.current == Some(id) {
                    next.current = None;
                    next.select_from(index, next.repeat == RepeatMode::All);
                    playback = true;
                }
            }
            MusicAction::MoveItem { from, to } => {
                if from >= next.order.len() || to >= next.order.len() {
                    return Err(index_error());
                }
                let id = next.order.remove(from);
                next.order.insert(to, id);
                if !next.shuffle {
                    let entry = next.original.remove(from);
                    next.original.insert(to, entry);
                }
            }
        }
        next.revision = self.revision.wrapping_add(1);
        *self = next;
        Ok(playback)
    }
    pub fn current_media_id(&self) -> Option<String> {
        self.current
            .and_then(|id| self.original.iter().find(|e| e.entry_id == id))
            .map(|e| e.item.media_id.clone())
    }
    fn select_from(&mut self, index: usize, wrap: bool) -> Option<String> {
        let count = self.order.len();
        let mut skipped = 0;
        self.current = None;
        for offset in 0..count {
            let i = index + offset;
            if i >= count && !wrap {
                break;
            }
            let id = self.order[i % count];
            let entry = self.original.iter().find(|e| e.entry_id == id).unwrap();
            if entry.item.availability == "available" {
                self.current = Some(id);
                break;
            }
            skipped += 1;
        }
        if skipped > 0 {
            self.message = Some(format!(
                "Skipped {skipped} unavailable queue entries. They remain in the queue."
            ));
        }
        if self.current.is_none() {
            self.message = Some(
                if skipped > 0 {
                    "No available tracks remain. Unavailable entries are retained."
                } else {
                    "End of queue."
                }
                .into(),
            );
        }
        self.current_media_id()
    }
    pub fn advance(&mut self, natural: bool, previous: bool) -> Option<String> {
        self.message = None;
        self.revision = self.revision.wrapping_add(1);
        let position = self
            .current
            .and_then(|id| self.order.iter().position(|v| *v == id));
        if natural
            && self.repeat == RepeatMode::One
            && self.current.is_some_and(|id| {
                self.original
                    .iter()
                    .any(|e| e.entry_id == id && e.item.availability == "available")
            })
        {
            return self.current_media_id();
        }
        if previous {
            let count = self.order.len();
            let start = position.unwrap_or(count);
            for (skipped, step) in (1..=count).enumerate() {
                if step > start && self.repeat != RepeatMode::All {
                    break;
                }
                let index = (start + count - step) % count;
                let id = self.order[index];
                if self
                    .original
                    .iter()
                    .any(|e| e.entry_id == id && e.item.availability == "available")
                {
                    self.current = Some(id);
                    if skipped > 0 {
                        self.message =
                            Some(format!("Skipped {skipped} unavailable queue entries."));
                    }
                    return self.current_media_id();
                }
            }
            if !self.current.is_some_and(|id| {
                self.original
                    .iter()
                    .any(|e| e.entry_id == id && e.item.availability == "available")
            }) {
                self.current = None;
                self.message =
                    Some("No available tracks remain. Unavailable entries are retained.".into());
            }
            return self.current_media_id();
        }
        self.select_from(
            position.map_or(0, |i| i + 1),
            self.repeat == RepeatMode::All,
        )
    }
    pub fn save(&self, db: &rusqlite::Connection) -> Result<(), AppError> {
        let json = serde_json::to_string(self)
            .map_err(|_| AppError::new("queue_invalid", "The music queue could not be saved."))?;
        db.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('music_queue',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",rusqlite::params![json,crate::app_state::now_ms() as i64])?;
        Ok(())
    }
}
fn limit() -> AppError {
    AppError::new(
        "queue_limit",
        "A music queue can contain at most 10,000 entries.",
    )
}
fn index_error() -> AppError {
    AppError::new(
        "queue_index",
        "This queue position is unavailable. Refresh the queue.",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: &str, available: bool) -> MusicItem {
        MusicItem {
            media_id: id.into(),
            title: id.into(),
            artist: None,
            album: None,
            availability: if available { "available" } else { "offline" }.into(),
        }
    }
    fn queue() -> Queue {
        let mut q = Queue::default();
        q.replace(vec![item("a", true), item("b", true), item("a", true)], 0)
            .unwrap();
        q
    }
    #[test]
    fn manual_stop_wins_over_a_delayed_natural_end_callback() {
        let mut q = queue();
        q.session_id = Some("ended".into());
        assert!(!q.can_advance("ended", None));
        assert!(!q.can_advance("ended", Some("replacement")));
        assert!(q.can_advance("ended", Some("ended")));
    }
    #[test]
    fn previous_never_reselects_an_unavailable_current_track() {
        let mut q = queue();
        q.repeat = RepeatMode::All;
        q.original
            .iter_mut()
            .for_each(|e| e.item.availability = "offline".into());
        assert_eq!(q.advance(false, true), None);
    }
    #[test]
    fn stale_session_cannot_advance_replacement_queue() {
        let mut q = queue();
        q.session_id = Some("current".into());
        assert!(!q.matches_session(Some("expired")));
        assert!(q.matches_session(Some("current")));
        assert!(!q.matches_session(None));
    }
    #[test]
    fn duplicates_advance_as_independent_entries_and_end() {
        let mut q = queue();
        assert_eq!(q.advance(false, false).as_deref(), Some("b"));
        assert_eq!(q.advance(true, false).as_deref(), Some("a"));
        assert_eq!(q.advance(true, false), None);
        assert_eq!(q.snapshot().items.len(), 3);
    }
    #[test]
    fn repeat_one_repeats_natural_end_but_next_advances() {
        let mut q = queue();
        q.repeat = RepeatMode::One;
        assert_eq!(q.advance(true, false).as_deref(), Some("a"));
        assert_eq!(q.advance(false, false).as_deref(), Some("b"));
    }
    #[test]
    fn repeat_all_wraps_and_previous_wraps() {
        let mut q = queue();
        q.repeat = RepeatMode::All;
        assert_eq!(q.advance(false, true).as_deref(), Some("a"));
        assert_eq!(q.snapshot().current_index, Some(2));
        assert_eq!(q.advance(true, false).as_deref(), Some("a"));
        assert_eq!(q.snapshot().current_index, Some(0));
    }
    #[test]
    fn offline_entries_are_retained_and_skipped_without_repeat_loop() {
        let mut q = Queue::default();
        q.replace(vec![item("a", true), item("b", false), item("c", true)], 0)
            .unwrap();
        assert_eq!(q.advance(true, false).as_deref(), Some("c"));
        assert!(q.snapshot().message.unwrap().contains("Skipped"));
        q.original
            .iter_mut()
            .for_each(|e| e.item.availability = "offline".into());
        q.repeat = RepeatMode::All;
        assert_eq!(q.advance(true, false), None);
        assert_eq!(q.snapshot().items.len(), 3);
    }
    #[test]
    fn seeded_shuffle_restores_original_order_and_current_occurrence() {
        let mut q = queue();
        let revision = q.revision;
        q.edit(
            revision,
            MusicAction::SetShuffle { enabled: true },
            vec![],
            19,
        )
        .unwrap();
        let shuffled = q.order.clone();
        let mut other = queue();
        other
            .edit(
                other.revision,
                MusicAction::SetShuffle { enabled: true },
                vec![],
                19,
            )
            .unwrap();
        assert_eq!(shuffled, other.order);
        q.edit(
            q.revision,
            MusicAction::SetShuffle { enabled: false },
            vec![],
            0,
        )
        .unwrap();
        assert_eq!(
            q.snapshot()
                .items
                .iter()
                .map(|e| e.item.media_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "a"]
        );
        assert_eq!(q.snapshot().current_index, Some(0));
    }
    #[test]
    fn stale_and_invalid_edits_leave_queue_unchanged() {
        let mut q = queue();
        let before = q.snapshot();
        assert!(q
            .edit(
                q.revision - 1,
                MusicAction::RemoveAt { index: 0 },
                vec![],
                0
            )
            .is_err());
        assert!(q
            .edit(
                q.revision,
                MusicAction::MoveItem { from: 0, to: 9 },
                vec![],
                0
            )
            .is_err());
        assert_eq!(q.snapshot().items, before.items);
        assert_eq!(q.revision, before.revision);
    }
    #[test]
    fn bounds_and_current_removal_are_explicit() {
        let mut q = queue();
        assert!(q.replace(vec![item("x", true); 10001], 0).is_err());
        assert!(q
            .edit(
                q.revision,
                MusicAction::Enqueue { media_ids: vec![] },
                vec![item("x", true); 10000],
                0
            )
            .is_err());
        assert!(q
            .edit(q.revision, MusicAction::RemoveAt { index: 0 }, vec![], 0)
            .unwrap());
        assert_eq!(q.snapshot().items.len(), 2);
        assert_eq!(q.snapshot().current_index, Some(0));
    }
    #[test]
    fn persistence_restores_order_and_modes_without_restarting_playback() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .unwrap();
        let mut q = queue();
        q.session_id = Some("old".into());
        q.edit(
            q.revision,
            MusicAction::SetShuffle { enabled: true },
            vec![],
            17,
        )
        .unwrap();
        q.repeat = RepeatMode::All;
        q.save(&db).unwrap();
        let mut restored = Queue::load(&db).unwrap();
        assert_eq!(restored.snapshot().items, q.snapshot().items);
        assert_eq!(restored.session_id, None);
        assert_eq!(restored.repeat, RepeatMode::All);
        restored
            .edit(
                restored.revision,
                MusicAction::SetShuffle { enabled: false },
                vec![],
                0,
            )
            .unwrap();
        assert_eq!(restored.snapshot().original_media_ids, vec!["a", "b", "a"]);
    }
}
