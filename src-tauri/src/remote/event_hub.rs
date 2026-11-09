//! Bounded, socket-independent native event delivery.
use super::protocol::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub const EVENT_CAPACITY: usize = 256;
#[derive(Clone, Copy, Debug)]
pub struct Stamp {
    pub topic: usize,
    pub revision: u64,
}
pub fn index(topic: &Topic) -> usize {
    match topic {
        Topic::Connection => 0,
        Topic::Activity => 1,
        Topic::Player => 2,
        Topic::QueueSummary => 3,
        Topic::Slideshow => 4,
        Topic::Outputs => 5,
        Topic::SourcesSummary => 6,
        Topic::Vpn => 7,
        Topic::Ui => 8,
        Topic::Operations => 9,
    }
}
#[derive(Default)]
struct Client {
    queue: VecDeque<RemoteEvent>,
    resync: bool,
    owner: Option<Id>,
    changed: Arc<tokio::sync::Notify>,
}
#[derive(Default)]
struct Inner {
    sequence: u64,
    published: [u64; 10],
    clients: Vec<Weak<Mutex<Client>>>,
}
pub struct EventHub {
    epoch: Id,
    revisions: [AtomicU64; 10],
    inner: Mutex<Inner>,
    position: Mutex<PositionPending>,
}
#[derive(Default)]
struct PositionPending {
    latest: u64,
    last: Option<std::time::Instant>,
    pending: Option<(Stamp, PlayerTopic)>,
    timer: bool,
}
impl Default for EventHub {
    fn default() -> Self {
        Self {
            epoch: serde_json::from_value(serde_json::json!(uuid::Uuid::new_v4().to_string()))
                .unwrap(),
            revisions: std::array::from_fn(|_| AtomicU64::new(0)),
            inner: Mutex::new(Inner::default()),
            position: Mutex::new(PositionPending::default()),
        }
    }
}
pub struct Subscription {
    client: Arc<Mutex<Client>>,
    pub boundary: u64,
}
impl EventHub {
    pub fn epoch(&self) -> Id {
        self.epoch.clone()
    }
    pub fn reserve(&self, topic: Topic) -> Stamp {
        let topic = index(&topic);
        Stamp {
            topic,
            revision: self.revisions[topic].fetch_add(1, Ordering::SeqCst) + 1,
        }
    }
    pub fn revision(&self, topic: Topic) -> u64 {
        self.revisions[index(&topic)].load(Ordering::SeqCst)
    }
    pub fn subscribe(&self) -> Subscription {
        self.subscribe_owner(None)
    }
    pub fn subscribe_owner(&self, owner: Option<Id>) -> Subscription {
        let mut inner = self.inner.lock().unwrap();
        let client = Arc::new(Mutex::new(Client {
            owner,
            ..Client::default()
        }));
        inner.clients.retain(|client| client.strong_count() > 0);
        inner.clients.push(Arc::downgrade(&client));
        Subscription {
            client,
            boundary: inner.sequence,
        }
    }
    pub fn publish(&self, stamp: Stamp, make: impl FnOnce(Id, Decimal) -> RemoteEvent) {
        let mut event = None;
        let mut make = Some(make);
        self.publish_projected(stamp, |_, epoch, sequence| {
            event
                .get_or_insert_with(|| make.take().unwrap()(epoch, sequence))
                .clone()
        });
    }
    pub fn publish_projected(
        &self,
        stamp: Stamp,
        mut make: impl FnMut(Option<&Id>, Id, Decimal) -> RemoteEvent,
    ) {
        let mut inner = self.inner.lock().unwrap();
        if stamp.revision <= inner.published[stamp.topic] {
            return;
        }
        inner.published[stamp.topic] = stamp.revision;
        inner.sequence = inner
            .sequence
            .checked_add(1)
            .expect("event sequence exhausted");
        let sequence = inner.sequence;
        inner.clients.retain(|weak| {
            let Some(client) = weak.upgrade() else {
                return false;
            };
            let mut client = client.lock().unwrap();
            if client.resync {
                return true;
            }
            if client.queue.len() == EVENT_CAPACITY {
                client.queue.clear();
                client.resync = true;
                client.changed.notify_one();
                return true;
            }
            let event = make(client.owner.as_ref(), self.epoch(), sequence.into());
            client.queue.push_back(event);
            client.changed.notify_one();
            true
        });
    }
    /// Immediate transitions cancel stale pending ticks. A single timer flushes the
    /// final periodic position even if mpv emits no subsequent observations.
    pub fn player(self: &Arc<Self>, stamp: Stamp, data: PlayerTopic, immediate: bool) {
        let mut position = self.position.lock().unwrap();
        if stamp.revision <= position.latest {
            return;
        }
        position.latest = stamp.revision;
        let interval = std::time::Duration::from_millis(500);
        if immediate || position.last.is_none_or(|last| last.elapsed() >= interval) {
            position.pending = None;
            position.last = Some(std::time::Instant::now());
            drop(position);
            self.publish(stamp, |server_epoch, sequence| RemoteEvent::Player {
                server_epoch,
                sequence,
                data,
            });
            return;
        }
        if position
            .pending
            .as_ref()
            .is_none_or(|(old, _)| old.revision < stamp.revision)
        {
            position.pending = Some((stamp, data));
        }
        if position.timer {
            return;
        }
        position.timer = true;
        let delay = interval.saturating_sub(position.last.unwrap().elapsed());
        drop(position);
        let hub = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            hub.flush_position();
        });
    }
    fn flush_position(&self) {
        let mut position = self.position.lock().unwrap();
        position.timer = false;
        let pending = position.pending.take();
        if pending.is_some() {
            position.last = Some(std::time::Instant::now());
        }
        drop(position);
        if let Some((stamp, data)) = pending {
            self.publish(stamp, |server_epoch, sequence| RemoteEvent::Player {
                server_epoch,
                sequence,
                data,
            });
        }
    }
}
impl Subscription {
    /// Await without holding the hub or native state mutexes. A stored Notify
    /// permit closes the race between draining and awaiting publication.
    pub async fn recv(&mut self) -> Result<Vec<RemoteEvent>, RemoteError> {
        let changed = self.client.lock().unwrap().changed.clone();
        loop {
            let events = self.drain()?;
            if !events.is_empty() {
                return Ok(events);
            }
            changed.notified().await;
        }
    }

    pub fn drain(&mut self) -> Result<Vec<RemoteEvent>, RemoteError> {
        let mut client = self.client.lock().unwrap();
        if client.resync {
            return Err(RemoteError::new(
                ErrorCode::ResyncRequired,
                "Fetch a new snapshot.",
            ));
        }
        Ok(client.queue.drain(..).collect())
    }
}
/// Reconcile every sequence (including superseded topic copies) before applying
/// data. Skipping a duplicate topic revision MUST still consume its sequence.
pub struct RevisionCursor {
    epoch: Id,
    sequence: u64,
    revisions: [u64; 10],
    stale: bool,
}
impl RevisionCursor {
    pub fn new(snapshot: &RemoteSnapshot) -> Self {
        Self {
            epoch: snapshot.server_epoch.clone(),
            sequence: snapshot.sequence.value(),
            revisions: [
                snapshot.connection.revision.value(),
                snapshot.activity.revision.value(),
                snapshot.player.revision.value(),
                snapshot.queue_summary.revision.value(),
                snapshot.slideshow.revision.value(),
                snapshot.outputs.revision.value(),
                snapshot.sources_summary.revision.value(),
                snapshot.vpn.revision.value(),
                snapshot.ui.revision.value(),
                snapshot.operations.revision.value(),
            ],
            stale: false,
        }
    }
    pub fn accept(&mut self, event: &RemoteEvent) -> Result<bool, RemoteError> {
        let (epoch, sequence, topic, revision) = event_position(event);
        if self.stale || epoch != &self.epoch || sequence != self.sequence + 1 {
            self.stale = true;
            return Err(RemoteError::new(
                ErrorCode::ResyncRequired,
                "Fetch a new snapshot.",
            ));
        }
        self.sequence = sequence;
        if revision <= self.revisions[topic] {
            return Ok(false);
        }
        self.revisions[topic] = revision;
        Ok(true)
    }
}
fn event_position(event: &RemoteEvent) -> (&Id, u64, usize, u64) {
    match event {
        RemoteEvent::Connection {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 0, data.revision.value()),
        RemoteEvent::Activity {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 1, data.revision.value()),
        RemoteEvent::Player {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 2, data.revision.value()),
        RemoteEvent::QueueSummary {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 3, data.revision.value()),
        RemoteEvent::Slideshow {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 4, data.revision.value()),
        RemoteEvent::Outputs {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 5, data.revision.value()),
        RemoteEvent::SourcesSummary {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 6, data.revision.value()),
        RemoteEvent::Vpn {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 7, data.revision.value()),
        RemoteEvent::Ui {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 8, data.revision.value()),
        RemoteEvent::Operations {
            server_epoch,
            sequence,
            data,
        } => (server_epoch, sequence.value(), 9, data.revision.value()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn send(hub: &EventHub, stamp: Stamp) {
        hub.publish(stamp, |server_epoch, sequence| {
            RemoteEvent::SourcesSummary {
                server_epoch,
                sequence,
                data: SourcesSummary {
                    revision: stamp.revision.into(),
                    count: stamp.revision.into(),
                    scanning: false,
                },
            }
        });
    }
    #[test]
    fn newer_publication_wins_when_old_capture_arrives_late() {
        let hub = EventHub::default();
        let mut sub = hub.subscribe();
        let old = hub.reserve(Topic::SourcesSummary);
        let new = hub.reserve(Topic::SourcesSummary);
        send(&hub, new);
        send(&hub, old);
        let events = sub.drain().unwrap();
        assert_eq!(events.len(), 1);
        if let RemoteEvent::SourcesSummary { data, .. } = &events[0] {
            assert_eq!(data.count.value(), 2)
        } else {
            panic!()
        }
    }
    #[test]
    fn message_257_requires_resync_only_for_slow_client() {
        let hub = EventHub::default();
        let mut slow = hub.subscribe();
        let mut fast = hub.subscribe();
        for _ in 0..256 {
            send(&hub, hub.reserve(Topic::SourcesSummary));
            assert_eq!(fast.drain().unwrap().len(), 1);
        }
        assert_eq!(slow.client.lock().unwrap().queue.len(), 256);
        send(&hub, hub.reserve(Topic::SourcesSummary));
        assert_eq!(slow.drain().unwrap_err().code, ErrorCode::ResyncRequired);
        assert_eq!(fast.drain().unwrap().len(), 1);
        assert_eq!(slow.client.lock().unwrap().queue.len(), 0);
    }
    fn player(stamp: Stamp) -> PlayerTopic {
        super::super::adapters::checked(serde_json::json!({"revision":stamp.revision.to_string(),"phase":"playing","positionSeconds":stamp.revision,"volume":50,"muted":false,"audioTracks":[],"subtitleTracks":[]})).unwrap()
    }
    #[tokio::test]
    async fn position_ticks_are_coalesced_with_final_flush_and_immediate_seek() {
        let hub = Arc::new(EventHub::default());
        let mut sub = hub.subscribe();
        let first = hub.reserve(Topic::Player);
        hub.player(first, player(first), false);
        assert_eq!(sub.drain().unwrap().len(), 1);
        let old = hub.reserve(Topic::Player);
        let latest = hub.reserve(Topic::Player);
        hub.player(latest, player(latest), false);
        // A delayed old immediate publication must not cancel the latest tick.
        hub.player(old, player(old), true);
        assert!(sub.drain().unwrap().is_empty());
        let flushed = tokio::time::timeout(std::time::Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(flushed.len(), 1);
        assert_eq!(event_position(&flushed[0]).3, latest.revision);
        let pending = hub.reserve(Topic::Player);
        hub.player(pending, player(pending), false);
        let seek = hub.reserve(Topic::Player);
        hub.player(seek, player(seek), true);
        assert_eq!(event_position(&sub.drain().unwrap()[0]).3, seek.revision);
        tokio::time::sleep(std::time::Duration::from_millis(550)).await;
        assert!(sub.drain().unwrap().is_empty());
    }
}
