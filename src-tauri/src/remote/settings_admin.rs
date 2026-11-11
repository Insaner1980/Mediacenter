//! Native-only network observations and narrowly shared pairing/revocation authority.
use super::{
    adapters::checked,
    auth::AuthManager,
    dispatch::{native_error, unavailable},
    pairing::PairingManager,
    protocol::*,
    store::RemoteStore,
};
use crate::app_state::AppState;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub struct NetworkObservation {
    pub generation: u64,
    pub addresses: Vec<SocketAddr>,
    pub diagnostics: ConnectionDiagnostics,
    leases: HashMap<String, (Instant, CancellationToken, CancellationToken, String)>,
}
impl Default for NetworkObservation {
    fn default() -> Self {
        Self {
            generation: 0,
            addresses: Vec::new(),
            diagnostics: ConnectionDiagnostics {
                selected_interface: None,
                listener: ListenerObservation::Unknown,
                discovery: DiscoveryObservation::Unknown,
                vpn_version: None,
            },
            leases: HashMap::new(),
        }
    }
}
impl NetworkObservation {
    pub fn note(
        &mut self,
        owner: &Id,
        socket: CancellationToken,
        revoked: CancellationToken,
        origin: &str,
    ) {
        self.leases.retain(|_, (at, socket, revoked, _)| {
            at.elapsed() <= Duration::from_secs(10)
                && !socket.is_cancelled()
                && !revoked.is_cancelled()
        });
        if self.leases.len() < 8 || self.leases.contains_key(owner.as_str()) {
            self.leases.insert(
                owner.as_str().into(),
                (Instant::now(), socket, revoked, origin.into()),
            );
        }
    }
    pub fn reachable(&self, owner: &Id) -> bool {
        !self.addresses.is_empty()
            && self
                .leases
                .get(owner.as_str())
                .is_some_and(|(at, s, r, _)| {
                    at.elapsed() <= Duration::from_secs(10)
                        && !s.is_cancelled()
                        && !r.is_cancelled()
                })
    }
    pub fn connection(&self, owner: &Id) -> Option<&str> {
        self.reachable(owner)
            .then(|| self.leases.get(owner.as_str()).unwrap().3.as_str())
    }
    pub fn replace(&mut self, addresses: Vec<SocketAddr>) {
        self.generation += 1;
        self.addresses = addresses;
        self.leases.clear();
        self.diagnostics.listener = if self.addresses.is_empty() {
            ListenerObservation::Stopped
        } else {
            ListenerObservation::Listening
        };
        self.diagnostics.discovery = if self.addresses.is_empty() {
            DiscoveryObservation::Stopped
        } else {
            DiscoveryObservation::Pending
        };
    }
}

pub struct RemoteAdmin {
    pub(crate) auth: Arc<AuthManager>,
    pub(crate) pairing: Arc<Mutex<PairingManager>>,
    pub(crate) gate: Mutex<()>,
    pub(crate) issued: Mutex<Option<(Id, Id)>>,
    pub(crate) network: Arc<Mutex<NetworkObservation>>,
    pub(crate) state: AppState,
    pub(crate) identity: Id,
}
impl RemoteAdmin {
    pub fn new(
        auth: Arc<AuthManager>,
        pairing: Arc<Mutex<PairingManager>>,
        services: &crate::control::ControlServices,
    ) -> Self {
        Self {
            auth,
            pairing,
            gate: Mutex::new(()),
            issued: Mutex::new(None),
            network: services.network.clone(),
            state: services.state.clone(),
            identity: services.server_identity.clone(),
        }
    }
    /// Call with the pairing mutex held, before taking the DB mutex everywhere.
    pub fn authorize(&self, owner: &RemotePrincipal) -> Result<Vec<PairedDevice>, RemoteError> {
        let mut db = self.state.db.lock().map_err(|_| unavailable())?;
        let devices = RemoteStore::new(&mut db, &self.identity)
            .active_devices()
            .map_err(native_error)?;
        if !devices.iter().any(|d| {
            d.device_id == owner.device_id && d.scopes.as_slice().contains(&Scope::Settings)
        }) {
            return Err(RemoteError::new(
                ErrorCode::Forbidden,
                "This phone is no longer authorized.",
            ));
        }
        Ok(devices)
    }
    pub fn replace_network(&self, addresses: Vec<SocketAddr>) -> Result<(), RemoteError> {
        let _gate = self.gate.lock().map_err(|_| unavailable())?;
        let mut pairing = self.pairing.lock().map_err(|_| unavailable())?;
        pairing.close();
        *self.issued.lock().map_err(|_| unavailable())? = None;
        self.network
            .lock()
            .map_err(|_| unavailable())?
            .replace(addresses);
        Ok(())
    }
    pub fn revoke(&self, device: &Id) -> Result<(), RemoteError> {
        let _gate = self.gate.lock().map_err(|_| unavailable())?;
        let mut pairing = self.pairing.lock().map_err(|_| unavailable())?;
        let mut issued = self.issued.lock().map_err(|_| unavailable())?;
        if issued.as_ref().is_some_and(|(owner, _)| owner == device) {
            pairing.close();
            *issued = None;
        }
        let mut db = self.state.db.lock().map_err(|_| unavailable())?;
        self.auth
            .revoke_device(&mut db, device, crate::app_state::now_ms() as i64)?;
        Ok(())
    }
    pub fn diagnostics(&self) -> Result<ConnectionDiagnostics, RemoteError> {
        Ok(self
            .network
            .lock()
            .map_err(|_| unavailable())?
            .diagnostics
            .clone())
    }
    pub fn generation(&self) -> Result<u64, RemoteError> {
        let net = self.network.lock().map_err(|_| unavailable())?;
        if net.addresses.is_empty() {
            return Err(unavailable());
        }
        Ok(net.generation)
    }
    pub fn addresses(&self) -> Result<(LocalAddresses, Port), RemoteError> {
        let net = self.network.lock().map_err(|_| unavailable())?;
        let port = net.addresses.first().ok_or_else(unavailable)?.port();
        Ok((
            checked(
                net.addresses
                    .iter()
                    .map(|a| a.ip().to_string())
                    .collect::<Vec<_>>(),
            )?,
            checked(port)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_lease_is_bounded_to_live_connection_owner_and_listener_generation() {
        let owner: Id = checked("phone").unwrap();
        let mut net = NetworkObservation::default();
        assert_eq!(net.diagnostics.listener, ListenerObservation::Unknown);
        net.replace(vec!["192.168.1.2:47832".parse().unwrap()]);
        let live = CancellationToken::new();
        let revoked = CancellationToken::new();
        net.note(&owner, live.clone(), revoked.clone(), "socket-a");
        assert!(net.reachable(&owner));
        assert!(!net.reachable(&checked("other").unwrap()));
        net.leases.get_mut(owner.as_str()).unwrap().0 = Instant::now() - Duration::from_secs(11);
        assert!(!net.reachable(&owner));
        net.note(&owner, live.clone(), revoked.clone(), "socket-a");
        live.cancel();
        assert!(!net.reachable(&owner));
        net.note(
            &owner,
            CancellationToken::new(),
            revoked.clone(),
            "socket-b",
        );
        revoked.cancel();
        assert!(!net.reachable(&owner));
        let old_socket = CancellationToken::new();
        net.note(
            &owner,
            old_socket.clone(),
            CancellationToken::new(),
            "socket-old",
        );
        net.note(
            &owner,
            CancellationToken::new(),
            CancellationToken::new(),
            "socket-c",
        );
        old_socket.cancel();
        assert_eq!(net.connection(&owner), Some("socket-c"));
        let before = net.generation;
        net.replace(vec!["192.168.1.3:47832".parse().unwrap()]);
        assert!(net.generation > before);
        assert!(!net.reachable(&owner));
        let text = serde_json::to_string(&net.diagnostics).unwrap();
        assert!(!text.contains("192.168"));
        assert!(!text.contains("47832"));
    }
}
