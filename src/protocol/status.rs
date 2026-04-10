use std::time::Instant;

use crate::device::types::*;
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{self, PacketType, STATUS_PORT};
use crate::util::{bytes_to_number, read_device_name};

// Minimum packet sizes
const MIN_CDJ_STATUS_LEN: usize = 0xCC;
const MIN_MIXER_STATUS_LEN: usize = 0x38;

// CDJ status field offsets
const NAME_OFFSET: usize = 0x0c;
const NAME_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x21;
const DEVICE_TYPE_OFFSET: usize = 0x23;
const TRACK_SOURCE_PLAYER_OFFSET: usize = 0x27;
const TRACK_SOURCE_SLOT_OFFSET: usize = 0x28;
const TRACK_TYPE_OFFSET: usize = 0x29;
const REKORDBOX_ID_OFFSET: usize = 0x2c;
const PLAY_STATE_OFFSET: usize = 0x7b;
const FLAGS_OFFSET: usize = 0x89;
const FIRMWARE_OFFSET: usize = 0x8b;
const FIRMWARE_LEN: usize = 2;
const ON_AIR_OFFSET: usize = 0x8d;
const SYNC_NUMBER_OFFSET: usize = 0x92;
const BPM_OFFSET: usize = 0x98;
const PITCH_OFFSET: usize = 0x9c;
const BEAT_NUMBER_OFFSET: usize = 0xa6;
const BEAT_WITHIN_BAR_OFFSET: usize = 0xaa;

// Bit masks for the flags byte at 0x89
const FLAG_PLAYING: u8 = 0x20;
const FLAG_MASTER: u8 = 0x40;
const FLAG_SYNCED: u8 = 0x08;

// Mixer on-air channel data starts at 0x27, one byte per channel
const MIXER_ON_AIR_OFFSET: usize = 0x27;
const MIXER_ON_AIR_COUNT: usize = 4;

/// Full CDJ status update parsed from a status packet.
#[derive(Debug, Clone)]
pub struct CdjStatus {
    pub name: String,
    pub device_number: DeviceNumber,
    pub device_type: DeviceType,
    /// Which player the current track was loaded from.
    pub track_source_player: DeviceNumber,
    /// Which slot on the source player (USB, SD, etc.).
    pub track_source_slot: TrackSourceSlot,
    /// Type of track loaded.
    pub track_type: TrackType,
    /// Rekordbox database ID of the loaded track, or 0 if none.
    pub rekordbox_id: u32,
    /// Current play state.
    pub play_state: PlayState,
    /// Whether the player is actively playing audio.
    pub is_playing: bool,
    /// Whether this player is the current tempo master.
    pub is_master: bool,
    /// Whether sync mode is enabled.
    pub is_synced: bool,
    /// Whether this channel is on-air (audible through mixer).
    pub is_on_air: bool,
    /// Current BPM (already pitch-adjusted from the device).
    pub bpm: Bpm,
    /// Raw pitch/tempo fader value.
    pub pitch: Pitch,
    /// Absolute beat number within the current track.
    pub beat_number: BeatNumber,
    /// Beat position within the current bar (1-4).
    pub beat_within_bar: u8,
    /// Firmware version string.
    pub firmware_version: String,
    /// Sync counter/number.
    pub sync_number: u16,
    /// When this status was received.
    pub timestamp: Instant,
}

/// Mixer status update.
#[derive(Debug, Clone)]
pub struct MixerStatus {
    pub name: String,
    pub device_number: DeviceNumber,
    /// Per-channel on-air status (indexed by channel 0..N).
    pub channels_on_air: Vec<bool>,
    pub timestamp: Instant,
}

/// Unified device update enum.
#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    Cdj(CdjStatus),
    Mixer(MixerStatus),
}

/// Parse a CDJ status packet.
pub fn parse_cdj_status(data: &[u8]) -> Result<CdjStatus> {
    if data.len() < MIN_CDJ_STATUS_LEN {
        return Err(ProDjLinkError::PacketTooShort {
            expected: MIN_CDJ_STATUS_LEN,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, NAME_OFFSET, NAME_LEN);
    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);
    let device_type = DeviceType::from(data[DEVICE_TYPE_OFFSET]);
    let track_source_player = DeviceNumber::from(data[TRACK_SOURCE_PLAYER_OFFSET]);
    let track_source_slot = TrackSourceSlot::from(data[TRACK_SOURCE_SLOT_OFFSET]);
    let track_type = TrackType::from(data[TRACK_TYPE_OFFSET]);
    let rekordbox_id = bytes_to_number(data, REKORDBOX_ID_OFFSET, 4);
    let play_state = PlayState::from(data[PLAY_STATE_OFFSET]);

    let flags = data[FLAGS_OFFSET];
    let is_playing = flags & FLAG_PLAYING != 0;
    let is_master = flags & FLAG_MASTER != 0;
    let is_synced = flags & FLAG_SYNCED != 0;
    let is_on_air = data[ON_AIR_OFFSET] != 0;

    let firmware_version = read_device_name(data, FIRMWARE_OFFSET, FIRMWARE_LEN);
    let sync_number = bytes_to_number(data, SYNC_NUMBER_OFFSET, 2) as u16;

    let raw_bpm = bytes_to_number(data, BPM_OFFSET, 2);
    let bpm = Bpm(raw_bpm as f64 / 100.0);

    let raw_pitch = bytes_to_number(data, PITCH_OFFSET, 4) as i32;
    let pitch = Pitch(raw_pitch);

    let beat_number = BeatNumber(bytes_to_number(data, BEAT_NUMBER_OFFSET, 4));
    let beat_within_bar = data[BEAT_WITHIN_BAR_OFFSET];

    Ok(CdjStatus {
        name,
        device_number,
        device_type,
        track_source_player,
        track_source_slot,
        track_type,
        rekordbox_id,
        play_state,
        is_playing,
        is_master,
        is_synced,
        is_on_air,
        bpm,
        pitch,
        beat_number,
        beat_within_bar,
        firmware_version,
        sync_number,
        timestamp: Instant::now(),
    })
}

/// Parse a mixer status packet.
pub fn parse_mixer_status(data: &[u8]) -> Result<MixerStatus> {
    if data.len() < MIN_MIXER_STATUS_LEN {
        return Err(ProDjLinkError::PacketTooShort {
            expected: MIN_MIXER_STATUS_LEN,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, NAME_OFFSET, NAME_LEN);
    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);

    let available = (data.len() - MIXER_ON_AIR_OFFSET).min(MIXER_ON_AIR_COUNT);
    let channels_on_air = (0..available)
        .map(|i| data[MIXER_ON_AIR_OFFSET + i] != 0)
        .collect();

    Ok(MixerStatus {
        name,
        device_number,
        channels_on_air,
        timestamp: Instant::now(),
    })
}

/// Parse any status packet, returning a [`DeviceUpdate`].
pub fn parse_status(data: &[u8]) -> Result<DeviceUpdate> {
    let ptype = header::parse_header_on_port(data, STATUS_PORT)?;
    match ptype {
        PacketType::CdjStatus => Ok(DeviceUpdate::Cdj(parse_cdj_status(data)?)),
        PacketType::MixerStatus => Ok(DeviceUpdate::Mixer(parse_mixer_status(data)?)),
        _ => Err(ProDjLinkError::Parse(format!(
            "unexpected packet type on status port: {:?}",
            ptype
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::header::MAGIC_HEADER;
    use crate::util::number_to_bytes;

    /// Build a synthetic CDJ status packet (0x0a on port 50002) with all
    /// fields set to known test values.
    fn make_cdj_packet() -> Vec<u8> {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];

        // Magic header + type byte
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x0a; // CdjStatus type byte

        // Device name "CDJ-2000NXS2"
        let name = b"CDJ-2000NXS2";
        pkt[NAME_OFFSET..NAME_OFFSET + name.len()].copy_from_slice(name);

        // Device number 3
        pkt[DEVICE_NUMBER_OFFSET] = 3;

        // Device type: CDJ = 1
        pkt[DEVICE_TYPE_OFFSET] = 1;

        // Track source: player 2, USB slot (3), Rekordbox track (1)
        pkt[TRACK_SOURCE_PLAYER_OFFSET] = 2;
        pkt[TRACK_SOURCE_SLOT_OFFSET] = 3; // USB
        pkt[TRACK_TYPE_OFFSET] = 1; // Rekordbox

        // Rekordbox track ID = 42
        number_to_bytes(42, &mut pkt, REKORDBOX_ID_OFFSET, 4);

        // Play state = Playing (0x04)
        pkt[PLAY_STATE_OFFSET] = 0x04;

        // Flags: playing (0x20) | synced (0x08) = 0x28
        pkt[FLAGS_OFFSET] = FLAG_PLAYING | FLAG_SYNCED;

        // On-air
        pkt[ON_AIR_OFFSET] = 0x01;

        // Firmware "1A"
        pkt[FIRMWARE_OFFSET] = b'1';
        pkt[FIRMWARE_OFFSET + 1] = b'A';

        // Sync number = 7
        number_to_bytes(7, &mut pkt, SYNC_NUMBER_OFFSET, 2);

        // BPM = 12800 → 128.00 BPM
        number_to_bytes(12800, &mut pkt, BPM_OFFSET, 2);

        // Pitch = PITCH_NORMAL (0x100000 = no adjustment)
        number_to_bytes(0x100000, &mut pkt, PITCH_OFFSET, 4);

        // Beat number = 97
        number_to_bytes(97, &mut pkt, BEAT_NUMBER_OFFSET, 4);

        // Beat within bar = 2
        pkt[BEAT_WITHIN_BAR_OFFSET] = 2;

        pkt
    }

    /// Build a synthetic mixer status packet (type 0x29).
    fn make_mixer_packet() -> Vec<u8> {
        let mut pkt = vec![0u8; MIN_MIXER_STATUS_LEN];

        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x29; // MixerStatus type byte

        let name = b"DJM-900NXS2";
        pkt[NAME_OFFSET..NAME_OFFSET + name.len()].copy_from_slice(name);

        pkt[DEVICE_NUMBER_OFFSET] = 33;

        // Channels on-air: ch1=on, ch2=off, ch3=on, ch4=on
        pkt[MIXER_ON_AIR_OFFSET] = 0x01;
        pkt[MIXER_ON_AIR_OFFSET + 1] = 0x00;
        pkt[MIXER_ON_AIR_OFFSET + 2] = 0x01;
        pkt[MIXER_ON_AIR_OFFSET + 3] = 0x01;

        pkt
    }

    // -----------------------------------------------------------------------
    // CDJ status parsing
    // -----------------------------------------------------------------------

    #[test]
    fn cdj_status_fields() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();

        assert_eq!(s.name, "CDJ-2000NXS2");
        assert_eq!(s.device_number, DeviceNumber(3));
        assert_eq!(s.device_type, DeviceType::Cdj);
        assert_eq!(s.track_source_player, DeviceNumber(2));
        assert_eq!(s.track_source_slot, TrackSourceSlot::UsbSlot);
        assert_eq!(s.track_type, TrackType::Rekordbox);
        assert_eq!(s.rekordbox_id, 42);
        assert_eq!(s.play_state, PlayState::Playing);
        assert!((s.bpm.0 - 128.0).abs() < f64::EPSILON);
        assert_eq!(s.pitch, Pitch(0x100000));
        assert_eq!(s.beat_number, BeatNumber(97));
        assert_eq!(s.beat_within_bar, 2);
        assert_eq!(s.firmware_version, "1A");
        assert_eq!(s.sync_number, 7);
    }

    #[test]
    fn cdj_status_flags_playing_synced() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();

        assert!(s.is_playing);
        assert!(!s.is_master);
        assert!(s.is_synced);
        assert!(s.is_on_air);
    }

    #[test]
    fn cdj_status_flags_master_only() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_MASTER;
        pkt[ON_AIR_OFFSET] = 0x00;

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_playing);
        assert!(s.is_master);
        assert!(!s.is_synced);
        assert!(!s.is_on_air);
    }

    #[test]
    fn cdj_status_flags_all_set() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_PLAYING | FLAG_MASTER | FLAG_SYNCED;
        pkt[ON_AIR_OFFSET] = 0x01;

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing);
        assert!(s.is_master);
        assert!(s.is_synced);
        assert!(s.is_on_air);
    }

    #[test]
    fn cdj_status_flags_none_set() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = 0x00;
        pkt[ON_AIR_OFFSET] = 0x00;

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_playing);
        assert!(!s.is_master);
        assert!(!s.is_synced);
        assert!(!s.is_on_air);
    }

    #[test]
    fn cdj_status_too_short() {
        let data = vec![0u8; MIN_CDJ_STATUS_LEN - 1];
        let err = parse_cdj_status(&data).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: MIN_CDJ_STATUS_LEN,
                ..
            }
        ));
    }

    // -----------------------------------------------------------------------
    // Mixer status parsing
    // -----------------------------------------------------------------------

    #[test]
    fn mixer_status_fields() {
        let pkt = make_mixer_packet();
        let s = parse_mixer_status(&pkt).unwrap();

        assert_eq!(s.name, "DJM-900NXS2");
        assert_eq!(s.device_number, DeviceNumber(33));
        assert_eq!(s.channels_on_air, vec![true, false, true, true]);
    }

    #[test]
    fn mixer_status_too_short() {
        let data = vec![0u8; MIN_MIXER_STATUS_LEN - 1];
        let err = parse_mixer_status(&data).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: MIN_MIXER_STATUS_LEN,
                ..
            }
        ));
    }

    // -----------------------------------------------------------------------
    // DeviceUpdate dispatch via parse_status
    // -----------------------------------------------------------------------

    #[test]
    fn parse_status_dispatches_cdj() {
        let pkt = make_cdj_packet();
        let update = parse_status(&pkt).unwrap();
        assert!(matches!(update, DeviceUpdate::Cdj(_)));

        if let DeviceUpdate::Cdj(s) = update {
            assert_eq!(s.device_number, DeviceNumber(3));
        }
    }

    #[test]
    fn parse_status_dispatches_mixer() {
        let pkt = make_mixer_packet();
        let update = parse_status(&pkt).unwrap();
        assert!(matches!(update, DeviceUpdate::Mixer(_)));

        if let DeviceUpdate::Mixer(s) = update {
            assert_eq!(s.device_number, DeviceNumber(33));
        }
    }

    #[test]
    fn parse_status_rejects_unknown_type() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0xFF; // unknown type

        let err = parse_status(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::Parse(_)));
    }

    #[test]
    fn parse_status_rejects_invalid_magic() {
        let mut pkt = make_cdj_packet();
        pkt[0] = 0x00;

        let err = parse_status(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidMagic));
    }
}
