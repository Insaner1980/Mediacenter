//! Main-window-only remote setup. Secrets are returned only by explicit pairing.
use super::{
    adapters::checked,
    discovery::Advertisement,
    network,
    protocol::Id,
    server::{default_port, BindAddresses, RemoteServer, ServerRuntime},
    store::RemoteStore,
    tls::{self, IdentityNamespace, ServerIdentity},
};
use super::{
    network::NetworkInterface,
    protocol::{ErrorCode, RemoteError},
};
use crate::{app_state::AppState, control::ControlServices};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use ts_rs::TS;
#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSettingsSnapshot {
    #[ts(type = "number")]
    pub revision: u64,
    pub enabled: bool,
    pub listening: bool,
    pub advertising: bool,
    pub interface_name: Option<String>,
    pub port: u16,
    pub interfaces: Vec<NetworkInterface>,
    pub addresses: Vec<String>,
    pub requires_repairing: bool,
    pub status: String,
    pub pairing_attempt_id: Option<String>,
}
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteEnableRequest {
    #[ts(type = "number")]
    pub revision: u64,
    pub enabled: bool,
    pub interface_name: Option<String>,
    pub port: u16,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairingDisplay {
    pub attempt_id: String,
    pub qr_rows: Vec<String>,
    pub addresses: Vec<String>,
    pub port: u16,
    pub manual_code: String,
    pub verification_words: Vec<String>,
    #[ts(type = "number")]
    pub expires_in_ms: u64,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDevice {
    pub device_id: String,
    pub label: String,
    #[ts(type = "number")]
    pub created_at_ms: u64,
    #[ts(type = "number | null")]
    pub last_seen_at_ms: Option<u64>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NetworkConfig {
    version: u8,
    revision: u64,
    interface_name: String,
    port: u16,
}
impl NetworkConfig {
    fn validate(&self) -> Result<(), RemoteError> {
        if self.version != 1
            || self.revision == 0
            || self.revision > 9_007_199_254_740_991
            || self.port == 0
            || self.interface_name.is_empty()
            || self.interface_name.len() > 64
            || !self
                .interface_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        {
            return Err(config_error());
        }
        Ok(())
    }
    fn read(path: &Path) -> Result<Option<Self>, RemoteError> {
        let bytes = tls::read_private(path).map_err(|_| config_error())?;
        bytes
            .map(|bytes| {
                let value: Self = serde_json::from_slice(&bytes).map_err(|_| config_error())?;
                value.validate()?;
                Ok(value)
            })
            .transpose()
    }
    fn save(&self, path: &Path) -> Result<(), RemoteError> {
        self.validate()?;
        tls::write_private(path, &serde_json::to_vec(self).map_err(|_| config_error())?)
            .map_err(|_| config_error())
    }
}
fn config_error() -> RemoteError {
    RemoteError::new(ErrorCode::Unavailable,"Remote network settings are missing or invalid. Disable remote control, choose a trusted network and port, then enable it again.")
}
fn unavailable() -> RemoteError {
    RemoteError::new(ErrorCode::Unavailable,"Remote control is unavailable. Restart Mediacenter or review its private remote identity files.")
}
struct SetupState {
    runtime: Option<Arc<ServerRuntime>>,
    config: Option<NetworkConfig>,
    enabled: bool,
    revision: u64,
    status: String,
    requires_repairing: bool,
    advertisement: Option<Advertisement>,
    suspended: bool,
    network_epoch: Option<u64>,
}
#[derive(Clone)]
pub struct RemoteSetup {
    inner: Arc<tokio::sync::Mutex<SetupState>>,
    server: Arc<RemoteServer>,
    state: AppState,
    path: PathBuf,
    opposite: PathBuf,
    default_port: u16,
    other_default_port: u16,
    name: &'static str,
    settings_gate: Arc<tokio::sync::Mutex<()>>,
    autostart: Option<super::autostart::Autostart>,
}
impl RemoteSetup {
    /// Construction is local-only and fallible remote state never aborts local startup.
    pub fn new(
        app: Option<tauri::AppHandle>,
        state: AppState,
        server: Arc<RemoteServer>,
        namespace: IdentityNamespace,
    ) -> Self {
        let autostart = app
            .as_ref()
            .and_then(|_| super::autostart::Autostart::production().ok());
        let other = match namespace {
            IdentityNamespace::Production => IdentityNamespace::Development,
            IdentityNamespace::Development => IdentityNamespace::Production,
        };
        let path = state
            .data_dir
            .join(namespace.directory())
            .join("network-v1.json");
        let opposite = state
            .data_dir
            .join(other.directory())
            .join("network-v1.json");
        let initialized = (|| {
            let (identity, preferences) = {
                let mut db = state.db.lock().map_err(|_| unavailable())?;
                let identity = Arc::new(ServerIdentity::load_or_create(
                    &state.data_dir,
                    namespace,
                    &mut db,
                    crate::app_state::now_ms() as i64,
                )?);
                let preferences = RemoteStore::new(&mut db, identity.id())
                    .preferences()
                    .map_err(|_| unavailable())?;
                (identity, preferences)
            };
            let requires_repairing = identity.requires_repairing();
            let services = match app {
                Some(app) => ControlServices::new(app, state.clone(), identity.id().clone()),
                None => ControlServices::native(state.clone(), identity.id().clone()),
            }
            .map_err(|_| unavailable())?;
            let interrupted = super::autostart::pending(&state, identity.id()).unwrap_or(true);
            let runtime = ServerRuntime::new(identity, services)?;
            let (config, status) = match NetworkConfig::read(&path) {
                Ok(config) => (
                    config,
                    if preferences.enabled {
                        "Checking the selected local network.".into()
                    } else {
                        "Remote control is off.".into()
                    },
                ),
                Err(error) => (None, error.to_string()),
            };
            Ok::<_, RemoteError>(SetupState {
                runtime: Some(runtime),
                config,
                enabled: preferences.enabled,
                revision: preferences.revision.value(),
                status: if interrupted {
                    "A Remote startup change was interrupted. Disable Remote, then enable it to restore the saved startup choice.".into()
                } else {
                    status
                },
                requires_repairing,
                advertisement: None,
                suspended: interrupted,
                network_epoch: None,
            })
        })();
        let inner=initialized.unwrap_or_else(|_|SetupState{runtime:None,config:None,enabled:false,revision:1,status:"Remote control could not initialize securely. Local media remains available. Restart Mediacenter or review its private remote identity files.".into(),requires_repairing:false,advertisement:None,suspended:true,network_epoch:None});
        let settings_gate = inner
            .runtime
            .as_ref()
            .map(|r| r.dispatcher.services.settings_gate.clone())
            .unwrap_or_default();
        Self {
            settings_gate,
            autostart,
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            server,
            state,
            path,
            opposite,
            default_port: default_port(namespace),
            other_default_port: default_port(other),
            name: match namespace {
                IdentityNamespace::Production => "Mediacenter",
                IdentityNamespace::Development => "Mediacenter (development)",
            },
        }
    }
    async fn stop_locked(&self, inner: &mut SetupState) {
        // Listener cancellation closes sockets and the active pairing attempt first.
        self.server.stop().await;
        if let Some(advertisement) = inner.advertisement.take() {
            advertisement.stop().await;
        }
    }
    fn save_enabled(&self, inner: &mut SetupState, enabled: bool) -> Result<(), RemoteError> {
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?;
        let mut db = self.state.db.lock().map_err(|_| unavailable())?;
        let mut store = RemoteStore::new(&mut db, runtime.identity().id());
        let mut preferences = store.preferences().map_err(|_| unavailable())?;
        if preferences.revision.value() != inner.revision {
            return Err(RemoteError::new(
                ErrorCode::StaleState,
                "Remote settings changed. Refresh and try again.",
            ));
        }
        preferences.enabled = enabled;
        let saved = store
            .save_preferences(&preferences)
            .map_err(|_| config_error())?;
        inner.enabled = saved.enabled;
        inner.revision = saved.revision.value();
        Ok(())
    }
    fn check_revision(&self, inner: &SetupState, revision: u64) -> Result<(), RemoteError> {
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?;
        let persisted = {
            let mut db = self.state.db.lock().map_err(|_| unavailable())?;
            RemoteStore::new(&mut db, runtime.identity().id())
                .preferences()
                .map_err(|_| unavailable())?
        };
        if revision != inner.revision
            || persisted.revision.value() != inner.revision
            || persisted.enabled != inner.enabled
        {
            return Err(RemoteError::new(
                ErrorCode::StaleState,
                "Remote settings changed. Refresh and try again.",
            ));
        }
        Ok(())
    }
    async fn reconcile_locked(
        &self,
        inner: &mut SetupState,
        interfaces: &[NetworkInterface],
        epoch: u64,
    ) -> Result<(), RemoteError> {
        if !inner.enabled || inner.suspended {
            return Ok(());
        }
        let config = inner
            .config
            .as_ref()
            .filter(|config| config.revision == inner.revision)
            .cloned()
            .ok_or_else(config_error)?;
        let addresses =
            network::selected_addresses(interfaces, &config.interface_name, config.port)?;
        if self.server.addresses().await == addresses && inner.network_epoch == Some(epoch) {
            Self::discovery_status(inner).await;
            return Ok(());
        }
        let selected = BindAddresses::selected(
            addresses.clone(),
            &addresses.iter().map(SocketAddr::ip).collect::<Vec<_>>(),
        )?;
        let addresses = self.replace_listener_locked(inner, selected, epoch).await?;
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?;
        match Advertisement::start(
            runtime.identity().id().as_str(),
            self.name,
            &config.interface_name,
            &addresses,
        ) {
            Ok(advertisement) => {
                inner.advertisement = Some(advertisement);
                inner.status = "Remote control is listening on the selected local network.".into();
            }
            Err(error) => inner.status = error.to_string(),
        }
        Self::discovery_status(inner).await;
        Ok(())
    }
    async fn replace_listener_locked(
        &self,
        inner: &mut SetupState,
        selected: BindAddresses,
        epoch: u64,
    ) -> Result<Vec<SocketAddr>, RemoteError> {
        self.stop_locked(inner).await;
        let runtime = inner.runtime.clone().ok_or_else(unavailable)?;
        let addresses = self.server.start(runtime, selected).await?;
        inner.network_epoch = Some(epoch);
        Ok(addresses)
    }
    async fn reconcile_observation_locked(
        &self,
        inner: &mut SetupState,
        observed: Result<(Vec<NetworkInterface>, u64), RemoteError>,
    ) {
        let result = match observed {
            Ok((interfaces, epoch)) => self.reconcile_locked(inner, &interfaces, epoch).await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            self.stop_locked(inner).await;
            inner.status = error.to_string();
        }
    }
    pub async fn reconcile(&self) {
        let mut inner = self.inner.lock().await;
        if !inner.enabled || inner.suspended {
            return;
        }
        if inner
            .config
            .as_ref()
            .is_none_or(|config| config.revision != inner.revision)
        {
            self.stop_locked(&mut inner).await;
            inner.status = config_error().to_string();
            return;
        }
        if let Err(error) = self.check_revision(&inner, inner.revision) {
            self.stop_locked(&mut inner).await;
            inner.suspended = true;
            inner.status = error.to_string();
            return;
        }
        let observed = network::inventory()
            .and_then(|interfaces| network::epoch().map(|epoch| (interfaces, epoch)));
        self.reconcile_observation_locked(&mut inner, observed)
            .await;
    }
    async fn discovery_status(inner: &mut SetupState) -> bool {
        let status = inner.advertisement.as_mut().map(Advertisement::status);
        if let Some(runtime) = &inner.runtime {
            if let Ok(mut net) = runtime.dispatcher.services.network.lock() {
                net.diagnostics.selected_interface = inner
                    .config
                    .as_ref()
                    .and_then(|c| checked(&c.interface_name).ok());
                net.diagnostics.discovery = match &status {
                    Some(Ok(true)) => super::protocol::DiscoveryObservation::Announced,
                    Some(Ok(false)) => super::protocol::DiscoveryObservation::Pending,
                    Some(Err(_)) => super::protocol::DiscoveryObservation::Unavailable,
                    None if net.addresses.is_empty() => {
                        super::protocol::DiscoveryObservation::Stopped
                    }
                    None => super::protocol::DiscoveryObservation::Unavailable,
                };
            }
        }
        match status {
            Some(Ok(true)) => {
                inner.status = "Remote control is listening on the selected local network.".into();
                true
            }
            Some(Ok(false)) => {
                inner.status="Remote control is listening. Checking local discovery; QR and manual pairing are available.".into();
                false
            }
            Some(Err(error)) => {
                if let Some(advertisement) = inner.advertisement.take() {
                    advertisement.stop().await;
                }
                inner.status = error.to_string();
                false
            }
            None => false,
        }
    }
    pub async fn settings(&self) -> RemoteSettingsSnapshot {
        let mut inner = self.inner.lock().await;
        let advertising = Self::discovery_status(&mut inner).await;
        let interfaces = network::inventory();
        let addresses = self.server.addresses().await;
        RemoteSettingsSnapshot {
            revision: inner.revision,
            enabled: inner.enabled,
            listening: !addresses.is_empty(),
            advertising,
            interface_name: inner.config.as_ref().map(|c| c.interface_name.clone()),
            port: inner
                .config
                .as_ref()
                .map(|c| c.port)
                .unwrap_or(self.default_port),
            interfaces: interfaces.unwrap_or_default(),
            addresses: addresses.iter().map(ToString::to_string).collect(),
            requires_repairing: inner.requires_repairing,
            status: inner.status.clone(),
            pairing_attempt_id: inner.runtime.as_ref().and_then(|runtime| {
                runtime
                    .pairing_status()
                    .ok()
                    .flatten()
                    .map(|(id, _)| id.as_str().to_owned())
            }),
        }
    }
    pub async fn set_enabled(
        &self,
        request: RemoteEnableRequest,
    ) -> Result<RemoteSettingsSnapshot, RemoteError> {
        let _settings = self.settings_gate.lock().await;
        let mut inner = self.inner.lock().await;
        self.check_revision(&inner, request.revision)?;
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?.clone();
        let interrupted =
            super::autostart::pending(&self.state, runtime.identity().id()).unwrap_or(true);
        if request.enabled && interrupted {
            let saved_start = {
                let mut db = self.state.db.lock().map_err(|_| unavailable())?;
                RemoteStore::new(&mut db, runtime.identity().id())
                    .preferences()
                    .map_err(super::dispatch::native_error)?
                    .start_at_login
            };
            // Explicit local Enable restores persisted intent; startup itself never repairs files.
            self.set_lifetime_preference_locked(&mut inner, Some(saved_start), None, None, None)
                .await?;
        }

        if !request.enabled {
            self.stop_locked(&mut inner).await;
            inner.suspended = true;
            if let Err(error) = self.save_enabled(&mut inner, false) {
                inner.status = error.to_string();
                return Err(error);
            }
            inner.status = if interrupted {
                "Remote control is off. Enable Remote to restore the saved startup choice.".into()
            } else {
                "Remote control is off.".into()
            };
            inner.suspended = false;
        } else {
            let name = request.interface_name.ok_or_else(config_error)?;
            let other = NetworkConfig::read(&self.opposite)?
                .map(|config| config.port)
                .unwrap_or(self.other_default_port);
            if request.port == other {
                return Err(RemoteError::new(ErrorCode::InvalidRequest,"Production and development must use different remote ports. Choose another port."));
            }
            let interfaces = network::inventory()?;
            network::selected_addresses(&interfaces, &name, request.port)?;
            let epoch = network::epoch()?;
            self.stop_locked(&mut inner).await;
            inner.suspended = true;
            // Persist disable before replacing an enabled configuration. Any later
            // interrupted write leaves either disabled preferences or a revision mismatch.
            if inner.enabled {
                if let Err(error) = self.save_enabled(&mut inner, false) {
                    inner.status = error.to_string();
                    return Err(error);
                }
            }
            let config = NetworkConfig {
                version: 1,
                revision: inner.revision.checked_add(1).ok_or_else(config_error)?,
                interface_name: name,
                port: request.port,
            };
            if let Err(error) = config.save(&self.path) {
                inner.status = error.to_string();
                return Err(error);
            }
            inner.config = Some(config);
            if let Err(error) = self.save_enabled(&mut inner, true) {
                inner.status = error.to_string();
                return Err(error);
            }
            inner.suspended = false;
            if let Err(error) = self.reconcile_locked(&mut inner, &interfaces, epoch).await {
                self.stop_locked(&mut inner).await;
                inner.status = error.to_string();
                return Err(error);
            }
        }
        drop(inner);
        Ok(self.settings().await)
    }
    pub async fn begin_pairing(&self) -> Result<RemotePairingDisplay, RemoteError> {
        let inner = self.inner.lock().await;
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?;
        let addresses = self.server.addresses().await;
        if !inner.enabled || inner.suspended || addresses.is_empty() {
            return Err(RemoteError::new(
                ErrorCode::Unavailable,
                "Enable remote control on a trusted local network before pairing.",
            ));
        }
        // The client supplies its own interface scope for IPv6 link-local candidates.
        let candidates = addresses
            .iter()
            .map(|address| address.ip().to_string())
            .collect::<Vec<_>>();
        let ready = runtime.begin_pairing(
            checked(serde_json::json!(candidates))?,
            checked(addresses[0].port())?,
        )?;
        let attempt = ready.candidate.attempt_id.clone();
        let display = (|| {
            let qr_rows = super::pairing::qr_rows(&ready.candidate)?
                .as_slice()
                .iter()
                .map(|r| r.as_str().to_owned())
                .collect();
            let expires_in_ms = runtime
                .pairing_status()?
                .ok_or_else(unavailable)?
                .1
                .as_millis() as u64;
            Ok(RemotePairingDisplay {
                attempt_id: attempt.as_str().to_owned(),
                qr_rows,
                addresses: candidates,
                port: addresses[0].port(),
                manual_code: ready.manual_code.as_str().to_owned(),
                verification_words: ready
                    .verification_phrase
                    .words
                    .as_slice()
                    .iter()
                    .map(|word| word.as_str().to_owned())
                    .collect(),
                expires_in_ms,
            })
        })();
        if display.is_err() {
            let _ = runtime.cancel_pairing(&attempt);
        }
        display
    }
    pub async fn cancel_pairing(&self, attempt: &Id) -> Result<(), RemoteError> {
        let inner = self.inner.lock().await;
        inner
            .runtime
            .as_ref()
            .ok_or_else(unavailable)?
            .cancel_pairing(attempt)
    }
    pub async fn devices(&self) -> Result<Vec<RemoteDevice>, RemoteError> {
        let inner = self.inner.lock().await;
        // An initialization failure should not prevent reading the local media UI.
        let Some(runtime) = &inner.runtime else {
            return Ok(Vec::new());
        };
        runtime
            .list_devices()?
            .into_iter()
            .map(|device| {
                Ok(RemoteDevice {
                    device_id: device.device_id.as_str().to_owned(),
                    label: device.label.as_str().to_owned(),
                    created_at_ms: serde_json::to_value(device.created_at_ms)
                        .ok()
                        .and_then(|v| v.as_u64())
                        .ok_or_else(unavailable)?,
                    last_seen_at_ms: device.last_seen_at_ms.and_then(|value| {
                        serde_json::to_value(value).ok().and_then(|v| v.as_u64())
                    }),
                })
            })
            .collect()
    }
    pub async fn revoke(&self, device: &Id) -> Result<(), RemoteError> {
        let inner = self.inner.lock().await;
        inner
            .runtime
            .as_ref()
            .ok_or_else(unavailable)?
            .revoke_device(device)
    }
    pub async fn lifetime_preferences(
        &self,
    ) -> Result<super::protocol::RemotePreferences, RemoteError> {
        let inner = self.inner.lock().await;
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?;
        let mut db = self.state.db.lock().map_err(|_| unavailable())?;
        RemoteStore::new(&mut db, runtime.identity().id())
            .preferences()
            .map_err(super::dispatch::native_error)
    }
    /// Preference writers lock settings -> setup -> DB. File writes are synchronous and bounded;
    /// there is no process, D-Bus, listener rebind or await while the database is held.
    pub(crate) async fn set_lifetime_preference(
        &self,
        start_at_login: Option<bool>,
        keep_available: Option<bool>,
        expected_settings: Option<u64>,
        actor: Option<&super::protocol::RemotePrincipal>,
    ) -> Result<(), RemoteError> {
        let _settings = self.settings_gate.lock().await;
        let mut inner = self.inner.lock().await;
        self.set_lifetime_preference_locked(
            &mut inner,
            start_at_login,
            keep_available,
            expected_settings,
            actor,
        )
        .await
    }
    async fn set_lifetime_preference_locked(
        &self,
        inner: &mut SetupState,
        start_at_login: Option<bool>,
        keep_available: Option<bool>,
        expected_settings: Option<u64>,
        actor: Option<&super::protocol::RemotePrincipal>,
    ) -> Result<(), RemoteError> {
        let runtime = inner.runtime.as_ref().ok_or_else(unavailable)?.clone();
        let mut uncertain = false;
        let mut interrupted = false;
        let result = (|| {
            let mut db = self.state.db.lock().map_err(|_| unavailable())?;
            if let Some(expected) = expected_settings {
                if crate::storage::settings_revision::revision(&db)
                    .map_err(super::dispatch::native_error)?
                    != expected
                {
                    return Err(RemoteError::new(
                        ErrorCode::StaleState,
                        "Settings changed. Refresh settings.",
                    ));
                }
            }
            let mut store = RemoteStore::new(&mut db, runtime.identity().id());
            if let Some(actor) = actor {
                if !store
                    .active_devices()
                    .map_err(super::dispatch::native_error)?
                    .iter()
                    .any(|d| {
                        d.device_id == actor.device_id
                            && d.scopes
                                .as_slice()
                                .contains(&super::protocol::Scope::Settings)
                    })
                {
                    return Err(RemoteError::new(
                        ErrorCode::Forbidden,
                        "Phone authorization changed.",
                    ));
                }
            }
            interrupted = super::autostart::pending(&self.state, runtime.identity().id())
                .map_err(super::dispatch::native_error)?;
            if interrupted && start_at_login.is_none() {
                return Err(RemoteError::new(
                    ErrorCode::Unavailable,
                    "An interrupted startup change must be repaired first.",
                ));
            }
            let before = store.preferences().map_err(super::dispatch::native_error)?;
            if before.revision.value() != inner.revision {
                return Err(RemoteError::new(
                    ErrorCode::StaleState,
                    "Remote preferences changed.",
                ));
            }
            if keep_available == Some(true) && !before.enabled {
                return Err(RemoteError::new(
                    ErrorCode::Unavailable,
                    "Enable Remote before keeping Mediacenter available.",
                ));
            }
            let mut next = before.clone();
            if let Some(value) = start_at_login {
                next.start_at_login = value;
            }
            if let Some(value) = keep_available {
                next.keep_available = value;
            }
            let adapter = if start_at_login.is_some() {
                Some(self.autostart.as_ref().ok_or_else(unavailable)?)
            } else {
                None
            };
            let old_start = adapter
                .map(|a| a.snapshot().map_err(super::dispatch::native_error))
                .transpose()?
                .flatten();
            let old_config = tls::read_private(&self.path).map_err(|_| unavailable())?;
            if let Some(expected) = &inner.config {
                let actual = old_config
                    .as_ref()
                    .and_then(|bytes| serde_json::from_slice::<NetworkConfig>(bytes).ok());
                if actual.as_ref() != Some(expected) {
                    return Err(RemoteError::new(
                        ErrorCode::StaleState,
                        "Remote network configuration changed.",
                    ));
                }
            }
            let mut config = inner.config.clone();
            if let Some(value) = config.as_mut() {
                value.revision = before
                    .revision
                    .value()
                    .checked_add(1)
                    .ok_or_else(unavailable)?;
            }
            let new_config = config
                .as_ref()
                .map(serde_json::to_vec)
                .transpose()
                .map_err(|_| unavailable())?;
            let new_start = adapter
                .map(|a| {
                    if next.start_at_login {
                        a.contents().map(|v| Some(v.into_bytes()))
                    } else {
                        Ok(None)
                    }
                })
                .transpose()
                .map_err(super::dispatch::native_error)?
                .flatten();
            if next.start_at_login == before.start_at_login
                && next.keep_available == before.keep_available
                && (adapter.is_none() || old_start == new_start)
                && !interrupted
            {
                return Ok(());
            }
            super::autostart::begin_change(&self.state, runtime.identity().id())
                .map_err(super::dispatch::native_error)?;
            let apply = (|| {
                if let Some(adapter) = adapter {
                    adapter
                        .set_enabled(next.start_at_login)
                        .map_err(super::dispatch::native_error)?;
                }
                if let Some(bytes) = &new_config {
                    tls::write_private(&self.path, bytes).map_err(|_| unavailable())?;
                }
                store
                    .save_preferences(&next)
                    .map_err(super::dispatch::native_error)
            })();
            match apply {
                Ok(saved) => {
                    inner.revision = saved.revision.value();
                    inner.config = config;
                    if super::autostart::finish_change(&self.state, runtime.identity().id())
                        .is_err()
                    {
                        uncertain = true;
                        return Err(unavailable());
                    }
                    Ok(())
                }
                Err(error) => {
                    if let Some(adapter) = adapter {
                        let current = adapter.snapshot();
                        if current.as_ref().ok() != Some(&old_start)
                            && adapter
                                .restore(old_start.as_deref(), new_start.as_deref())
                                .is_err()
                        {
                            uncertain = true;
                        }
                    }
                    let current = tls::read_private(&self.path);
                    if current.as_ref().ok() != Some(&old_config) {
                        if current.as_ref().ok() == Some(&new_config) {
                            let restored = match &old_config {
                                Some(bytes) => tls::write_private(&self.path, bytes),
                                None => std::fs::remove_file(&self.path).map_err(|_| unavailable()),
                            };
                            if restored.is_err() {
                                uncertain = true;
                            }
                        } else {
                            uncertain = true;
                        }
                    }
                    // Rolling back this attempt does not repair an older interrupted
                    // change: those prior file bytes are still untrusted.
                    if !uncertain
                        && !interrupted
                        && super::autostart::finish_change(&self.state, runtime.identity().id())
                            .is_err()
                    {
                        uncertain = true;
                    }
                    Err(error)
                }
            }
        })();
        if uncertain || (interrupted && result.is_err()) {
            inner.suspended = true;
            self.stop_locked(inner).await;
            inner.status = "Startup recovery is incomplete. Disable and enable Remote to restore the saved startup choice.".into();
            if uncertain {
                return Err(RemoteError::new(
                    ErrorCode::OutcomeUnknown,
                    "Preference rollback could not be verified. Remote availability is suspended.",
                ));
            }
        }
        result
    }
    pub(crate) async fn keep_available(
        &self,
        hide: impl FnOnce() -> Result<(), crate::contracts::AppError>,
    ) -> Result<(), crate::contracts::AppError> {
        self.set_lifetime_preference(None, Some(true), None, None)
            .await
            .map_err(|_| {
                crate::contracts::AppError::new(
                    "unavailable",
                    "Remote availability preference could not be saved.",
                )
            })?;
        hide()
    }
    pub async fn stop(&self) {
        let mut inner = self.inner.lock().await;
        inner.suspended = true;
        self.stop_locked(&mut inner).await;
    }
}

#[tauri::command]
pub async fn get_remote_settings(
    setup: tauri::State<'_, RemoteSetup>,
) -> Result<RemoteSettingsSnapshot, RemoteError> {
    Ok(setup.settings().await)
}
#[tauri::command]
pub async fn set_remote_enabled(
    setup: tauri::State<'_, RemoteSetup>,
    config: RemoteEnableRequest,
) -> Result<RemoteSettingsSnapshot, RemoteError> {
    setup.set_enabled(config).await
}
#[tauri::command]
pub async fn begin_remote_pairing(
    setup: tauri::State<'_, RemoteSetup>,
) -> Result<RemotePairingDisplay, RemoteError> {
    setup.begin_pairing().await
}
#[tauri::command]
pub async fn cancel_remote_pairing(
    setup: tauri::State<'_, RemoteSetup>,
    attempt_id: String,
) -> Result<(), RemoteError> {
    setup.cancel_pairing(&checked(attempt_id)?).await
}
#[tauri::command]
pub async fn list_remote_devices(
    setup: tauri::State<'_, RemoteSetup>,
) -> Result<Vec<RemoteDevice>, RemoteError> {
    setup.devices().await
}
#[tauri::command]
pub async fn revoke_remote_device(
    setup: tauri::State<'_, RemoteSetup>,
    device_id: String,
) -> Result<(), RemoteError> {
    setup.revoke(&checked(device_id)?).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_config_roundtrips_atomically_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network-v1.json");
        assert!(NetworkConfig::read(&path).unwrap().is_none());
        let config = NetworkConfig {
            version: 1,
            revision: 2,
            interface_name: "wlan0".into(),
            port: 47840,
        };
        config.save(&path).unwrap();
        let saved = NetworkConfig::read(&path).unwrap().unwrap();
        assert_eq!(saved.interface_name, "wlan0");
        assert_eq!(saved.port, 47840);
        for contents in [
            r#"{"version":2,"revision":2,"interfaceName":"wlan0","port":47840}"#,
            r#"{"version":1,"revision":2,"interfaceName":"../tun0","port":0}"#,
            "invalid",
        ] {
            std::fs::write(&path, contents).unwrap();
            assert!(NetworkConfig::read(&path).is_err());
        }
    }
    fn fixture() -> (tempfile::TempDir, RemoteSetup) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().join("app")).unwrap();
        let setup = RemoteSetup::new(
            None,
            state,
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        (dir, setup)
    }
    #[tokio::test]
    async fn task20_lifetime_preferences_preserve_listener_and_rollback_exact_owned_files() {
        let (dir, mut setup) = fixture();
        let exe = dir.path().join("installed media");
        std::fs::write(&exe, b"fixture").unwrap();
        setup.autostart = Some(super::super::autostart::Autostart::new(dir.path(), &exe).unwrap());
        let runtime = {
            let mut inner = setup.inner.lock().await;
            setup.save_enabled(&mut inner, true).unwrap();
            let config = NetworkConfig {
                version: 1,
                revision: inner.revision,
                interface_name: "fixture-lan".into(),
                port: 47832,
            };
            config.save(&setup.path).unwrap();
            inner.config = Some(config);
            inner.network_epoch = Some(19);
            inner.runtime.clone().unwrap()
        };
        let addresses = setup
            .server
            .start(runtime.clone(), BindAddresses::loopback_fixture(0))
            .await
            .unwrap();
        let revision =
            crate::storage::settings_revision::revision(&setup.state.db.lock().unwrap()).unwrap();
        setup
            .set_lifetime_preference(Some(true), None, Some(revision), None)
            .await
            .unwrap();
        assert_eq!(setup.server.addresses().await, addresses);
        assert!(tokio::net::TcpStream::connect(addresses[0]).await.is_ok());
        let prefs = setup.lifetime_preferences().await.unwrap();
        assert!(prefs.enabled && prefs.start_at_login);
        let inner = setup.inner.lock().await;
        assert_eq!(inner.network_epoch, Some(19));
        assert!(!inner.suspended);
        assert_eq!(
            inner.config.as_ref().unwrap().revision,
            prefs.revision.value()
        );
        assert!(Arc::ptr_eq(inner.runtime.as_ref().unwrap(), &runtime));
        drop(inner);
        let before_config = tls::read_private(&setup.path).unwrap();
        let before_start = setup.autostart.as_ref().unwrap().snapshot().unwrap();
        setup.state.db.lock().unwrap().execute_batch("CREATE TRIGGER task20_fail BEFORE UPDATE ON remote_preferences BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
        assert!(setup
            .set_lifetime_preference(Some(false), None, None, None)
            .await
            .is_err());
        assert_eq!(tls::read_private(&setup.path).unwrap(), before_config);
        assert_eq!(
            setup.autostart.as_ref().unwrap().snapshot().unwrap(),
            before_start
        );
        assert_eq!(setup.server.addresses().await, addresses);
        assert!(setup.lifetime_preferences().await.unwrap().start_at_login);
        assert!(!super::super::autostart::pending(&setup.state, runtime.identity().id()).unwrap());

        assert_eq!(
            crate::storage::settings_revision::revision(&setup.state.db.lock().unwrap()).unwrap(),
            revision + 1
        );
        setup.stop().await;
    }
    #[tokio::test]
    async fn task20_fix1_rollback_sync_failure_keeps_marker_and_settings_unavailable() {
        let (dir, mut setup) = fixture();
        let exe = dir.path().join("installed");
        std::fs::write(&exe, b"fixture").unwrap();
        let mut adapter = super::super::autostart::Autostart::new(dir.path(), &exe).unwrap();
        let runtime = setup.inner.lock().await.runtime.clone().unwrap();
        let marker = super::super::autostart::pending_path(&setup.state, runtime.identity().id());
        let directory = dir.path().join("autostart");
        let entry = directory.join("io.emma.mediacenter-remote.desktop");
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = attempts.clone();
        adapter.sync_probe = Some(Arc::new(move |path| {
            assert!(
                marker.exists(),
                "write-ahead marker must precede every owned effect sync"
            );
            if path == directory {
                assert!(!entry.exists());
                // First sync publishes directories; second sync follows rollback unlink.
                if observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                    return Err(crate::contracts::AppError::new(
                        "unavailable",
                        "fixture sync failure",
                    ));
                }
            }
            Ok(())
        }));
        setup.autostart = Some(adapter);
        setup.state.db.lock().unwrap().execute_batch("CREATE TRIGGER task20_fail BEFORE UPDATE ON remote_preferences BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
        let revision = setup.lifetime_preferences().await.unwrap().revision;
        let before_config = tls::read_private(&setup.path).unwrap();
        assert_eq!(
            setup
                .set_lifetime_preference(Some(true), None, None, None)
                .await
                .unwrap_err()
                .code,
            ErrorCode::OutcomeUnknown
        );
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(setup
            .autostart
            .as_ref()
            .unwrap()
            .snapshot()
            .unwrap()
            .is_none());
        assert_eq!(tls::read_private(&setup.path).unwrap(), before_config);
        assert_eq!(
            setup.lifetime_preferences().await.unwrap().revision,
            revision
        );
        assert!(super::super::autostart::pending(&setup.state, runtime.identity().id()).unwrap());
        assert!(setup.inner.lock().await.suspended);
        assert_eq!(
            crate::control::settings::read(&runtime.dispatcher)
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        let reopened = RemoteSetup::new(
            None,
            AppState::open(dir.path().join("app")).unwrap(),
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        assert!(reopened.inner.lock().await.suspended);
    }
    #[tokio::test]
    async fn task20_stale_and_unowned_startup_changes_have_no_effect() {
        let (dir, mut setup) = fixture();
        let exe = dir.path().join("installed");
        std::fs::write(&exe, b"fixture").unwrap();
        setup.autostart = Some(super::super::autostart::Autostart::new(dir.path(), &exe).unwrap());
        assert_eq!(
            setup
                .set_lifetime_preference(Some(true), None, Some(999), None)
                .await
                .unwrap_err()
                .code,
            ErrorCode::StaleState
        );
        assert!(!dir.path().join("autostart").exists());
        std::fs::create_dir(dir.path().join("autostart")).unwrap();
        let path = dir
            .path()
            .join("autostart/io.emma.mediacenter-remote.desktop");
        std::fs::write(&path, b"user owned").unwrap();
        assert!(setup
            .set_lifetime_preference(Some(true), None, None, None)
            .await
            .is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"user owned");
        assert!(!setup.lifetime_preferences().await.unwrap().start_at_login);
    }
    #[tokio::test]
    async fn task20_failed_owned_rollback_suspends_availability_and_reports_unknown() {
        let (dir, mut setup) = fixture();
        let exe = dir.path().join("installed");
        std::fs::write(&exe, b"fixture").unwrap();
        let mut adapter = super::super::autostart::Autostart::new(dir.path(), &exe).unwrap();
        adapter.fail_restore = true;
        setup.autostart = Some(adapter);
        setup.state.db.lock().unwrap().execute_batch("CREATE TRIGGER task20_fail BEFORE UPDATE ON remote_preferences BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
        assert_eq!(
            setup
                .set_lifetime_preference(Some(true), None, None, None)
                .await
                .unwrap_err()
                .code,
            ErrorCode::OutcomeUnknown
        );
        assert!(setup.inner.lock().await.suspended);
        assert!(setup.server.addresses().await.is_empty());
        assert!(!setup.lifetime_preferences().await.unwrap().start_at_login);
        let reopened = RemoteSetup::new(
            None,
            AppState::open(dir.path().join("app")).unwrap(),
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        assert!(
            reopened.inner.lock().await.suspended,
            "uncertain owned file effects must survive restart"
        );
        let runtime = reopened.inner.lock().await.runtime.clone().unwrap();
        assert_eq!(
            crate::control::settings::read(&runtime.dispatcher)
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );

        // A subsequent explicit disabled value repairs the file even when its DB preference
        // was already false; the failed operation is never silently treated as success.
        setup
            .state
            .db
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER task20_fail")
            .unwrap();
        setup.autostart.as_mut().unwrap().fail_restore = false;
        setup
            .set_lifetime_preference(Some(false), None, None, None)
            .await
            .unwrap();
        assert!(setup
            .autostart
            .as_ref()
            .unwrap()
            .snapshot()
            .unwrap()
            .is_none());
    }
    #[tokio::test]
    async fn task20_keep_available_hides_only_ui_and_explicit_stop_closes_listener() {
        let (_dir, setup) = fixture();
        let runtime = {
            let mut inner = setup.inner.lock().await;
            setup.save_enabled(&mut inner, true).unwrap();
            let config = NetworkConfig {
                version: 1,
                revision: inner.revision,
                interface_name: "fixture-lan".into(),
                port: 47832,
            };
            config.save(&setup.path).unwrap();
            inner.config = Some(config);
            inner.runtime.clone().unwrap()
        };
        let address = setup
            .server
            .start(runtime, BindAddresses::loopback_fixture(0))
            .await
            .unwrap()[0];
        crate::photos::presenter::seed_test_session(&setup.state, "owned-photo").await;
        let before = crate::control::activity::identity(&setup.state).await;
        let mut hidden = false;
        setup
            .keep_available(|| {
                hidden = true;
                Ok(())
            })
            .await
            .unwrap();
        assert!(hidden);
        assert_eq!(
            crate::control::activity::identity(&setup.state).await,
            before
        );
        assert_eq!(setup.server.addresses().await, vec![address]);
        assert!(tokio::net::TcpStream::connect(address).await.is_ok());
        assert!(setup.lifetime_preferences().await.unwrap().keep_available);
        setup.stop().await;
        assert!(setup.server.addresses().await.is_empty());
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
    #[tokio::test]
    async fn task20_fix1_write_ahead_crash_marker_survives_failed_recovery_until_repair() {
        let (dir, mut setup) = fixture();
        let executable = dir.path().join("installed");
        std::fs::write(&executable, b"fixture").unwrap();
        setup.autostart =
            Some(super::super::autostart::Autostart::new(dir.path(), &executable).unwrap());
        let identity = setup
            .inner
            .lock()
            .await
            .runtime
            .as_ref()
            .unwrap()
            .identity()
            .id()
            .clone();
        super::super::autostart::begin_change(&setup.state, &identity).unwrap();
        setup.autostart.as_ref().unwrap().set_enabled(true).unwrap(); // crash before DB commit
        let mut reopened = RemoteSetup::new(
            None,
            AppState::open(dir.path().join("app")).unwrap(),
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        reopened.autostart = setup.autostart.clone();
        assert!(reopened.inner.lock().await.suspended);
        let runtime = reopened.inner.lock().await.runtime.clone().unwrap();
        assert_eq!(
            crate::control::settings::read(&runtime.dispatcher)
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        let disabled = reopened
            .set_enabled(RemoteEnableRequest {
                revision: 1,
                enabled: false,
                interface_name: None,
                port: 47832,
            })
            .await
            .unwrap();
        assert!(disabled.status.contains("restore the saved startup choice"));
        assert!(super::super::autostart::pending(&reopened.state, &identity).unwrap());
        let old_start = reopened.autostart.as_ref().unwrap().snapshot().unwrap();
        let old_config = tls::read_private(&reopened.path).unwrap();
        let old_preferences = reopened.lifetime_preferences().await.unwrap();
        reopened.state.db.lock().unwrap().execute_batch("CREATE TRIGGER task20_recovery_fail BEFORE UPDATE ON remote_preferences BEGIN SELECT RAISE(FAIL,'fixture recovery'); END;").unwrap();
        assert!(reopened
            .set_enabled(RemoteEnableRequest {
                revision: disabled.revision,
                enabled: true,
                interface_name: Some("fixture".into()),
                port: 47831,
            })
            .await
            .is_err());
        assert_eq!(
            reopened.autostart.as_ref().unwrap().snapshot().unwrap(),
            old_start
        );
        assert_eq!(tls::read_private(&reopened.path).unwrap(), old_config);
        assert_eq!(
            reopened.lifetime_preferences().await.unwrap().revision,
            old_preferences.revision
        );
        assert!(
            !reopened
                .lifetime_preferences()
                .await
                .unwrap()
                .start_at_login
        );
        assert!(
            super::super::autostart::pending(&reopened.state, &identity).unwrap(),
            "failed repair must preserve the prior uncertainty marker"
        );
        assert!(reopened.inner.lock().await.suspended);
        assert_eq!(
            crate::control::settings::read(&runtime.dispatcher)
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        let still_uncertain = RemoteSetup::new(
            None,
            AppState::open(dir.path().join("app")).unwrap(),
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        assert!(still_uncertain.inner.lock().await.suspended);
        assert!(still_uncertain.server.addresses().await.is_empty());
        reopened
            .state
            .db
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER task20_recovery_fail")
            .unwrap();
        // The deliberately colliding port refuses network selection AFTER safe local recovery;
        // this fixture never probes an actual interface or starts a production listener.
        let error = reopened
            .set_enabled(RemoteEnableRequest {
                revision: disabled.revision,
                enabled: true,
                interface_name: Some("fixture".into()),
                port: 47831,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(!super::super::autostart::pending(&reopened.state, &identity).unwrap());
        assert!(reopened
            .autostart
            .as_ref()
            .unwrap()
            .snapshot()
            .unwrap()
            .is_none());
        assert!(
            !reopened
                .lifetime_preferences()
                .await
                .unwrap()
                .start_at_login
        );
        let clean = RemoteSetup::new(
            None,
            AppState::open(dir.path().join("app")).unwrap(),
            Arc::new(RemoteServer::default()),
            IdentityNamespace::Development,
        );
        assert!(!clean.inner.lock().await.suspended);
        assert!(clean.server.addresses().await.is_empty());
    }
    #[tokio::test]
    async fn task20_unwritable_or_unowned_marker_refuses_before_startup_effects() {
        let (dir, mut setup) = fixture();
        let executable = dir.path().join("installed");
        std::fs::write(&executable, b"fixture").unwrap();
        setup.autostart =
            Some(super::super::autostart::Autostart::new(dir.path(), &executable).unwrap());
        let identity = setup
            .inner
            .lock()
            .await
            .runtime
            .as_ref()
            .unwrap()
            .identity()
            .id()
            .clone();
        let path = super::super::autostart::pending_path(&setup.state, &identity);
        std::fs::write(&path, b"unowned marker").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(setup
            .set_lifetime_preference(Some(true), None, None, None)
            .await
            .is_err());
        assert!(setup
            .autostart
            .as_ref()
            .unwrap()
            .snapshot()
            .unwrap()
            .is_none());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(std::fs::read(path).unwrap(), b"unowned marker");
        assert!(!setup.lifetime_preferences().await.unwrap().start_at_login);
    }
    #[tokio::test]
    async fn fresh_setup_is_disabled_and_cannot_issue_pairing_material() {
        let (_dir, setup) = fixture();
        let value = setup.settings().await;
        assert!(!value.enabled);
        assert!(!value.listening);
        assert!(!value.advertising);
        assert!(value.pairing_attempt_id.is_none());
        assert_eq!(
            setup.begin_pairing().await.err().unwrap().code,
            ErrorCode::Unavailable
        );
    }
    #[tokio::test]
    async fn stale_persisted_revision_is_rejected_before_mutating_lifecycle() {
        let (_dir, setup) = fixture();
        let id = setup
            .inner
            .lock()
            .await
            .runtime
            .as_ref()
            .unwrap()
            .identity()
            .id()
            .clone();
        {
            let mut db = setup.state.db.lock().unwrap();
            let mut store = RemoteStore::new(&mut db, &id);
            let prefs = store.preferences().unwrap();
            store.save_preferences(&prefs).unwrap();
        }
        let error = setup
            .set_enabled(RemoteEnableRequest {
                revision: 1,
                enabled: false,
                interface_name: None,
                port: 47832,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::StaleState);
        assert!(
            !setup.inner.lock().await.suspended,
            "a stale request must have no lifecycle side effects"
        );
    }
    #[tokio::test]
    async fn namespace_port_collision_is_rejected_before_network_selection() {
        let (_dir, setup) = fixture();
        let error = setup
            .set_enabled(RemoteEnableRequest {
                revision: 1,
                enabled: true,
                interface_name: Some("missing".into()),
                port: 47831,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(!setup.settings().await.enabled);
        let config = NetworkConfig {
            version: 1,
            revision: 2,
            interface_name: "wlan0".into(),
            port: 47845,
        };
        std::fs::create_dir_all(setup.opposite.parent().unwrap()).unwrap();
        config.save(&setup.opposite).unwrap();
        let error = setup
            .set_enabled(RemoteEnableRequest {
                revision: 1,
                enabled: true,
                interface_name: Some("missing".into()),
                port: 47845,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }
    #[tokio::test]
    async fn persisted_enablement_with_missing_corrupt_or_mismatched_config_never_binds() {
        for config in [
            None,
            Some("invalid"),
            Some(r#"{"version":1,"revision":1,"interfaceName":"missing","port":47832}"#),
        ] {
            let (_dir, setup) = fixture();
            {
                let mut inner = setup.inner.lock().await;
                setup.save_enabled(&mut inner, true).unwrap();
            }
            if let Some(config) = config {
                std::fs::write(&setup.path, config).unwrap();
            }
            let restarted = RemoteSetup::new(
                None,
                AppState::open(setup.state.data_dir.clone()).unwrap(),
                Arc::new(RemoteServer::default()),
                IdentityNamespace::Development,
            );
            restarted.reconcile().await;
            let value = restarted.settings().await;
            assert!(value.enabled);
            assert!(!value.listening);
            assert!(!value.advertising);
            assert!(value.status.contains("missing or invalid"));
        }
    }
    #[tokio::test]
    async fn failed_preference_write_suspends_reconciliation_until_explicit_recovery() {
        let (_dir, setup) = fixture();
        setup.state.db.lock().unwrap().execute_batch("CREATE TRIGGER block_remote_preferences BEFORE UPDATE ON remote_preferences BEGIN SELECT RAISE(FAIL, 'test write failure'); END;").unwrap();
        let error = setup
            .set_enabled(RemoteEnableRequest {
                revision: 1,
                enabled: false,
                interface_name: None,
                port: 47832,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(setup.inner.lock().await.suspended);
        setup.reconcile().await;
        assert!(!setup.settings().await.listening);
    }

    #[tokio::test]
    async fn disappeared_selected_interface_closes_listener_and_pairing_but_retains_runtime_identity_and_devices(
    ) {
        use super::super::{
            protocol::{BearerCredential, PairedDevice},
            store::CredentialHash,
        };
        let (_dir, setup) = fixture();
        let runtime = {
            let mut inner = setup.inner.lock().await;
            setup.save_enabled(&mut inner, true).unwrap();
            inner.config = Some(NetworkConfig {
                version: 1,
                revision: inner.revision,
                interface_name: "fixture-lan".into(),
                port: 47832,
            });
            inner.runtime.clone().unwrap()
        };
        let id = runtime.identity().id().clone();
        let fingerprint = runtime.identity().fingerprint().clone();
        let credential: BearerCredential = checked("x".repeat(43)).unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse"],"createdAtMs":1,"revoked":false})).unwrap();
        {
            let mut db = setup.state.db.lock().unwrap();
            RemoteStore::new(&mut db, &id)
                .add_device(&device, &CredentialHash::from_credential(&credential))
                .unwrap();
        }
        let old = setup
            .server
            .start(runtime.clone(), BindAddresses::loopback_fixture(0))
            .await
            .unwrap()[0];
        runtime
            .begin_pairing(
                checked(serde_json::json!(["192.168.1.4"])).unwrap(),
                checked(47832).unwrap(),
            )
            .unwrap();
        {
            let mut inner = setup.inner.lock().await;
            setup
                .reconcile_observation_locked(&mut inner, Ok((Vec::new(), 2)))
                .await;
        }
        assert!(setup.server.addresses().await.is_empty());
        assert!(runtime.pairing_status().unwrap().is_none());
        assert!(tokio::net::TcpStream::connect(old).await.is_err());
        let rebound = {
            let mut inner = setup.inner.lock().await;
            setup
                .replace_listener_locked(&mut inner, BindAddresses::loopback_fixture(0), 3)
                .await
                .unwrap()[0]
        };
        assert!(tokio::net::TcpStream::connect(rebound).await.is_ok());
        assert_eq!(runtime.identity().id(), &id);
        assert_eq!(runtime.identity().fingerprint(), fingerprint);
        let json = serde_json::to_value(setup.devices().await.unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{"deviceId":"phone","label":"Phone","createdAtMs":1,"lastSeenAtMs":null}])
        );
        {
            let mut db = setup.state.db.lock().unwrap();
            assert!(RemoteStore::new(&mut db, &id)
                .lookup_credential(&CredentialHash::from_credential(&credential))
                .unwrap()
                .is_some());
        }
        let revision = setup.settings().await.revision;
        setup
            .set_enabled(RemoteEnableRequest {
                revision,
                enabled: false,
                interface_name: None,
                port: 47832,
            })
            .await
            .unwrap();
        assert!(setup.server.addresses().await.is_empty());
        assert!(tokio::net::TcpStream::connect(rebound).await.is_err());
    }
    #[tokio::test]
    async fn occupied_port_is_actionable_without_partial_listener_or_alternative_port() {
        let (_dir, setup) = fixture();
        let runtime = setup.inner.lock().await.runtime.clone().unwrap();
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let error = setup
            .server
            .start(
                runtime,
                BindAddresses::loopback_fixture(occupied.local_addr().unwrap().port()),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(error.message.as_str().contains("alternative port"));
        assert!(setup.server.addresses().await.is_empty());
    }

    #[tokio::test]
    async fn desktop_revocation_cancels_current_admissions_and_removes_credentials_and_metadata() {
        use super::super::{
            protocol::{BearerCredential, PairedDevice},
            store::CredentialHash,
        };
        let (_dir, setup) = fixture();
        let id = setup
            .inner
            .lock()
            .await
            .runtime
            .as_ref()
            .unwrap()
            .identity()
            .id()
            .clone();
        let credential: BearerCredential = checked("y".repeat(43)).unwrap();
        let device:PairedDevice=checked(serde_json::json!({"deviceId":"phone","label":"Phone","scopes":["browse"],"createdAtMs":1,"revoked":false})).unwrap();
        {
            let mut db = setup.state.db.lock().unwrap();
            RemoteStore::new(&mut db, &id)
                .add_device(&device, &CredentialHash::from_credential(&credential))
                .unwrap();
        }
        let admission = setup
            .state
            .event_hub
            .subscribe_owner(Some(device.device_id.clone()));
        assert!(!admission.revocation().is_cancelled());
        assert_eq!(setup.devices().await.unwrap().len(), 1);
        setup.revoke(&device.device_id).await.unwrap();
        assert!(admission.revocation().is_cancelled());
        assert!(setup.devices().await.unwrap().is_empty());
        {
            let mut db = setup.state.db.lock().unwrap();
            assert!(RemoteStore::new(&mut db, &id)
                .lookup_credential(&CredentialHash::from_credential(&credential))
                .unwrap()
                .is_none());
        }
    }
}

#[tauri::command]
pub async fn get_renderer_confirmations(
    window: tauri::WebviewWindow,
    setup: tauri::State<'_, RemoteSetup>,
) -> Result<Vec<super::protocol::ConfirmationPreview>, RemoteError> {
    super::renderer::main_window(window.label())?;
    let runtime = setup
        .inner
        .lock()
        .await
        .runtime
        .clone()
        .ok_or_else(unavailable)?;
    let registry = runtime
        .dispatcher
        .services
        .confirmations
        .lock()
        .map_err(|_| unavailable())?;
    Ok(registry.previews(crate::app_state::now_ms()))
}
#[tauri::command]
pub async fn resolve_renderer_confirmation(
    window: tauri::WebviewWindow,
    setup: tauri::State<'_, RemoteSetup>,
    confirmation_id: super::protocol::Id,
    confirm: bool,
) -> Result<(), RemoteError> {
    super::renderer::main_window(window.label())?;
    let runtime = setup
        .inner
        .lock()
        .await
        .runtime
        .clone()
        .ok_or_else(unavailable)?;
    let owner = {
        let registry = runtime
            .dispatcher
            .services
            .confirmations
            .lock()
            .map_err(|_| unavailable())?;
        registry
            .pending
            .get(confirmation_id.as_str())
            .ok_or_else(|| {
                RemoteError::new(ErrorCode::NotFound, "The confirmation was not found.")
            })?
            .preview
            .owner_device_id
            .clone()
    };
    // Local approval can only resolve the saved action, and cannot select scopes or an actor.
    let principal = {
        let mut db = setup.state.db.lock().map_err(|_| unavailable())?;
        let device = RemoteStore::new(&mut db, runtime.identity().id())
            .active_devices()
            .map_err(|_| unavailable())?
            .into_iter()
            .find(|d| d.device_id == owner)
            .ok_or_else(|| {
                RemoteError::new(
                    ErrorCode::Forbidden,
                    "The originating phone is no longer authorized.",
                )
            })?;
        super::protocol::RemotePrincipal {
            device_id: device.device_id,
            scopes: device.scopes,
        }
    };
    runtime
        .dispatcher
        .semantic_action(
            &principal,
            super::renderer::SemanticAction::Confirmation {
                confirmation_id,
                confirm,
            },
        )
        .await
}
