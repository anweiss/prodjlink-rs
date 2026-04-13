use async_trait::async_trait;

use crate::data::artwork::AlbumArt;
use crate::data::beatgrid::BeatGrid;
use crate::data::cue::CueList;
use crate::data::metadata::{DataReference, TrackMetadata};
use crate::data::waveform::{WaveformDetail, WaveformPreview};
use crate::error::Result;

/// A pluggable source of track metadata and analysis data.
///
/// This trait allows different backends (network dbserver, local SQLite,
/// file system) to provide track data through a unified interface.
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// Fetch full track metadata for the given data reference.
    async fn get_metadata(&self, reference: &DataReference) -> Result<TrackMetadata>;

    /// Fetch album artwork for the given artwork ID.
    async fn get_artwork(&self, reference: &DataReference, artwork_id: u32) -> Result<AlbumArt>;

    /// Fetch the beat grid for a track.
    async fn get_beatgrid(&self, reference: &DataReference) -> Result<BeatGrid>;

    /// Fetch the cue list for a track.
    async fn get_cue_list(&self, reference: &DataReference) -> Result<CueList>;

    /// Fetch the waveform preview for a track.
    async fn get_waveform_preview(&self, reference: &DataReference) -> Result<WaveformPreview>;

    /// Fetch the detailed waveform for a track.
    async fn get_waveform_detail(&self, reference: &DataReference) -> Result<WaveformDetail>;
}
