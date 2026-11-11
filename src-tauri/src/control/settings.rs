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
        if expected.is_some_and(|r| r != home.revision) {
            saved = Err(AppError::new("stale_state", "Settings changed."));
            return;
        }
        saved = (|| -> Result<(), AppError> {
            state.db.lock().map_err(|_|AppError::new("unavailable", "Settings unavailable."))?.execute(
                "INSERT INTO settings(key,value_json,updated_at_ms) VALUES('theme',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",
                rusqlite::params![serde_json::to_string(&theme).unwrap(), now_ms() as i64])?;
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
    let mut saved = Ok(());
    let publication = crate::events::commit(state, |home| {
        if expected.is_some_and(|r| r != home.revision) {
            saved = Err(AppError::new("stale_state", "Settings changed."));
            return;
        }
        saved = (|| -> Result<(), AppError> {
            state.db.lock().map_err(|_|AppError::new("unavailable", "Settings unavailable."))?.execute(
                "INSERT INTO service_preferences(service_id,vpn_required) VALUES(?1,?2) ON CONFLICT(service_id) DO UPDATE SET vpn_required=excluded.vpn_required",
                rusqlite::params![crate::streaming::catalog::key(id), required])?;
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
    let home = d.services.state.snapshot().map_err(native_error)?;
    let start = {
        let mut db = d.services.state.db.lock().map_err(|_| unavailable())?;
        RemoteStore::new(&mut db, &d.services.server_identity)
            .preferences()
            .map_err(native_error)?
            .start_at_login
    };
    checked(
        serde_json::json!({"revision":home.revision.to_string(),"theme":home.theme,"startAtLogin":start,
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
pub(crate) fn check_revision(d: &Dispatcher, request: &RemoteRequest) -> Result<(), RemoteError> {
    if request
        .expected
        .as_ref()
        .and_then(|e| e.settings_revision.as_ref())
        .map(Decimal::value)
        != Some(d.services.state.snapshot().map_err(native_error)?.revision)
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
            parse_request(&serde_json::json!({"v":1,"id":id,"method":method,"params":params,"expected":{"serverEpoch":d.services.state.event_hub.epoch(),"settingsRevision":d.services.state.snapshot().unwrap().revision.to_string()}}).to_string()).unwrap()
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
            Some(home.revision - 1)
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
}
