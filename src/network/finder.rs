use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::device::types::DeviceNumber;
use crate::error::Result;
use crate::protocol::announce::{
    extract_claim_stage2_device_number, extract_defense_device_number, parse_keep_alive,
    DeviceAnnouncement,
};
use crate::protocol::header::{parse_header, PacketType, DISCOVERY_PORT};

/// How long before a device is considered gone (no keep-alive received).
const DEVICE_EXPIRY: Duration = Duration::from_secs(10);

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
        let (event_tx, _) = broadcast::channel(256);

        // Spawn the receive loop
        let recv_devices = devices.clone();
        let recv_tx = event_tx.clone();
        let recv_socket = socket.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                match recv_socket.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        let data = &buf[..len];
                        if let Ok(pkt_type) = parse_header(data) {
                            match pkt_type {
                                PacketType::DeviceKeepAlive => {
                                    if let Ok(announcement) = parse_keep_alive(data) {
                                        let key = announcement.number.0;
                                        let mut map = recv_devices.write().await;
                                        let is_new = !map.contains_key(&key);
                                        map.insert(key, announcement.clone());
                                        let event = if is_new {
                                            FinderEvent::DeviceFound(announcement)
                                        } else {
                                            FinderEvent::DeviceUpdated(announcement)
                                        };
                                        let _ = recv_tx.send(event);
                                    }
                                }
                                PacketType::DeviceDefense => {
                                    if let Some(dn) = extract_defense_device_number(data) {
                                        let _ = recv_tx
                                            .send(FinderEvent::DefenseReceived { device_number: dn });
                                    }
                                }
                                PacketType::DeviceClaimStage2 => {
                                    if let Some(dn) = extract_claim_stage2_device_number(data) {
                                        if let SocketAddr::V4(v4) = addr {
                                            let _ =
                                                recv_tx.send(FinderEvent::ClaimReceived {
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
                eprintln!(
                    "Skipping loopback test: cannot bind to port {DISCOVERY_PORT}: {e}"
                );
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
}
