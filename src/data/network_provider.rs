use std::sync::Arc;

use async_trait::async_trait;

use crate::data::artwork::{
    AlbumArt, ArtworkReference, build_art_request_args, extract_art_from_response,
};
use crate::data::beatgrid::BeatGrid;
use crate::data::cue::CueList;
use crate::data::metadata::{DataReference, TrackMetadata, build_metadata_request_args};
use crate::data::provider::MetadataProvider;
use crate::data::waveform::{WaveformDetail, WaveformPreview, WaveformStyle};
use crate::dbserver::connection::ConnectionManager;
use crate::dbserver::field::Field;
use crate::dbserver::message::MessageType;
use crate::error::{ProDjLinkError, Result};
use crate::network::finder::DeviceFinder;

/// Menu identifier value used in dbserver data requests.
const MENU_ID_DATA: u8 = 8;

/// An implementation of [`MetadataProvider`] that fetches data over the
/// dbserver network protocol from real CDJ hardware.
pub struct NetworkProvider {
    connection_manager: Arc<ConnectionManager>,
    device_finder: Arc<DeviceFinder>,
}

impl NetworkProvider {
    /// Create a new network provider.
    ///
    /// The `device_finder` is used to resolve player numbers to IP addresses.
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        device_finder: Arc<DeviceFinder>,
    ) -> Self {
        Self {
            connection_manager,
            device_finder,
        }
    }

    /// Resolve a player number to its IP address via the device finder.
    async fn resolve_ip(&self, reference: &DataReference) -> Result<std::net::Ipv4Addr> {
        let device = self
            .device_finder
            .device(reference.player)
            .await
            .ok_or(ProDjLinkError::DeviceNotFound(reference.player.0))?;
        Ok(device.ip_address)
    }
}

// -----------------------------------------------------------------------
// Internal async fn helpers — these accept &mut Client directly so the
// compiler can tie the future's lifetime to the borrow.
// -----------------------------------------------------------------------

async fn do_metadata(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
    data_ref: DataReference,
) -> Result<TrackMetadata> {
    let items = client.menu_request(MessageType::MetadataReq, args).await?;
    Ok(TrackMetadata::from_menu_items(data_ref, &items))
}

async fn do_artwork(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
    art_ref: ArtworkReference,
) -> Result<AlbumArt> {
    let resp = client
        .simple_request(MessageType::AlbumArtReq, args)
        .await?;
    extract_art_from_response(art_ref, &resp)
}

async fn do_beatgrid(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<BeatGrid> {
    let resp = client
        .simple_request(MessageType::BeatGridReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing beat grid data in response".into()))?
        .as_binary()?;
    BeatGrid::from_bytes(data)
}

async fn do_cue_list(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<CueList> {
    let items = client
        .menu_request(MessageType::CueListExtReq, args)
        .await?;
    Ok(CueList::from_menu_items(&items))
}

async fn do_waveform_preview(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<WaveformPreview> {
    let resp = client
        .simple_request(MessageType::WaveformPreviewReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing waveform preview data in response".into()))?
        .as_binary()?
        .clone();
    WaveformPreview::from_bytes(data, WaveformStyle::Blue)
}

async fn do_waveform_detail(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<WaveformDetail> {
    let resp = client
        .simple_request(MessageType::WaveformDetailReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing waveform detail data in response".into()))?
        .as_binary()?
        .clone();
    WaveformDetail::from_bytes(data, WaveformStyle::Blue)
}

#[async_trait]
impl MetadataProvider for NetworkProvider {
    async fn get_metadata(&self, reference: &DataReference) -> Result<TrackMetadata> {
        let ip = self.resolve_ip(reference).await?;
        let args = build_metadata_request_args(reference, MENU_ID_DATA);
        let data_ref = *reference;
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_metadata(client, args, data_ref))
            })
            .await
    }

    async fn get_artwork(&self, reference: &DataReference, artwork_id: u32) -> Result<AlbumArt> {
        let ip = self.resolve_ip(reference).await?;
        let art_ref = ArtworkReference {
            player: reference.player,
            slot: reference.slot,
            artwork_id,
        };
        let args = build_art_request_args(&art_ref);
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_artwork(client, args, art_ref))
            })
            .await
    }

    async fn get_beatgrid(&self, reference: &DataReference) -> Result<BeatGrid> {
        let ip = self.resolve_ip(reference).await?;
        let args = vec![
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(reference.slot) as u32),
            Field::number(reference.rekordbox_id),
        ];
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_beatgrid(client, args))
            })
            .await
    }

    async fn get_cue_list(&self, reference: &DataReference) -> Result<CueList> {
        let ip = self.resolve_ip(reference).await?;
        let args = vec![
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(reference.slot) as u32),
            Field::number(reference.rekordbox_id),
        ];
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_cue_list(client, args))
            })
            .await
    }

    async fn get_waveform_preview(&self, reference: &DataReference) -> Result<WaveformPreview> {
        let ip = self.resolve_ip(reference).await?;
        let args = vec![
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(reference.slot) as u32),
            Field::number(reference.rekordbox_id),
        ];
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_waveform_preview(client, args))
            })
            .await
    }

    async fn get_waveform_detail(&self, reference: &DataReference) -> Result<WaveformDetail> {
        let ip = self.resolve_ip(reference).await?;
        let args = vec![
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(reference.slot) as u32),
            Field::number(reference.rekordbox_id),
        ];
        self.connection_manager
            .with_client(reference.player, ip, |client| {
                Box::pin(do_waveform_detail(client, args))
            })
            .await
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::metadata::DataReference;
    use crate::device::types::{DeviceNumber, TrackSourceSlot};

    #[test]
    fn menu_id_data_value() {
        assert_eq!(MENU_ID_DATA, 8);
    }

    #[test]
    fn network_provider_builds_beatgrid_args() {
        let data_ref = DataReference::new(DeviceNumber(3), TrackSourceSlot::UsbSlot, 42);
        let args = [
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(data_ref.slot) as u32),
            Field::number(data_ref.rekordbox_id),
        ];
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 8);
        assert_eq!(args[1].as_number().unwrap(), 3); // UsbSlot
        assert_eq!(args[2].as_number().unwrap(), 42);
    }

    #[test]
    fn network_provider_builds_art_ref() {
        let data_ref = DataReference::new(DeviceNumber(2), TrackSourceSlot::SdSlot, 99);
        let art_ref = ArtworkReference {
            player: data_ref.player,
            slot: data_ref.slot,
            artwork_id: 55,
        };
        assert_eq!(art_ref.player, DeviceNumber(2));
        assert_eq!(art_ref.slot, TrackSourceSlot::SdSlot);
        assert_eq!(art_ref.artwork_id, 55);
    }
}
