//! DNS-SD is a location hint only; never publish pairing material or credentials.
use super::protocol::{ErrorCode, RemoteError};
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
pub const SERVICE_TYPE: &str = "_emma-mc._tcp.local.";
fn service_info(
    id: &str,
    name: &str,
    interface: &str,
    addresses: &[SocketAddr],
) -> Result<ServiceInfo, RemoteError> {
    let port = addresses.first().ok_or_else(unavailable)?.port();
    let ips = addresses
        .iter()
        .map(SocketAddr::ip)
        .collect::<Vec<IpAddr>>();
    let mut info = ServiceInfo::new(
        SERVICE_TYPE,
        id,
        &format!("mc-{id}.local."),
        ips.as_slice(),
        port,
        [("v", "1"), ("id", id), ("name", name)].as_slice(),
    )
    .map_err(|_| unavailable())?;
    info.set_interfaces(vec![IfKind::Name(interface.to_owned())]);
    Ok(info)
}
#[derive(Default)]
struct AdvertisementStatus {
    announced: bool,
}
impl AdvertisementStatus {
    fn observe(&mut self, event: mdns_sd::DaemonEvent) -> Result<(), RemoteError> {
        match event {
            mdns_sd::DaemonEvent::Announce(_, _) => self.announced = true,
            mdns_sd::DaemonEvent::Error(_) => {
                self.announced = false;
                return Err(unavailable());
            }
            _ => {}
        }
        Ok(())
    }
}
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
    monitor: mdns_sd::Receiver<mdns_sd::DaemonEvent>,
    status: AdvertisementStatus,
    started: std::time::Instant,
    stopped: bool,
}
impl Advertisement {
    pub fn start(
        id: &str,
        name: &str,
        interface: &str,
        addresses: &[SocketAddr],
    ) -> Result<Self, RemoteError> {
        let info = service_info(id, name, interface, addresses)?;
        let daemon = ServiceDaemon::new().map_err(|_| unavailable())?;
        let result = (|| {
            let monitor = daemon.monitor().map_err(|_| unavailable())?;
            // FIFO daemon commands apply the deny-all policy before any registration.
            daemon
                .disable_interface(IfKind::All)
                .map_err(|_| unavailable())?;
            daemon
                .enable_interface(IfKind::Name(interface.to_owned()))
                .map_err(|_| unavailable())?;
            daemon.set_ip_check_interval(0).map_err(|_| unavailable())?;
            let fullname = info.get_fullname().to_owned();
            daemon.register(info).map_err(|_| unavailable())?;
            Ok(Self {
                daemon: daemon.clone(),
                fullname,
                monitor,
                status: AdvertisementStatus::default(),
                started: std::time::Instant::now(),
                stopped: false,
            })
        })();
        if result.is_err() {
            let _ = daemon.shutdown();
        }
        result
    }
    /// Queue acceptance alone does not prove that an announcement was sent.
    pub fn status(&mut self) -> Result<bool, RemoteError> {
        for _ in 0..100 {
            match self.monitor.try_recv() {
                Ok(event) => self.status.observe(event)?,
                Err(mdns_sd::TryRecvError::Empty) => break,
                Err(mdns_sd::TryRecvError::Disconnected) => return Err(unavailable()),
            }
        }
        if !self.status.announced && self.started.elapsed() > Duration::from_secs(5) {
            return Err(unavailable());
        }
        Ok(self.status.announced)
    }
    pub async fn stop(mut self) {
        self.stopped = true;
        let daemon = self.daemon.clone();
        let fullname = self.fullname.clone();
        drop(self);
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(done) = daemon.unregister(&fullname) {
                let _ = done.recv_timeout(Duration::from_secs(1));
            }
            if let Ok(done) = daemon.shutdown() {
                let _ = done.recv_timeout(Duration::from_secs(1));
            }
        })
        .await;
    }
}
impl Drop for Advertisement {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.daemon.unregister(&self.fullname);
            let _ = self.daemon.shutdown();
        }
    }
}
fn unavailable() -> RemoteError {
    RemoteError::new(ErrorCode::Unavailable,"Local discovery is unavailable. Scan the pairing code or enter the address manually; check local-network firewall permissions.")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advertisement_contains_only_public_identity_and_explicit_addresses() {
        let info = service_info(
            "server-id",
            "Mediacenter",
            "wlan0",
            &["192.168.1.4:47831".parse().unwrap()],
        )
        .unwrap();
        assert_eq!(info.get_type(), "_emma-mc._tcp.local.");
        assert_eq!(info.get_port(), 47831);
        assert!(!info.is_addr_auto());
        let keys = info
            .get_properties()
            .iter()
            .map(|p| p.key())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["v", "id", "name"]);
        assert_eq!(info.get_property_val_str("id"), Some("server-id"));
        assert_eq!(info.get_addresses().len(), 1);
        assert!(info
            .get_addresses()
            .contains(&"192.168.1.4".parse().unwrap()));
    }
    #[test]
    fn queued_registration_does_not_claim_advertising_and_daemon_errors_clear_it() {
        let mut status = AdvertisementStatus::default();
        assert!(!status.announced);
        status
            .observe(mdns_sd::DaemonEvent::Announce(
                "service".into(),
                "host:wlan0".into(),
            ))
            .unwrap();
        assert!(status.announced);
        assert!(status
            .observe(mdns_sd::DaemonEvent::Error(mdns_sd::Error::Msg(
                "fixture failure".into()
            )))
            .is_err());
        assert!(!status.announced);
    }
}
