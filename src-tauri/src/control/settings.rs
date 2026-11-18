//! Routine preferences share the desktop catalog and persistence boundary.
use crate::{
    app_state::{now_ms, AppState},
    contracts::{AppError, ServiceId, Theme},
    events::HomePublication,
    remote::{
        adapters::checked,
        dispatch::{native_error, unavailable, value, Dispatcher},
        protocol::*,
        store::RemoteStore,
    },
};

pub fn save_theme(state: &AppState, theme: Theme) -> Result<HomePublication, AppError> {
    save_theme_at(state, theme, None)
}
fn save_theme_at(
    state: &AppState,
    theme: Theme,
    expected: Option<u64>,
) -> Result<HomePublication, AppError> {
    let mut saved = Ok(());
    let publication = crate::events::commit(state, |home| {
        saved = (|| -> Result<(), AppError> {
            let mut db = state
                .db
                .lock()
                .map_err(|_| AppError::new("unavailable", "Settings unavailable."))?;
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let revision = crate::storage::settings_revision::check(&tx, expected)?;
            if home.theme != theme {
                tx.execute(
                    "INSERT INTO settings(key,value_json,updated_at_ms) VALUES('theme',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",
                    rusqlite::params![serde_json::to_string(&theme).unwrap(), now_ms() as i64])?;
                crate::storage::settings_revision::advance(&tx, revision)?;
            }
            tx.commit()?;
            Ok(())
        })();
        if saved.is_ok() {
            home.theme = theme;
        }
    })?;
    saved?;
    Ok(publication)
}
pub fn save_requirement(
    state: &AppState,
    id: ServiceId,
    required: bool,
) -> Result<HomePublication, AppError> {
    save_requirement_at(state, id, required, None)
}
fn save_requirement_at(
    state: &AppState,
    id: ServiceId,
    required: bool,
    expected: Option<u64>,
) -> Result<HomePublication, AppError> {
    let _requirement = state.vpn_requirement_gate.try_lock().map_err(|_| {
        AppError::new(
            "busy",
            "Wait for the browser to finish opening before changing its VPN requirement.",
        )
    })?;
    let mut saved = Ok(());
    let publication = crate::events::commit(state, |home| {
        saved = (|| -> Result<(), AppError> {
            let mut db = state
                .db
                .lock()
                .map_err(|_| AppError::new("unavailable", "Settings unavailable."))?;
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let revision = crate::storage::settings_revision::check(&tx, expected)?;
            let current = home
                .services
                .iter()
                .find(|service| service.id == id)
                .ok_or_else(|| AppError::new("invalid_request", "Unknown service."))?;
            if current.vpn_required != required {
                tx.execute(
                    "INSERT INTO service_preferences(service_id,vpn_required) VALUES(?1,?2) ON CONFLICT(service_id) DO UPDATE SET vpn_required=excluded.vpn_required",
                    rusqlite::params![crate::streaming::catalog::key(id), required])?;
                crate::storage::settings_revision::advance(&tx, revision)?;
            }
            tx.commit()?;
            Ok(())
        })();
        if saved.is_ok() {
            if let Some(service) = home.services.iter_mut().find(|s| s.id == id) {
                service.vpn_required = required;
            }
        }
    })?;
    saved?;
    Ok(publication)
}
pub fn read(d: &Dispatcher) -> Result<SettingsState, RemoteError> {
    if crate::remote::autostart::pending(&d.services.state, &d.services.server_identity)
        .map_err(native_error)?
    {
        return Err(RemoteError::new(ErrorCode::Unavailable,"A startup preference change is incomplete. Review Remote availability on the computer."));
    }
    let (home, revision, start) = {
        let home = d.services.state.home.lock().map_err(|_| unavailable())?;
        let mut db = d.services.state.db.lock().map_err(|_| unavailable())?;
        let revision = crate::storage::settings_revision::revision(&db).map_err(native_error)?;
        let start = RemoteStore::new(&mut db, &d.services.server_identity)
            .preferences()
            .map_err(native_error)?
            .start_at_login;
        (home.clone(), revision, start)
    };
    checked(
        serde_json::json!({"revision":revision.to_string(),"theme":home.theme,"startAtLogin":start,"idleInhibition":d.services.state.idle.status(),
        "servicePreferences":home.services.iter().map(|s|serde_json::json!({"serviceId":crate::streaming::catalog::key(s.id),"label":s.name,"vpnRequired":s.vpn_required})).collect::<Vec<_>>(),
        "connectionDiagnostics":d.services.network.lock().map_err(|_|unavailable())?.diagnostics}),
    )
}
pub fn handles(command: &RemoteCommand) -> bool {
    matches!(
        command,
        RemoteCommand::SettingsGet(_)
            | RemoteCommand::SettingsThemeSet(_)
            | RemoteCommand::VpnRequirementSet(_)
    )
}
pub(crate) fn check_epoch(d: &Dispatcher, request: &RemoteRequest) -> Result<(), RemoteError> {
    if request
        .expected
        .as_ref()
        .and_then(|e| e.server_epoch.as_ref())
        != Some(&d.services.state.event_hub.epoch())
    {
        return Err(RemoteError::new(ErrorCode::StaleState, "Refresh settings."));
    }
    Ok(())
}
pub(crate) fn revision(state: &AppState) -> Result<u64, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::new("unavailable", "Settings unavailable."))?;
    crate::storage::settings_revision::revision(&db)
}
pub(crate) fn check_revision(d: &Dispatcher, request: &RemoteRequest) -> Result<(), RemoteError> {
    if request
        .expected
        .as_ref()
        .and_then(|e| e.settings_revision.as_ref())
        .map(Decimal::value)
        != Some(revision(&d.services.state).map_err(native_error)?)
    {
        return Err(RemoteError::new(
            ErrorCode::StaleState,
            "Settings changed. Refresh settings.",
        ));
    }
    Ok(())
}
pub(crate) fn authorize_active(d: &Dispatcher, owner: &RemotePrincipal) -> Result<(), RemoteError> {
    let mut db = d.services.state.db.lock().map_err(|_| unavailable())?;
    let active = RemoteStore::new(&mut db, &d.services.server_identity)
        .active_devices()
        .map_err(native_error)?;
    if !active
        .iter()
        .any(|a| a.device_id == owner.device_id && a.scopes.as_slice().contains(&Scope::Settings))
    {
        return Err(RemoteError::new(
            ErrorCode::Forbidden,
            "This phone is no longer authorized.",
        ));
    }
    Ok(())
}
pub async fn dispatch(
    d: &Dispatcher,
    owner: &RemotePrincipal,
    request: RemoteRequest,
) -> Result<serde_json::Value, RemoteError> {
    Dispatcher::authorize(owner, &request)?;
    if matches!(request.command, RemoteCommand::SettingsGet(_)) {
        return value(read(d)?);
    }
    check_epoch(d, &request)?;
    let _gate = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        d.services.settings_gate.lock(),
    )
    .await
    .map_err(|_| {
        RemoteError::new(
            ErrorCode::Busy,
            "Settings are busy. Wait for the current action to finish.",
        )
    })?;
    if let Some(replay) = d.activity_replay(owner, &request)? {
        return Ok(replay);
    }
    check_revision(d, &request)?;
    authorize_active(d, owner)?;
    let command = request.command.clone();
    let service = if let RemoteCommand::VpnRequirementSet(p) = &command {
        Some(
            d.services
                .state
                .snapshot()
                .map_err(native_error)?
                .services
                .into_iter()
                .find(|s| crate::streaming::catalog::key(s.id) == p.service_id.as_str())
                .ok_or_else(RemoteError::invalid)?
                .id,
        )
    } else {
        None
    };
    let native = d.clone();
    let actor = owner.clone();
    let expected = request.clone();
    let outcome = d
        .services
        .operations()
        .start(d.context(owner, &request), request, move |_| async move {
            let _gate = native.services.settings_gate.lock().await;
            authorize_active(&native, &actor)
                .map_err(|_| AppError::new("forbidden", "Phone authorization changed."))?;
            check_revision(&native, &expected)
                .map_err(|_| AppError::new("stale_state", "Settings changed."))?;
            let publication = match command {
                RemoteCommand::SettingsThemeSet(p) => save_theme_at(
                    &native.services.state,
                    p.theme.into(),
                    expected
                        .expected
                        .as_ref()
                        .and_then(|e| e.settings_revision.as_ref())
                        .map(Decimal::value),
                )?,
                RemoteCommand::VpnRequirementSet(p) => save_requirement_at(
                    &native.services.state,
                    service.unwrap(),
                    p.required,
                    expected
                        .expected
                        .as_ref()
                        .and_then(|e| e.settings_revision.as_ref())
                        .map(Decimal::value),
                )?,
                _ => return Err(AppError::new("unsupported", "Unsupported setting.")),
            };
            if let Some(app) = native.services.app() {
                publication.emit(app, &native.services.state);
            }
            Ok(())
        })
        .map_err(native_error)?;
    value(AcceptedResult {
        status: OperationAccepted::Accepted,
        operation_id: outcome.operation_id,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        app_state::AppState,
        control::ControlServices,
        remote::{adapters::checked, dispatch::Dispatcher, protocol::*},
    };

    #[tokio::test]
    async fn settings_read_uses_native_catalog_and_redacts_private_state() {
        let dir = tempfile::tempdir().unwrap();
        let services = ControlServices::native(
            AppState::open(dir.path().into()).unwrap(),
            checked("server").unwrap(),
        )
        .unwrap();
        let dispatcher = Dispatcher::new(services);
        let owner = checked(serde_json::json!({"deviceId":"phone","scopes":["browse","settings"]}))
            .unwrap();
        let request =
            parse_request(r#"{"v":1,"id":"read","method":"settings.get","params":{}}"#).unwrap();
        let result = dispatcher.dispatch(&owner, request).await.unwrap();
        assert_eq!(result["servicePreferences"].as_array().unwrap().len(), 8);
        assert_eq!(result["servicePreferences"][0]["label"], "Netflix");
        assert!(result.get("credential").is_none());
        assert!(result.get("pairingAttemptId").is_none());
    }

    #[tokio::test]
    async fn settings_scope_and_unsupported_keys_are_denied() {
        let dir = tempfile::tempdir().unwrap();
        let services = ControlServices::native(
            AppState::open(dir.path().into()).unwrap(),
            checked("server").unwrap(),
        )
        .unwrap();
        let dispatcher = Dispatcher::new(services);
        let owner = checked(serde_json::json!({"deviceId":"phone","scopes":["browse"]})).unwrap();
        let request =
            parse_request(r#"{"v":1,"id":"read","method":"settings.get","params":{}}"#).unwrap();
        assert_eq!(
            dispatcher.dispatch(&owner, request).await.unwrap_err().code,
            ErrorCode::Forbidden
        );
        assert!(parse_request(r#"{"v":1,"id":"write","method":"settings.set","params":{"key":"killswitch","value":false}}"#).is_err());
    }

    #[tokio::test]
    async fn routine_writes_use_catalog_persistence_and_check_revision_at_commit() {
        use crate::remote::store::{CredentialHash, RemoteStore};
        let dir = tempfile::tempdir().unwrap();
        let services = ControlServices::native(
            AppState::open(dir.path().into()).unwrap(),
            checked("server").unwrap(),
        )
        .unwrap();
        let d = Dispatcher::new(services);
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse","settings"],"createdAtMs":1,"revoked":false})).unwrap();
        RemoteStore::new(
            &mut d.services.state.db.lock().unwrap(),
            &d.services.server_identity,
        )
        .add_device(
            &device,
            &CredentialHash::from_credential(&checked("a".repeat(43)).unwrap()),
        )
        .unwrap();
        let owner = RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        };
        let request = |id: &str, method: &str, params: serde_json::Value| {
            parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":super::read(&d).unwrap().revision}}).to_string()).unwrap()
        };
        assert_eq!(
            d.dispatch(
                &owner,
                request(
                    "unknown",
                    "vpn.requirement.set",
                    serde_json::json!({"serviceId":"firewall","required":false})
                )
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );
        for (id, method, params) in [
            (
                "theme",
                "settings.theme.set",
                serde_json::json!({"theme":"light"}),
            ),
            (
                "requirement",
                "vpn.requirement.set",
                serde_json::json!({"serviceId":"netflix","required":false}),
            ),
        ] {
            let r = request(id, method, params);
            let request_id = r.id.clone();
            d.dispatch(&owner, r).await.unwrap();
            for _ in 0..100 {
                let op = d
                    .services
                    .operations()
                    .lookup_request(&owner, &request_id)
                    .unwrap()
                    .unwrap();
                if op.status == OperationStatus::Succeeded {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            assert_eq!(
                d.services
                    .operations()
                    .lookup_request(&owner, &request_id)
                    .unwrap()
                    .unwrap()
                    .status,
                OperationStatus::Succeeded
            );
        }
        let home = d.services.state.snapshot().unwrap();
        assert_eq!(home.theme, crate::contracts::Theme::Light);
        assert!(!home.services[0].vpn_required);
        let db = d.services.state.db.lock().unwrap();
        assert_eq!(
            db.query_row(
                "SELECT value_json FROM settings WHERE key='theme'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "\"light\""
        );
        assert!(!db
            .query_row(
                "SELECT vpn_required FROM service_preferences WHERE service_id='netflix'",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        drop(db);
        assert!(super::save_theme_at(
            &d.services.state,
            crate::contracts::Theme::Dark,
            Some(super::read(&d).unwrap().revision.value() - 1)
        )
        .is_err());
        assert_eq!(
            d.services.state.snapshot().unwrap().theme,
            crate::contracts::Theme::Light
        );
    }

    #[tokio::test]
    async fn busy_routine_write_returns_typed_busy_without_a_journal_intent() {
        let dir = tempfile::tempdir().unwrap();
        let services = ControlServices::native(
            AppState::open(dir.path().into()).unwrap(),
            checked("server").unwrap(),
        )
        .unwrap();
        let d = Dispatcher::new(services);
        let owner = checked(serde_json::json!({"deviceId":"phone","scopes":["browse","settings"]}))
            .unwrap();
        let request=parse_request(&serde_json::json!({"v":1,"id":"busy","method":"settings.theme.set","params":{"theme":"light"},"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":"1"}}).to_string()).unwrap();
        let _gate = d.services.settings_gate.lock().await;
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(3500),
            d.dispatch(&owner, request),
        )
        .await
        .expect("typed busy before transport deadline");
        assert_eq!(result.unwrap_err().code, ErrorCode::Busy);
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
    #[test]
    fn native_vpn_publication_preserves_preference_revision_and_atomic_edit_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let d = Dispatcher::new(
            ControlServices::native(
                AppState::open(dir.path().into()).unwrap(),
                checked("server").unwrap(),
            )
            .unwrap(),
        );
        let before = super::read(&d).unwrap();
        let home_before = d.services.state.snapshot().unwrap().revision;
        crate::events::commit(&d.services.state, |home| {
            home.vpn.observed_at_ms = crate::app_state::now_ms()
        })
        .unwrap();
        assert!(d.services.state.snapshot().unwrap().revision > home_before);
        assert_eq!(super::read(&d).unwrap().revision, before.revision);
        super::save_theme_at(
            &d.services.state,
            crate::contracts::Theme::Light,
            Some(before.revision.value()),
        )
        .unwrap();
        let edited = super::read(&d).unwrap();
        assert!(edited.revision.value() > before.revision.value());
        super::save_requirement(
            &d.services.state,
            crate::contracts::ServiceId::Netflix,
            false,
        )
        .unwrap();
        assert!(super::save_theme_at(
            &d.services.state,
            crate::contracts::Theme::Dark,
            Some(edited.revision.value())
        )
        .is_err());
        assert_eq!(super::read(&d).unwrap().theme, Theme::Light);
    }
    fn preference_fixture() -> (tempfile::TempDir, Dispatcher) {
        let dir = tempfile::tempdir().unwrap();
        let d = Dispatcher::new(
            ControlServices::native(
                AppState::open(dir.path().into()).unwrap(),
                checked("server").unwrap(),
            )
            .unwrap(),
        );
        (dir, d)
    }
    #[test]
    fn preference_revision_survives_restart_ignores_noops_and_tracks_setup_preferences() {
        let (dir, d) = preference_fixture();
        let original = super::read(&d).unwrap().revision;
        super::save_theme(&d.services.state, crate::contracts::Theme::Dark).unwrap();
        super::save_requirement(
            &d.services.state,
            crate::contracts::ServiceId::Netflix,
            true,
        )
        .unwrap();
        {
            let mut db = d.services.state.db.lock().unwrap();
            let mut store =
                crate::remote::store::RemoteStore::new(&mut db, &d.services.server_identity);
            let before = store.preferences().unwrap();
            let mut saved = store.save_preferences(&before).unwrap();
            assert!(saved.revision.value() > before.revision.value());
            drop(db);
            assert_eq!(super::read(&d).unwrap().revision, original);
            saved.start_at_login = true;
            let mut db = d.services.state.db.lock().unwrap();
            crate::remote::store::RemoteStore::new(&mut db, &d.services.server_identity)
                .save_preferences(&saved)
                .unwrap();
        }
        let edited = super::read(&d).unwrap();
        assert_eq!(edited.revision.value(), original.value() + 1);
        assert!(edited.start_at_login);
        let reopened = Dispatcher::new(
            ControlServices::native(
                AppState::open(dir.path().into()).unwrap(),
                checked("server").unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(super::read(&reopened).unwrap().revision, edited.revision);
        assert!(super::read(&reopened).unwrap().start_at_login);
        assert_ne!(
            reopened.services.state.event_hub.epoch(),
            d.services.state.event_hub.epoch()
        );
        let stale = parse_request(&serde_json::json!({"v":1,"id":"old-epoch","method":"settings.theme.set","params":{"theme":"light"},"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":edited.revision}}).to_string()).unwrap();
        assert_eq!(
            super::check_epoch(&reopened, &stale).unwrap_err().code,
            ErrorCode::StaleState
        );
    }
    #[test]
    fn concurrent_native_preference_edits_share_one_atomic_revision() {
        let (_dir, d) = preference_fixture();
        let revision = super::read(&d).unwrap().revision.value();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let theme = {
            let state = d.services.state.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                super::save_theme_at(&state, crate::contracts::Theme::Light, Some(revision)).is_ok()
            })
        };
        let service = {
            let state = d.services.state.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                super::save_requirement_at(
                    &state,
                    crate::contracts::ServiceId::Netflix,
                    false,
                    Some(revision),
                )
                .is_ok()
            })
        };
        barrier.wait();
        assert_ne!(theme.join().unwrap(), service.join().unwrap());
        assert_eq!(super::read(&d).unwrap().revision.value(), revision + 1);
    }
    #[test]
    fn invalid_or_exhausted_preference_counter_rolls_back_every_writer() {
        use rusqlite::OptionalExtension;
        for invalid in ["not-json", "0", "18446744073709551615"] {
            let (_dir, d) = preference_fixture();
            d.services.state.db.lock().unwrap().execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('routine_settings_revision',?1,0)", [invalid]).unwrap();
            assert!(super::save_theme(&d.services.state, crate::contracts::Theme::Light).is_err());
            assert!(super::save_requirement(
                &d.services.state,
                crate::contracts::ServiceId::Netflix,
                false
            )
            .is_err());
            let mut db = d.services.state.db.lock().unwrap();
            let mut store =
                crate::remote::store::RemoteStore::new(&mut db, &d.services.server_identity);
            let before = store.preferences().unwrap();
            let mut changed = before.clone();
            changed.start_at_login = true;
            assert!(store.save_preferences(&changed).is_err());
            assert_eq!(store.preferences().unwrap().revision, before.revision);
            assert!(!store.preferences().unwrap().start_at_login);
            let theme: Option<String> = db
                .query_row(
                    "SELECT value_json FROM settings WHERE key='theme'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .unwrap();
            assert!(theme.is_none());
            assert_eq!(
                db.query_row("SELECT count(*) FROM service_preferences", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                db.query_row(
                    "SELECT value_json FROM settings WHERE key='routine_settings_revision'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                invalid
            );
            drop(db);
            assert_eq!(
                d.services.state.snapshot().unwrap().theme,
                crate::contracts::Theme::Dark
            );
            assert!(d.services.state.snapshot().unwrap().services[0].vpn_required);
        }
    }
    #[test]
    fn failed_counter_persistence_rolls_back_preference_and_home_update() {
        let (_dir, d) = preference_fixture();
        d.services.state.db.lock().unwrap().execute_batch("CREATE TRIGGER fail_counter BEFORE INSERT ON settings WHEN NEW.key='routine_settings_revision' BEGIN SELECT RAISE(ABORT, 'fixture write failure'); END;").unwrap();
        assert!(super::save_theme(&d.services.state, crate::contracts::Theme::Light).is_err());
        assert!(super::save_requirement(
            &d.services.state,
            crate::contracts::ServiceId::Netflix,
            false
        )
        .is_err());
        assert_eq!(super::read(&d).unwrap().revision.value(), 1);
        assert_eq!(super::read(&d).unwrap().theme, Theme::Dark);
        assert!(
            super::read(&d)
                .unwrap()
                .service_preferences
                .unwrap()
                .as_slice()[0]
                .vpn_required
        );
        let mut db = d.services.state.db.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM settings WHERE key='theme'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM service_preferences", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let mut store =
            crate::remote::store::RemoteStore::new(&mut db, &d.services.server_identity);
        let before = store.preferences().unwrap();
        let mut changed = before.clone();
        changed.start_at_login = true;
        assert!(store.save_preferences(&changed).is_err());
        assert_eq!(store.preferences().unwrap().revision, before.revision);
        assert!(!store.preferences().unwrap().start_at_login);
    }
}
