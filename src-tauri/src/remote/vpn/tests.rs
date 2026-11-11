use super::*;
use crate::{
    app_state::AppState,
    remote::store::{CredentialHash, RemoteStore},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
#[derive(Clone)]
struct Fake {
    observed: Arc<Mutex<VpnSnapshot>>,
    calls: Arc<AtomicUsize>,
    uncertain: bool,
    lose_authority_before_effect: bool,
    delay: std::time::Duration,
}
impl Fake {
    fn new() -> Self {
        Self {
            observed: Arc::new(Mutex::new(VpnSnapshot {
                revision: 1,
                state: "connected".into(),
                country: Some("Finland".into()),
                kill_switch: "enabled".into(),
                observed_at_ms: now_ms(),
                error_code: None,
            })),
            calls: Arc::default(),
            uncertain: false,
            lose_authority_before_effect: false,
            delay: std::time::Duration::ZERO,
        }
    }
}
impl VpnBackend for Fake {
    async fn observe(&self, s: &ControlServices) -> Result<VpnSnapshot, AppError> {
        let mut v = self.observed.lock().unwrap().clone();
        v.observed_at_ms = now_ms();
        v.revision += 1;
        s.state.home.lock().unwrap().vpn = v.clone();
        Ok(v)
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        Ok(vec![
            CountryOption {
                id: "Finland".into(),
                name: "Finland".into(),
            },
            CountryOption {
                id: "Sweden".into(),
                name: "Sweden".into(),
            },
        ])
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        _: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        if self.lose_authority_before_effect {
            s.network.lock().unwrap().replace(Vec::new());
        }
        authorize()?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        if self.uncertain {
            s.network.lock().unwrap().replace(Vec::new());
            return Err(AppError::new("outcome_unknown", "Fixture route lost."));
        }
        let mut v = self.observed.lock().unwrap();
        v.state = if p.action == VpnAction::Connect {
            "connected"
        } else {
            "disconnected"
        }
        .into();
        v.country = p.country_id.map(|id| id.as_str().into());
        Ok(())
    }
}
fn fixture() -> (tempfile::TempDir, Dispatcher, RemotePrincipal, Fake) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path().into()).unwrap();
    let s = ControlServices::native(state.clone(), checked("server").unwrap()).unwrap();
    let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","settings"],"createdAtMs":1,"revoked":false})).unwrap();
    RemoteStore::new(&mut state.db.lock().unwrap(), &s.server_identity)
        .add_device(
            &device,
            &CredentialHash::from_credential(&checked("a".repeat(43)).unwrap()),
        )
        .unwrap();
    {
        let mut net = s.network.lock().unwrap();
        net.replace(vec!["192.168.1.2:47832".parse().unwrap()]);
        net.note(
            &device.device_id,
            CancellationToken::new(),
            CancellationToken::new(),
            "socket-a",
        );
    }
    (
        dir,
        Dispatcher::new(s),
        RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        },
        Fake::new(),
    )
}
fn request(d: &Dispatcher, id: &str, method: &str, params: serde_json::Value) -> RemoteRequest {
    parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":crate::control::settings::read(d).unwrap().revision}}).to_string()).unwrap()
}
async fn prepare(d: &Dispatcher, o: &RemotePrincipal, b: &Fake) -> ConfirmationPreview {
    let v = dispatch_with(
        d,
        o,
        request(
            d,
            "prepare",
            "vpn.prepareChange",
            serde_json::json!({"action":"connect","countryId":"Sweden"}),
        ),
        b.clone(),
    )
    .await
    .unwrap();
    serde_json::from_value(v["preview"].clone()).unwrap()
}
async fn terminal(d: &Dispatcher, o: &RemotePrincipal, id: &Id) -> OperationState {
    for _ in 0..100 {
        let value = d
            .services
            .operations()
            .lookup_request(o, id)
            .unwrap()
            .unwrap();
        if !matches!(
            value.status,
            OperationStatus::Accepted | OperationStatus::Running
        ) {
            return value;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("Operation did not finish")
}
#[tokio::test]
async fn country_change_preserves_kill_switch_and_launch_policy_and_never_replays() {
    let (_dir, d, o, b) = fixture();
    let p = prepare(&d, &o, &b).await;
    let r = request(
        &d,
        "confirm",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":p.confirmation_id}),
    );
    dispatch_with(&d, &o, r.clone(), b.clone()).await.unwrap();
    assert_eq!(
        terminal(&d, &o, &r.id).await.status,
        OperationStatus::Succeeded
    );
    dispatch_with(&d, &o, r, b.clone()).await.unwrap();
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    let v = b.observed.lock().unwrap().clone();
    assert_eq!(v.country.as_deref(), Some("Sweden"));
    assert_eq!(v.kill_switch, "enabled");
    assert!(crate::vpn::policy::evaluate_launch(true, &v, v.observed_at_ms).is_ok());
    assert!(crate::vpn::policy::evaluate_launch(true, &v, v.observed_at_ms + 10001).is_err());
    let mut unprotected = v.clone();
    unprotected.kill_switch = "disabled".into();
    assert!(
        crate::vpn::policy::evaluate_launch(true, &unprotected, unprotected.observed_at_ms)
            .is_err()
    );
}
#[tokio::test]
async fn route_loss_retains_original_uncertain_operation_without_a_second_change() {
    let (_dir, d, o, mut b) = fixture();
    b.uncertain = true;
    let p = prepare(&d, &o, &b).await;
    let r = request(
        &d,
        "confirm",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":p.confirmation_id}),
    );
    dispatch_with(&d, &o, r.clone(), b.clone()).await.unwrap();
    let original = terminal(&d, &o, &r.id).await;
    assert_eq!(original.status, OperationStatus::OutcomeUnknown);
    let lookup = d
        .dispatch(
            &o,
            request(
                &d,
                "lookup",
                "operations.get",
                serde_json::json!({"requestId":r.id}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        lookup["operationId"],
        serde_json::to_value(original.operation_id).unwrap()
    );
    dispatch_with(&d, &o, r, b.clone()).await.unwrap();
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn stale_semantics_owner_expiry_and_dead_lan_reject_before_effect() {
    let (_dir, d, o, b) = fixture();
    let p = prepare(&d, &o, &b).await;
    let other = RemotePrincipal {
        device_id: checked("other").unwrap(),
        scopes: o.scopes.clone(),
    };
    let r = request(
        &d,
        "wrong",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":p.confirmation_id}),
    );
    assert_eq!(
        dispatch_with(&d, &other, r, b.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    b.observed.lock().unwrap().country = Some("Norway".into());
    let r = request(
        &d,
        "stale",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":p.confirmation_id}),
    );
    let error = dispatch_with(&d, &o, r, b.clone()).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::ConfirmationStale);
    let fresh = error.replacement_preview.unwrap();
    d.services
        .confirmations
        .lock()
        .unwrap()
        .pending
        .get_mut(fresh.confirmation_id.as_str())
        .unwrap()
        .expires_at = 0;
    assert_eq!(
        dispatch_with(
            &d,
            &o,
            request(
                &d,
                "expired",
                "vpn.confirmChange",
                serde_json::json!({"confirmationId":fresh.confirmation_id})
            ),
            b.clone()
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::ConfirmationExpired
    );
    d.services.network.lock().unwrap().replace(Vec::new());
    assert_eq!(
        dispatch_with(
            &d,
            &o,
            request(
                &d,
                "offline",
                "vpn.prepareChange",
                serde_json::json!({"action":"disconnect"})
            ),
            b.clone()
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Unavailable
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn stale_future_and_error_observations_remain_unknown() {
    let (_dir, d, _, b) = fixture();
    for at in [0, now_ms().saturating_sub(10001), now_ms() + 10000] {
        let mut home = d.services.state.home.lock().unwrap();
        home.vpn = b.observed.lock().unwrap().clone();
        home.vpn.observed_at_ms = at;
        let topic = super::super::state::vpn_topic(&home, 1).unwrap();
        assert_eq!(topic.state, VpnState::Unknown);
        assert_eq!(topic.kill_switch, SwitchState::Unknown);
        assert!(topic.country.is_none());
    }
    assert_eq!(
        crate::vpn::nordvpn::parse_version("NordVPN Version 3.20.1"),
        Some("3.20.1".into())
    );
    assert!(crate::vpn::nordvpn::parse_version("token=private").is_none());
    assert!(crate::vpn::nordvpn::parse_version("NordVPN Version 3.20.1\nsecret").is_none());
}

#[derive(Clone)]
struct Hanging(Fake);
impl VpnBackend for Hanging {
    async fn observe(&self, s: &ControlServices) -> Result<VpnSnapshot, AppError> {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        self.0.observe(s).await
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        self.0.countries().await
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        c: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        self.0.change(s, p, c, authorize).await
    }
}
#[tokio::test]
async fn hanging_provider_and_busy_gate_return_typed_errors_before_socket_deadline() {
    let (_dir, d, o, b) = fixture();
    for (id, method, params) in [
        ("state", "vpn.state", serde_json::json!({})),
        ("countries", "vpn.countries", serde_json::json!({})),
        (
            "prepare",
            "vpn.prepareChange",
            serde_json::json!({"action":"disconnect"}),
        ),
    ] {
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(3500),
            dispatch_with(&d, &o, request(&d, id, method, params), Hanging(b.clone())),
        )
        .await;
        assert!(
            result.is_ok(),
            "Provider reads must yield a typed result before the five-second socket deadline"
        );
        assert_eq!(result.unwrap().unwrap_err().code, ErrorCode::Unavailable);
    }
    let _gate = d.services.settings_gate.lock().await;
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(3500),
        dispatch_with(
            &d,
            &o,
            request(&d, "busy", "vpn.state", serde_json::json!({})),
            b.clone(),
        ),
    )
    .await;
    assert!(
        result.is_ok(),
        "Gate wait is part of the same admission budget"
    );
    assert_eq!(result.unwrap().unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        d.services
            .state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM command_journal", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[derive(Clone)]
struct Slow(Fake);
impl VpnBackend for Slow {
    async fn observe(&self, s: &ControlServices) -> Result<VpnSnapshot, AppError> {
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
        self.0.observe(s).await
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
        self.0.countries().await
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        c: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        self.0.change(s, p, c, authorize).await
    }
}
#[tokio::test]
async fn preparation_has_one_total_budget_and_no_effect_when_country_read_exceeds_it() {
    let (_dir, d, o, b) = fixture();
    let r = request(
        &d,
        "slow",
        "vpn.prepareChange",
        serde_json::json!({"action":"connect","countryId":"Sweden"}),
    );
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(3500),
        dispatch_with(&d, &o, r, Slow(b.clone())),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    assert!(d
        .services
        .confirmations
        .lock()
        .unwrap()
        .previews(now_ms())
        .is_empty());
}
#[tokio::test]
async fn admitted_native_change_runs_beyond_ack_budget_without_replay() {
    let (_dir, d, o, mut b) = fixture();
    b.delay = std::time::Duration::from_secs(6);
    let preview = prepare(&d, &o, &b).await;
    let r = request(
        &d,
        "long",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        dispatch_with(&d, &o, r.clone(), b.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(6100)).await;
    assert_eq!(
        terminal(&d, &o, &r.id).await.status,
        OperationStatus::Succeeded
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct ChangesAuthority(Fake, bool);
impl VpnBackend for ChangesAuthority {
    async fn observe(&self, s: &ControlServices) -> Result<VpnSnapshot, AppError> {
        let observation = self.0.observe(s).await?;
        if self.1 {
            s.state
                .db
                .lock()
                .unwrap()
                .execute("UPDATE paired_devices SET scopes_json='[\"browse\"]'", [])
                .unwrap();
        } else {
            let mut network = s.network.lock().unwrap();
            network.replace(vec!["192.168.1.3:47832".parse().unwrap()]);
            network.note(
                &checked("phone").unwrap(),
                CancellationToken::new(),
                CancellationToken::new(),
                "socket-a",
            );
        }
        Ok(observation)
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        self.0.countries().await
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        c: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        self.0.change(s, p, c, authorize).await
    }
}
#[tokio::test]
async fn authority_changes_during_observation_never_issue_preview_or_intent() {
    for remove_scope in [false, true] {
        let (_dir, d, o, b) = fixture();
        let r = request(
            &d,
            "authority-race",
            "vpn.prepareChange",
            serde_json::json!({"action":"disconnect"}),
        );
        let id = r.id.clone();
        assert!(
            dispatch_with(&d, &o, r, ChangesAuthority(b.clone(), remove_scope))
                .await
                .is_err()
        );
        assert!(d
            .services
            .operations()
            .lookup_request(&o, &id)
            .unwrap()
            .is_none());
        assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn replacement_socket_cannot_confirm_another_connections_preview() {
    let (_dir, d, o, b) = fixture();
    let preview = prepare(&d, &o, &b).await;
    d.services.network.lock().unwrap().note(
        &o.device_id,
        CancellationToken::new(),
        CancellationToken::new(),
        "socket-b",
    );
    let r = request(
        &d,
        "replacement-socket",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    assert!(dispatch_with(&d, &o, r.clone(), b.clone()).await.is_err());
    assert!(d
        .services
        .operations()
        .lookup_request(&o, &r.id)
        .unwrap()
        .is_none());
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn provider_final_read_cannot_outlive_effect_authority() {
    let (_dir, d, o, mut b) = fixture();
    let preview = prepare(&d, &o, &b).await;
    b.lose_authority_before_effect = true;
    let r = request(
        &d,
        "final-authority",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    dispatch_with(&d, &o, r.clone(), b.clone()).await.unwrap();
    assert_ne!(
        terminal(&d, &o, &r.id).await.status,
        OperationStatus::Succeeded
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct PausedObservation {
    fake: Fake,
    entered: Arc<tokio::sync::Notify>,
    resume: Arc<tokio::sync::Notify>,
    paused: Arc<std::sync::atomic::AtomicBool>,
}
impl VpnBackend for PausedObservation {
    async fn observe(&self, s: &ControlServices) -> Result<VpnSnapshot, AppError> {
        if !self.paused.swap(true, Ordering::SeqCst) {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        self.fake.observe(s).await
    }
    async fn countries(&self) -> Result<Vec<CountryOption>, AppError> {
        self.fake.countries().await
    }
    async fn change(
        &self,
        s: &ControlServices,
        p: VpnPrepareChangeParams,
        c: CancellationToken,
        authorize: EffectCheck,
    ) -> Result<(), AppError> {
        self.fake.change(s, p, c, authorize).await
    }
}
#[derive(Clone, Copy)]
enum ConfirmationInterruption {
    Cancel,
    Expire,
    Replace,
}
async fn interrupted_confirmation(mode: ConfirmationInterruption) {
    let (_dir, d, o, b) = fixture();
    let preview = prepare(&d, &o, &b).await;
    let r = request(
        &d,
        "paused-confirm",
        "vpn.confirmChange",
        serde_json::json!({"confirmationId":preview.confirmation_id}),
    );
    let backend = PausedObservation {
        fake: b.clone(),
        entered: Arc::default(),
        resume: Arc::default(),
        paused: Arc::default(),
    };
    let task = {
        let (dispatcher, actor, command, provider) =
            (d.clone(), o.clone(), r.clone(), backend.clone());
        tokio::spawn(async move { dispatch_with(&dispatcher, &actor, command, provider).await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    match mode {
        ConfirmationInterruption::Cancel => {
            d.dispatch(
                &o,
                request(
                    &d,
                    "cancel-from-phone",
                    "confirmations.cancel",
                    serde_json::json!({"confirmationId":preview.confirmation_id}),
                ),
            )
            .await
            .unwrap();
        }
        ConfirmationInterruption::Expire => {
            d.services
                .confirmations
                .lock()
                .unwrap()
                .pending
                .get_mut(preview.confirmation_id.as_str())
                .unwrap()
                .expires_at = 0;
        }
        ConfirmationInterruption::Replace => {
            let mut registry = d.services.confirmations.lock().unwrap();
            let mut replacement = registry
                .get(&o.device_id, &preview.confirmation_id, now_ms())
                .unwrap();
            replacement.preview.confirmation_id = checked("replacement-preview").unwrap();
            registry.insert(replacement, now_ms()).unwrap();
        }
    }
    backend.resume.notify_one();
    let result = task
        .await
        .expect("confirmation refusal must not panic or poison the registry");
    let code = result.unwrap_err().code;
    assert_eq!(
        code,
        if matches!(mode, ConfirmationInterruption::Expire) {
            ErrorCode::ConfirmationExpired
        } else {
            ErrorCode::NotFound
        }
    );
    assert!(
        d.services
            .operations()
            .lookup_request(&o, &r.id)
            .unwrap()
            .is_none(),
        "no accepted effect intent after interruption"
    );
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    let registry = d
        .services
        .confirmations
        .lock()
        .expect("registry remains usable");
    if matches!(mode, ConfirmationInterruption::Replace) {
        assert!(registry
            .get(
                &o.device_id,
                &checked("replacement-preview").unwrap(),
                now_ms()
            )
            .is_ok());
    }
}
#[tokio::test]
async fn cancel_during_paused_confirm_refuses_without_effect_or_poisoning_registry() {
    interrupted_confirmation(ConfirmationInterruption::Cancel).await;
}
#[tokio::test]
async fn expiry_during_paused_confirm_refuses_without_effect_or_intent() {
    interrupted_confirmation(ConfirmationInterruption::Expire).await;
}
#[tokio::test]
async fn replacement_during_paused_confirm_preserves_new_sheet_without_effect() {
    interrupted_confirmation(ConfirmationInterruption::Replace).await;
}

#[tokio::test]
async fn native_vpn_publication_does_not_stale_preparation_or_preview_settings_revision() {
    let (_dir, d, o, b) = fixture();
    let before = crate::control::settings::read(&d).unwrap();
    let r = request(
        &d,
        "after-poll",
        "vpn.prepareChange",
        serde_json::json!({"action":"disconnect"}),
    );
    crate::events::commit(&d.services.state, |home| home.vpn.observed_at_ms = now_ms()).unwrap();
    let result = dispatch_with(&d, &o, r, b).await.unwrap();
    let preview: ConfirmationPreview = serde_json::from_value(result["preview"].clone()).unwrap();
    assert_eq!(preview.expected.settings_revision, Some(before.revision));
}

#[tokio::test]
async fn native_preference_edit_during_preparation_refuses_old_revision() {
    let (_dir, d, o, b) = fixture();
    let r = request(
        &d,
        "prepare-during-edit",
        "vpn.prepareChange",
        serde_json::json!({"action":"disconnect"}),
    );
    let provider = PausedObservation {
        fake: b.clone(),
        entered: Arc::default(),
        resume: Arc::default(),
        paused: Arc::default(),
    };
    let task = {
        let (dispatcher, actor, command, provider) =
            (d.clone(), o.clone(), r.clone(), provider.clone());
        tokio::spawn(async move { dispatch_with(&dispatcher, &actor, command, provider).await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        provider.entered.notified(),
    )
    .await
    .unwrap();
    crate::control::settings::save_theme(&d.services.state, crate::contracts::Theme::Light)
        .unwrap();
    provider.resume.notify_one();
    assert_eq!(task.await.unwrap().unwrap_err().code, ErrorCode::StaleState);
    assert!(d
        .services
        .operations()
        .lookup_request(&o, &r.id)
        .unwrap()
        .is_none());
    assert_eq!(b.calls.load(Ordering::SeqCst), 0);
}
