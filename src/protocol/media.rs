//! Media details packet parsing.
//!
//! When a CDJ responds to a media query, it sends back information about the
//! media (USB/SD/CD) inserted in a particular slot. This module parses those
//! response packets and can build the corresponding query packets.
//!
//! Packet offsets are derived from beat-link's `MediaDetails.java` and the
//! dysentery protocol analysis. Where exact offsets are uncertain they are
//! marked with comments.

use crate::device::types::{DeviceNumber, TrackSourceSlot};
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::MAGIC_HEADER;
use crate::util::bytes_to_number;

// ---------------------------------------------------------------------------
// Packet layout constants
// ---------------------------------------------------------------------------
// Offsets are based on beat-link's MediaDetails.java analysis.
// Firmware variations may shift some fields; uncertain offsets are noted.

const DEVICE_NAME_OFFSET: usize = 0x0c;
const DEVICE_NAME_LEN: usize = 20;
/// Device number of the responding player.
const PLAYER_OFFSET: usize = 0x21;
/// Which slot the media is in (TrackSourceSlot byte).
const SLOT_OFFSET: usize = 0x27;
/// Media type byte (MediaType).
const MEDIA_TYPE_OFFSET: usize = 0x28;
/// Start of media name encoded as UTF-16BE with null terminator.
const NAME_OFFSET: usize = 0x2c;
/// Maximum byte length of the UTF-16BE media name field.
const NAME_LEN: usize = 0x40; // 64 bytes → up to 32 UTF-16 code units
/// Track count (u32 big-endian).
const TRACK_COUNT_OFFSET: usize = 0x6c;
/// Playlist count (u16 big-endian).
const PLAYLIST_COUNT_OFFSET: usize = 0x70;
/// Colour ID associated with the media slot.
const COLOR_OFFSET: usize = 0x72;
/// Non-zero if the media has been analysed by rekordbox.
const REKORDBOX_OFFSET: usize = 0x73;
/// Total media size in bytes (u64 big-endian).
const TOTAL_SIZE_OFFSET: usize = 0x74;
/// Free space in bytes (u64 big-endian).
const FREE_SPACE_OFFSET: usize = 0x7c;

/// Start of the creation date field (UTF-16BE, up to 24 bytes / 12 code units).
const CREATION_DATE_OFFSET: usize = 0x6c;
/// Maximum byte length of the creation date field.
const CREATION_DATE_LEN: usize = 0x18;
/// Non-zero if the DJ has stored "My Settings" preferences on the media.
const HAS_MY_SETTINGS_OFFSET: usize = 0xab;
/// Minimum usable media size in bytes (u64 big-endian, if present).
const MIN_SIZE_OFFSET: usize = 0xc0;

/// Minimum packet size for a valid media details response.
const MIN_MEDIA_DETAILS_SIZE: usize = 0x84; // 132 bytes

/// Packet type byte used for media queries (approximate — may vary by firmware).
const MEDIA_QUERY_TYPE: u8 = 0x05;
/// Total size of a media query packet.
const MEDIA_QUERY_SIZE: usize = 0x24;

// ---------------------------------------------------------------------------
// MediaType
// ---------------------------------------------------------------------------

/// The type of media inserted in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    /// No media present.
    None,
    /// CD in the disc slot.
    Cd,
    /// SD card.
    Sd,
    /// USB storage device.
    Usb,
    /// Unknown media type.
    Unknown(u8),
}

impl From<u8> for MediaType {
    fn from(n: u8) -> Self {
        match n {
            0 => Self::None,
            1 => Self::Cd,
            2 => Self::Sd,
            3 => Self::Usb,
            _ => Self::Unknown(n),
        }
    }
}

// ---------------------------------------------------------------------------
// MediaDetails
// ---------------------------------------------------------------------------

/// Details about media inserted in a player slot.
#[derive(Debug, Clone)]
pub struct MediaDetails {
    /// The player this media is in.
    pub player: DeviceNumber,
    /// Which slot (USB, SD, CD).
    pub slot: TrackSourceSlot,
    /// The type of media.
    pub media_type: MediaType,
    /// Name/label of the media (volume label or disc title).
    pub name: String,
    /// Number of tracks on the media.
    pub track_count: u32,
    /// Number of playlists.
    pub playlist_count: u16,
    /// Total size in bytes (if available).
    pub total_size: u64,
    /// Free space in bytes (if available).
    pub free_space: u64,
    /// Whether the media is analysed by rekordbox.
    pub is_rekordbox: bool,
    /// Colour ID associated with the media slot.
    pub color: u8,
    /// Creation date of the media (from rekordbox).
    pub creation_date: String,
    /// Whether the DJ has stored "My Settings" preferences on the media.
    pub has_my_settings: Option<bool>,
    /// Minimum usable size in bytes, if present in the packet.
    pub min_size: Option<u64>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a media detail response packet.
///
/// These are received in response to media queries on the status port.
/// The caller is responsible for routing the correct packet type here;
/// this function validates the magic header and minimum length.
pub fn parse_media_details(data: &[u8]) -> Result<MediaDetails> {
    if data.len() < MIN_MEDIA_DETAILS_SIZE {
        return Err(ProDjLinkError::PacketTooShort {
            expected: MIN_MEDIA_DETAILS_SIZE,
            actual: data.len(),
        });
    }

    if data[..MAGIC_HEADER.len()] != MAGIC_HEADER {
        return Err(ProDjLinkError::InvalidMagic);
    }

    let player = DeviceNumber::from(data[PLAYER_OFFSET]);
    let slot = TrackSourceSlot::from(data[SLOT_OFFSET]);
    let media_type = MediaType::from(data[MEDIA_TYPE_OFFSET]);
    let name = read_utf16be_name(data, NAME_OFFSET, NAME_LEN);
    let track_count = bytes_to_number(data, TRACK_COUNT_OFFSET, 4);
    let playlist_count = bytes_to_number(data, PLAYLIST_COUNT_OFFSET, 2) as u16;
    let color = data[COLOR_OFFSET];
    let is_rekordbox = data[REKORDBOX_OFFSET] != 0;
    let total_size = read_u64_be(data, TOTAL_SIZE_OFFSET);
    let free_space = read_u64_be(data, FREE_SPACE_OFFSET);

    let creation_date = read_utf16be_name(data, CREATION_DATE_OFFSET, CREATION_DATE_LEN);

    let has_my_settings = if data.len() > HAS_MY_SETTINGS_OFFSET {
        Some(data[HAS_MY_SETTINGS_OFFSET] != 0)
    } else {
        None
    };

    let min_size = if data.len() >= MIN_SIZE_OFFSET + 8 {
        Some(read_u64_be(data, MIN_SIZE_OFFSET))
    } else {
        None
    };

    Ok(MediaDetails {
        player,
        slot,
        media_type,
        name,
        track_count,
        playlist_count,
        total_size,
        free_space,
        is_rekordbox,
        color,
        creation_date,
        has_my_settings,
        min_size,
    })
}

/// Build a media query packet to ask a player about its media slots.
///
/// The returned packet has the standard DJ-Link header followed by the
/// source/target device numbers and the slot to query.
pub fn build_media_query(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    slot: TrackSourceSlot,
) -> Vec<u8> {
    let mut packet = vec![0u8; MEDIA_QUERY_SIZE];

    // Magic header
    packet[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);

    // Packet type
    packet[0x0a] = MEDIA_QUERY_TYPE;

    // Device name — "prodjlink-rs" padded with nulls
    let name = b"prodjlink-rs";
    let copy_len = name.len().min(DEVICE_NAME_LEN);
    packet[DEVICE_NAME_OFFSET..DEVICE_NAME_OFFSET + copy_len]
        .copy_from_slice(&name[..copy_len]);

    // Source device number
    packet[0x21] = source_device.0;

    // Target device number
    packet[0x22] = target_device.0;

    // Slot being queried
    packet[0x23] = slot_to_u8(slot);

    packet
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a null-terminated UTF-16BE string from `data` starting at `offset`,
/// consuming at most `max_bytes` bytes.
fn read_utf16be_name(data: &[u8], offset: usize, max_bytes: usize) -> String {
    let end = (offset + max_bytes).min(data.len());
    // Ensure we only process complete 16-bit code units.
    let usable = (end - offset) & !1;
    let mut code_units = Vec::with_capacity(usable / 2);

    for i in (0..usable).step_by(2) {
        let unit = u16::from_be_bytes([data[offset + i], data[offset + i + 1]]);
        if unit == 0 {
            break;
        }
        code_units.push(unit);
    }

    String::from_utf16_lossy(&code_units)
}

/// Read an 8-byte big-endian u64 at the given offset.
fn read_u64_be(data: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

/// Convert a [`TrackSourceSlot`] back to its wire byte.
fn slot_to_u8(slot: TrackSourceSlot) -> u8 {
    match slot {
        TrackSourceSlot::NoTrack => 0,
        TrackSourceSlot::CdSlot => 1,
        TrackSourceSlot::SdSlot => 2,
        TrackSourceSlot::UsbSlot => 3,
        TrackSourceSlot::Collection => 4,
        TrackSourceSlot::Usb2Slot => 7,
        TrackSourceSlot::Unknown(n) => n,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers -----------------------------------------------------------

    /// Build a minimal valid media details response packet with the given
    /// fields pre-filled.
    fn make_media_details_packet(
        player: u8,
        slot: u8,
        media_type: u8,
        name_utf16: &[u16],
        track_count: u32,
        playlist_count: u16,
        color: u8,
        is_rekordbox: bool,
        total_size: u64,
        free_space: u64,
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; MIN_MEDIA_DETAILS_SIZE];

        // Magic header
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);

        // Packet type byte (use a placeholder value)
        pkt[0x0a] = 0x19;

        // Player / slot / media type
        pkt[PLAYER_OFFSET] = player;
        pkt[SLOT_OFFSET] = slot;
        pkt[MEDIA_TYPE_OFFSET] = media_type;

        // UTF-16BE media name
        for (i, &unit) in name_utf16.iter().enumerate() {
            let off = NAME_OFFSET + i * 2;
            if off + 1 >= pkt.len() {
                break;
            }
            let be = unit.to_be_bytes();
            pkt[off] = be[0];
            pkt[off + 1] = be[1];
        }

        // Track count (u32 BE)
        pkt[TRACK_COUNT_OFFSET..TRACK_COUNT_OFFSET + 4]
            .copy_from_slice(&track_count.to_be_bytes());

        // Playlist count (u16 BE)
        pkt[PLAYLIST_COUNT_OFFSET..PLAYLIST_COUNT_OFFSET + 2]
            .copy_from_slice(&playlist_count.to_be_bytes());

        // Colour & rekordbox flag
        pkt[COLOR_OFFSET] = color;
        pkt[REKORDBOX_OFFSET] = if is_rekordbox { 1 } else { 0 };

        // Total size / free space (u64 BE)
        pkt[TOTAL_SIZE_OFFSET..TOTAL_SIZE_OFFSET + 8]
            .copy_from_slice(&total_size.to_be_bytes());
        pkt[FREE_SPACE_OFFSET..FREE_SPACE_OFFSET + 8]
            .copy_from_slice(&free_space.to_be_bytes());

        pkt
    }

    /// Encode an ASCII string as UTF-16BE code units.
    fn ascii_to_utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    // -- MediaType conversion -----------------------------------------------

    #[test]
    fn media_type_none() {
        assert_eq!(MediaType::from(0), MediaType::None);
    }

    #[test]
    fn media_type_cd() {
        assert_eq!(MediaType::from(1), MediaType::Cd);
    }

    #[test]
    fn media_type_sd() {
        assert_eq!(MediaType::from(2), MediaType::Sd);
    }

    #[test]
    fn media_type_usb() {
        assert_eq!(MediaType::from(3), MediaType::Usb);
    }

    #[test]
    fn media_type_unknown() {
        assert_eq!(MediaType::from(42), MediaType::Unknown(42));
    }

    // -- parse_media_details -------------------------------------------------

    #[test]
    fn parse_usb_media_details() {
        let name_units = ascii_to_utf16("MY_USB");
        let total: u64 = 32_000_000_000; // ~32 GB
        let free: u64 = 16_000_000_000;  // ~16 GB

        let pkt = make_media_details_packet(
            2,           // player 2
            3,           // UsbSlot
            3,           // MediaType::Usb
            &name_units,
            1024,        // track count
            8,           // playlists
            5,           // colour
            true,        // rekordbox analysed
            total,
            free,
        );

        let md = parse_media_details(&pkt).expect("should parse successfully");

        assert_eq!(md.player, DeviceNumber(2));
        assert_eq!(md.slot, TrackSourceSlot::UsbSlot);
        assert_eq!(md.media_type, MediaType::Usb);
        assert_eq!(md.name, "MY_USB");
        assert_eq!(md.track_count, 1024);
        assert_eq!(md.playlist_count, 8);
        assert_eq!(md.color, 5);
        assert!(md.is_rekordbox);
        assert_eq!(md.total_size, total);
        assert_eq!(md.free_space, free);
    }

    #[test]
    fn parse_no_media() {
        let pkt = make_media_details_packet(
            1,    // player 1
            0,    // NoTrack
            0,    // MediaType::None
            &[],  // empty name
            0,    // no tracks
            0,    // no playlists
            0,    // colour
            false,
            0,
            0,
        );

        let md = parse_media_details(&pkt).expect("should parse successfully");

        assert_eq!(md.player, DeviceNumber(1));
        assert_eq!(md.slot, TrackSourceSlot::NoTrack);
        assert_eq!(md.media_type, MediaType::None);
        assert_eq!(md.name, "");
        assert_eq!(md.track_count, 0);
        assert_eq!(md.playlist_count, 0);
        assert!(!md.is_rekordbox);
        assert_eq!(md.total_size, 0);
        assert_eq!(md.free_space, 0);
    }

    #[test]
    fn parse_sd_media_details() {
        let name_units = ascii_to_utf16("DJ_SD_CARD");
        let pkt = make_media_details_packet(
            3,           // player 3
            2,           // SdSlot
            2,           // MediaType::Sd
            &name_units,
            512,
            4,
            2,
            true,
            8_000_000_000,
            2_000_000_000,
        );

        let md = parse_media_details(&pkt).expect("should parse");
        assert_eq!(md.slot, TrackSourceSlot::SdSlot);
        assert_eq!(md.media_type, MediaType::Sd);
        assert_eq!(md.name, "DJ_SD_CARD");
        assert_eq!(md.track_count, 512);
    }

    #[test]
    fn reject_too_short_packet() {
        let short = vec![0u8; MIN_MEDIA_DETAILS_SIZE - 1];
        let err = parse_media_details(&short).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: MIN_MEDIA_DETAILS_SIZE,
                ..
            }
        ));
    }

    #[test]
    fn reject_invalid_magic() {
        let mut pkt = vec![0u8; MIN_MEDIA_DETAILS_SIZE];
        pkt[0] = 0xFF; // corrupt first byte
        let err = parse_media_details(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidMagic));
    }

    // -- build_media_query ---------------------------------------------------

    #[test]
    fn build_query_has_valid_header() {
        let pkt = build_media_query(
            DeviceNumber(1),
            DeviceNumber(3),
            TrackSourceSlot::UsbSlot,
        );

        assert_eq!(pkt.len(), MEDIA_QUERY_SIZE);
        assert_eq!(&pkt[..MAGIC_HEADER.len()], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], MEDIA_QUERY_TYPE);
    }

    #[test]
    fn build_query_encodes_devices_and_slot() {
        let pkt = build_media_query(
            DeviceNumber(1),
            DeviceNumber(4),
            TrackSourceSlot::SdSlot,
        );

        assert_eq!(pkt[0x21], 1); // source
        assert_eq!(pkt[0x22], 4); // target
        assert_eq!(pkt[0x23], 2); // SdSlot
    }

    // -- read_utf16be_name edge cases ----------------------------------------

    #[test]
    fn utf16be_all_nulls_returns_empty() {
        let data = vec![0u8; 64];
        assert_eq!(read_utf16be_name(&data, 0, 64), "");
    }

    #[test]
    fn utf16be_with_non_ascii() {
        // "Ü" is U+00DC → 0x00 0xDC in UTF-16BE
        let mut data = vec![0u8; 8];
        data[0] = 0x00;
        data[1] = 0xDC; // Ü
        data[2] = 0x00;
        data[3] = 0x53; // S
        // null terminator at [4..5]
        assert_eq!(read_utf16be_name(&data, 0, 8), "ÜS");
    }

    // -- slot_to_u8 round-trip -----------------------------------------------

    #[test]
    fn slot_round_trip() {
        let cases: &[(u8, TrackSourceSlot)] = &[
            (0, TrackSourceSlot::NoTrack),
            (1, TrackSourceSlot::CdSlot),
            (2, TrackSourceSlot::SdSlot),
            (3, TrackSourceSlot::UsbSlot),
            (4, TrackSourceSlot::Collection),
            (7, TrackSourceSlot::Usb2Slot),
            (99, TrackSourceSlot::Unknown(99)),
        ];
        for &(byte, slot) in cases {
            assert_eq!(slot_to_u8(slot), byte);
            assert_eq!(TrackSourceSlot::from(byte), slot);
        }
    }

    // -- New MediaDetails fields tests ---------------------------------------

    #[test]
    fn parse_creation_date() {
        let mut pkt = make_media_details_packet(
            1, 3, 3, &ascii_to_utf16("USB"), 100, 5, 0, true,
            1_000_000, 500_000,
        );
        // Write a UTF-16BE creation date at CREATION_DATE_OFFSET with null terminator
        let date_units = ascii_to_utf16("2024-03-15");
        for (i, &unit) in date_units.iter().enumerate() {
            let off = CREATION_DATE_OFFSET + i * 2;
            if off + 1 < pkt.len() {
                let be = unit.to_be_bytes();
                pkt[off] = be[0];
                pkt[off + 1] = be[1];
            }
        }
        // Null-terminate
        let term_off = CREATION_DATE_OFFSET + date_units.len() * 2;
        if term_off + 1 < pkt.len() {
            pkt[term_off] = 0;
            pkt[term_off + 1] = 0;
        }
        let md = parse_media_details(&pkt).unwrap();
        assert_eq!(md.creation_date, "2024-03-15");
    }

    #[test]
    fn parse_creation_date_empty() {
        // Default packet has zeros at CREATION_DATE_OFFSET → empty string
        let pkt = make_media_details_packet(
            1, 3, 3, &[], 0, 0, 0, false, 0, 0,
        );
        let md = parse_media_details(&pkt).unwrap();
        assert_eq!(md.creation_date, "");
    }

    #[test]
    fn parse_has_my_settings_present() {
        // Make a packet large enough to contain HAS_MY_SETTINGS_OFFSET
        let base = make_media_details_packet(1, 3, 3, &[], 0, 0, 0, false, 0, 0);
        let mut pkt = vec![0u8; HAS_MY_SETTINGS_OFFSET + 1];
        pkt[..base.len()].copy_from_slice(&base);
        pkt[HAS_MY_SETTINGS_OFFSET] = 1;
        let md = parse_media_details(&pkt).unwrap();
        assert_eq!(md.has_my_settings, Some(true));
    }

    #[test]
    fn parse_has_my_settings_false() {
        let base = make_media_details_packet(1, 3, 3, &[], 0, 0, 0, false, 0, 0);
        let mut pkt = vec![0u8; HAS_MY_SETTINGS_OFFSET + 1];
        pkt[..base.len()].copy_from_slice(&base);
        pkt[HAS_MY_SETTINGS_OFFSET] = 0;
        let md = parse_media_details(&pkt).unwrap();
        assert_eq!(md.has_my_settings, Some(false));
    }

    #[test]
    fn parse_has_my_settings_absent_in_short_packet() {
        // MIN_MEDIA_DETAILS_SIZE = 0x84 which is < 0xab + 1
        let pkt = make_media_details_packet(1, 3, 3, &[], 0, 0, 0, false, 0, 0);
        let md = parse_media_details(&pkt).unwrap();
        assert!(md.has_my_settings.is_none());
    }

    #[test]
    fn parse_min_size_present() {
        let base = make_media_details_packet(1, 3, 3, &[], 0, 0, 0, false, 0, 0);
        let mut pkt = vec![0u8; MIN_SIZE_OFFSET + 8];
        pkt[..base.len()].copy_from_slice(&base);
        let min_val: u64 = 4_000_000_000;
        pkt[MIN_SIZE_OFFSET..MIN_SIZE_OFFSET + 8].copy_from_slice(&min_val.to_be_bytes());
        let md = parse_media_details(&pkt).unwrap();
        assert_eq!(md.min_size, Some(min_val));
    }

    #[test]
    fn parse_min_size_absent_in_short_packet() {
        let pkt = make_media_details_packet(1, 3, 3, &[], 0, 0, 0, false, 0, 0);
        let md = parse_media_details(&pkt).unwrap();
        assert!(md.min_size.is_none());
    }
}
