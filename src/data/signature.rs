use sha1::{Digest, Sha1};

use crate::data::beatgrid::BeatGrid;
use crate::data::metadata::TrackMetadata;
use crate::data::waveform::WaveformDetail;

/// A unique signature identifying a track across different media sources.
///
/// Computed from a SHA-1 hash of the track title, artist, duration,
/// detail waveform data, and beat grid data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackSignature {
    /// The SHA-1 hash bytes.
    pub hash: [u8; 20],
    /// Human-readable hex string.
    pub hex: String,
}

impl TrackSignature {
    /// Compute a track signature from its components.
    /// Any missing component is simply skipped (empty contribution).
    pub fn compute(
        title: &str,
        artist: Option<&str>,
        duration: u32,
        waveform_data: Option<&[u8]>,
        beatgrid_data: Option<&[u8]>,
    ) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(title.as_bytes());
        if let Some(artist) = artist {
            hasher.update(artist.as_bytes());
        }
        hasher.update(duration.to_string().as_bytes());
        if let Some(data) = waveform_data {
            hasher.update(data);
        }
        if let Some(data) = beatgrid_data {
            hasher.update(data);
        }
        let result = hasher.finalize();
        let hash: [u8; 20] = result.into();
        let hex = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
        Self { hash, hex }
    }
}

impl std::fmt::Display for TrackSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.hex)
    }
}

/// Compute a [`TrackSignature`] from metadata plus optional waveform and beat grid.
pub fn signature_from_metadata(
    metadata: &TrackMetadata,
    waveform: Option<&WaveformDetail>,
    beatgrid: Option<&BeatGrid>,
) -> TrackSignature {
    let artist_label = &metadata.artist.label;
    let artist = if artist_label.is_empty() {
        None
    } else {
        Some(artist_label.as_str())
    };

    TrackSignature::compute(
        &metadata.title,
        artist,
        metadata.duration,
        waveform.map(|w| w.data()),
        beatgrid.map(|b| b.raw_data()).as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::metadata::{DataReference, SearchableItem};
    use crate::device::types::{DeviceNumber, TrackSourceSlot};
    use bytes::Bytes;

    #[test]
    fn compute_basic_signature() {
        let sig = TrackSignature::compute("My Track", Some("Artist"), 240, None, None);
        assert_eq!(sig.hash.len(), 20);
        assert_eq!(sig.hex.len(), 40);
    }

    #[test]
    fn compute_deterministic() {
        let a = TrackSignature::compute("Title", Some("Art"), 100, None, None);
        let b = TrackSignature::compute("Title", Some("Art"), 100, None, None);
        assert_eq!(a, b);
        assert_eq!(a.hex, b.hex);
    }

    #[test]
    fn different_inputs_different_signatures() {
        let a = TrackSignature::compute("Track A", Some("Artist"), 120, None, None);
        let b = TrackSignature::compute("Track B", Some("Artist"), 120, None, None);
        assert_ne!(a, b);
    }

    #[test]
    fn missing_artist() {
        let with = TrackSignature::compute("Title", Some("Artist"), 100, None, None);
        let without = TrackSignature::compute("Title", None, 100, None, None);
        assert_ne!(with, without);
    }

    #[test]
    fn with_waveform_and_beatgrid_data() {
        let waveform = vec![1u8, 2, 3, 4, 5];
        let beatgrid = vec![10u8, 20, 30];
        let sig = TrackSignature::compute(
            "Title",
            Some("Artist"),
            200,
            Some(&waveform),
            Some(&beatgrid),
        );
        let sig_no_wave =
            TrackSignature::compute("Title", Some("Artist"), 200, None, Some(&beatgrid));
        let sig_no_beat =
            TrackSignature::compute("Title", Some("Artist"), 200, Some(&waveform), None);
        assert_ne!(sig, sig_no_wave);
        assert_ne!(sig, sig_no_beat);
        assert_ne!(sig_no_wave, sig_no_beat);
    }

    #[test]
    fn display_matches_hex() {
        let sig = TrackSignature::compute("Hello", None, 0, None, None);
        assert_eq!(format!("{sig}"), sig.hex);
    }

    #[test]
    fn hex_is_lowercase() {
        let sig = TrackSignature::compute("Test", None, 42, None, None);
        assert!(sig.hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!sig.hex.chars().any(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn signature_from_metadata_basic() {
        let data_ref = DataReference::new(DeviceNumber(1), TrackSourceSlot::UsbSlot, 1);
        let mut meta = TrackMetadata::new(data_ref);
        meta.title = "Test Track".to_string();
        meta.artist = SearchableItem::new(1, "Test Artist");
        meta.duration = 180;

        let sig = signature_from_metadata(&meta, None, None);
        let expected = TrackSignature::compute("Test Track", Some("Test Artist"), 180, None, None);
        assert_eq!(sig, expected);
    }

    #[test]
    fn signature_from_metadata_empty_artist_treated_as_none() {
        let data_ref = DataReference::new(DeviceNumber(1), TrackSourceSlot::UsbSlot, 1);
        let mut meta = TrackMetadata::new(data_ref);
        meta.title = "Track".to_string();
        meta.artist = SearchableItem::new(0, "");
        meta.duration = 60;

        let sig = signature_from_metadata(&meta, None, None);
        let expected = TrackSignature::compute("Track", None, 60, None, None);
        assert_eq!(sig, expected);
    }

    #[test]
    fn signature_from_metadata_with_waveform() {
        let data_ref = DataReference::new(DeviceNumber(1), TrackSourceSlot::UsbSlot, 1);
        let mut meta = TrackMetadata::new(data_ref);
        meta.title = "Track".to_string();
        meta.duration = 60;

        let mut buf = vec![0u8; 19];
        buf.extend_from_slice(&[0x01, 0x02, 0x03]);
        let waveform = WaveformDetail::from_bytes(
            Bytes::from(buf),
            crate::data::waveform::WaveformStyle::Blue,
        )
        .unwrap();

        let sig = signature_from_metadata(&meta, Some(&waveform), None);
        let sig_no_wave = signature_from_metadata(&meta, None, None);
        assert_ne!(sig, sig_no_wave);
    }

    #[test]
    fn signature_from_metadata_with_beatgrid() {
        let data_ref = DataReference::new(DeviceNumber(1), TrackSourceSlot::UsbSlot, 1);
        let mut meta = TrackMetadata::new(data_ref);
        meta.title = "Track".to_string();
        meta.duration = 60;

        let mut grid_data = vec![0u8; 20];
        let mut entry = vec![0u8; 16];
        entry[0..2].copy_from_slice(&1u16.to_le_bytes());
        entry[2..4].copy_from_slice(&12800u16.to_le_bytes());
        entry[4..8].copy_from_slice(&0u32.to_le_bytes());
        grid_data.extend_from_slice(&entry);
        let beatgrid = BeatGrid::from_bytes(&grid_data).unwrap();

        let sig = signature_from_metadata(&meta, None, Some(&beatgrid));
        let sig_no_grid = signature_from_metadata(&meta, None, None);
        assert_ne!(sig, sig_no_grid);
    }
}
