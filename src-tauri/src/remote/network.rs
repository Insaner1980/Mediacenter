//! Read-only interface inventory. Only up, physical LAN interfaces are offered.
use super::{
    protocol::{ErrorCode, RemoteError},
    server::BindAddresses,
};
use nix::net::if_::InterfaceFlags;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr, SocketAddrV6},
};
use ts_rs::TS;
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub name: String,
    pub label: String,
    pub addresses: Vec<String>,
}
fn eligible(name: &str, flags: InterfaceFlags, physical: bool, ip: IpAddr) -> bool {
    physical
        && ![
            "tun",
            "tap",
            "wg",
            "veth",
            "docker",
            "br-",
            "virbr",
            "tailscale",
            "nordlynx",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        && flags.contains(InterfaceFlags::IFF_UP | InterfaceFlags::IFF_RUNNING)
        && !flags.intersects(InterfaceFlags::IFF_LOOPBACK | InterfaceFlags::IFF_POINTOPOINT)
        && match ip {
            IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
            IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
        }
}
pub fn inventory() -> Result<Vec<NetworkInterface>, RemoteError> {
    let mut interfaces = BTreeMap::<String, NetworkInterface>::new();
    for interface in nix::ifaddrs::getifaddrs().map_err(|_| unavailable())? {
        let Some(address) = interface.address else {
            continue;
        };
        let address = if let Some(v4) = address.as_sockaddr_in() {
            SocketAddr::new(v4.ip().into(), 0)
        } else if let Some(v6) = address.as_sockaddr_in6() {
            SocketAddr::V6(SocketAddrV6::new(v6.ip(), 0, 0, v6.scope_id()))
        } else {
            continue;
        };
        let name = interface.interface_name;
        if !eligible(
            &name,
            interface.flags,
            std::path::Path::new("/sys/class/net")
                .join(&name)
                .join("device")
                .exists(),
            address.ip(),
        ) {
            continue;
        }
        let value = match address {
            SocketAddr::V6(v6) if v6.ip().is_unicast_link_local() => {
                let index = if v6.scope_id() != 0 {
                    v6.scope_id()
                } else {
                    nix::net::if_::if_nametoindex(name.as_str()).map_err(|_| unavailable())?
                };
                format!("{}%{}", v6.ip(), index)
            }
            _ => address.ip().to_string(),
        };
        let label = if name.starts_with("wl") {
            format!("Wi-Fi ({name})")
        } else {
            format!("Ethernet ({name})")
        };
        let item = interfaces
            .entry(name.clone())
            .or_insert_with(|| NetworkInterface {
                name,
                label,
                addresses: Vec::new(),
            });
        if !item.addresses.contains(&value) && item.addresses.len() < 8 {
            item.addresses.push(value);
        }
    }
    for interface in interfaces.values_mut() {
        interface.addresses.sort();
    }
    Ok(interfaces.into_values().collect())
}
pub fn selected_addresses(
    interfaces: &[NetworkInterface],
    name: &str,
    port: u16,
) -> Result<Vec<SocketAddr>, RemoteError> {
    let interface = interfaces
        .iter()
        .find(|i| i.name == name)
        .ok_or_else(unavailable)?;
    let addresses = interface
        .addresses
        .iter()
        .map(|address| {
            if address.contains(':') {
                format!("[{address}]:{port}")
            } else {
                format!("{address}:{port}")
            }
            .parse()
            .map_err(|_| unavailable())
        })
        .collect::<Result<Vec<SocketAddr>, RemoteError>>()?;
    BindAddresses::selected(
        addresses.clone(),
        &addresses.iter().map(SocketAddr::ip).collect::<Vec<_>>(),
    )?;
    Ok(addresses)
}
/// Includes tunnel changes so a VPN transition closes old admissions before rebind.
pub fn epoch() -> Result<u64, RemoteError> {
    use std::hash::{Hash, Hasher};
    let mut entries = nix::ifaddrs::getifaddrs()
        .map_err(|_| unavailable())?
        .map(|i| format!("{}:{:?}:{:?}", i.interface_name, i.flags, i.address))
        .collect::<Vec<_>>();
    entries.sort();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hash);
    Ok(hash.finish())
}
fn unavailable() -> RemoteError {
    RemoteError::new(ErrorCode::Unavailable,"The selected local network is unavailable. Reconnect it or disable remote control and choose another network.")
}
#[cfg(test)]
mod tests {
    use super::*;
    use nix::net::if_::InterfaceFlags as F;
    #[test]
    fn defaults_exclude_loopback_tunnels_containers_and_public_addresses() {
        let up = F::IFF_UP | F::IFF_RUNNING;
        for (name, flags, physical, ip, want) in [
            ("wlan0", up, true, "192.168.1.4", true),
            ("enp2s0", up, true, "fd00::2", true),
            ("lo", up | F::IFF_LOOPBACK, false, "127.0.0.1", false),
            ("tun0", up | F::IFF_POINTOPOINT, false, "10.1.0.2", false),
            ("docker0", up, false, "172.17.0.1", false),
            ("veth0", up, false, "192.168.1.3", false),
            ("eth0", up, true, "8.8.8.8", false),
            ("wlan0", F::empty(), true, "192.168.1.4", false),
        ] {
            assert_eq!(
                eligible(name, flags, physical, ip.parse().unwrap()),
                want,
                "{name} {ip}"
            );
        }
    }
    #[test]
    fn selected_interface_never_falls_back_and_scopes_ipv6_link_local() {
        let interfaces = vec![NetworkInterface {
            name: "wlan0".into(),
            label: "Wi-Fi".into(),
            addresses: vec!["192.168.1.4".into(), "fe80::2%7".into()],
        }];
        let selected = selected_addresses(&interfaces, "wlan0", 47832).unwrap();
        assert_eq!(
            selected,
            vec![
                "192.168.1.4:47832".parse().unwrap(),
                "[fe80::2%7]:47832".parse().unwrap()
            ]
        );
        assert!(selected_addresses(&interfaces, "missing", 47832).is_err());
        assert!(selected_addresses(&interfaces, "wlan0", 0).is_err());
        assert!(selected_addresses(&[], "wlan0", 47832).is_err());
    }
}
