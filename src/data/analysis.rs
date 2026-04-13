use crate::dbserver::client::Client;
use crate::device::types::TrackSourceSlot;
use crate::error::Result;

/// Represents an ANLZ file extension type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnlzFileType {
    /// .DAT file (basic analysis data).
    Dat,
    /// .EXT file (extended analysis data, Nexus).
    Ext,
    /// .2EX file (extended analysis data, Nxs2).
    Ext2,
}

impl AnlzFileType {
    /// Returns the file extension string for this type.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Dat => "DAT",
            Self::Ext => "EXT",
            Self::Ext2 => "2EX",
        }
    }
}

/// Represents an ANLZ tag type (4-byte identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnlzTagType(pub [u8; 4]);

impl AnlzTagType {
    /// Cue points.
    pub const PCOB: Self = Self(*b"PCOB");
    /// Nxs2 cue points.
    pub const PCO2: Self = Self(*b"PCO2");
    /// File path.
    pub const PPTH: Self = Self(*b"PPTH");
    /// VBR info.
    pub const PVBR: Self = Self(*b"PVBR");
    /// Beat grid.
    pub const PQTZ: Self = Self(*b"PQTZ");
    /// Waveform preview.
    pub const PWAV: Self = Self(*b"PWAV");
    /// Waveform detail (mono).
    pub const PWV2: Self = Self(*b"PWV2");
    /// Waveform preview (color).
    pub const PWV3: Self = Self(*b"PWV3");
    /// Waveform detail (color).
    pub const PWV4: Self = Self(*b"PWV4");
    /// Waveform detail (3-band).
    pub const PWV5: Self = Self(*b"PWV5");
    /// Waveform preview (3-band).
    pub const PWV6: Self = Self(*b"PWV6");
    /// Waveform detail (3-band, high-res).
    pub const PWV7: Self = Self(*b"PWV7");
    /// Song structure / phrases.
    pub const PSSI: Self = Self(*b"PSSI");
}

impl std::fmt::Display for AnlzTagType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match std::str::from_utf8(&self.0) {
            Ok(s) => write!(f, "{s}"),
            Err(_) => write!(
                f,
                "{:02x}{:02x}{:02x}{:02x}",
                self.0[0], self.0[1], self.0[2], self.0[3]
            ),
        }
    }
}

/// A raw ANLZ tag section fetched from a player.
#[derive(Debug, Clone)]
pub struct AnlzTag {
    /// The file type this tag was found in.
    pub file_type: AnlzFileType,
    /// The 4-byte tag type identifier.
    pub tag_type: AnlzTagType,
    /// The raw tag data (including the tag header).
    pub data: Vec<u8>,
}

/// Fetch a specific ANLZ tag from a player.
///
/// This is a stub — the actual implementation requires the full dbserver
/// ANLZ protocol, which sends a request for the analysis file and parses
/// the response to extract matching tag sections.
pub async fn fetch_anlz_tag(
    client: &mut Client,
    slot: TrackSourceSlot,
    track_id: u32,
    file_type: AnlzFileType,
    tag_type: AnlzTagType,
) -> Result<Option<AnlzTag>> {
    let _ = (client, slot, track_id, file_type, tag_type);
    Ok(None)
}

/// Parse ANLZ tag sections from raw ANLZ file data.
///
/// ANLZ files start with a file header whose length is stored at bytes 4..8
/// (big-endian u32). After the header, tag sections follow sequentially.
/// Each tag has:
/// - bytes 0..4: tag type (4 ASCII bytes)
/// - bytes 4..8: tag header length (big-endian u32)
/// - bytes 8..12: tag total length (big-endian u32)
///
/// The `file_type` parameter is attached to each returned tag.
pub fn parse_anlz_tags(data: &[u8], file_type: AnlzFileType) -> Vec<AnlzTag> {
    let mut tags = Vec::new();

    if data.len() < 8 {
        return tags;
    }

    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut offset = header_len;

    while offset + 12 <= data.len() {
        let tag_bytes: [u8; 4] = [
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ];
        let tag_total_len = u32::from_be_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
        ]) as usize;

        if tag_total_len == 0 || offset + tag_total_len > data.len() {
            break;
        }

        tags.push(AnlzTag {
            file_type,
            tag_type: AnlzTagType(tag_bytes),
            data: data[offset..offset + tag_total_len].to_vec(),
        });

        offset += tag_total_len;
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AnlzFileType ----

    #[test]
    fn file_type_extensions() {
        assert_eq!(AnlzFileType::Dat.extension(), "DAT");
        assert_eq!(AnlzFileType::Ext.extension(), "EXT");
        assert_eq!(AnlzFileType::Ext2.extension(), "2EX");
    }

    #[test]
    fn file_type_equality() {
        assert_eq!(AnlzFileType::Dat, AnlzFileType::Dat);
        assert_ne!(AnlzFileType::Dat, AnlzFileType::Ext);
    }

    // ---- AnlzTagType ----

    #[test]
    fn tag_type_constants() {
        assert_eq!(&AnlzTagType::PCOB.0, b"PCOB");
        assert_eq!(&AnlzTagType::PCO2.0, b"PCO2");
        assert_eq!(&AnlzTagType::PPTH.0, b"PPTH");
        assert_eq!(&AnlzTagType::PVBR.0, b"PVBR");
        assert_eq!(&AnlzTagType::PQTZ.0, b"PQTZ");
        assert_eq!(&AnlzTagType::PWAV.0, b"PWAV");
        assert_eq!(&AnlzTagType::PWV2.0, b"PWV2");
        assert_eq!(&AnlzTagType::PWV3.0, b"PWV3");
        assert_eq!(&AnlzTagType::PWV4.0, b"PWV4");
        assert_eq!(&AnlzTagType::PWV5.0, b"PWV5");
        assert_eq!(&AnlzTagType::PWV6.0, b"PWV6");
        assert_eq!(&AnlzTagType::PWV7.0, b"PWV7");
        assert_eq!(&AnlzTagType::PSSI.0, b"PSSI");
    }

    #[test]
    fn tag_type_display() {
        assert_eq!(format!("{}", AnlzTagType::PQTZ), "PQTZ");
        assert_eq!(format!("{}", AnlzTagType::PSSI), "PSSI");
    }

    #[test]
    fn tag_type_equality_and_hash() {
        use std::collections::HashSet;
        let a = AnlzTagType::PCOB;
        let b = AnlzTagType(*b"PCOB");
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    // ---- parse_anlz_tags ----

    /// Build synthetic ANLZ file data with the given tags.
    /// Each tag is (4-byte type, payload bytes excluding the 12-byte tag header).
    fn build_anlz_data(file_header_len: u32, tags: &[([u8; 4], &[u8])]) -> Vec<u8> {
        let mut data = vec![0u8; file_header_len as usize];
        data[4..8].copy_from_slice(&file_header_len.to_be_bytes());

        for (tag_type, payload) in tags {
            let tag_header_len: u32 = 12;
            let tag_total_len = tag_header_len as usize + payload.len();

            data.extend_from_slice(tag_type);
            data.extend_from_slice(&tag_header_len.to_be_bytes());
            data.extend_from_slice(&(tag_total_len as u32).to_be_bytes());
            data.extend_from_slice(payload);
        }

        data
    }

    #[test]
    fn parse_empty_data() {
        let tags = parse_anlz_tags(&[], AnlzFileType::Dat);
        assert!(tags.is_empty());
    }

    #[test]
    fn parse_too_short_data() {
        let tags = parse_anlz_tags(&[0u8; 5], AnlzFileType::Dat);
        assert!(tags.is_empty());
    }

    #[test]
    fn parse_header_only_no_tags() {
        let data = build_anlz_data(16, &[]);
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);
        assert!(tags.is_empty());
    }

    #[test]
    fn parse_single_tag() {
        let payload = b"hello";
        let data = build_anlz_data(16, &[(*b"PQTZ", payload)]);
        let tags = parse_anlz_tags(&data, AnlzFileType::Ext);

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, AnlzTagType::PQTZ);
        assert_eq!(tags[0].file_type, AnlzFileType::Ext);
        assert_eq!(tags[0].data.len(), 12 + payload.len());
        assert_eq!(&tags[0].data[12..], payload);
    }

    #[test]
    fn parse_multiple_tags() {
        let data = build_anlz_data(
            16,
            &[
                (*b"PQTZ", &[1, 2, 3]),
                (*b"PWAV", &[4, 5]),
                (*b"PSSI", &[6, 7, 8, 9]),
            ],
        );
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);

        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].tag_type, AnlzTagType::PQTZ);
        assert_eq!(tags[1].tag_type, AnlzTagType::PWAV);
        assert_eq!(tags[2].tag_type, AnlzTagType::PSSI);

        assert_eq!(&tags[0].data[12..], &[1, 2, 3]);
        assert_eq!(&tags[1].data[12..], &[4, 5]);
        assert_eq!(&tags[2].data[12..], &[6, 7, 8, 9]);
    }

    #[test]
    fn parse_stops_on_truncated_tag() {
        let mut data = build_anlz_data(16, &[(*b"PQTZ", &[1, 2, 3])]);
        data.extend_from_slice(b"PWAVxxxx");
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn parse_stops_on_zero_length_tag() {
        let mut data = build_anlz_data(16, &[(*b"PQTZ", &[1, 2])]);
        data.extend_from_slice(b"PWAV");
        data.extend_from_slice(&12u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn parse_stops_when_tag_exceeds_data() {
        let mut data = build_anlz_data(16, &[(*b"PQTZ", &[1])]);
        data.extend_from_slice(b"PWAV");
        data.extend_from_slice(&12u32.to_be_bytes());
        data.extend_from_slice(&1000u32.to_be_bytes());
        data.extend_from_slice(&[0u8; 10]);
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn parse_file_type_is_propagated() {
        let data = build_anlz_data(16, &[(*b"PQTZ", &[1])]);

        for ft in [AnlzFileType::Dat, AnlzFileType::Ext, AnlzFileType::Ext2] {
            let tags = parse_anlz_tags(&data, ft);
            assert_eq!(tags.len(), 1);
            assert_eq!(tags[0].file_type, ft);
        }
    }

    #[test]
    fn parse_custom_tag_type() {
        let data = build_anlz_data(16, &[(*b"XXXX", &[0xAB, 0xCD])]);
        let tags = parse_anlz_tags(&data, AnlzFileType::Dat);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, AnlzTagType(*b"XXXX"));
    }

    #[test]
    fn parse_large_file_header() {
        let data = build_anlz_data(128, &[(*b"PSSI", &[0xFF; 20])]);
        let tags = parse_anlz_tags(&data, AnlzFileType::Ext2);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, AnlzTagType::PSSI);
        assert_eq!(&tags[0].data[12..], &[0xFF; 20]);
    }
}
