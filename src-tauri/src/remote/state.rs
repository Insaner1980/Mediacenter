//! Native snapshots and subscription coordination. No filesystem paths cross here.
use super::{
    adapters::{self, checked},
    event_hub::{Stamp, Subscription},
    protocol::*,
};
use crate::{app_state::AppState, contracts as native, control::ControlServices};
/// Send the snapshot, then all buffered events in sequence, then recv() batches.
/// Apply events through RevisionCursor (or equivalent revision logic): a
/// superseded topic still consumes its sequence, so private projections cannot
/// create apparent transport gaps. Send each event as its own control message.
pub struct RemoteSubscription {
    pub initial: super::protocol::RemoteSubscription,
    pub events: Subscription,
}
pub async fn snapshot(
    principal: &RemotePrincipal,
    services: &ControlServices,
) -> Result<RemoteSnapshot, RemoteError> {
    Ok(subscribe_with_snapshot(principal, services)
        .await?
        .initial
        .snapshot)
}
pub async fn subscribe_with_snapshot(
    principal: &RemotePrincipal,
    services: &ControlServices,
) -> Result<RemoteSubscription, RemoteError> {
    let events = services
        .state
        .event_hub
        .subscribe_owner(Some(principal.device_id.clone()));
    snapshot_from_admitted_subscription(principal, services, events).await
}
/// Authenticated transports MUST pass the subscription obtained atomically from
/// AuthManager here. Creating a fresh subscription after authentication loses the
/// revocation boundary. No authentication gate is held across snapshot awaits.
pub async fn snapshot_from_admitted_subscription(
    principal: &RemotePrincipal,
    services: &ControlServices,
    mut events: Subscription,
) -> Result<RemoteSubscription, RemoteError> {
    let snapshot = capture(principal, services, events.boundary, || {}).await?;
    let buffered_events = checked(events.drain()?)?;
    if serde_json::to_vec(&snapshot)
        .map_err(|_| unavailable())?
        .len()
        > 256 * 1024
    {
        return Err(RemoteError::new(
            ErrorCode::ResyncRequired,
            "Snapshot exceeds the control message limit.",
        ));
    }
    Ok(RemoteSubscription {
        initial: super::protocol::RemoteSubscription {
            snapshot,
            buffered_events,
        },
        events,
    })
}
/// Refresh using the already authenticated stream; never lose its revocation
/// boundary. Queued events remain for the stream and snapshot revisions discard
/// superseded deltas. The boundary is the last successfully delivered event.
pub async fn snapshot_on_admitted_subscription(
    principal: &RemotePrincipal,
    services: &ControlServices,
    events: &Subscription,
) -> Result<RemoteSnapshot, RemoteError> {
    events.check_live()?;
    let snapshot = capture(principal, services, events.boundary, || {}).await?;
    events.check_live()?;
    Ok(snapshot)
}
async fn capture(
    principal: &RemotePrincipal,
    services: &ControlServices,
    boundary: u64,
    after_player: impl FnOnce(),
) -> Result<RemoteSnapshot, RemoteError> {
    if !principal.scopes.as_slice().contains(&Scope::Browse) {
        return Err(RemoteError::new(
            ErrorCode::Forbidden,
            "Browse permission is required.",
        ));
    }
    let state = &services.state;
    let player = {
        let player = state.player.lock().await;
        let revision = state.event_hub.revision(Topic::Player);
        match player.as_ref() {
            Some(player) => {
                let mut topic = PlayerTopic::try_from(&player.snapshot)?;
                topic.revision = revision.into();
                topic
            }
            None => {
                let preferences = state.player_preferences.lock().map_err(|_| unavailable())?;
                idle_player(revision, preferences.volume, preferences.muted)?
            }
        }
    };
    after_player();
    let queue_summary = {
        let queue = state.music.lock().await;
        QueueSummary::try_from(&queue.snapshot())?
    };
    let slideshow = {
        let photos = state.photos.lock().await;
        adapters::slideshow_topic(
            &photos.snapshot(),
            state.event_hub.revision(Topic::Slideshow).into(),
        )?
    };
    let (activity, outputs, vpn) = {
        let home = state.home.lock().map_err(|_| unavailable())?;
        (
            activity_topic(&home, state.event_hub.revision(Topic::Activity))?,
            outputs_topic(state, &home, state.event_hub.revision(Topic::Outputs))?,
            vpn_topic(&home, state.event_hub.revision(Topic::Vpn))?,
        )
    };
    let sources_summary = sources(state, false)?.1;
    let operations = services
        .operations()
        .recent(principal)
        .map_err(|_| unavailable())?;
    Ok(RemoteSnapshot {
        v: checked(1)?,
        r#type: SnapshotType::Snapshot,
        server_epoch: state.event_hub.epoch(),
        sequence: boundary.into(),
        connection: ConnectionTopic {
            revision: 0.into(),
            server_device_id: services.server_identity.clone(),
            display_name: checked("Mediacenter")?,
        },
        activity,
        player,
        queue_summary,
        slideshow,
        outputs,
        sources_summary,
        vpn,
        ui: state.renderer.snapshot(state.event_hub.revision(Topic::Ui)),
        operations,
    })
}

fn idle_player(revision: u64, volume: f64, muted: bool) -> Result<PlayerTopic, RemoteError> {
    Ok(PlayerTopic {
        revision: revision.into(),
        session_id: None,
        item_id: None,
        phase: PlayerPhase::Idle,
        position_seconds: checked(0.)?,
        duration_seconds: None,
        volume: checked(volume)?,
        muted,
        audio_tracks: checked(Vec::<Track>::new())?,
        subtitle_tracks: checked(Vec::<Track>::new())?,
    })
}

fn unavailable() -> RemoteError {
    RemoteError::new(ErrorCode::Unavailable, "Native state is unavailable.")
}
pub(crate) fn activity_topic(
    home: &native::HomeSnapshot,
    revision: u64,
) -> Result<ActivityTopic, RemoteError> {
    let session = home.sessions.last();
    Ok(ActivityTopic {
        revision: revision.into(),
        mode: match session.map(|s| s.kind.as_str()) {
            Some("local-video") => ActivityMode::Video,
            Some("local-audio") => ActivityMode::Music,
            Some("slideshow") => ActivityMode::Photos,
            Some("streaming") => ActivityMode::Browser,
            _ => ActivityMode::Idle,
        },
        session_id: session.map(|s| checked(&s.id)).transpose()?,
        title: None,
    })
}
pub(crate) fn outputs_topic(
    state: &AppState,
    home: &native::HomeSnapshot,
    revision: u64,
) -> Result<OutputsTopic, RemoteError> {
    let inventory = state.audio_outputs.lock().map_err(|_| unavailable())?;
    let unavailable = home.outputs.audio_unavailable
        || home
            .outputs
            .effective
            .audio_sink_id
            .as_ref()
            .is_some_and(|id| {
                !inventory
                    .iter()
                    .any(|sink| &sink.id == id && sink.available)
            });
    let outputs = inventory
        .iter()
        .take(100)
        .map(|sink| {
            Ok(AudioOutput {
                id: checked(&sink.id)?,
                name: checked(sink.name.chars().take(200).collect::<String>())?,
                available: sink.available,
            })
        })
        .collect::<Result<Vec<_>, RemoteError>>()?;
    Ok(OutputsTopic {
        revision: revision.into(),
        requested_output_id: home
            .outputs
            .desired
            .audio_sink_id
            .as_ref()
            .map(checked)
            .transpose()?,
        effective_output_id: home
            .outputs
            .effective
            .audio_sink_id
            .as_ref()
            .map(checked)
            .transpose()?,
        unavailable,
        outputs: checked(outputs)?,
    })
}
pub(crate) fn vpn_topic(
    home: &native::HomeSnapshot,
    revision: u64,
) -> Result<VpnTopic, RemoteError> {
    Ok(VpnTopic {
        revision: revision.into(),
        state: if super::vpn::fresh(&home.vpn, crate::app_state::now_ms()) {
            checked(&home.vpn.state).unwrap_or(VpnState::Unknown)
        } else {
            VpnState::Unknown
        },
        country: if super::vpn::fresh(&home.vpn, crate::app_state::now_ms()) {
            home.vpn.country.as_ref().map(checked).transpose()?
        } else {
            None
        },
        kill_switch: if super::vpn::fresh(&home.vpn, crate::app_state::now_ms()) {
            checked(&home.vpn.kill_switch).unwrap_or(SwitchState::Unknown)
        } else {
            SwitchState::Unknown
        },
        observed_at_ms: checked(home.vpn.observed_at_ms)?,
    })
}
pub(crate) fn home_changed(state: &AppState, home: &native::HomeSnapshot, stamps: [Stamp; 3]) {
    if let Ok(data) = activity_topic(home, stamps[0].revision) {
        state
            .event_hub
            .publish(stamps[0], |server_epoch, sequence| RemoteEvent::Activity {
                server_epoch,
                sequence,
                data,
            });
    }
    if let Ok(data) = outputs_topic(state, home, stamps[1].revision) {
        state
            .event_hub
            .publish(stamps[1], |server_epoch, sequence| RemoteEvent::Outputs {
                server_epoch,
                sequence,
                data,
            });
    }
    if let Ok(data) = vpn_topic(home, stamps[2].revision) {
        state
            .event_hub
            .publish(stamps[2], |server_epoch, sequence| RemoteEvent::Vpn {
                server_epoch,
                sequence,
                data,
            });
    }
}
pub(crate) fn queue_changed(
    state: &AppState,
    snapshot: &crate::music::queue::MusicQueue,
    stamp: Stamp,
) {
    // The wire revision is the native CAS token used by queue pages/mutations.
    // The independent stamp only rejects out-of-order publication internally.
    if let Ok(data) = QueueSummary::try_from(snapshot) {
        state
            .event_hub
            .publish(stamp, |server_epoch, sequence| RemoteEvent::QueueSummary {
                server_epoch,
                sequence,
                data,
            });
    }
}
pub(crate) fn player_changed(
    state: &AppState,
    snapshot: Option<&native::PlayerSnapshot>,
    stamp: Stamp,
    immediate: bool,
) {
    let data = match snapshot {
        Some(snapshot) => PlayerTopic::try_from(snapshot).map(|mut data| {
            data.revision = stamp.revision.into();
            data
        }),
        None => {
            let preferences = state.player_preferences.lock().unwrap();
            idle_player(stamp.revision, preferences.volume, preferences.muted)
        }
    };
    if let Ok(data) = data {
        state.event_hub.player(stamp, data, immediate);
    }
}
pub(crate) fn slideshow_changed(
    state: &AppState,
    snapshot: &crate::photos::presenter::SlideshowSnapshot,
    stamp: Stamp,
) {
    if let Ok(data) = adapters::slideshow_topic(snapshot, stamp.revision.into()) {
        state
            .event_hub
            .publish(stamp, |server_epoch, sequence| RemoteEvent::Slideshow {
                server_epoch,
                sequence,
                data,
            });
    }
}
fn sources(state: &AppState, reserve: bool) -> Result<(Stamp, SourcesSummary), RemoteError> {
    let db = state.db.lock().map_err(|_| unavailable())?;
    let count: i64 = db
        .query_row("SELECT count(*) FROM media_sources", [], |row| row.get(0))
        .map_err(|_| unavailable())?;
    let scanning = !state.scans.lock().map_err(|_| unavailable())?.is_empty();
    let stamp = if reserve {
        state.event_hub.reserve(Topic::SourcesSummary)
    } else {
        Stamp {
            topic: 6,
            revision: state.event_hub.revision(Topic::SourcesSummary),
        }
    };
    Ok((
        stamp,
        SourcesSummary {
            revision: stamp.revision.into(),
            count: (count as u64).into(),
            scanning,
        },
    ))
}
pub(crate) fn sources_changed(state: &AppState) {
    if let Ok((stamp, data)) = sources(state, true) {
        state.event_hub.publish(stamp, |server_epoch, sequence| {
            RemoteEvent::SourcesSummary {
                server_epoch,
                sequence,
                data,
            }
        });
    }
}

pub(crate) fn outputs_discovered(state: &AppState, outputs: Vec<native::AudioSinkDescriptor>) {
    // Same home -> inventory lock order as snapshots and home publications.
    let captured = (|| -> Result<_, RemoteError> {
        let home = state.home.lock().map_err(|_| unavailable())?;
        {
            let mut inventory = state.audio_outputs.lock().map_err(|_| unavailable())?;
            *inventory = outputs;
        }
        let stamp = state.event_hub.reserve(Topic::Outputs);
        let topic = outputs_topic(state, &home, stamp.revision)?;
        Ok((stamp, topic))
    })();
    if let Ok((stamp, data)) = captured {
        state
            .event_hub
            .publish(stamp, |server_epoch, sequence| RemoteEvent::Outputs {
                server_epoch,
                sequence,
                data,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn principal() -> RemotePrincipal {
        checked(serde_json::json!({"deviceId":"phone","scopes":["browse"]})).unwrap()
    }
    #[tokio::test]
    async fn revoked_admission_before_or_during_snapshot_never_resubscribes() {
        use crate::remote::{
            auth::AuthManager,
            store::{CredentialHash, RemoteStore},
        };
        use std::{future::Future, task::Poll};
        for during_capture in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let state = AppState::open(dir.path().into()).unwrap();
            let identity: Id = checked("server").unwrap();
            let services = ControlServices::native(state.clone(), identity.clone()).unwrap();
            let auth = AuthManager::new(&identity, state.event_hub.clone()).unwrap();
            let credential: BearerCredential = checked("a".repeat(43)).unwrap();
            let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse"],"createdAtMs":1,"revoked":false})).unwrap();
            let (_, admitted) = {
                let mut db = state.db.lock().unwrap();
                RemoteStore::new(&mut db, &identity)
                    .add_device(&device, &CredentialHash::from_credential(&credential))
                    .unwrap();
                auth.authenticate_subscription(&mut db, &credential)
                    .unwrap()
            };
            let player = state.player.lock().await;
            let principal = principal();
            let snapshot = snapshot_from_admitted_subscription(&principal, &services, admitted);
            tokio::pin!(snapshot);
            if during_capture {
                std::future::poll_fn(|cx| {
                    assert!(snapshot.as_mut().poll(cx).is_pending());
                    Poll::Ready(())
                })
                .await;
            }
            auth.revoke_device(&mut state.db.lock().unwrap(), &device.device_id, 2)
                .unwrap();
            drop(player);
            assert!(
                matches!(snapshot.await,Err(e) if e.code==ErrorCode::Unauthorized),
                "snapshot must preserve revoked admission instead of silently resubscribing"
            );
        }
    }
    #[tokio::test]
    async fn native_snapshot_has_actual_queue_and_no_renderer_claim() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        state.music.lock().await.shuffle = true;
        let services = ControlServices::native(state, checked("server").unwrap()).unwrap();
        let snapshot = snapshot(&principal(), &services).await.unwrap();
        assert!(snapshot.queue_summary.shuffle);
        assert!(!snapshot.ui.available);
        assert!(snapshot.operations.items.as_slice().is_empty());
    }
    #[tokio::test]
    async fn player_change_during_capture_is_delivered_after_old_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let mut events = state.event_hub.subscribe_owner(Some(principal().device_id));
        let snapshot = capture(&principal(), &services, events.boundary, || {
            // The native publisher commits a new player observation after capture has
            // read the player. Exercise the real coordinator subscription boundary.
            let stamp = state.event_hub.reserve(Topic::Player);
            let mut data = idle_player(0, 50., false).unwrap();
            data.revision = stamp.revision.into();
            data.phase = PlayerPhase::Playing;
            state.event_hub.player(stamp, data, true);
        })
        .await
        .unwrap();
        let buffered = events.drain().unwrap();
        assert_eq!(buffered.len(), 1);
        if let RemoteEvent::Player { sequence, data, .. } = &buffered[0] {
            assert!(sequence.value() > snapshot.sequence.value());
            assert!(data.revision.value() > snapshot.player.revision.value());
            assert_eq!(data.phase, PlayerPhase::Playing);
        } else {
            panic!()
        }
        assert!(events.drain().unwrap().is_empty());
    }
    #[tokio::test]
    async fn superseded_buffered_topic_consumes_sequence_and_gap_requires_resync() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let mut events = state.event_hub.subscribe();
        let snapshot = capture(&principal(), &services, events.boundary, || {
            let mut queue = state.music.try_lock().unwrap();
            queue.shuffle = true;
            let stamp = state.event_hub.reserve(Topic::QueueSummary);
            let copy = queue.snapshot();
            drop(queue);
            queue_changed(&state, &copy, stamp);
        })
        .await
        .unwrap();
        let buffered = events.drain().unwrap();
        assert_eq!(buffered.len(), 1);
        assert!(snapshot.queue_summary.shuffle);
        let mut cursor = super::super::event_hub::RevisionCursor::new(&snapshot);
        assert!(!cursor.accept(&buffered[0]).unwrap());
        let stamp = state.event_hub.reserve(Topic::Player);
        let data = idle_player(stamp.revision, 50., false).unwrap();
        state.event_hub.player(stamp, data, true);
        let next = events.drain().unwrap();
        assert!(cursor.accept(&next[0]).unwrap());
        assert_eq!(
            cursor.accept(&next[0]).unwrap_err().code,
            ErrorCode::ResyncRequired
        );
    }
    #[tokio::test]
    async fn unknown_outputs_are_not_fabricated_and_discovery_is_projected() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let before = snapshot(&principal(), &services).await.unwrap();
        assert!(!before.outputs.unavailable);
        assert!(before.outputs.outputs.as_slice().is_empty());
        outputs_discovered(
            &state,
            vec![native::AudioSinkDescriptor {
                id: "hdmi".into(),
                backend_name: "hdmi".into(),
                name: "HDMI TV".into(),
                available: true,
            }],
        );
        let after = snapshot(&principal(), &services).await.unwrap();
        assert_eq!(after.outputs.outputs.as_slice()[0].id.as_str(), "hdmi");
        assert!(after.outputs.effective_output_id.is_none());
    }
    #[test]
    fn task10_fix1_audio_projection_distinguishes_idle_preference_and_lost_output() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let sink = native::AudioSinkDescriptor {
            id: "hdmi".into(),
            backend_name: "hdmi".into(),
            name: "TV".into(),
            available: true,
        };
        outputs_discovered(&state, vec![sink.clone()]);
        let project = || outputs_topic(&state, &state.snapshot().unwrap(), 7).unwrap();
        let idle = project();
        crate::control::audio::save_preference(&state, "hdmi").unwrap();
        let preference = project();
        state.home.lock().unwrap().outputs.effective.audio_sink_id = Some("hdmi".into());
        let active = project();
        outputs_discovered(&state, vec![]);
        let lost = project();
        crate::events::commit(&state, |home| {
            home.outputs
                .observe_audio(&Err(crate::contracts::AppError::new(
                    "sink_unavailable",
                    "Private lost-sink fixture",
                )));
            home.outputs.effective.audio_sink_id = None;
        })
        .unwrap();
        let lost_after_clear = project();
        outputs_discovered(&state, vec![sink]);
        crate::events::commit(&state, |home| {
            home.outputs
                .observe_audio(&Err(crate::contracts::AppError::new(
                    "audio_stream_pending",
                    "Private pending retry fixture",
                )))
        })
        .unwrap();
        let returned_without_stream = project();
        crate::events::commit(&state, |home| {
            home.outputs.observe_audio(&Ok("hdmi".into()));
            home.outputs.effective.audio_sink_id = Some("hdmi".into());
        })
        .unwrap();
        let recovered = project();
        println!(
            "TASK10_AUDIO_PROJECTIONS={}",
            serde_json::json!({"idle":idle,"preference":preference,"active":active,"lost":lost,"lostAfterClear":lost_after_clear,"returnedWithoutStream":returned_without_stream,"recovered":recovered})
        );
        assert!(
            !idle.unavailable,
            "fresh idle has no failed playback stream"
        );
        assert!(
            !preference.unavailable,
            "saved available output is a preference until a stream exists"
        );
        assert!(!active.unavailable);
        assert!(!recovered.unavailable);
        assert!(lost.unavailable);
        assert!(
            lost_after_clear.unavailable,
            "clearing effective output cannot erase a known hotplug loss"
        );
        assert!(
            returned_without_stream.unavailable,
            "rediscovery alone does not verify active stream routing"
        );
    }

    #[tokio::test]
    async fn snapshot_and_subscription_require_browse_scope() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state, checked("server").unwrap()).unwrap();
        let principal = RemotePrincipal {
            device_id: checked("phone").unwrap(),
            scopes: checked(Vec::<Scope>::new()).unwrap(),
        };
        assert_eq!(
            snapshot(&principal, &services).await.unwrap_err().code,
            ErrorCode::Forbidden
        );
        assert!(
            matches!(subscribe_with_snapshot(&principal,&services).await,Err(error) if error.code==ErrorCode::Forbidden)
        );
    }
    #[tokio::test]
    async fn loaded_queue_summary_and_events_keep_native_cas_through_reads_and_edit() {
        use crate::music::queue::{MusicAction, RepeatMode as NativeRepeat};
        let dir = tempfile::tempdir().unwrap();
        {
            let saved = AppState::open(dir.path().into()).unwrap();
            let mut queue = saved.music.lock().await;
            let revision = queue.snapshot().revision;
            queue
                .edit(
                    revision,
                    MusicAction::SetShuffle { enabled: true },
                    vec![],
                    1,
                )
                .unwrap();
            queue.save(&saved.db.lock().unwrap()).unwrap();
        }
        let state = AppState::open(dir.path().into()).unwrap();
        let loaded = state.music.lock().await.snapshot();
        assert!(loaded.revision > 0);
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let mut subscription = subscribe_with_snapshot(&principal(), &services)
            .await
            .unwrap();
        assert_eq!(
            subscription.initial.snapshot.queue_summary.revision.value(),
            u64::from(loaded.revision)
        );
        let mut cursor =
            super::super::event_hub::RevisionCursor::new(&subscription.initial.snapshot);
        for _ in 0..3 {
            let native = crate::music::service::get(&state).await.unwrap();
            assert_eq!(native.revision, loaded.revision);
            let wire = snapshot(&principal(), &services)
                .await
                .unwrap()
                .queue_summary;
            assert_eq!(wire.revision.value(), u64::from(native.revision));
            state
                .music
                .lock()
                .await
                .validate(adapters::native_queue_revision(&wire.revision).unwrap())
                .unwrap();
            let events = subscription.events.drain().unwrap();
            assert_eq!(events.len(), 1);
            assert!(
                matches!(&events[0],RemoteEvent::QueueSummary{data,..} if data.revision==wire.revision)
            );
            assert!(!cursor.accept(&events[0]).unwrap());
        }
        let (copy, stamp) = {
            let mut queue = state.music.lock().await;
            queue
                .edit(
                    adapters::native_queue_revision(
                        &subscription.initial.snapshot.queue_summary.revision,
                    )
                    .unwrap(),
                    MusicAction::SetRepeat {
                        mode: NativeRepeat::All,
                    },
                    vec![],
                    2,
                )
                .unwrap();
            (
                queue.snapshot(),
                state.event_hub.reserve(Topic::QueueSummary),
            )
        };
        queue_changed(&state, &copy, stamp);
        let after = snapshot(&principal(), &services)
            .await
            .unwrap()
            .queue_summary;
        assert_eq!(after.revision.value(), u64::from(copy.revision));
        assert_ne!(copy.revision, loaded.revision);
        assert!(state.music.lock().await.validate(loaded.revision).is_err());
        state
            .music
            .lock()
            .await
            .validate(adapters::native_queue_revision(&after.revision).unwrap())
            .unwrap();
        let events = subscription.events.drain().unwrap();
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0],RemoteEvent::QueueSummary{data,..} if data.revision==after.revision)
        );
        assert!(cursor.accept(&events[0]).unwrap());
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[tokio::test]
    async fn refresh_snapshot_preserves_sequence_for_events_not_yet_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().into()).unwrap();
        let services = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
        let principal =
            checked(serde_json::json!({"deviceId":"phone","scopes":["browse"]})).unwrap();
        let mut subscription = subscribe_with_snapshot(&principal, &services)
            .await
            .unwrap();
        let stamp = state.event_hub.reserve(Topic::Connection);
        state
            .event_hub
            .publish(stamp, |server_epoch, sequence| RemoteEvent::Connection {
                server_epoch,
                sequence,
                data: ConnectionTopic {
                    revision: stamp.revision.into(),
                    server_device_id: checked("server").unwrap(),
                    display_name: checked("Mediacenter").unwrap(),
                },
            });
        let refreshed =
            snapshot_on_admitted_subscription(&principal, &services, &subscription.events)
                .await
                .unwrap();
        let mut cursor = super::super::event_hub::RevisionCursor::new(&refreshed);
        for event in subscription.events.drain().unwrap() {
            cursor
                .accept(&event)
                .expect("pending event must remain contiguous after refresh");
        }
    }
}
