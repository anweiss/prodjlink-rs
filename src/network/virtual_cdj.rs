use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

use crate::device::types::{DeviceNumber, TrackSourceSlot, TrackType};
use crate::error::{ProDjLinkError, Result};
use crate::network::finder::DeviceFinder;
use crate::protocol::announce::build_keep_alive;
use crate::protocol::command;
use crate::protocol::header::{DISCOVERY_PORT, STATUS_PORT};

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
        let packet = command::build_fader_start(self.config.device_number, target, start);
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
        self.send_command(&packet).await
    }

    /// Request to become the tempo master.
    pub async fn become_master(&self) -> Result<()> {
        let packet = command::build_master_command(self.config.device_number);
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        self.status_socket.send_to(&packet, broadcast_addr).await?;
        Ok(())
    }

    /// Send a command packet via broadcast on the status port.
    async fn send_command(&self, packet: &[u8]) -> Result<()> {
        // In a full implementation, we'd resolve the target's IP via DeviceFinder.
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        self.status_socket.send_to(packet, broadcast_addr).await?;
        Ok(())
    }

    /// Stop the virtual CDJ and its keep-alive loop.
    pub fn stop(mut self) {
        if let Some(task) = self.keepalive_task.take() {
            task.abort();
        }
    }
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
}
