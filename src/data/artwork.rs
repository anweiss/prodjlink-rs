use bytes::Bytes;

use crate::device::types::{DeviceNumber, TrackSourceSlot};

/// A reference to a specific piece of album art on a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArtworkReference {
    pub player: DeviceNumber,
    pub slot: TrackSourceSlot,
    pub artwork_id: u32,
}

/// Album art retrieved from a CDJ.
#[derive(Debug, Clone)]
pub struct AlbumArt {
    /// The reference this art belongs to.
    pub art_ref: ArtworkReference,
    /// Raw image data (typically JPEG).
    pub data: Bytes,
}

impl AlbumArt {
    pub fn new(art_ref: ArtworkReference, data: Bytes) -> Self {
        Self { art_ref, data }
    }

    /// Get the raw image bytes.
    pub fn image_data(&self) -> &[u8] {
        &self.data
    }

    /// Check if this appears to be a JPEG image.
    pub fn is_jpeg(&self) -> bool {
        self.data.len() >= 2 && self.data[0] == 0xFF && self.data[1] == 0xD8
    }

    /// Check if this appears to be a PNG image.
    pub fn is_png(&self) -> bool {
        self.data.len() >= 8 && &self.data[..8] == b"\x89PNG\r\n\x1a\n"
    }

    /// Get the estimated image format.
    pub fn format(&self) -> ImageFormat {
        if self.is_jpeg() {
            ImageFormat::Jpeg
        } else if self.is_png() {
            ImageFormat::Png
        } else {
            ImageFormat::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Unknown,
}

/// Build the dbserver request fields for an album art request.
pub fn build_art_request_args(art_ref: &ArtworkReference) -> Vec<crate::dbserver::field::Field> {
    use crate::dbserver::field::Field;
    vec![
        Field::number(8), // MenuIdentifier::Data
        Field::number(0), // unused
        Field::number(art_ref.artwork_id),
    ]
}

/// Extract album art bytes from a dbserver response message.
pub fn extract_art_from_response(
    art_ref: ArtworkReference,
    response: &crate::dbserver::message::Message,
) -> crate::error::Result<AlbumArt> {
    // The album art is typically in arg 3 as a BinaryField
    let field = response.args.get(3).ok_or_else(|| {
        crate::error::ProDjLinkError::Parse("missing art data in response".into())
    })?;
    let data = field.as_binary()?.clone();
    Ok(AlbumArt::new(art_ref, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbserver::field::Field;
    use crate::dbserver::message::{Message, MessageType};

    fn sample_ref() -> ArtworkReference {
        ArtworkReference {
            player: DeviceNumber(3),
            slot: TrackSourceSlot::UsbSlot,
            artwork_id: 42,
        }
    }

    #[test]
    fn album_art_creation() {
        let art_ref = sample_ref();
        let data = Bytes::from_static(&[0xFF, 0xD8, 0x01, 0x02]);
        let art = AlbumArt::new(art_ref, data.clone());

        assert_eq!(art.art_ref, art_ref);
        assert_eq!(art.data, data);
        assert_eq!(art.image_data(), &[0xFF, 0xD8, 0x01, 0x02]);
    }

    #[test]
    fn jpeg_detection() {
        let art = AlbumArt::new(sample_ref(), Bytes::from_static(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(art.is_jpeg());
        assert!(!art.is_png());
        assert_eq!(art.format(), ImageFormat::Jpeg);
    }

    #[test]
    fn png_detection() {
        let art = AlbumArt::new(
            sample_ref(),
            Bytes::from_static(b"\x89PNG\r\n\x1a\n extra data"),
        );
        assert!(art.is_png());
        assert!(!art.is_jpeg());
        assert_eq!(art.format(), ImageFormat::Png);
    }

    #[test]
    fn unknown_format_for_other_data() {
        let art = AlbumArt::new(sample_ref(), Bytes::from_static(&[0x00, 0x01, 0x02]));
        assert!(!art.is_jpeg());
        assert!(!art.is_png());
        assert_eq!(art.format(), ImageFormat::Unknown);
    }

    #[test]
    fn unknown_format_for_empty_data() {
        let art = AlbumArt::new(sample_ref(), Bytes::new());
        assert_eq!(art.format(), ImageFormat::Unknown);
    }

    #[test]
    fn build_art_request_args_fields() {
        let art_ref = ArtworkReference {
            player: DeviceNumber(1),
            slot: TrackSourceSlot::SdSlot,
            artwork_id: 99,
        };
        let args = build_art_request_args(&art_ref);

        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 8);
        assert_eq!(args[1].as_number().unwrap(), 0);
        assert_eq!(args[2].as_number().unwrap(), 99);
    }

    #[test]
    fn extract_art_from_response_success() {
        let art_ref = sample_ref();
        let jpeg_bytes = Bytes::from_static(&[0xFF, 0xD8, 0xFF, 0xE0]);

        // Build a mock response message with art data at arg index 3
        let msg = Message::new(
            1,
            MessageType::AlbumArtReq,
            vec![
                Field::number(0),                  // arg 0
                Field::number(0),                  // arg 1
                Field::number(0),                  // arg 2
                Field::binary(jpeg_bytes.clone()), // arg 3: art data
            ],
        );

        let art = extract_art_from_response(art_ref, &msg).unwrap();
        assert_eq!(art.art_ref, art_ref);
        assert_eq!(art.data, jpeg_bytes);
        assert!(art.is_jpeg());
    }

    #[test]
    fn extract_art_from_response_missing_field() {
        let art_ref = sample_ref();
        let msg = Message::new(
            1,
            MessageType::AlbumArtReq,
            vec![Field::number(0), Field::number(0)], // only 2 args, need index 3
        );

        let err = extract_art_from_response(art_ref, &msg).unwrap_err();
        assert!(err.to_string().contains("missing art data"));
    }

    #[test]
    fn extract_art_from_response_wrong_field_type() {
        let art_ref = sample_ref();
        let msg = Message::new(
            1,
            MessageType::AlbumArtReq,
            vec![
                Field::number(0),
                Field::number(0),
                Field::number(0),
                Field::number(42), // arg 3 is Number, not Binary
            ],
        );

        let err = extract_art_from_response(art_ref, &msg).unwrap_err();
        assert!(err.to_string().contains("expected Binary field"));
    }
}
