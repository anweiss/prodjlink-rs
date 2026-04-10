pub mod error;

pub mod data;
pub mod dbserver;
pub mod device;
pub mod network;
pub mod protocol;

pub mod util;

// ---------------------------------------------------------------------------
// Re-exports — flat access to the most commonly used types
// ---------------------------------------------------------------------------

// Core types
pub use device::types::{
    BeatNumber, Bpm, DeviceNumber, DeviceType, OnAirStatus, Pitch, PlayState, PlayState2,
    PlayState3, TrackSourceSlot, TrackType,
};
pub use error::{ProDjLinkError, Result};

// Protocol types
pub use protocol::announce::DeviceAnnouncement;
pub use protocol::beat::{Beat, PrecisePosition};
pub use protocol::status::{CdjStatus, DeviceUpdate, MixerStatus};

// Network services
pub use network::beat::{BeatEvent, BeatFinder};
pub use network::finder::{DeviceFinder, FinderEvent};
pub use network::status::StatusListener;
pub use network::virtual_cdj::{VirtualCdj, VirtualCdjConfig};

// Data types
pub use data::artwork::{AlbumArt, ArtworkReference, ImageFormat};
pub use data::beatgrid::{BeatGrid, BeatGridEntry};
pub use data::cue::{CueColor, CueEntry, CueList, CueType};
pub use data::metadata::{DataReference, SearchableItem, TrackMetadata};
pub use data::waveform::{WaveformDetail, WaveformPreview, WaveformStyle};

// DBServer
pub use dbserver::connection::ConnectionManager;

// ---------------------------------------------------------------------------
// ProDjLink — unified entry point
// ---------------------------------------------------------------------------

use std::net::Ipv4Addr;

/// Builder for creating a [`ProDjLink`] session.
///
/// This is the main entry point for using the library.
///
/// # Example
/// ```no_run
/// use prodjlink_rs::ProDjLink;
/// use std::net::Ipv4Addr;
///
/// #[tokio::main]
/// async fn main() -> prodjlink_rs::Result<()> {
///     let pdl = ProDjLink::builder()
///         .device_name("my-app")
///         .device_number(5)
///         .interface_address(Ipv4Addr::new(192, 168, 1, 100))
///         .build()
///         .await?;
///
///     // List devices on the network
///     for device in pdl.devices().await {
///         println!("Found: {} ({})", device.name, device.number);
///     }
///
///     // Subscribe to beats
///     let mut _beats = pdl.subscribe_beats();
///
///     pdl.shutdown();
///     Ok(())
/// }
/// ```
pub struct ProDjLinkBuilder {
    device_name: String,
    device_number: u8,
    interface_address: Option<Ipv4Addr>,
}

impl Default for ProDjLinkBuilder {
    fn default() -> Self {
        Self {
            device_name: "prodjlink-rs".to_string(),
            device_number: 5,
            interface_address: None,
        }
    }
}

impl ProDjLinkBuilder {
    pub fn device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = name.into();
        self
    }

    pub fn device_number(mut self, number: u8) -> Self {
        self.device_number = number;
        self
    }

    pub fn interface_address(mut self, addr: Ipv4Addr) -> Self {
        self.interface_address = Some(addr);
        self
    }

    /// Build and start all services.
    pub async fn build(self) -> Result<ProDjLink> {
        let interface_addr = self.interface_address.unwrap_or(Ipv4Addr::UNSPECIFIED);

        let finder = DeviceFinder::start().await?;
        let beat_finder = BeatFinder::start().await?;
        let status_listener = StatusListener::start().await?;

        let cdj_config =
            VirtualCdjConfig::new(self.device_number, interface_addr)?.with_name(self.device_name);
        let virtual_cdj = VirtualCdj::start(cdj_config, Some(&finder)).await?;

        let connection_manager = ConnectionManager::new(self.device_number);

        Ok(ProDjLink {
            finder,
            beat_finder,
            status_listener,
            virtual_cdj,
            connection_manager,
        })
    }
}

/// A running ProDjLink session with all services active.
pub struct ProDjLink {
    finder: DeviceFinder,
    beat_finder: BeatFinder,
    status_listener: StatusListener,
    virtual_cdj: VirtualCdj,
    connection_manager: ConnectionManager,
}

impl ProDjLink {
    /// Create a new builder.
    pub fn builder() -> ProDjLinkBuilder {
        ProDjLinkBuilder::default()
    }

    // --- Device Discovery ---

    /// Get all currently known devices on the network.
    pub async fn devices(&self) -> Vec<DeviceAnnouncement> {
        self.finder.devices().await
    }

    /// Subscribe to device found/lost events.
    pub fn subscribe_devices(&self) -> tokio::sync::broadcast::Receiver<FinderEvent> {
        self.finder.subscribe()
    }

    // --- Beats ---

    /// Subscribe to beat events from all players.
    pub fn subscribe_beats(&self) -> tokio::sync::broadcast::Receiver<BeatEvent> {
        self.beat_finder.subscribe()
    }

    // --- Status ---

    /// Subscribe to CDJ/mixer status updates.
    pub fn subscribe_status(&self) -> tokio::sync::broadcast::Receiver<DeviceUpdate> {
        self.status_listener.subscribe()
    }

    /// Get the latest status for a specific device.
    pub async fn latest_status(&self, device: DeviceNumber) -> Option<DeviceUpdate> {
        self.status_listener.latest(device).await
    }

    // --- Virtual CDJ ---

    /// Get a reference to the virtual CDJ for sending commands.
    pub fn virtual_cdj(&self) -> &VirtualCdj {
        &self.virtual_cdj
    }

    // --- Connection Manager ---

    /// Get a reference to the database connection manager.
    pub fn connection_manager(&self) -> &ConnectionManager {
        &self.connection_manager
    }

    // --- Shutdown ---

    /// Shut down all services gracefully.
    pub fn shutdown(self) {
        self.virtual_cdj.stop();
        self.beat_finder.stop();
        self.status_listener.stop();
        self.finder.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let builder = ProDjLinkBuilder::default();
        assert_eq!(builder.device_name, "prodjlink-rs");
        assert_eq!(builder.device_number, 5);
        assert!(builder.interface_address.is_none());
    }

    #[test]
    fn builder_chaining() {
        let addr = Ipv4Addr::new(192, 168, 1, 50);
        let builder = ProDjLink::builder()
            .device_name("test-app")
            .device_number(3)
            .interface_address(addr);

        assert_eq!(builder.device_name, "test-app");
        assert_eq!(builder.device_number, 3);
        assert_eq!(builder.interface_address, Some(addr));
    }

    #[test]
    fn reexports_core_types() {
        // Verify types are accessible at the crate root.
        let _bpm = Bpm(128.0);
        let _beat = BeatNumber(1);
        let _pitch = Pitch(0);
        let dn = DeviceNumber::new(1);
        assert!(dn.is_some());
    }

    #[test]
    fn reexports_data_types() {
        // Verify data types are accessible.
        let cue_list = CueList::new(vec![]);
        assert!(cue_list.is_empty());
    }
}
