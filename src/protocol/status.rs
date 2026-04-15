use std::time::Instant;

use crate::device::types::*;
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{self, PacketType, STATUS_PORT};
use crate::util::{bytes_to_number, number_to_bytes, read_device_name};

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
const LOCAL_CD_STATE_OFFSET: usize = 0x37;
const DISC_TRACK_COUNT_OFFSET: usize = 0x46;

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
    /// Raw disc (CD) slot state byte at 0x37.
    pub local_disc_state: u8,
    /// Number of tracks on the mounted disc (or loaded playlist/menu).
    pub disc_track_count: u16,
    pub timestamp: Instant,
}

impl CdjStatus {
    /// Whether the player is actively playing audio.
    ///
    /// For nexus-era packets (≥ 0xd4 bytes) this uses the flag bit at 0x89.
    /// For pre-nexus packets the flag byte is unreliable so we fall back to
    /// checking PlayState == Playing **and** PlayState2 is moving.
    pub fn is_playing(&self) -> bool {
        if self.packet_length >= 0xd4 {
            self.is_playing_flag
        } else {
            // Pre-nexus: playing, looping, or searching while moving
            match self.play_state {
                PlayState::Playing | PlayState::Looping | PlayState::Searching => {
                    self.play_state_2.is_moving()
                }
                _ => false,
            }
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

    /// Whether this status packet appears to originate from an Opus Quad.
    ///
    /// Detection is based on the device name containing "OPUS-QUAD".
    pub fn is_opus_quad(&self) -> bool {
        self.name.contains("OPUS-QUAD")
    }

    /// Whether the status flags in this packet are reliable.
    ///
    /// The Opus Quad has a firmware bug where status flags are sometimes
    /// "replayed" from a previous state.  We detect this by checking whether
    /// the firmware-version bytes (0x7c–0x7f) look like an Opus Quad *and*
    /// the packet counter has wrapped in an unexpected way (low-order byte
    /// is zero while the overall counter is non-zero, which indicates a
    /// replayed packet).
    pub fn are_flags_reliable(&self) -> bool {
        if !self.is_opus_quad() {
            return true;
        }
        // Heuristic: a replayed packet has a non-zero counter whose low byte
        // is zero, indicating the counter jumped rather than incremented
        // normally.
        if self.packet_number != 0 && (self.packet_number & 0xFF) == 0 {
            return false;
        }
        true
    }

    /// Playing with the jog wheel in vinyl mode.
    pub fn is_playing_vinyl_mode(&self) -> bool {
        self.play_state_3 == PlayState3::ForwardVinyl
    }

    /// Playing with the jog wheel in CDJ mode.
    pub fn is_playing_cdj_mode(&self) -> bool {
        self.play_state_3 == PlayState3::ForwardCdj
    }

    /// Actual tempo after pitch adjustment (BPM × pitch multiplier).
    pub fn effective_tempo(&self) -> f64 {
        self.bpm.0 * self.pitch.to_multiplier()
    }

    /// Format the cue countdown the way the CDJ display shows it.
    ///
    /// Returns `"--.-"` when no countdown is active, `"00.0"` when sitting
    /// on the cue point, or `"BB.b"` (bars.beats) for values 1–256.
    pub fn format_cue_countdown(&self) -> String {
        match self.cue_countdown {
            None => "--.-".to_string(),
            Some(0) => "00.0".to_string(),
            Some(count @ 1..=256) => {
                let bars = (count - 1) / 4;
                let beats = ((count - 1) % 4) + 1;
                format!("{:02}.{}", bars, beats)
            }
            Some(_) => "??.?".to_string(),
        }
    }

    /// Whether this packet is large enough to contain CDJ-3000 loop data.
    pub fn can_report_looping(&self) -> bool {
        self.packet_length >= CDJ_LOOP_THRESHOLD
    }

    /// Active loop length in beats, if the packet supports it and a loop is active.
    pub fn active_loop_beats(&self) -> Option<u16> {
        if self.can_report_looping() {
            match self.loop_beats {
                Some(b) if b != 0 => Some(b),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Whether the local USB slot is currently unloading.
    pub fn is_local_usb_unloading(&self) -> bool {
        self.local_usb_state == 2
    }

    /// Whether the local SD slot is currently unloading.
    pub fn is_local_sd_unloading(&self) -> bool {
        self.local_sd_state == 2
    }

    /// Whether the disc slot is empty (or the CD drive has powered off).
    ///
    /// Returns `true` unless the disc state indicates an actively mounted disc.
    pub fn is_disc_slot_empty(&self) -> bool {
        self.local_disc_state != 0x1e && self.local_disc_state != 0x11
    }

    /// Whether the CD drive has powered down due to prolonged disuse.
    pub fn is_disc_slot_asleep(&self) -> bool {
        self.local_disc_state == 1
    }

    /// Alias for `is_master` matching the Java `isTempoMaster()` name.
    pub fn is_tempo_master(&self) -> bool {
        self.is_master
    }

    /// Whether `beat_within_bar` has musical significance.
    ///
    /// True when a nexus-era player is playing an analysed rekordbox track.
    pub fn is_beat_within_bar_meaningful(&self) -> bool {
        self.beat_within_bar > 0 && self.beat_within_bar <= 4
    }

    /// Whether the player is in BPM-only sync mode (jog wheel nudge while synced).
    pub fn is_bpm_only_synced(&self) -> bool {
        self.is_bpm_synced && !self.is_synced
    }

    /// The device number that master is being yielded to, if any.
    pub fn master_yielding_to(&self) -> Option<DeviceNumber> {
        self.master_hand_off.map(DeviceNumber::from)
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

impl MixerStatus {
    /// Actual tempo after pitch adjustment (BPM × pitch multiplier).
    pub fn effective_tempo(&self) -> f64 {
        self.bpm.0 * self.pitch.to_multiplier()
    }

    /// Alias for `is_master` matching the Java `isTempoMaster()` name.
    pub fn is_tempo_master(&self) -> bool {
        self.is_master
    }

    /// Whether `beat_within_bar` has musical significance.
    ///
    /// Always returns `false` for mixers — they do not track beat positions
    /// within a bar in a meaningful way (matches Java MixerStatus behaviour).
    pub fn is_beat_within_bar_meaningful(&self) -> bool {
        false
    }

    /// The device number that master is being yielded to, if any.
    pub fn master_yielding_to(&self) -> Option<DeviceNumber> {
        self.master_hand_off.map(DeviceNumber::from)
    }
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
        let raw = u16::from_be_bytes([data[CUE_COUNTDOWN_OFFSET], data[CUE_COUNTDOWN_OFFSET + 1]]);
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
    let local_disc_state = data[LOCAL_CD_STATE_OFFSET];
    let disc_track_count = u16::from_be_bytes([
        data[DISC_TRACK_COUNT_OFFSET],
        data[DISC_TRACK_COUNT_OFFSET + 1],
    ]);

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
        local_disc_state,
        disc_track_count,
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

// -----------------------------------------------------------------------
// CDJ status packet builder
// -----------------------------------------------------------------------

/// Flags that can be set in the CDJ status flags byte at offset 0x89.
#[derive(Debug, Clone, Copy, Default)]
pub struct CdjStatusFlags {
    pub playing: bool,
    pub master: bool,
    pub synced: bool,
    pub on_air: bool,
    pub bpm_sync: bool,
}

impl CdjStatusFlags {
    fn to_byte(self) -> u8 {
        let mut f = 0u8;
        if self.playing {
            f |= FLAG_PLAYING;
        }
        if self.master {
            f |= FLAG_MASTER;
        }
        if self.synced {
            f |= FLAG_SYNCED;
        }
        if self.on_air {
            f |= FLAG_ON_AIR;
        }
        if self.bpm_sync {
            f |= FLAG_BPM_SYNC;
        }
        f
    }

    /// Build flags from a raw byte.
    pub fn from_byte(b: u8) -> Self {
        Self {
            playing: b & FLAG_PLAYING != 0,
            master: b & FLAG_MASTER != 0,
            synced: b & FLAG_SYNCED != 0,
            on_air: b & FLAG_ON_AIR != 0,
            bpm_sync: b & FLAG_BPM_SYNC != 0,
        }
    }
}

/// Parameters for building a CDJ status packet.
#[derive(Debug, Clone)]
pub struct CdjStatusBuilder {
    pub device_name: String,
    pub device_number: DeviceNumber,
    pub flags: CdjStatusFlags,
    /// Track BPM (before pitch). Encoded as `bpm * 100` in the packet.
    pub bpm: Bpm,
    /// Raw pitch value (0x100000 = normal speed).
    pub pitch: Pitch,
    /// Beat number within the track (0xFFFFFFFF = unknown).
    pub beat_number: Option<u32>,
    /// Beat within bar (1–4).
    pub beat_within_bar: u8,
    /// Device number being yielded to, or `None` (encoded as 0xFF).
    pub master_hand_off: Option<u8>,
    /// Packet sequence number.
    pub packet_number: u32,
}

impl Default for CdjStatusBuilder {
    fn default() -> Self {
        Self {
            device_name: "prodjlink-rs".to_string(),
            device_number: DeviceNumber(5),
            flags: CdjStatusFlags::default(),
            bpm: Bpm(0.0),
            pitch: Pitch(0x100000),
            beat_number: None,
            beat_within_bar: 1,
            master_hand_off: None,
            packet_number: 0,
        }
    }
}

/// Build a CdjStatus packet (type 0x0a) suitable for broadcast on port 50002.
///
/// The returned packet is at least 0xCC bytes (nexus-era minimum) and can be
/// parsed back with [`parse_cdj_status`].
pub fn build_cdj_status(params: &CdjStatusBuilder) -> Vec<u8> {
    // Use 0xd4 so parsers take the nexus-era flag path in is_playing().
    let mut pkt = vec![0u8; 0xd4];

    // Magic header
    pkt[..10].copy_from_slice(&header::MAGIC_HEADER);

    // Packet type = CdjStatus (0x0a on status port)
    pkt[0x0a] = 0x0a;

    // Device name (null-padded to 20 bytes)
    let name_bytes = params.device_name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // Device number and type
    pkt[DEVICE_NUMBER_OFFSET] = params.device_number.0;
    pkt[DEVICE_TYPE_OFFSET] = 1; // CDJ

    // Flags byte
    pkt[FLAGS_OFFSET] = params.flags.to_byte();

    // BPM (2 bytes, value × 100)
    let bpm_raw = (params.bpm.0 * 100.0) as u32;
    number_to_bytes(bpm_raw, &mut pkt, BPM_OFFSET, 2);

    // Pitch (3 bytes)
    let pitch_be = (params.pitch.0 as u32).to_be_bytes();
    pkt[PITCH_OFFSET..PITCH_OFFSET + PITCH_LEN].copy_from_slice(&pitch_be[1..4]);

    // Beat number (4 bytes, 0xFFFFFFFF = unknown)
    let beat_raw = params.beat_number.unwrap_or(0xFFFFFFFF);
    number_to_bytes(beat_raw, &mut pkt, BEAT_NUMBER_OFFSET, 4);

    // Beat within bar
    pkt[BEAT_WITHIN_BAR_OFFSET] = params.beat_within_bar;

    // Master hand-off
    pkt[MASTER_HAND_OFF_OFFSET] = params.master_hand_off.unwrap_or(NO_HAND_OFF);

    // Play state 1 — Playing (0x03) or Paused (0x05) depending on flag
    pkt[PLAY_STATE_OFFSET] = if params.flags.playing { 0x03 } else { 0x05 };

    // Play state 2 — Moving (0x6a) or Stopped (0x6e)
    pkt[PLAY_STATE_2_OFFSET] = if params.flags.playing { 0x6a } else { 0x6e };

    // Play state 3 — ForwardCdj (0x0d) if playing, PausedOrReverse (0x01) if not
    pkt[PLAY_STATE_3_OFFSET] = if params.flags.playing { 0x0d } else { 0x01 };

    // Cue countdown sentinel
    pkt[CUE_COUNTDOWN_OFFSET] = 0x01;
    pkt[CUE_COUNTDOWN_OFFSET + 1] = 0xFF;

    // Packet number (4 bytes at 0xC8)
    if pkt.len() >= PACKET_NUMBER_OFFSET + 4 {
        number_to_bytes(params.packet_number, &mut pkt, PACKET_NUMBER_OFFSET, 4);
    }

    pkt
}

/// Parameters for building a mixer status packet.
#[derive(Debug, Clone)]
pub struct MixerStatusBuilder {
    pub device_name: String,
    pub device_number: DeviceNumber,
    pub bpm: Bpm,
    pub pitch: Pitch,
    pub beat_within_bar: u8,
    pub is_master: bool,
    pub is_synced: bool,
    pub master_hand_off: Option<u8>,
}

impl Default for MixerStatusBuilder {
    fn default() -> Self {
        Self {
            device_name: "DJM-A9".to_string(),
            device_number: DeviceNumber(33),
            bpm: Bpm(0.0),
            pitch: Pitch(0x100000),
            beat_within_bar: 1,
            is_master: false,
            is_synced: false,
            master_hand_off: None,
        }
    }
}

/// Build a MixerStatus packet (type 0x29) suitable for broadcast on port 50002.
///
/// The returned packet is 0x38 bytes and can be parsed back with
/// [`parse_mixer_status`].
pub fn build_mixer_status(params: &MixerStatusBuilder) -> Vec<u8> {
    let mut pkt = vec![0u8; MIN_MIXER_STATUS_LEN];

    pkt[..10].copy_from_slice(&header::MAGIC_HEADER);
    pkt[0x0a] = 0x29; // MixerStatus type byte

    let name_bytes = params.device_name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[DEVICE_NUMBER_OFFSET] = params.device_number.0;

    let mut flags: u8 = 0;
    if params.is_master {
        flags |= FLAG_MASTER;
    }
    if params.is_synced {
        flags |= FLAG_SYNCED;
    }
    pkt[MIXER_FLAGS_OFFSET] = flags;

    let pitch_bytes = (params.pitch.0 as u32).to_be_bytes();
    pkt[MIXER_PITCH_OFFSET..MIXER_PITCH_OFFSET + 4].copy_from_slice(&pitch_bytes);

    let bpm_raw = (params.bpm.0 * 100.0) as u32;
    number_to_bytes(bpm_raw, &mut pkt, MIXER_BPM_OFFSET, 2);

    pkt[MIXER_MASTER_HAND_OFF_OFFSET] = params.master_hand_off.unwrap_or(NO_HAND_OFF);
    pkt[MIXER_BEAT_WITHIN_BAR_OFFSET] = params.beat_within_bar;

    pkt
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
        let base = make_cdj_packet();
        let mut pkt = vec![0u8; CDJ_LOOP_THRESHOLD];
        pkt[..base.len()].copy_from_slice(&base);

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
    }

    #[test]
    fn play_state_2_opus_moving() {
        assert_eq!(PlayState2::from(0xfa), PlayState2::OpusMoving);
        assert!(PlayState2::OpusMoving.is_moving());
        assert!(PlayState2::Moving.is_moving());
        assert!(!PlayState2::Stopped.is_moving());
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

    // -- New field tests --

    #[test]
    fn cdj_status_new_field_defaults() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();

        assert!(!s.is_busy);
        assert_eq!(s.track_number, 0);
        assert!(s.cue_countdown.is_none()); // sentinel 0x01FF
        assert_eq!(s.packet_number, 0);
        assert!(!s.link_media_available);
    }

    #[test]
    fn cdj_status_is_busy() {
        let mut pkt = make_cdj_packet();
        pkt[IS_BUSY_OFFSET] = 1;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_busy);
    }

    #[test]
    fn cdj_status_track_number() {
        let mut pkt = make_cdj_packet();
        pkt[TRACK_NUMBER_OFFSET] = 0x00;
        pkt[TRACK_NUMBER_OFFSET + 1] = 0x05;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.track_number, 5);
    }

    #[test]
    fn cdj_status_cue_countdown_present() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x00;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x10;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.cue_countdown, Some(16));
    }

    #[test]
    fn cdj_status_cue_countdown_sentinel() {
        let pkt = make_cdj_packet(); // sentinel already set
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.cue_countdown.is_none());
    }

    #[test]
    fn cdj_status_local_usb_loaded() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_USB_STATE_OFFSET] = 4;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_usb_loaded());
        assert!(!s.is_local_usb_empty());
    }

    #[test]
    fn cdj_status_local_usb_empty() {
        let pkt = make_cdj_packet(); // default 0
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_usb_empty());
        assert!(!s.is_local_usb_loaded());
    }

    #[test]
    fn cdj_status_local_sd_loaded() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_SD_STATE_OFFSET] = 4;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_sd_loaded());
        assert!(!s.is_local_sd_empty());
    }

    #[test]
    fn cdj_status_local_sd_empty() {
        let pkt = make_cdj_packet(); // default 0
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_sd_empty());
        assert!(!s.is_local_sd_loaded());
    }

    #[test]
    fn cdj_status_link_media_available() {
        let mut pkt = make_cdj_packet();
        pkt[LINK_MEDIA_AVAILABLE_OFFSET] = 1;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.link_media_available);
    }

    #[test]
    fn cdj_status_packet_number() {
        let mut pkt = make_cdj_packet();
        pkt[PACKET_NUMBER_OFFSET] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 1] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 2] = 0x01;
        pkt[PACKET_NUMBER_OFFSET + 3] = 0x00;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.packet_number, 256);
    }

    // -- Opus Quad status tests --

    /// Helper: make a CDJ status packet that looks like it came from an Opus Quad.
    fn make_opus_quad_cdj_packet() -> Vec<u8> {
        let mut pkt = make_cdj_packet();
        // Overwrite name with OPUS-QUAD
        let name = b"OPUS-QUAD";
        pkt[NAME_OFFSET..NAME_OFFSET + name.len()].copy_from_slice(name);
        // Zero out rest of name field
        for i in NAME_OFFSET + name.len()..NAME_OFFSET + NAME_LEN {
            pkt[i] = 0;
        }
        pkt
    }

    #[test]
    fn cdj_status_is_opus_quad_true() {
        let pkt = make_opus_quad_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_opus_quad());
    }

    #[test]
    fn cdj_status_is_opus_quad_false() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_opus_quad());
    }

    #[test]
    fn cdj_status_flags_reliable_for_normal_cdj() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.are_flags_reliable());
    }

    #[test]
    fn cdj_status_flags_reliable_for_opus_quad_normal_counter() {
        let mut pkt = make_opus_quad_cdj_packet();
        // Normal counter value: 42
        pkt[PACKET_NUMBER_OFFSET] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 1] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 2] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 3] = 42;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.are_flags_reliable());
    }

    #[test]
    fn cdj_status_flags_unreliable_for_opus_quad_replayed() {
        let mut pkt = make_opus_quad_cdj_packet();
        // Suspect counter: non-zero but low byte is 0 (e.g. 0x00000100)
        pkt[PACKET_NUMBER_OFFSET] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 1] = 0x00;
        pkt[PACKET_NUMBER_OFFSET + 2] = 0x01;
        pkt[PACKET_NUMBER_OFFSET + 3] = 0x00;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.are_flags_reliable());
    }

    #[test]
    fn cdj_status_flags_reliable_for_opus_quad_counter_zero() {
        let pkt = make_opus_quad_cdj_packet();
        // Counter = 0 is OK (initial state)
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.packet_number, 0);
        assert!(s.are_flags_reliable());
    }

    #[test]
    fn play_state_2_opus_moving_in_is_playing_fallback() {
        // Pre-nexus sized packet with OpusMoving state
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0; // clear flag
        pkt[PLAY_STATE_OFFSET] = 0x03; // Playing
        pkt[PLAY_STATE_2_OFFSET] = 0xfa; // OpusMoving
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(s.play_state_2.is_moving());
        assert!(s.is_playing());
    }

    // -- build_cdj_status tests --

    #[test]
    fn build_cdj_status_minimum_size() {
        let pkt = build_cdj_status(&CdjStatusBuilder::default());
        assert_eq!(pkt.len(), 0xd4); // nexus-era size so is_playing() uses flags
    }

    #[test]
    fn build_cdj_status_has_magic_header() {
        let pkt = build_cdj_status(&CdjStatusBuilder::default());
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
    }

    #[test]
    fn build_cdj_status_type_byte() {
        let pkt = build_cdj_status(&CdjStatusBuilder::default());
        assert_eq!(pkt[0x0a], 0x0a);
    }

    #[test]
    fn build_cdj_status_device_name() {
        let params = CdjStatusBuilder {
            device_name: "TestPlayer".to_string(),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        let name = read_device_name(&pkt, NAME_OFFSET, NAME_LEN);
        assert_eq!(name, "TestPlayer");
    }

    #[test]
    fn build_cdj_status_device_number() {
        let params = CdjStatusBuilder {
            device_number: DeviceNumber(3),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        assert_eq!(pkt[DEVICE_NUMBER_OFFSET], 3);
    }

    #[test]
    fn build_cdj_status_flags_all_set() {
        let params = CdjStatusBuilder {
            flags: CdjStatusFlags {
                playing: true,
                master: true,
                synced: true,
                on_air: true,
                bpm_sync: true,
            },
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        let flags = pkt[FLAGS_OFFSET];
        assert_ne!(flags & FLAG_PLAYING, 0);
        assert_ne!(flags & FLAG_MASTER, 0);
        assert_ne!(flags & FLAG_SYNCED, 0);
        assert_ne!(flags & FLAG_ON_AIR, 0);
        assert_ne!(flags & FLAG_BPM_SYNC, 0);
    }

    #[test]
    fn build_cdj_status_flags_master_only() {
        let params = CdjStatusBuilder {
            flags: CdjStatusFlags {
                master: true,
                ..CdjStatusFlags::default()
            },
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        assert_eq!(pkt[FLAGS_OFFSET], FLAG_MASTER);
    }

    #[test]
    fn build_cdj_status_bpm() {
        let params = CdjStatusBuilder {
            bpm: Bpm(128.0),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        let raw = bytes_to_number(&pkt, BPM_OFFSET, 2);
        assert_eq!(raw, 12800);
    }

    #[test]
    fn build_cdj_status_pitch() {
        let params = CdjStatusBuilder {
            pitch: Pitch(0x100000),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        let raw = bytes_to_number(&pkt, PITCH_OFFSET, PITCH_LEN);
        assert_eq!(raw, 0x100000);
    }

    #[test]
    fn build_cdj_status_master_hand_off_none() {
        let pkt = build_cdj_status(&CdjStatusBuilder::default());
        assert_eq!(pkt[MASTER_HAND_OFF_OFFSET], NO_HAND_OFF);
    }

    #[test]
    fn build_cdj_status_master_hand_off_yielding() {
        let params = CdjStatusBuilder {
            master_hand_off: Some(3),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        assert_eq!(pkt[MASTER_HAND_OFF_OFFSET], 3);
    }

    #[test]
    fn build_cdj_status_round_trip() {
        let params = CdjStatusBuilder {
            device_name: "RoundTrip".to_string(),
            device_number: DeviceNumber(2),
            flags: CdjStatusFlags {
                playing: true,
                master: true,
                synced: true,
                on_air: true,
                bpm_sync: false,
            },
            bpm: Bpm(140.0),
            pitch: Pitch(0x100000),
            beat_number: Some(42),
            beat_within_bar: 3,
            master_hand_off: None,
            packet_number: 99,
        };
        let pkt = build_cdj_status(&params);
        let parsed = parse_cdj_status(&pkt).unwrap();

        assert_eq!(parsed.name, "RoundTrip");
        assert_eq!(parsed.device_number, DeviceNumber(2));
        assert!(parsed.is_playing_flag);
        assert!(parsed.is_master);
        assert!(parsed.is_synced);
        assert!(parsed.is_on_air);
        assert!(!parsed.is_bpm_synced);
        assert!((parsed.bpm.0 - 140.0).abs() < f64::EPSILON);
        assert_eq!(parsed.pitch, Pitch(0x100000));
        assert_eq!(parsed.beat_number, Some(BeatNumber(42)));
        assert_eq!(parsed.beat_within_bar, 3);
        assert!(parsed.master_hand_off.is_none());
        assert_eq!(parsed.packet_number, 99);
    }

    #[test]
    fn build_cdj_status_round_trip_with_handoff() {
        let params = CdjStatusBuilder {
            master_hand_off: Some(4),
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        let parsed = parse_cdj_status(&pkt).unwrap();
        assert_eq!(parsed.master_hand_off, Some(4));
    }

    #[test]
    fn cdj_status_flags_from_byte_round_trip() {
        let flags = CdjStatusFlags {
            playing: true,
            master: false,
            synced: true,
            on_air: false,
            bpm_sync: true,
        };
        let byte = flags.to_byte();
        let back = CdjStatusFlags::from_byte(byte);
        assert_eq!(back.playing, true);
        assert_eq!(back.master, false);
        assert_eq!(back.synced, true);
        assert_eq!(back.on_air, false);
        assert_eq!(back.bpm_sync, true);
    }

    // -- New convenience method tests (CdjStatus) --

    #[test]
    fn cdj_is_playing_vinyl_mode() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_3_OFFSET] = 0x09; // ForwardVinyl
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing_vinyl_mode());
        assert!(!s.is_playing_cdj_mode());
    }

    #[test]
    fn cdj_is_playing_cdj_mode() {
        let pkt = make_cdj_packet(); // default ForwardCdj (0x0d)
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_playing_cdj_mode());
        assert!(!s.is_playing_vinyl_mode());
    }

    #[test]
    fn cdj_playing_mode_neither_when_paused() {
        let mut pkt = make_cdj_packet();
        pkt[PLAY_STATE_3_OFFSET] = 0x01; // PausedOrReverse
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_playing_vinyl_mode());
        assert!(!s.is_playing_cdj_mode());
    }

    #[test]
    fn cdj_effective_tempo_normal_pitch() {
        let pkt = make_cdj_packet(); // 128 BPM, pitch 0x100000 (1.0×)
        let s = parse_cdj_status(&pkt).unwrap();
        assert!((s.effective_tempo() - 128.0).abs() < 0.001);
    }

    #[test]
    fn cdj_effective_tempo_with_pitch_change() {
        let mut pkt = make_cdj_packet();
        let pitch_raw = (0x100000 as f64 * 1.05) as u32;
        let pitch_bytes = pitch_raw.to_be_bytes();
        pkt[PITCH_OFFSET..PITCH_OFFSET + 3].copy_from_slice(&pitch_bytes[1..4]);
        let s = parse_cdj_status(&pkt).unwrap();
        assert!((s.effective_tempo() - 134.4).abs() < 0.1);
    }

    #[test]
    fn cdj_format_cue_countdown_no_cue() {
        let pkt = make_cdj_packet(); // sentinel → None
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "--.-");
    }

    #[test]
    fn cdj_format_cue_countdown_on_cue() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x00;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x00;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "00.0");
    }

    #[test]
    fn cdj_format_cue_countdown_one_beat() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x00;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x01;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "00.1");
    }

    #[test]
    fn cdj_format_cue_countdown_256_beats() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x01;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x00;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "63.4");
    }

    #[test]
    fn cdj_format_cue_countdown_five_beats() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x00;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x05;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "01.1");
    }

    #[test]
    fn cdj_format_cue_countdown_out_of_range() {
        let mut pkt = make_cdj_packet();
        pkt[CUE_COUNTDOWN_OFFSET] = 0x01;
        pkt[CUE_COUNTDOWN_OFFSET + 1] = 0x2C; // 300
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.format_cue_countdown(), "??.?");
    }

    #[test]
    fn cdj_can_report_looping_standard() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.can_report_looping());
    }

    #[test]
    fn cdj_can_report_looping_extended() {
        let base = make_cdj_packet();
        let mut pkt = vec![0u8; CDJ_LOOP_THRESHOLD];
        pkt[..base.len()].copy_from_slice(&base);
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.can_report_looping());
    }

    #[test]
    fn cdj_active_loop_beats_not_looping() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.active_loop_beats().is_none());
    }

    #[test]
    fn cdj_active_loop_beats_zero() {
        let base = make_cdj_packet();
        let mut pkt = vec![0u8; CDJ_LOOP_THRESHOLD];
        pkt[..base.len()].copy_from_slice(&base);
        number_to_bytes(0, &mut pkt, LOOP_BEATS_OFFSET, 2);
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.active_loop_beats().is_none());
    }

    #[test]
    fn cdj_active_loop_beats_nonzero() {
        let base = make_cdj_packet();
        let mut pkt = vec![0u8; CDJ_LOOP_THRESHOLD];
        pkt[..base.len()].copy_from_slice(&base);
        number_to_bytes(8, &mut pkt, LOOP_BEATS_OFFSET, 2);
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.active_loop_beats(), Some(8));
    }

    #[test]
    fn cdj_is_local_usb_unloading() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_USB_STATE_OFFSET] = 2;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_usb_unloading());
        assert!(!s.is_local_usb_loaded());
        assert!(!s.is_local_usb_empty());
    }

    #[test]
    fn cdj_is_local_usb_not_unloading() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_local_usb_unloading());
    }

    #[test]
    fn cdj_is_local_sd_unloading() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_SD_STATE_OFFSET] = 2;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_local_sd_unloading());
        assert!(!s.is_local_sd_loaded());
        assert!(!s.is_local_sd_empty());
    }

    #[test]
    fn cdj_is_local_sd_not_unloading() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_local_sd_unloading());
    }

    #[test]
    fn cdj_disc_slot_empty_default() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_disc_slot_empty());
    }

    #[test]
    fn cdj_disc_slot_not_empty_loaded_0x1e() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_CD_STATE_OFFSET] = 0x1e;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_disc_slot_empty());
    }

    #[test]
    fn cdj_disc_slot_not_empty_0x11() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_CD_STATE_OFFSET] = 0x11;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_disc_slot_empty());
    }

    #[test]
    fn cdj_disc_slot_asleep() {
        let mut pkt = make_cdj_packet();
        pkt[LOCAL_CD_STATE_OFFSET] = 1;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_disc_slot_asleep());
        assert!(s.is_disc_slot_empty());
    }

    #[test]
    fn cdj_disc_slot_not_asleep() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_disc_slot_asleep());
    }

    #[test]
    fn cdj_disc_track_count() {
        let mut pkt = make_cdj_packet();
        pkt[DISC_TRACK_COUNT_OFFSET] = 0x00;
        pkt[DISC_TRACK_COUNT_OFFSET + 1] = 0x0A;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.disc_track_count, 10);
    }

    #[test]
    fn cdj_disc_track_count_default_zero() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.disc_track_count, 0);
    }

    #[test]
    fn cdj_is_tempo_master_true() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_MASTER;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_tempo_master());
        assert!(s.is_master);
    }

    #[test]
    fn cdj_is_tempo_master_false() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_tempo_master());
    }

    #[test]
    fn cdj_beat_within_bar_meaningful() {
        let pkt = make_cdj_packet(); // beat_within_bar = 2
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn cdj_beat_within_bar_not_meaningful_zero() {
        let mut pkt = make_cdj_packet();
        pkt[BEAT_WITHIN_BAR_OFFSET] = 0;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn cdj_beat_within_bar_not_meaningful_five() {
        let mut pkt = make_cdj_packet();
        pkt[BEAT_WITHIN_BAR_OFFSET] = 5;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn cdj_beat_within_bar_meaningful_boundary() {
        for b in 1..=4u8 {
            let mut pkt = make_cdj_packet();
            pkt[BEAT_WITHIN_BAR_OFFSET] = b;
            let s = parse_cdj_status(&pkt).unwrap();
            assert!(
                s.is_beat_within_bar_meaningful(),
                "beat {b} should be meaningful"
            );
        }
    }

    #[test]
    fn cdj_master_yielding_to_none() {
        let pkt = make_cdj_packet();
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.master_yielding_to().is_none());
    }

    #[test]
    fn cdj_master_yielding_to_some() {
        let mut pkt = make_cdj_packet();
        pkt[MASTER_HAND_OFF_OFFSET] = 2;
        let s = parse_cdj_status(&pkt).unwrap();
        assert_eq!(s.master_yielding_to(), Some(DeviceNumber(2)));
    }

    // -- New convenience method tests (MixerStatus) --

    #[test]
    fn mixer_effective_tempo_normal_pitch() {
        let pkt = make_mixer_packet();
        let s = parse_mixer_status(&pkt).unwrap();
        assert!((s.effective_tempo() - 128.0).abs() < 0.001);
    }

    #[test]
    fn mixer_effective_tempo_with_pitch_change() {
        let mut pkt = make_mixer_packet();
        let pitch_raw = (0x100000 as f64 * 1.1) as u32;
        let pitch_bytes = pitch_raw.to_be_bytes();
        pkt[MIXER_PITCH_OFFSET..MIXER_PITCH_OFFSET + 4].copy_from_slice(&pitch_bytes);
        let s = parse_mixer_status(&pkt).unwrap();
        assert!((s.effective_tempo() - 140.8).abs() < 0.1);
    }

    #[test]
    fn mixer_is_tempo_master_true() {
        let pkt = make_mixer_packet();
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(s.is_tempo_master());
    }

    #[test]
    fn mixer_is_tempo_master_false() {
        let mut pkt = make_mixer_packet();
        pkt[MIXER_FLAGS_OFFSET] = FLAG_SYNCED;
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(!s.is_tempo_master());
    }

    #[test]
    fn mixer_beat_within_bar_never_meaningful() {
        // Mixers never report meaningful beat positions (matches Java MixerStatus).
        let pkt = make_mixer_packet(); // beat_within_bar = 3
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(!s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn mixer_beat_within_bar_not_meaningful_zero() {
        let mut pkt = make_mixer_packet();
        pkt[MIXER_BEAT_WITHIN_BAR_OFFSET] = 0;
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(!s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn mixer_beat_within_bar_not_meaningful_five() {
        let mut pkt = make_mixer_packet();
        pkt[MIXER_BEAT_WITHIN_BAR_OFFSET] = 5;
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(!s.is_beat_within_bar_meaningful());
    }

    #[test]
    fn mixer_master_yielding_to_none() {
        let pkt = make_mixer_packet();
        let s = parse_mixer_status(&pkt).unwrap();
        assert!(s.master_yielding_to().is_none());
    }

    #[test]
    fn mixer_master_yielding_to_some() {
        let mut pkt = make_mixer_packet();
        pkt[MIXER_MASTER_HAND_OFF_OFFSET] = 4;
        let s = parse_mixer_status(&pkt).unwrap();
        assert_eq!(s.master_yielding_to(), Some(DeviceNumber(4)));
    }

    // -- is_bpm_only_synced tests --

    #[test]
    fn cdj_is_bpm_only_synced_true() {
        let mut pkt = make_cdj_packet();
        // bpm_sync set, synced cleared
        pkt[FLAGS_OFFSET] = FLAG_BPM_SYNC;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.is_bpm_only_synced());
    }

    #[test]
    fn cdj_is_bpm_only_synced_false_both_set() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = FLAG_BPM_SYNC | FLAG_SYNCED;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_bpm_only_synced());
    }

    #[test]
    fn cdj_is_bpm_only_synced_false_neither() {
        let mut pkt = make_cdj_packet();
        pkt[FLAGS_OFFSET] = 0;
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(!s.is_bpm_only_synced());
    }

    // -- Pre-nexus is_playing fallback: looping + searching --

    #[test]
    fn is_playing_pre_nexus_looping_moving() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0;
        pkt[PLAY_STATE_OFFSET] = 0x04; // Looping
        pkt[PLAY_STATE_2_OFFSET] = 0x6a; // Moving
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(s.is_playing());
    }

    #[test]
    fn is_playing_pre_nexus_searching_moving() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0;
        pkt[PLAY_STATE_OFFSET] = 0x09; // Searching
        pkt[PLAY_STATE_2_OFFSET] = 0x6a; // Moving
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(s.is_playing());
    }

    #[test]
    fn is_playing_pre_nexus_searching_stopped() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0;
        pkt[PLAY_STATE_OFFSET] = 0x09; // Searching
        pkt[PLAY_STATE_2_OFFSET] = 0x6e; // Stopped
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(!s.is_playing());
    }

    #[test]
    fn is_playing_pre_nexus_paused_moving() {
        let mut pkt = vec![0u8; MIN_CDJ_STATUS_LEN];
        let base = make_cdj_packet();
        pkt.copy_from_slice(&base[..MIN_CDJ_STATUS_LEN]);
        pkt[FLAGS_OFFSET] = 0;
        pkt[PLAY_STATE_OFFSET] = 0x05; // Paused
        pkt[PLAY_STATE_2_OFFSET] = 0x6a; // Moving
        let s = parse_cdj_status(&pkt).unwrap();
        assert!(s.packet_length < 0xd4);
        assert!(!s.is_playing()); // Paused is not considered playing
    }
}
