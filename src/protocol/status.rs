use std::time::Instant;

use crate::device::types::*;
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{self, PacketType, STATUS_PORT};
use crate::util::{bytes_to_number, read_device_name};

// Minimum packet sizes
const MIN_CDJ_STATUS_LEN: usize = 0xCC;
const MIN_MIXER_STATUS_LEN: usize = 0x38;

// CDJ-3000 extended packet threshold (loop fields available)
const CDJ_LOOP_THRESHOLD: usize = 0x1CA;

// -----------------------------------------------------------------------
// CDJ status field offsets (verified against CdjStatus.java)
// -----------------------------------------------------------------------
const NAME_OFFSET: usize = 0x0b;
const NAME_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x21;
const DEVICE_TYPE_OFFSET: usize = 0x23;
const TRACK_SOURCE_PLAYER_OFFSET: usize = 0x28;
const TRACK_SOURCE_SLOT_OFFSET: usize = 0x29;
const TRACK_TYPE_OFFSET: usize = 0x2a;
const REKORDBOX_ID_OFFSET: usize = 0x2c;
const FIRMWARE_OFFSET: usize = 0x7c;
const FIRMWARE_LEN: usize = 4;
const PLAY_STATE_OFFSET: usize = 0x7b;
const SYNC_NUMBER_OFFSET: usize = 0x84;
const FLAGS_OFFSET: usize = 0x89;
const PLAY_STATE_2_OFFSET: usize = 0x8b;
const PLAY_STATE_3_OFFSET: usize = 0x9d;
/// Pitch (3 bytes at 0x8d — "Pitch₁" in protocol docs)
const PITCH_OFFSET: usize = 0x8d;
const PITCH_LEN: usize = 3;
/// BPM (2 bytes at 0x92, value × 100)
const BPM_OFFSET: usize = 0x92;
const MASTER_HAND_OFF_OFFSET: usize = 0x9f;
const BEAT_NUMBER_OFFSET: usize = 0xa0;
const BEAT_WITHIN_BAR_OFFSET: usize = 0xa6;
const IS_BUSY_OFFSET: usize = 0x27;
const TRACK_NUMBER_OFFSET: usize = 0x32;
const LOCAL_USB_STATE_OFFSET: usize = 0x6f;
const LOCAL_SD_STATE_OFFSET: usize = 0x73;
const LINK_MEDIA_AVAILABLE_OFFSET: usize = 0x75;
const CUE_COUNTDOWN_OFFSET: usize = 0xa4;
const PACKET_NUMBER_OFFSET: usize = 0xc8;

// CDJ-3000 loop field offsets
const LOOP_START_OFFSET: usize = 0x1b6;
const LOOP_END_OFFSET: usize = 0x1be;
const LOOP_BEATS_OFFSET: usize = 0x1c8;

// -----------------------------------------------------------------------
// Bit masks for the flags byte at 0x89 (verified against CdjStatus.java)
// -----------------------------------------------------------------------
const FLAG_BPM_SYNC: u8 = 0x02;
const FLAG_ON_AIR: u8 = 0x08;
const FLAG_SYNCED: u8 = 0x10;
const FLAG_MASTER: u8 = 0x20;
const FLAG_PLAYING: u8 = 0x40;

// -----------------------------------------------------------------------
// Mixer status field offsets (verified against MixerStatus.java)
// -----------------------------------------------------------------------
const MIXER_FLAGS_OFFSET: usize = 0x27;
const MIXER_PITCH_OFFSET: usize = 0x28;
const MIXER_BPM_OFFSET: usize = 0x2e;
const MIXER_MASTER_HAND_OFF_OFFSET: usize = 0x36;
const MIXER_BEAT_WITHIN_BAR_OFFSET: usize = 0x37;

/// Sentinel value meaning "not handing off master to anyone."
const NO_HAND_OFF: u8 = 0xFF;

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
    /// Current play state (byte at 0x7b).
    pub play_state: PlayState,
    /// Secondary play state indicating motion (byte at 0x8b).
    pub play_state_2: PlayState2,
    /// Tertiary play state indicating jog mode and direction (byte at 0x9d).
    pub play_state_3: PlayState3,
    /// Raw playing flag bit from the flags byte.
    pub is_playing_flag: bool,
    /// Whether this player is the current tempo master.
    pub is_master: bool,
    /// Whether sync mode is enabled.
    pub is_synced: bool,
    /// Whether degraded BPM-sync mode is active (jog wheel nudge while synced).
    pub is_bpm_synced: bool,
    /// Whether this channel is on-air (audible through mixer).
    pub is_on_air: bool,
    /// Current track BPM (before pitch adjustment).
    pub bpm: Bpm,
    /// Raw pitch/tempo fader value (0–2097152 range; 0x100000 = normal).
    pub pitch: Pitch,
    /// Absolute beat number within the current track, or `None` if unknown.
    pub beat_number: Option<BeatNumber>,
    /// Beat position within the current bar (1–4).
    pub beat_within_bar: u8,
    /// Firmware version string.
    pub firmware_version: String,
    /// Sync counter.
    pub sync_number: u32,
    /// Device number that master is being yielded to, if any.
    pub master_hand_off: Option<u8>,
    /// CDJ-3000 loop start position (ms × 65536 / 1000), if available.
    pub loop_start: Option<u64>,
    /// CDJ-3000 loop end position (ms × 65536 / 1000), if available.
    pub loop_end: Option<u64>,
    /// CDJ-3000 loop length in beats, if available.
    pub loop_beats: Option<u16>,
    /// Total packet length (used for pre-nexus fallback in `is_playing()`).
    pub packet_length: usize,
    /// Whether the player is currently busy (loading, etc).
    pub is_busy: bool,
    /// The track number on disc media.
    pub track_number: u16,
    /// Cue countdown (beats until next cue point, `None` = no upcoming cue).
    pub cue_countdown: Option<u16>,
    /// Packet sequence number.
    pub packet_number: u32,
    /// Raw USB slot state byte at 0x6f (0 = empty, 4 = loaded).
    pub local_usb_state: u8,
    /// Raw SD slot state byte at 0x73 (0 = empty, 4 = loaded).
    pub local_sd_state: u8,
    /// Whether link media is available from another player.
    pub link_media_available: bool,
    pub timestamp: Instant,
}

impl CdjStatus {
    /// Whether the player is actively playing audio.
    ///
    /// For nexus-era packets (≥ 0xd4 bytes) this uses the flag bit at 0x89.
    /// For pre-nexus packets the flag byte is unreliable so we fall back to
    /// checking PlayState == Playing **and** PlayState2 == Moving.
    pub fn is_playing(&self) -> bool {
        if self.packet_length >= 0xd4 {
            self.is_playing_flag
        } else {
            self.play_state == PlayState::Playing && self.play_state_2 == PlayState2::Moving
        }
    }

    /// Playing in the forward direction.
    pub fn is_playing_forwards(&self) -> bool {
        self.play_state == PlayState::Playing && self.play_state_3 != PlayState3::PausedOrReverse
    }

    /// Playing in reverse.
    pub fn is_playing_backwards(&self) -> bool {
        self.play_state == PlayState::Playing && self.play_state_3 == PlayState3::PausedOrReverse
    }

    pub fn is_looping(&self) -> bool {
        self.play_state == PlayState::Looping
    }

    pub fn is_paused(&self) -> bool {
        self.play_state == PlayState::Paused
    }

    pub fn is_cued(&self) -> bool {
        self.play_state == PlayState::Cued
    }

    pub fn is_searching(&self) -> bool {
        self.play_state == PlayState::Searching
    }

    pub fn is_at_end(&self) -> bool {
        self.play_state == PlayState::Ended
    }

    pub fn is_track_loaded(&self) -> bool {
        self.play_state != PlayState::NoTrack
    }

    /// Whether a USB drive is loaded and ready in the local slot.
    pub fn is_local_usb_loaded(&self) -> bool {
        self.local_usb_state == 4
    }

    /// Whether the local USB slot is empty.
    pub fn is_local_usb_empty(&self) -> bool {
        self.local_usb_state == 0
    }

    /// Whether an SD card is loaded and ready in the local slot.
    pub fn is_local_sd_loaded(&self) -> bool {
        self.local_sd_state == 4
    }

    /// Whether the local SD slot is empty.
    pub fn is_local_sd_empty(&self) -> bool {
        self.local_sd_state == 0
    }
}

/// Mixer status update (verified against MixerStatus.java).
#[derive(Debug, Clone)]
pub struct MixerStatus {
    pub name: String,
    pub device_number: DeviceNumber,
    /// Current BPM reported by the mixer (before pitch adjustment).
    pub bpm: Bpm,
    /// Raw pitch value (mixers typically report +0%).
    pub pitch: Pitch,
    /// Beat position within the current bar (1–4).
    pub beat_within_bar: u8,
    /// Whether the mixer is the tempo master.
    pub is_master: bool,
    /// Whether the mixer is synced.
    pub is_synced: bool,
    /// Device number that master is being yielded to, if any.
    pub master_hand_off: Option<u8>,
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
    let is_playing_flag = flags & FLAG_PLAYING != 0;
    let is_master = flags & FLAG_MASTER != 0;
    let is_synced = flags & FLAG_SYNCED != 0;
    let is_bpm_synced = flags & FLAG_BPM_SYNC != 0;
    let is_on_air = flags & FLAG_ON_AIR != 0;

    let play_state_2 = PlayState2::from(data[PLAY_STATE_2_OFFSET]);
    let play_state_3 = PlayState3::from(data[PLAY_STATE_3_OFFSET]);

    let firmware_version = read_device_name(data, FIRMWARE_OFFSET, FIRMWARE_LEN);
    let sync_number = bytes_to_number(data, SYNC_NUMBER_OFFSET, 4);

    let raw_bpm = bytes_to_number(data, BPM_OFFSET, 2);
    let bpm = Bpm(raw_bpm as f64 / 100.0);

    let raw_pitch = bytes_to_number(data, PITCH_OFFSET, PITCH_LEN) as i32;
    let pitch = Pitch(raw_pitch);

    let raw_beat = bytes_to_number(data, BEAT_NUMBER_OFFSET, 4);
    let beat_number = if raw_beat == 0xFFFFFFFF {
        None
    } else {
        Some(BeatNumber(raw_beat))
    };
    let beat_within_bar = data[BEAT_WITHIN_BAR_OFFSET];

    let hand_off_byte = data[MASTER_HAND_OFF_OFFSET];
    let master_hand_off = if hand_off_byte == NO_HAND_OFF {
        None
    } else {
        Some(hand_off_byte)
    };

    // CDJ-3000 loop fields (only present in extended packets)
    let (loop_start, loop_end, loop_beats) = if data.len() >= CDJ_LOOP_THRESHOLD {
        let ls = bytes_to_number(data, LOOP_START_OFFSET, 4) as u64 * 65536 / 1000;
        let le = bytes_to_number(data, LOOP_END_OFFSET, 4) as u64 * 65536 / 1000;
        let lb = bytes_to_number(data, LOOP_BEATS_OFFSET, 2) as u16;
        (Some(ls), Some(le), Some(lb))
    } else {
        (None, None, None)
    };

    let is_busy = data[IS_BUSY_OFFSET] != 0;
    let track_number =
        u16::from_be_bytes([data[TRACK_NUMBER_OFFSET], data[TRACK_NUMBER_OFFSET + 1]]);

    let cue_countdown = {
        let raw =
            u16::from_be_bytes([data[CUE_COUNTDOWN_OFFSET], data[CUE_COUNTDOWN_OFFSET + 1]]);
        if raw == 0x01FF { None } else { Some(raw) }
    };

    let packet_number = if data.len() >= PACKET_NUMBER_OFFSET + 4 {
        u32::from_be_bytes([
            data[PACKET_NUMBER_OFFSET],
            data[PACKET_NUMBER_OFFSET + 1],
            data[PACKET_NUMBER_OFFSET + 2],
            data[PACKET_NUMBER_OFFSET + 3],
        ])
    } else {
        0
    };

    let local_usb_state = data[LOCAL_USB_STATE_OFFSET];
    let local_sd_state = data[LOCAL_SD_STATE_OFFSET];
    let link_media_available = data[LINK_MEDIA_AVAILABLE_OFFSET] != 0;

    Ok(CdjStatus {
        name,
        device_number,
        device_type,
        track_source_player,
        track_source_slot,
        track_type,
        rekordbox_id,
        play_state,
        play_state_2,
        play_state_3,
        is_playing_flag,
        is_master,
        is_synced,
        is_bpm_synced,
        is_on_air,
        bpm,
        pitch,
        beat_number,
        beat_within_bar,
        firmware_version,
        sync_number,
        master_hand_off,
        loop_start,
        loop_end,
        loop_beats,
        packet_length: data.len(),
        is_busy,
        track_number,
        cue_countdown,
        packet_number,
        local_usb_state,
        local_sd_state,
        link_media_available,
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

    let raw_bpm = bytes_to_number(data, MIXER_BPM_OFFSET, 2);
    let bpm = Bpm(raw_bpm as f64 / 100.0);

    let raw_pitch = bytes_to_number(data, MIXER_PITCH_OFFSET, 4) as i32;
    let pitch = Pitch(raw_pitch);

    let beat_within_bar = data[MIXER_BEAT_WITHIN_BAR_OFFSET];

    let flags = data[MIXER_FLAGS_OFFSET];
    let is_master = flags & FLAG_MASTER != 0;
    let is_synced = flags & FLAG_SYNCED != 0;

    let hand_off_byte = data[MIXER_MASTER_HAND_OFF_OFFSET];
    let master_hand_off = if hand_off_byte == NO_HAND_OFF {
        None
    } else {
        Some(hand_off_byte)
    };

    Ok(MixerStatus {
        name,
        device_number,
        bpm,
        pitch,
        beat_within_bar,
        is_master,
        is_synced,
        master_hand_off,
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

    /// Build a synthetic CDJ status packet with correct offsets.
    /// Sized at 0xd4 bytes (nexus-era) so `is_playing()` uses the flag.
    fn make_cdj_packet() -> Vec<u8> {
        let mut pkt = vec![0u8; 0xd4];
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x0a; // CdjStatus type byte

        let name = b"CDJ-2000NXS2";
        pkt[NAME_OFFSET..NAME_OFFSET + name.len()].copy_from_slice(name);

        pkt[DEVICE_NUMBER_OFFSET] = 3;
        pkt[DEVICE_TYPE_OFFSET] = 1; // CDJ

        pkt[TRACK_SOURCE_PLAYER_OFFSET] = 2;
        pkt[TRACK_SOURCE_SLOT_OFFSET] = 3; // USB
        pkt[TRACK_TYPE_OFFSET] = 1; // Rekordbox

        number_to_bytes(42, &mut pkt, REKORDBOX_ID_OFFSET, 4);

        // Play state = Playing (0x03)
        pkt[PLAY_STATE_OFFSET] = 0x03;

        // Flags: playing (0x40) | synced (0x10) | on_air (0x08)
        pkt[FLAGS_OFFSET] = FLAG_PLAYING | FLAG_SYNCED | FLAG_ON_AIR;

        // Firmware "1A01"
        pkt[FIRMWARE_OFFSET] = b'1';
        pkt[FIRMWARE_OFFSET + 1] = b'A';
        pkt[FIRMWARE_OFFSET + 2] = b'0';
        pkt[FIRMWARE_OFFSET + 3] = b'1';

        // Sync number = 7
        number_to_bytes(7, &mut pkt, SYNC_NUMBER_OFFSET, 4);

        // BPM = 12800 → 128.00 BPM
        number_to_bytes(12800, &mut pkt, BPM_OFFSET, 2);

        // Pitch (3 bytes at 0x8d) = 0x100000 (normal speed)
        let pitch_bytes = 0x100000u32.to_be_bytes();
        pkt[PITCH_OFFSET..PITCH_OFFSET + 3].copy_from_slice(&pitch_bytes[1..4]);

        // Beat number = 97
        number_to_bytes(97, &mut pkt, BEAT_NUMBER_OFFSET, 4);

        // Beat within bar = 2
        pkt[BEAT_WITHIN_BAR_OFFSET] = 2;

        // Master hand-off = none (0xFF)
        pkt[MASTER_HAND_OFF_OFFSET] = NO_HAND_OFF;

        // Play state 2 = Moving (0x6a)
        pkt[PLAY_STATE_2_OFFSET] = 0x6a;
        // Play state 3 = ForwardCdj (0x0d)
        pkt[PLAY_STATE_3_OFFSET] = 0x0d;

        // cue_countdown = sentinel (no upcoming cue)
        pkt[CUE_COUNTDOWN_OFFSET] = 0x01;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0xFF;

        pkt
    }

    /// Build a synthetic mixer status packet (type 0x29).
    fn make_mixer_packet() -> Vec<u8> {
        let mut pkt = vec![0u8; MIN_MIXER_STATUS_LEN];
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x29; // MixerStatus type byte

        let name = b"DJM-A9";
        pkt[NAME_OFFSET..NAME_OFFSET + name.len()].copy_from_slice(name);

        pkt[DEVICE_NUMBER_OFFSET] = 33;

        // Flags: master (0x20) | synced (0x10)
        pkt[MIXER_FLAGS_OFFSET] = FLAG_MASTER | FLAG_SYNCED;

        // Pitch = 0x100000 (normal, 4 bytes)
        let pitch_bytes = 0x100000u32.to_be_bytes();
        pkt[MIXER_PITCH_OFFSET..MIXER_PITCH_OFFSET + 4].copy_from_slice(&pitch_bytes);

        // BPM = 12800 → 128.00
        number_to_bytes(12800, &mut pkt, MIXER_BPM_OFFSET, 2);

        // Master hand-off = none
        pkt[MIXER_MASTER_HAND_OFF_OFFSET] = NO_HAND_OFF;

        // Beat within bar = 3
        pkt[MIXER_BEAT_WITHIN_BAR_OFFSET] = 3;

        pkt
    }

    // -- CDJ status tests --

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
        assert_eq!(s.beat_number, Some(BeatNumber(97)));
        assert_eq!(s.beat_within_bar, 2);
        assert_eq!(s.firmware_version, "1A01");
        assert_eq!(s.sync_number, 7);
        assert!(s.master_hand_off.is_none());
    }

    #[test]
    fn cdj_status_flags_playing_synced_onair() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();

        assert!(s.is_playing());
        assert!(!s.is_master);
        assert!(s.is_synced);
        assert!(s.is_on_air);
        assert!(!s.is_bpm_synced);
    }

    #[test]
    fn cdj_status_flags_master_only() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_MASTER;

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_playing());
        assert!(s.is_master);
        assert!(!s.is_synced);
        assert!(!s.is_on_air);
    }

    #[test]
    fn cdj_status_flags_all_set() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_PLAYING | FLAG_MASTER | FLAG_SYNCED | FLAG_ON_AIR | FLAG_BPM_SYNC;

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing());
        assert!(s.is_master);
        assert!(s.is_synced);
        assert!(s.is_on_air);
        assert!(s.is_bpm_synced);
    }

    #[test]
    fn cdj_status_unknown_beat_number() {
        let mut pkt = make_cdj_packet();
        number_to_bytes(0xFFFFFFFF, &mut pkt, BEAT_NUMBER_OFFSET, 4);

        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.beat_number.is_none());
    }

    #[test]
    fn cdj_status_master_hand_off() {
        let mut pkt = make_cdj_packet();
        pkt[MASTER_HAND_OFF_OFFSET] = 2;

        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.master_hand_off, Some(2));
    }

    #[test]
    fn cdj_status_no_loop_in_standard_packet() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.loop_start.is_none());
        assert!(s.loop_end.is_none());
        assert!(s.loop_beats.is_none());
    }

    #[test]
    fn cdj_status_loop_in_extended_packet() {
        let mut pkt = vec![0u8; CDJ_LOOP_THRESHOLD];
        pkt[..MIN_CDJ_STATUS_LEN].copy_from_slice(&make_cdj_packet());

        // Loop start raw = 1000, loop end raw = 2000, loop beats = 4
        number_to_bytes(1000, &mut pkt, LOOP_START_OFFSET, 4);
        number_to_bytes(2000, &mut pkt, LOOP_END_OFFSET, 4);
        number_to_bytes(4, &mut pkt, LOOP_BEATS_OFFSET, 2);

        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.loop_start, Some(1000 * 65536 / 1000));
        assert_eq!(s.loop_end, Some(2000 * 65536 / 1000));
        assert_eq!(s.loop_beats, Some(4));
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

    // -- Mixer status tests --

    #[test]
    fn mixer_status_fields() {
        let pkt = make_mixer_packet();
        let s = parse_mixer_status(&pkt).unwrap();

        assert_eq!(s.name, "DJM-A9");
        assert_eq!(s.device_number, DeviceNumber(33));
        assert!((s.bpm.0 - 128.0).abs() < f64::EPSILON);
        assert_eq!(s.pitch, Pitch(0x100000));
        assert_eq!(s.beat_within_bar, 3);
        assert!(s.is_master);
        assert!(s.is_synced);
        assert!(s.master_hand_off.is_none());
    }

    #[test]
    fn mixer_status_hand_off() {
        let mut pkt = make_mixer_packet();
        pkt[MIXER_MASTER_HAND_OFF_OFFSET] = 1;

        let s = parse_mixer_status(&pkt).unwrap();
        assert_eq!(s.master_hand_off, Some(1));
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

    // -- DeviceUpdate dispatch --

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
        pkt[0x0a] = 0xFF;
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

    // -- PlayState2 conversion tests --

    #[test]
    fn play_state_2_moving_variants() {
        assert_eq!(PlayState2::from(0x6a), PlayState2::Moving);
        assert_eq!(PlayState2::from(0x7a), PlayState2::Moving);
        assert_eq!(PlayState2::from(0xfa), PlayState2::Moving);
    }

    #[test]
    fn play_state_2_stopped_variants() {
        assert_eq!(PlayState2::from(0x6e), PlayState2::Stopped);
        assert_eq!(PlayState2::from(0x7e), PlayState2::Stopped);
        assert_eq!(PlayState2::from(0xfe), PlayState2::Stopped);
    }

    #[test]
    fn play_state_2_unknown() {
        assert_eq!(PlayState2::from(0x01), PlayState2::Unknown(0x01));
    }

    // -- PlayState3 conversion tests --

    #[test]
    fn play_state_3_known_values() {
        assert_eq!(PlayState3::from(0x00), PlayState3::NoTrack);
        assert_eq!(PlayState3::from(0x01), PlayState3::PausedOrReverse);
        assert_eq!(PlayState3::from(0x09), PlayState3::ForwardVinyl);
        assert_eq!(PlayState3::from(0x0d), PlayState3::ForwardCdj);
    }

    #[test]
    fn play_state_3_unknown() {
        assert_eq!(PlayState3::from(0xff), PlayState3::Unknown(0xff));
    }

    // -- is_playing() nexus vs pre-nexus --

    #[test]
    fn is_playing_nexus_uses_flag() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length >= 0xd4);
        assert!(s.is_playing_flag);
        assert!(s.is_playing());
    }

    #[test]
    fn is_playing_nexus_flag_cleared() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_MASTER; // playing flag cleared
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_playing_flag);
        assert!(!s.is_playing());
    }

    #[test]
    fn is_playing_pre_nexus_fallback_playing_moving() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0; // clear flag
        pkt[PLAY_STATE_OFFSET] = 0x03; // Playing
        pkt[PLAY_STATE_2_OFFSET] = 0x6a; // Moving
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(!s.is_playing_flag);
        assert!(s.is_playing()); // fallback: Playing + Moving
    }

    #[test]
    fn is_playing_pre_nexus_fallback_playing_stopped() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = FLAG_PLAYING; // flag set but ignored
        pkt[PLAY_STATE_OFFSET] = 0x03; // Playing
        pkt[PLAY_STATE_2_OFFSET] = 0x6e; // Stopped
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(!s.is_playing()); // fallback: Playing + Stopped → false
    }

    // -- Convenience method tests --

    #[test]
    fn cdj_status_convenience_methods() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing_forwards());
        assert!(!s.is_playing_backwards());
        assert!(!s.is_looping());
        assert!(!s.is_paused());
        assert!(!s.is_cued());
        assert!(!s.is_searching());
        assert!(!s.is_at_end());
        assert!(s.is_track_loaded());
    }

    #[test]
    fn cdj_status_playing_backwards() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_3_OFFSET] = 0x01; // PausedOrReverse
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing_backwards());
        assert!(!s.is_playing_forwards());
    }

    #[test]
    fn cdj_status_looping() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x04;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_looping());
    }

    #[test]
    fn cdj_status_paused() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x05;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_paused());
    }

    #[test]
    fn cdj_status_cued() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x06;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_cued());
    }

    #[test]
    fn cdj_status_searching() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x09;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_searching());
    }

    #[test]
    fn cdj_status_at_end() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x11;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_at_end());
    }

    #[test]
    fn cdj_status_no_track_loaded() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_OFFSET] = 0x00;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_track_loaded());
    }

    #[test]
    fn cdj_status_play_state_2_and_3() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.play_state_2, PlayState2::Moving);
        assert_eq!(s.play_state_3, PlayState3::ForwardCdj);
    }
}
