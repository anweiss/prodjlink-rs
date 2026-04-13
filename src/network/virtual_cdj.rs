use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::device::types::{DeviceNumber, TrackSourceSlot, TrackType};
use crate::error::{ProDjLinkError, Result};
use crate::network::finder::{DeviceFinder, FinderEvent};
use crate::protocol::announce::{
    build_claim_stage1, build_claim_stage2, build_claim_stage3, build_defense, build_device_hello,
    build_keep_alive,
};
use crate::protocol::command;
use crate::protocol::header::{BEAT_PORT, DISCOVERY_PORT, STATUS_PORT};

/// Interval between keep-alive packets.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Configuration for the virtual CDJ.
#[derive(Debug)]
pub struct VirtualCdjConfig {
    /// Device name to announce (max 20 chars).
    pub name: String,
    /// Desired device number (1-6 typical for CDJs).
    pub device_number: DeviceNumber,
    /// Network interface IP to bind to.
    pub interface_address: Ipv4Addr,
}

impl VirtualCdjConfig {
    pub fn new(device_number: u8, interface_address: Ipv4Addr) -> Result<Self> {
        if device_number == 0 {
            return Err(ProDjLinkError::InvalidDeviceNumber(device_number));
        }
        Ok(Self {
            name: "prodjlink-rs".to_string(),
            device_number: DeviceNumber(device_number),
            interface_address,
        })
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

/// A virtual CDJ that appears on the DJ Link network.
pub struct VirtualCdj {
    config: Arc<VirtualCdjConfig>,
    /// Socket for sending keep-alive announcements (port 50000).
    #[allow(dead_code)]
    discovery_socket: Arc<UdpSocket>,
    /// Socket for sending commands (port 50002).
    status_socket: Arc<UdpSocket>,
    /// The MAC address we're using.
    #[allow(dead_code)]
    mac_address: [u8; 6],
    /// Keep-alive background task.
    keepalive_task: Option<JoinHandle<()>>,
    /// Defense background task — defends our device number against claims.
    defense_task: Option<JoinHandle<()>>,
}

impl VirtualCdj {
    /// Start the virtual CDJ with the given configuration.
    ///
    /// Optionally checks the DeviceFinder for number conflicts before claiming.
    pub async fn start(config: VirtualCdjConfig, finder: Option<&DeviceFinder>) -> Result<Self> {
        // Check for device number conflicts
        if let Some(finder) = finder {
            if let Some(existing) = finder.device(config.device_number).await {
                return Err(ProDjLinkError::Parse(format!(
                    "device number {} already in use by {}",
                    config.device_number, existing.name
                )));
            }
        }

        // Use a locally-administered MAC (real implementation would read from interface)
        let mac_address = [0x02, 0x00, 0x00, 0x00, 0x00, config.device_number.0];

        let discovery_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        discovery_socket.set_broadcast(true)?;
        let discovery_socket = Arc::new(discovery_socket);

        let status_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        status_socket.set_broadcast(true)?;
        let status_socket = Arc::new(status_socket);

        let config = Arc::new(config);

        let ka_config = config.clone();
        let ka_socket = discovery_socket.clone();
        let ka_mac = mac_address;
        let keepalive_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(KEEP_ALIVE_INTERVAL);
            let broadcast_addr: SocketAddr =
                SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
            loop {
                interval.tick().await;
                let packet = build_keep_alive(
                    &ka_config.name,
                    ka_config.device_number,
                    ka_mac,
                    ka_config.interface_address,
                );
                let _ = ka_socket.send_to(&packet, broadcast_addr).await;
            }
        });

        Ok(Self {
            config,
            discovery_socket,
            status_socket,
            mac_address,
            keepalive_task: Some(keepalive_task),
            defense_task: None,
        })
    }

    /// Get the device number we're using.
    pub fn device_number(&self) -> DeviceNumber {
        self.config.device_number
    }

    /// Get the device name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Send a fader start/stop command to a target device.
    pub async fn fader_start(&self, target: DeviceNumber, start: bool) -> Result<()> {
        let packet =
            command::build_fader_start_single(self.config.device_number, target, start);
        self.send_command(&packet).await
    }

    /// Tell a target device to load a specific track.
    pub async fn load_track(
        &self,
        target: DeviceNumber,
        source_player: DeviceNumber,
        source_slot: TrackSourceSlot,
        track_type: TrackType,
        rekordbox_id: u32,
    ) -> Result<()> {
        let packet = command::build_load_track(
            self.config.device_number,
            target,
            source_player,
            source_slot,
            track_type,
            rekordbox_id,
        );
        self.send_command(&packet).await
    }

    /// Enable or disable sync mode on a target device.
    pub async fn set_sync(&self, target: DeviceNumber, enable: bool) -> Result<()> {
        let packet = command::build_sync_command(self.config.device_number, target, enable);
        self.send_beat_command(&packet).await
    }

    /// Request to become the tempo master.
    pub async fn become_master(&self) -> Result<()> {
        let packet = command::build_master_command(self.config.device_number);
        self.send_beat_command(&packet).await
    }

    /// Send a command packet via broadcast on the status port (50002).
    ///
    /// Used for fader start (0x02) and load track (0x19) commands.
    async fn send_command(&self, packet: &[u8]) -> Result<()> {
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        self.status_socket.send_to(packet, broadcast_addr).await?;
        Ok(())
    }

    /// Send a command packet via broadcast on the beat port (50001).
    ///
    /// Used for sync control (0x2a) and master handoff (0x26) commands.
    async fn send_beat_command(&self, packet: &[u8]) -> Result<()> {
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), BEAT_PORT);
        self.status_socket.send_to(packet, broadcast_addr).await?;
        Ok(())
    }

    /// Stop the virtual CDJ and its keep-alive loop.
    pub fn stop(mut self) {
        if let Some(task) = self.keepalive_task.take() {
            task.abort();
        }
        if let Some(task) = self.defense_task.take() {
            task.abort();
        }
    }

    /// Start the virtual CDJ with the full 3-stage device number claim protocol.
    ///
    /// Runs the claim handshake before starting the keep-alive loop. After
    /// claiming, a background defense task monitors for conflicting claims.
    pub async fn start_claimed(
        config: VirtualCdjConfig,
        finder: &DeviceFinder,
    ) -> Result<Self> {
        let mac_address = [0x02, 0x00, 0x00, 0x00, 0x00, config.device_number.0];

        let discovery_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        discovery_socket.set_broadcast(true)?;
        let discovery_socket = Arc::new(discovery_socket);

        // Run the claim protocol before starting keep-alive
        run_claim_protocol(
            &discovery_socket,
            finder,
            Ipv4Addr::BROADCAST,
            config.device_number.0,
            &config.name,
            mac_address,
            config.interface_address,
            true, // auto_assign flag
        )
        .await?;

        let status_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        status_socket.set_broadcast(true)?;
        let status_socket = Arc::new(status_socket);

        let config = Arc::new(config);

        // Start keep-alive loop
        let ka_config = config.clone();
        let ka_socket = discovery_socket.clone();
        let ka_mac = mac_address;
        let keepalive_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(KEEP_ALIVE_INTERVAL);
            let broadcast_addr: SocketAddr =
                SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
            loop {
                interval.tick().await;
                let packet = build_keep_alive(
                    &ka_config.name,
                    ka_config.device_number,
                    ka_mac,
                    ka_config.interface_address,
                );
                let _ = ka_socket.send_to(&packet, broadcast_addr).await;
            }
        });

        // Start defense task
        let def_socket = discovery_socket.clone();
        let def_events = finder.subscribe();
        let def_number = config.device_number.0;
        let def_name = config.name.clone();
        let def_ip = config.interface_address;
        let defense_task = tokio::spawn(async move {
            defense_loop(def_socket, def_events, def_number, def_name, def_ip).await;
        });

        Ok(Self {
            config,
            discovery_socket,
            status_socket,
            mac_address,
            keepalive_task: Some(keepalive_task),
            defense_task: Some(defense_task),
        })
    }

    /// Start with automatic device number assignment.
    ///
    /// Watches the network for 4 seconds via the [`DeviceFinder`], picks the
    /// first unused number in the preferred range, and claims it using the
    /// 3-stage protocol.
    ///
    /// When `use_player_numbers` is `true`, prefers numbers 1–4 (standard CDJ
    /// range) before falling back to 7–15. Otherwise only tries 7–15, avoiding
    /// channels 5–6 which cause CDJ-3000 issues.
    pub async fn start_auto(
        name: impl Into<String>,
        interface_address: Ipv4Addr,
        finder: &DeviceFinder,
        use_player_numbers: bool,
    ) -> Result<Self> {
        let name = name.into();

        // Watch the network for 4 seconds
        tokio::time::sleep(Duration::from_secs(4)).await;

        let devices = finder.devices().await;
        let used: HashSet<u8> = devices.iter().map(|d| d.number.0).collect();
        let candidates = candidate_device_numbers(&used, use_player_numbers);

        if candidates.is_empty() {
            return Err(ProDjLinkError::NoAvailableDeviceNumber);
        }

        for &device_number in &candidates {
            let config = VirtualCdjConfig::new(device_number, interface_address)?
                .with_name(name.clone());
            match Self::start_claimed(config, finder).await {
                Ok(vcdj) => return Ok(vcdj),
                Err(ProDjLinkError::DeviceNumberInUse(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(ProDjLinkError::NoAvailableDeviceNumber)
    }
}

// ---------------------------------------------------------------------------
// Claim Protocol Helpers
// ---------------------------------------------------------------------------

/// Execute the 3-stage device number claim protocol on the DJ Link network.
///
/// Sends the hello → stage 1 → stage 2 → stage 3 packet series on the
/// broadcast address, checking for defense packets between each send.
/// Returns `Err(DeviceNumberInUse)` if another device defends the number.
async fn run_claim_protocol(
    socket: &UdpSocket,
    finder: &DeviceFinder,
    broadcast_addr: Ipv4Addr,
    device_number: u8,
    name: &str,
    mac: [u8; 6],
    ip: Ipv4Addr,
    auto_assign: bool,
) -> Result<()> {
    let mut events = finder.subscribe();
    let dest = SocketAddr::new(broadcast_addr.into(), DISCOVERY_PORT);

    // Phase 1: Broadcast DeviceHello 3 times, 300 ms apart
    let hello = build_device_hello(name);
    for _ in 0..3 {
        socket.send_to(&hello, dest).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // Phase 2: Stage 1 claim — 3 packets, 300 ms apart, watch for defense
    for i in 1..=3u8 {
        let pkt = build_claim_stage1(name, mac, i);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    // Phase 3: Stage 2 claim — 3 packets, 300 ms apart, watch for defense
    for i in 1..=3u8 {
        let pkt = build_claim_stage2(name, mac, ip, device_number, i, auto_assign);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    // Phase 4: Stage 3 (final) claim — 3 packets, 300 ms apart
    for i in 1..=3u8 {
        let pkt = build_claim_stage3(name, device_number, i);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    Ok(())
}

/// Wait up to `duration` for a defense event matching `device_number`.
///
/// Returns `true` if a defense was received, `false` on timeout.
async fn wait_for_defense(
    events: &mut broadcast::Receiver<FinderEvent>,
    duration: Duration,
    device_number: u8,
) -> bool {
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            result = events.recv() => {
                match result {
                    Ok(FinderEvent::DefenseReceived { device_number: dn })
                        if dn == device_number =>
                    {
                        return true;
                    }
                    Err(broadcast::error::RecvError::Closed) => return false,
                    _ => continue, // other event or Lagged — keep waiting
                }
            }
            () = &mut sleep => {
                return false;
            }
        }
    }
}

/// Background loop that defends our device number against incoming claims.
///
/// Listens for [`FinderEvent::ClaimReceived`] events and responds with a
/// defense packet sent directly to the claimer's IP address.
async fn defense_loop(
    socket: Arc<UdpSocket>,
    mut events: broadcast::Receiver<FinderEvent>,
    device_number: u8,
    name: String,
    ip: Ipv4Addr,
) {
    loop {
        match events.recv().await {
            Ok(FinderEvent::ClaimReceived {
                device_number: dn,
                source_ip,
            }) => {
                if dn == device_number {
                    let pkt = build_defense(&name, device_number, ip);
                    let target = SocketAddr::new(source_ip.into(), DISCOVERY_PORT);
                    let _ = socket.send_to(&pkt, target).await;
                    tracing::info!(
                        device_number,
                        %source_ip,
                        "Defended device number against incoming claim"
                    );
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            _ => {} // ignore other events
        }
    }
}

/// Return candidate device numbers in priority order, excluding numbers
/// already in use on the network.
///
/// When `use_player_numbers` is `true`, tries 1–4 first, then 7–15.
/// Otherwise tries 7–15 only. Channels 5–6 are always skipped to avoid
/// CDJ-3000 issues.
fn candidate_device_numbers(used: &HashSet<u8>, use_player_numbers: bool) -> Vec<u8> {
    let mut candidates = Vec::new();
    if use_player_numbers {
        for n in 1..=4 {
            if !used.contains(&n) {
                candidates.push(n);
            }
        }
    }
    for n in 7..=15 {
        if !used.contains(&n) {
            candidates.push(n);
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_valid_device_number() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        assert_eq!(cfg.device_number, DeviceNumber(5));
        assert_eq!(cfg.name, "prodjlink-rs");
        assert_eq!(cfg.interface_address, Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn config_rejects_zero_device_number() {
        let err = VirtualCdjConfig::new(0, Ipv4Addr::LOCALHOST).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidDeviceNumber(0)));
    }

    #[test]
    fn config_with_name_builder() {
        let cfg = VirtualCdjConfig::new(1, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_name("MyPlayer");
        assert_eq!(cfg.name, "MyPlayer");
        assert_eq!(cfg.device_number, DeviceNumber(1));
    }

    #[tokio::test]
    async fn start_and_accessors() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        assert_eq!(vcdj.device_number(), DeviceNumber(4));
        assert_eq!(vcdj.name(), "prodjlink-rs");

        vcdj.stop();
    }

    #[tokio::test]
    async fn start_with_custom_name() {
        let cfg = VirtualCdjConfig::new(2, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_name("TestCDJ");
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        assert_eq!(vcdj.name(), "TestCDJ");
        assert_eq!(vcdj.device_number(), DeviceNumber(2));

        vcdj.stop();
    }

    // === Claim Protocol / Candidate Number Tests ===

    #[test]
    fn candidate_numbers_non_standard_default() {
        let used = HashSet::new();
        let candidates = candidate_device_numbers(&used, false);
        // Should start at 7, not include 1-6
        assert_eq!(candidates[0], 7);
        assert!(!candidates.contains(&5));
        assert!(!candidates.contains(&6));
        assert!(!candidates.iter().any(|&n| n <= 4));
        assert_eq!(candidates.len(), 9); // 7..=15
    }

    #[test]
    fn candidate_numbers_standard_prefers_1_to_4() {
        let used = HashSet::new();
        let candidates = candidate_device_numbers(&used, true);
        assert_eq!(&candidates[..4], &[1, 2, 3, 4]);
        // 7-15 follow as fallback
        assert_eq!(candidates[4], 7);
    }

    #[test]
    fn candidate_numbers_skips_used() {
        let used: HashSet<u8> = [1, 3, 7, 9].into_iter().collect();
        let candidates = candidate_device_numbers(&used, true);
        assert_eq!(candidates[0], 2);
        assert_eq!(candidates[1], 4);
        assert_eq!(candidates[2], 8);
        assert!(!candidates.contains(&1));
        assert!(!candidates.contains(&3));
        assert!(!candidates.contains(&7));
        assert!(!candidates.contains(&9));
    }

    #[test]
    fn candidate_numbers_avoids_5_and_6() {
        let used = HashSet::new();
        for use_player in [true, false] {
            let candidates = candidate_device_numbers(&used, use_player);
            assert!(!candidates.contains(&5));
            assert!(!candidates.contains(&6));
        }
    }

    #[test]
    fn candidate_numbers_all_taken() {
        let used: HashSet<u8> = (1..=15).collect();
        assert!(candidate_device_numbers(&used, false).is_empty());
        assert!(candidate_device_numbers(&used, true).is_empty());
    }

    #[test]
    fn candidate_numbers_non_standard_with_some_taken() {
        let used: HashSet<u8> = [7, 8, 10].into_iter().collect();
        let candidates = candidate_device_numbers(&used, false);
        assert_eq!(candidates[0], 9);
        assert_eq!(candidates[1], 11);
    }

    #[tokio::test]
    async fn wait_for_defense_returns_false_on_timeout() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        // Don't send any defense events
        drop(tx);
        let result =
            wait_for_defense(&mut rx, Duration::from_millis(50), 7).await;
        // Channel closed before timeout, should return false
        assert!(!result);
    }

    #[tokio::test]
    async fn wait_for_defense_detects_matching_defense() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        let _ = tx.send(FinderEvent::DefenseReceived { device_number: 7 });
        let result =
            wait_for_defense(&mut rx, Duration::from_millis(500), 7).await;
        assert!(result);
    }

    #[tokio::test]
    async fn wait_for_defense_ignores_non_matching_defense() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        // Send defense for a different number
        let _ = tx.send(FinderEvent::DefenseReceived { device_number: 8 });
        drop(tx); // Close channel so the test terminates
        let result =
            wait_for_defense(&mut rx, Duration::from_millis(50), 7).await;
        assert!(!result);
    }
}
