//! Network interface discovery for the Pro DJ Link protocol.
//!
//! Finds the local network interface on the same subnet as a given device IP,
//! providing its name, IP address, MAC address, netmask, and broadcast address.

use std::net::Ipv4Addr;

/// A discovered local network interface with all the addressing details needed
/// to participate in the DJ Link protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterface {
    /// OS-level interface name (e.g. "en0", "eth0").
    pub name: String,
    /// IPv4 address assigned to this interface.
    pub ip: Ipv4Addr,
    /// Hardware (MAC) address of the interface.
    pub mac: [u8; 6],
    /// Subnet mask.
    pub netmask: Ipv4Addr,
    /// Broadcast address for the subnet.
    pub broadcast: Ipv4Addr,
}

/// Compute the broadcast address from an IP and netmask.
fn compute_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    let ip_bits = u32::from(ip);
    let mask_bits = u32::from(netmask);
    Ipv4Addr::from(ip_bits | !mask_bits)
}

/// Enumerate all non-loopback IPv4 network interfaces on the host.
pub fn list_interfaces() -> Vec<NetworkInterface> {
    let addrs = match if_addrs::get_if_addrs() {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("Failed to enumerate network interfaces: {e}");
            return Vec::new();
        }
    };

    addrs
        .into_iter()
        .filter_map(|iface| {
            let if_addrs::IfAddr::V4(v4) = &iface.addr else {
                return None;
            };

            if v4.ip.is_loopback() {
                return None;
            }

            let mac = mac_address::mac_address_by_name(&iface.name)
                .ok()
                .flatten()
                .map(|m| m.bytes())
                .unwrap_or([0; 6]);

            let broadcast = v4
                .broadcast
                .unwrap_or_else(|| compute_broadcast(v4.ip, v4.netmask));

            Some(NetworkInterface {
                name: iface.name,
                ip: v4.ip,
                mac,
                netmask: v4.netmask,
                broadcast,
            })
        })
        .collect()
}

/// Find the local network interface on the same subnet as `device_ip`.
///
/// Iterates through all IPv4 interfaces and returns the first whose network
/// address (IP & netmask) matches the device's network address.
pub fn find_matching_interface(device_ip: Ipv4Addr) -> Option<NetworkInterface> {
    list_interfaces().into_iter().find(|iface| {
        let mask = u32::from(iface.netmask);
        let iface_net = u32::from(iface.ip) & mask;
        let device_net = u32::from(device_ip) & mask;
        iface_net == device_net
    })
}

/// Find the local network interface with the given IP address.
///
/// Useful when you already know the local interface address (e.g. from
/// [`VirtualCdjConfig`](super::virtual_cdj::VirtualCdjConfig)) and need the
/// MAC address and other details.
pub fn find_interface_by_ip(ip: Ipv4Addr) -> Option<NetworkInterface> {
    list_interfaces().into_iter().find(|iface| iface.ip == ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_broadcast_class_c() {
        let ip = Ipv4Addr::new(192, 168, 1, 100);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        assert_eq!(compute_broadcast(ip, mask), Ipv4Addr::new(192, 168, 1, 255));
    }

    #[test]
    fn compute_broadcast_class_b() {
        let ip = Ipv4Addr::new(172, 16, 5, 10);
        let mask = Ipv4Addr::new(255, 255, 0, 0);
        assert_eq!(
            compute_broadcast(ip, mask),
            Ipv4Addr::new(172, 16, 255, 255)
        );
    }

    #[test]
    fn compute_broadcast_slash_25() {
        let ip = Ipv4Addr::new(10, 0, 0, 50);
        let mask = Ipv4Addr::new(255, 255, 255, 128);
        assert_eq!(compute_broadcast(ip, mask), Ipv4Addr::new(10, 0, 0, 127));
    }

    #[test]
    fn compute_broadcast_host_mask() {
        let ip = Ipv4Addr::new(10, 0, 0, 1);
        let mask = Ipv4Addr::new(255, 255, 255, 255);
        assert_eq!(compute_broadcast(ip, mask), Ipv4Addr::new(10, 0, 0, 1));
    }

    #[test]
    fn network_interface_struct_is_debug_clone_eq() {
        let iface = NetworkInterface {
            name: "en0".to_string(),
            ip: Ipv4Addr::new(192, 168, 1, 100),
            mac: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            netmask: Ipv4Addr::new(255, 255, 255, 0),
            broadcast: Ipv4Addr::new(192, 168, 1, 255),
        };
        let cloned = iface.clone();
        assert_eq!(iface, cloned);
        let _ = format!("{:?}", iface);
    }

    #[test]
    fn list_interfaces_returns_no_loopback() {
        let ifaces = list_interfaces();
        for iface in &ifaces {
            assert!(!iface.ip.is_loopback(), "loopback should be filtered out");
        }
    }

    #[test]
    fn list_interfaces_does_not_panic() {
        // Smoke test: should not panic even in CI environments with few interfaces.
        let _ = list_interfaces();
    }

    #[test]
    fn find_matching_interface_no_match_for_loopback() {
        // Loopback addresses are filtered out, so no match expected.
        assert!(find_matching_interface(Ipv4Addr::LOCALHOST).is_none());
    }

    #[test]
    fn find_interface_by_ip_no_match_for_nonexistent() {
        assert!(find_interface_by_ip(Ipv4Addr::new(203, 0, 113, 1)).is_none());
    }

    /// If the host has a real network interface, verify that
    /// `find_matching_interface` returns it for an IP on the same subnet.
    #[test]
    fn find_matching_interface_returns_same_subnet() {
        let ifaces = list_interfaces();
        if let Some(iface) = ifaces.first() {
            // Construct an IP on the same subnet as this interface
            let mask = u32::from(iface.netmask);
            let net = u32::from(iface.ip) & mask;
            // Pick a host part that is different but within the subnet
            let host = u32::from(iface.ip) & !mask;
            let other_host = if host == 1 { 2 } else { 1 };
            let device_ip = Ipv4Addr::from(net | other_host);

            let found = find_matching_interface(device_ip);
            assert!(
                found.is_some(),
                "Expected to find interface for device IP {device_ip}"
            );
            let found = found.unwrap();
            assert_eq!(found.name, iface.name);
            assert_eq!(found.ip, iface.ip);
        }
    }

    /// Verify `find_interface_by_ip` works for a real local address.
    #[test]
    fn find_interface_by_ip_for_real_address() {
        let ifaces = list_interfaces();
        if let Some(iface) = ifaces.first() {
            let found = find_interface_by_ip(iface.ip);
            assert!(found.is_some());
            assert_eq!(found.unwrap().ip, iface.ip);
        }
    }
}
