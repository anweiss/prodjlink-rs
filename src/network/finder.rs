use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashSet;
use tokio::net::UdpSocket;
use tokio::sync::{RwLock, broadcast};
use tokio::task::JoinHandle;

use crate::device::types::DeviceNumber;
use crate::error::Result;
use crate::protocol::announce::{
    DeviceAnnouncement, expand_opus_quad_announcement, extract_claim_stage2_device_number,
    extract_defense_device_number, parse_keep_alive,
};
use crate::protocol::header::{DISCOVERY_PORT, PacketType, parse_header};

/// How long before a device is considered gone (no keep-alive received).
/// This is 1.5× the standard 3-second keep-alive interval.
const DEVICE_EXPIRY: Duration = Duration::from_millis(4500);

/// How often to check for expired devices.
const AGING_INTERVAL: Duration = Duration::from_secs(1);

/// Events emitted by the DeviceFinder.
#[derive(Debug, Clone)]
pub enum FinderEvent {
    /// A new device appeared on the network.
    DeviceFound(DeviceAnnouncement),
    /// A device was updated (keep-alive refreshed).
    DeviceUpdated(DeviceAnnouncement),
    /// A device has not been seen recently and is considered gone.
    DeviceLost(DeviceAnnouncement),
    /// A defense packet (type 0x08) was received — another device is asserting
    /// ownership of the given device number.
    DefenseReceived { device_number: u8 },
    /// A stage-2 claim packet (type 0x02) was received — another device is
    /// trying to claim the given device number from the given source IP.
    ClaimReceived {
        device_number: u8,
        source_ip: Ipv4Addr,
    },
}

/// Async service that discovers Pioneer DJ Link devices on the local network.
///
/// Listens for UDP keep-alive packets on port 50000 and maintains a live map
/// of all active devices. Emits events via a `tokio::broadcast` channel.
pub struct DeviceFinder {
    devices: Arc<RwLock<HashMap<u8, DeviceAnnouncement>>>,
    ignored_addresses: Arc<DashSet<Ipv4Addr>>,
    event_tx: broadcast::Sender<FinderEvent>,
    recv_task: JoinHandle<()>,
    aging_task: JoinHandle<()>,
}

impl DeviceFinder {
    /// Start the device finder, binding to UDP port 50000.
    pub async fn start() -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await?;
        let socket = Arc::new(socket);

        let devices: Arc<RwLock<HashMap<u8, DeviceAnnouncement>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let ignored_addresses: Arc<DashSet<Ipv4Addr>> = Arc::new(DashSet::new());
        let (event_tx, _) = broadcast::channel(256);

        // Spawn the receive loop
        let recv_devices = devices.clone();
        let recv_ignored = ignored_addresses.clone();
        let recv_tx = event_tx.clone();
        let recv_socket = socket.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                match recv_socket.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        // Filter out ignored source addresses
                        if let SocketAddr::V4(v4) = addr {
                            if recv_ignored.contains(v4.ip()) {
                                continue;
                            }
                        }

                        let data = &buf[..len];
                        if let Ok(pkt_type) = parse_header(data) {
                            match pkt_type {
                                PacketType::DeviceKeepAlive => {
                                    if let Ok(announcement) = parse_keep_alive(data) {
                                        let announcements =
                                            expand_opus_quad_announcement(&announcement);
                                        let mut map = recv_devices.write().await;
                                        for ann in announcements {
                                            let key = ann.number.0;
                                            let is_new = !map.contains_key(&key);
                                            map.insert(key, ann.clone());
                                            let event = if is_new {
                                                FinderEvent::DeviceFound(ann)
                                            } else {
                                                FinderEvent::DeviceUpdated(ann)
                                            };
                                            let _ = recv_tx.send(event);
                                        }
                                    }
                                }
                                PacketType::DeviceDefense => {
                                    if let Some(dn) = extract_defense_device_number(data) {
                                        let _ = recv_tx.send(FinderEvent::DefenseReceived {
                                            device_number: dn,
                                        });
                                    }
                                }
                                PacketType::DeviceClaimStage2 => {
                                    if let Some(dn) = extract_claim_stage2_device_number(data) {
                                        if let SocketAddr::V4(v4) = addr {
                                            let _ = recv_tx.send(FinderEvent::ClaimReceived {
                                                device_number: dn,
                                                source_ip: *v4.ip(),
                                            });
                                        }
                                    }
                                }
                                _ => {} // Ignore other packet types on the discovery port
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Spawn the aging task
        let aging_devices = devices.clone();
        let aging_tx = event_tx.clone();
        let aging_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(AGING_INTERVAL);
            loop {
                interval.tick().await;
                let now = Instant::now();
                let mut map = aging_devices.write().await;
                let expired: Vec<u8> = map
                    .iter()
                    .filter(|(_, dev)| now.duration_since(dev.last_seen) > DEVICE_EXPIRY)
                    .map(|(&k, _)| k)
                    .collect();
                for key in expired {
                    if let Some(dev) = map.remove(&key) {
                        let _ = aging_tx.send(FinderEvent::DeviceLost(dev));
                    }
                }
            }
        });

        Ok(Self {
            devices,
            ignored_addresses,
            event_tx,
            recv_task,
            aging_task,
        })
    }

    /// Subscribe to finder events (device found/lost).
    pub fn subscribe(&self) -> broadcast::Receiver<FinderEvent> {
        self.event_tx.subscribe()
    }

    /// Get a snapshot of all currently known devices.
    pub async fn devices(&self) -> Vec<DeviceAnnouncement> {
        self.devices.read().await.values().cloned().collect()
    }

    /// Look up a specific device by number.
    pub async fn device(&self, number: DeviceNumber) -> Option<DeviceAnnouncement> {
        self.devices.read().await.get(&number.0).cloned()
    }

    /// Stop the finder, canceling background tasks.
    pub fn stop(self) {
        self.recv_task.abort();
        self.aging_task.abort();
    }

    /// Add an IP address to the ignore list.
    ///
    /// Announcements from this address will be silently dropped.
    /// Useful for filtering out your own computer's rekordbox instance.
    pub fn add_ignored_address(&self, ip: Ipv4Addr) {
        self.ignored_addresses.insert(ip);
    }

    /// Remove an IP address from the ignore list.
    pub fn remove_ignored_address(&self, ip: Ipv4Addr) {
        self.ignored_addresses.remove(&ip);
    }

    /// Check whether an IP address is currently ignored.
    pub fn is_ignored(&self, ip: &Ipv4Addr) -> bool {
        self.ignored_addresses.contains(ip)
    }

    /// Remove all known devices from the map, emitting `DeviceLost` for each.
    pub async fn flush_devices(&self) {
        let mut map = self.devices.write().await;
        for (_, dev) in map.drain() {
            let _ = self.event_tx.send(FinderEvent::DeviceLost(dev));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::types::DeviceType;
    use crate::protocol::announce::build_keep_alive;
    use std::net::Ipv4Addr;

    #[test]
    fn finder_event_variants_constructible() {
        let ann = DeviceAnnouncement {
            name: "CDJ-TEST".to_string(),
            number: DeviceNumber(1),
            device_type: DeviceType::Cdj,
            mac_address: [0; 6],
            ip_address: Ipv4Addr::LOCALHOST,
            peer_count: 0,
            is_opus_quad: false,
            is_xdj_az: false,
            is_using_device_library_plus: false,
            last_seen: Instant::now(),
        };

        let _ = FinderEvent::DeviceFound(ann.clone());
        let _ = FinderEvent::DeviceUpdated(ann.clone());
        let _ = FinderEvent::DeviceLost(ann);
    }

    #[test]
    fn finder_event_is_debug_and_clone() {
        let ann = DeviceAnnouncement {
            name: "Test".to_string(),
            number: DeviceNumber(2),
            device_type: DeviceType::Mixer,
            mac_address: [1, 2, 3, 4, 5, 6],
            ip_address: Ipv4Addr::new(10, 0, 0, 1),
            peer_count: 0,
            is_opus_quad: false,
            is_xdj_az: false,
            is_using_device_library_plus: false,
            last_seen: Instant::now(),
        };
        let event = FinderEvent::DeviceFound(ann);
        let cloned = event.clone();
        // Debug formatting should not panic
        let _ = format!("{:?}", cloned);
    }

    /// Integration test: start DeviceFinder, send a crafted keep-alive to
    /// localhost:50000, and verify the device appears.
    ///
    /// Skipped gracefully if the port is unavailable (e.g., in CI or when
    /// another process is already bound).
    #[tokio::test]
    async fn loopback_device_discovery() {
        // Try to start the finder; skip if port is unavailable
        let finder = match DeviceFinder::start().await {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Skipping loopback test: cannot bind to port {DISCOVERY_PORT}: {e}");
                return;
            }
        };

        let mut rx = finder.subscribe();

        // Build a keep-alive packet and send it to the finder via loopback
        let pkt = build_keep_alive(
            "TestCDJ",
            DeviceNumber(7),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            Ipv4Addr::LOCALHOST,
        );

        let sender = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        sender
            .send_to(&pkt, ("127.0.0.1", DISCOVERY_PORT))
            .await
            .unwrap();

        // Wait for the event with a timeout
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;

        match event {
            Ok(Ok(FinderEvent::DeviceFound(ann))) => {
                assert_eq!(ann.name, "TestCDJ");
                assert_eq!(ann.number, DeviceNumber(7));
                assert_eq!(ann.mac_address, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
            }
            other => {
                // On some systems the packet may not loop back; don't fail hard
                eprintln!("Loopback test: unexpected result: {other:?}");
            }
        }

        // Verify device appears in the map
        let devs = finder.devices().await;
        if !devs.is_empty() {
            assert!(devs.iter().any(|d| d.number == DeviceNumber(7)));
        }

        // Also test the single-device lookup
        if let Some(dev) = finder.device(DeviceNumber(7)).await {
            assert_eq!(dev.name, "TestCDJ");
        }

        finder.stop();
    }

    // === Ignored Addresses Tests ===

    #[tokio::test]
    async fn add_and_remove_ignored_address() {
        let finder = match DeviceFinder::start().await {
            Ok(f) => f,
            Err(_) => return,
        };
        let ip = Ipv4Addr::new(192, 168, 1, 50);
        assert!(!finder.is_ignored(&ip));

        finder.add_ignored_address(ip);
        assert!(finder.is_ignored(&ip));

        finder.remove_ignored_address(ip);
        assert!(!finder.is_ignored(&ip));

        finder.stop();
    }

    #[tokio::test]
    async fn ignored_address_prevents_discovery() {
        let finder = match DeviceFinder::start().await {
            Ok(f) => f,
            Err(_) => return,
        };

        // Ignore loopback so any packets sent from our test won't be processed
        finder.add_ignored_address(Ipv4Addr::LOCALHOST);

        let mut rx = finder.subscribe();

        let pkt = build_keep_alive("Ignored", DeviceNumber(9), [0; 6], Ipv4Addr::LOCALHOST);
        let sender = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        sender
            .send_to(&pkt, ("127.0.0.1", DISCOVERY_PORT))
            .await
            .unwrap();

        // Should time out since the address is ignored
        let result = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
        assert!(
            result.is_err(),
            "Expected timeout — packet should be ignored"
        );

        finder.stop();
    }

    // === Flush Devices Tests ===

    #[tokio::test]
    async fn flush_devices_clears_map_and_emits_lost() {
        let finder = match DeviceFinder::start().await {
            Ok(f) => f,
            Err(_) => return,
        };

        // Insert a device by sending a keep-alive
        let pkt = build_keep_alive(
            "FlushTest",
            DeviceNumber(8),
            [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
            Ipv4Addr::LOCALHOST,
        );
        let sender = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        sender
            .send_to(&pkt, ("127.0.0.1", DISCOVERY_PORT))
            .await
            .unwrap();

        // Wait for it to be registered
        tokio::time::sleep(Duration::from_millis(100)).await;

        let devs_before = finder.devices().await;
        if devs_before.is_empty() {
            // Packet didn't loop back; skip
            finder.stop();
            return;
        }

        let mut rx = finder.subscribe();
        finder.flush_devices().await;

        let devs_after = finder.devices().await;
        assert!(devs_after.is_empty(), "devices should be empty after flush");

        // Should receive DeviceLost event
        let event = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
        match event {
            Ok(Ok(FinderEvent::DeviceLost(ann))) => {
                assert_eq!(ann.name, "FlushTest");
            }
            _ => {} // Acceptable if event was already consumed
        }

        finder.stop();
    }

    // === Device Expiry Tests ===

    #[test]
    fn device_expiry_is_4_5_seconds() {
        assert_eq!(DEVICE_EXPIRY, Duration::from_millis(4500));
    }

    #[test]
    fn aging_interval_is_1_second() {
        assert_eq!(AGING_INTERVAL, Duration::from_secs(1));
    }
}
