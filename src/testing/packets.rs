//! Mock packet builders that produce protocol-correct byte arrays.
//!
//! Every builder generates packets that round-trip through the library's
//! parsers without error.

use crate::protocol::header::MAGIC_HEADER;
use crate::util::number_to_bytes;

// -----------------------------------------------------------------------
// Protocol constants (mirrored from the parser modules)
// -----------------------------------------------------------------------

// Keep-alive offsets
const KA_NAME_OFFSET: usize = 0x0c;
const KA_NAME_LEN: usize = 20;
const KA_DEVICE_NUMBER_OFFSET: usize = 0x24;
const KA_DEVICE_TYPE_OFFSET: usize = 0x25;
const KA_MAC_OFFSET: usize = 0x26;
const KA_IP_OFFSET: usize = 0x2c;
const KA_PACKET_SIZE: usize = 0x36;

// CDJ status offsets
const CDJ_NAME_OFFSET: usize = 0x0b;
const CDJ_NAME_LEN: usize = 20;
const CDJ_DEVICE_NUMBER_OFFSET: usize = 0x21;
const CDJ_DEVICE_TYPE_OFFSET: usize = 0x23;
const CDJ_TRACK_SOURCE_PLAYER_OFFSET: usize = 0x28;
const CDJ_TRACK_SOURCE_SLOT_OFFSET: usize = 0x29;
const CDJ_TRACK_TYPE_OFFSET: usize = 0x2a;
const CDJ_REKORDBOX_ID_OFFSET: usize = 0x2c;
const CDJ_IS_BUSY_OFFSET: usize = 0x27;
const _CDJ_TRACK_NUMBER_OFFSET: usize = 0x32;
const CDJ_LOCAL_USB_STATE_OFFSET: usize = 0x6f;
const CDJ_LOCAL_SD_STATE_OFFSET: usize = 0x73;
const CDJ_PLAY_STATE_OFFSET: usize = 0x7b;
const CDJ_FIRMWARE_OFFSET: usize = 0x7c;
const CDJ_SYNC_NUMBER_OFFSET: usize = 0x84;
const CDJ_FLAGS_OFFSET: usize = 0x89;
const CDJ_PLAY_STATE_2_OFFSET: usize = 0x8b;
const CDJ_PITCH_OFFSET: usize = 0x8d;
const CDJ_PITCH_LEN: usize = 3;
const CDJ_BPM_OFFSET: usize = 0x92;
const CDJ_PLAY_STATE_3_OFFSET: usize = 0x9d;
const CDJ_MASTER_HAND_OFF_OFFSET: usize = 0x9f;
const CDJ_BEAT_NUMBER_OFFSET: usize = 0xa0;
const CDJ_CUE_COUNTDOWN_OFFSET: usize = 0xa4;
const CDJ_BEAT_WITHIN_BAR_OFFSET: usize = 0xa6;
const CDJ_PACKET_NUMBER_OFFSET: usize = 0xc8;
const CDJ_MIN_LEN: usize = 0xCC;
const CDJ_LOOP_THRESHOLD: usize = 0x1CA;
const CDJ_LOOP_START_OFFSET: usize = 0x1b6;
const CDJ_LOOP_END_OFFSET: usize = 0x1be;
const CDJ_LOOP_BEATS_OFFSET: usize = 0x1c8;

// Flag bits at 0x89
const _FLAG_BPM_SYNC: u8 = 0x02;
const FLAG_ON_AIR: u8 = 0x08;
const FLAG_SYNCED: u8 = 0x10;
const FLAG_MASTER: u8 = 0x20;
const FLAG_PLAYING: u8 = 0x40;

// Beat offsets
const BEAT_NAME_OFFSET: usize = 0x0b;
const BEAT_NAME_LEN: usize = 20;
const BEAT_DEVICE_NUMBER_OFFSET: usize = 0x21;
const BEAT_DEVICE_TYPE_OFFSET: usize = 0x23;
const BEAT_NEXT_BEAT_OFFSET: usize = 0x24;
const BEAT_SECOND_BEAT_OFFSET: usize = 0x28;
const BEAT_NEXT_BAR_OFFSET: usize = 0x2c;
const BEAT_FOURTH_BEAT_OFFSET: usize = 0x30;
const BEAT_SECOND_BAR_OFFSET: usize = 0x34;
const BEAT_EIGHTH_BEAT_OFFSET: usize = 0x38;
const BEAT_PITCH_OFFSET: usize = 0x55;
const BEAT_BPM_OFFSET: usize = 0x5a;
const BEAT_WITHIN_BAR_OFFSET: usize = 0x5c;
const BEAT_PACKET_LENGTH: usize = 0x60;

// PrecisePosition offsets
const PP_NAME_OFFSET: usize = 0x0b;
const PP_NAME_LEN: usize = 20;
const PP_DEVICE_NUMBER_OFFSET: usize = 0x21;
const PP_TRACK_LENGTH_OFFSET: usize = 0x24;
const PP_POSITION_OFFSET: usize = 0x28;
const PP_PITCH_OFFSET: usize = 0x2c;
const PP_BPM_OFFSET: usize = 0x38;
const PP_PACKET_LENGTH: usize = 0x3c;

// Mixer status offsets
const MIXER_NAME_OFFSET: usize = 0x0b;
const MIXER_NAME_LEN: usize = 20;
const MIXER_DEVICE_NUMBER_OFFSET: usize = 0x21;
const MIXER_FLAGS_OFFSET: usize = 0x27;
const MIXER_PITCH_OFFSET: usize = 0x28;
const MIXER_BPM_OFFSET: usize = 0x2e;
const MIXER_MASTER_HAND_OFF_OFFSET: usize = 0x36;
const MIXER_BEAT_WITHIN_BAR_OFFSET: usize = 0x37;
const MIXER_MIN_LEN: usize = 0x38;

// ChannelsOnAir offsets
const OA_NAME_OFFSET: usize = 0x0b;
const OA_NAME_LEN: usize = 20;
const OA_DEVICE_NUMBER_OFFSET: usize = 0x21;
const OA_CHANNEL_FLAGS_OFFSET: usize = 0x24;

// Media details offsets
const MD_NAME_OFFSET: usize = 0x0c;
const MD_NAME_LEN: usize = 20;
const MD_PLAYER_OFFSET: usize = 0x21;
const MD_SLOT_OFFSET: usize = 0x27;
const MD_MEDIA_TYPE_OFFSET: usize = 0x28;
const MD_UTF16_NAME_OFFSET: usize = 0x2c;
const MD_UTF16_NAME_LEN: usize = 0x40;
const MD_TRACK_COUNT_OFFSET: usize = 0x6c;
const MD_PLAYLIST_COUNT_OFFSET: usize = 0x70;
const MD_COLOR_OFFSET: usize = 0x72;
const MD_REKORDBOX_OFFSET: usize = 0x73;
const MD_TOTAL_SIZE_OFFSET: usize = 0x74;
const MD_FREE_SPACE_OFFSET: usize = 0x7c;
const MD_MIN_SIZE: usize = 0x84;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn write_name(pkt: &mut [u8], offset: usize, max_len: usize, name: &str) {
    let bytes = name.as_bytes();
    let copy_len = bytes.len().min(max_len);
    pkt[offset..offset + copy_len].copy_from_slice(&bytes[..copy_len]);
}

fn write_magic(pkt: &mut [u8]) {
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
}

/// Convert a BPM value to the 2-byte raw encoding (value × 100).
fn bpm_to_raw(bpm: f64) -> u16 {
    (bpm * 100.0) as u16
}

/// Convert a pitch percentage to the raw 3-byte pitch value.
fn pitch_pct_to_raw(pct: f64) -> u32 {
    ((pct / 100.0 * 0x100000 as f64) + 0x100000 as f64) as u32
}

// -----------------------------------------------------------------------
// Keep-alive builder
// -----------------------------------------------------------------------

/// Build a realistic keep-alive packet (type 0x06, exactly 0x36 bytes).
pub fn mock_keep_alive(
    name: &str,
    device_number: u8,
    device_type: u8,
    ip: [u8; 4],
    mac: [u8; 6],
) -> Vec<u8> {
    let mut pkt = vec![0u8; KA_PACKET_SIZE];
    write_magic(&mut pkt);
    pkt[0x0a] = 0x06;

    write_name(&mut pkt, KA_NAME_OFFSET, KA_NAME_LEN, name);

    // Structure marker and subtype (matches real keep-alive format)
    pkt[0x20] = 0x01;
    pkt[0x21] = 0x02;
    let len_bytes = (KA_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);

    pkt[KA_DEVICE_NUMBER_OFFSET] = device_number;
    pkt[KA_DEVICE_TYPE_OFFSET] = device_type;
    pkt[KA_MAC_OFFSET..KA_MAC_OFFSET + 6].copy_from_slice(&mac);
    pkt[KA_IP_OFFSET..KA_IP_OFFSET + 4].copy_from_slice(&ip);

    // Device type mirror at 0x34
    pkt[0x34] = device_type;

    pkt
}

// -----------------------------------------------------------------------
// CDJ status builder
// -----------------------------------------------------------------------

/// Ergonomic builder for CDJ status packets.
///
/// All fields default to reasonable idle-player values.  Chain setter
/// methods to configure the desired state, then call [`build`](MockCdjStatusBuilder::build)
/// to produce the raw bytes.
pub struct MockCdjStatusBuilder {
    name: String,
    device_number: u8,
    device_type: u8,
    bpm: f64,
    pitch_percent: f64,
    play_state: u8,
    play_state_2: u8,
    play_state_3: u8,
    flags: u8,
    rekordbox_id: u32,
    track_source_player: u8,
    track_source_slot: u8,
    track_type: u8,
    beat_number: u32,
    beat_within_bar: u8,
    packet_number: u32,
    firmware: [u8; 4],
    sync_number: u32,
    local_usb_state: u8,
    local_sd_state: u8,
    // CDJ-3000 loop fields
    loop_start: Option<u64>,
    loop_end: Option<u64>,
    loop_beats: Option<u16>,
}

impl MockCdjStatusBuilder {
    /// Create a new builder with defaults for the given device number.
    pub fn new(device_number: u8) -> Self {
        Self {
            name: "CDJ-2000NXS2".to_string(),
            device_number,
            device_type: 1, // CDJ
            bpm: 0.0,
            pitch_percent: 0.0,
            play_state: 0x00,     // NoTrack
            play_state_2: 0x6e,   // Stopped
            play_state_3: 0x00,   // NoTrack
            flags: 0,
            rekordbox_id: 0,
            track_source_player: 0,
            track_source_slot: 0,
            track_type: 0,
            beat_number: 0xFFFFFFFF,
            beat_within_bar: 0,
            packet_number: 0,
            firmware: *b"1A01",
            sync_number: 0,
            local_usb_state: 0,
            local_sd_state: 0,
            loop_start: None,
            loop_end: None,
            loop_beats: None,
        }
    }

    /// Set the device name (e.g. "CDJ-3000", "OPUS-QUAD").
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set the track BPM (before pitch adjustment).
    pub fn bpm(mut self, bpm: f64) -> Self {
        self.bpm = bpm;
        self
    }

    /// Set the pitch fader percentage (0.0 = normal, +6.0 = +6%).
    pub fn pitch(mut self, pitch_percent: f64) -> Self {
        self.pitch_percent = pitch_percent;
        self
    }

    /// Set the player to "playing" state.
    pub fn playing(mut self) -> Self {
        self.play_state = 0x03;   // Playing
        self.play_state_2 = 0x6a; // Moving
        self.play_state_3 = 0x0d; // ForwardCdj
        self.flags |= FLAG_PLAYING;
        self
    }

    /// Set the player to "paused" state.
    pub fn paused(mut self) -> Self {
        self.play_state = 0x05;   // Paused
        self.play_state_2 = 0x6e; // Stopped
        self.play_state_3 = 0x01; // PausedOrReverse
        self.flags &= !FLAG_PLAYING;
        self
    }

    /// Set the player to "cued" state.
    pub fn cued(mut self) -> Self {
        self.play_state = 0x06;   // Cued
        self.play_state_2 = 0x6e; // Stopped
        self.play_state_3 = 0x01; // PausedOrReverse
        self.flags &= !FLAG_PLAYING;
        self
    }

    /// Set the player to "looping" state.
    pub fn looping(mut self) -> Self {
        self.play_state = 0x04;   // Looping
        self.play_state_2 = 0x6a; // Moving
        self.play_state_3 = 0x0d; // ForwardCdj
        self.flags |= FLAG_PLAYING;
        self
    }

    /// Mark this player as the tempo master.
    pub fn master(mut self) -> Self {
        self.flags |= FLAG_MASTER;
        self
    }

    /// Enable sync mode.
    pub fn synced(mut self) -> Self {
        self.flags |= FLAG_SYNCED;
        self
    }

    /// Mark this channel as on-air.
    pub fn on_air(mut self) -> Self {
        self.flags |= FLAG_ON_AIR;
        self
    }

    /// Set the loaded track info.
    pub fn track(mut self, rekordbox_id: u32, source_player: u8, slot: u8) -> Self {
        self.rekordbox_id = rekordbox_id;
        self.track_source_player = source_player;
        self.track_source_slot = slot;
        self.track_type = 1; // Rekordbox
        self
    }

    /// Set the current beat position.
    pub fn beat(mut self, beat_number: u32, beat_within_bar: u8) -> Self {
        self.beat_number = beat_number;
        self.beat_within_bar = beat_within_bar;
        self
    }

    /// Set local USB as loaded.
    pub fn usb_loaded(mut self) -> Self {
        self.local_usb_state = 4;
        self
    }

    /// Set local SD as loaded.
    pub fn sd_loaded(mut self) -> Self {
        self.local_sd_state = 4;
        self
    }

    /// Set CDJ-3000 loop fields. This causes the packet to be extended
    /// to the CDJ-3000 threshold size (0x1CA bytes).
    ///
    /// `start` and `end` are in the CDJ-3000 native format (ms × 65536 / 1000).
    /// `beats` is the loop length in beats.
    pub fn cdj3000_loop(mut self, start: u64, end: u64, beats: u16) -> Self {
        self.loop_start = Some(start);
        self.loop_end = Some(end);
        self.loop_beats = Some(beats);
        self
    }

    /// Set the packet sequence number.
    pub fn packet_number(mut self, n: u32) -> Self {
        self.packet_number = n;
        self
    }

    /// Build the raw CDJ status packet bytes.
    pub fn build(self) -> Vec<u8> {
        let needs_loop = self.loop_start.is_some();
        let pkt_len = if needs_loop { CDJ_LOOP_THRESHOLD } else { CDJ_MIN_LEN };
        let mut pkt = vec![0u8; pkt_len];

        write_magic(&mut pkt);
        pkt[0x0a] = 0x0a; // CdjStatus type byte

        write_name(&mut pkt, CDJ_NAME_OFFSET, CDJ_NAME_LEN, &self.name);
        pkt[CDJ_DEVICE_NUMBER_OFFSET] = self.device_number;
        pkt[CDJ_DEVICE_TYPE_OFFSET] = self.device_type;

        pkt[CDJ_TRACK_SOURCE_PLAYER_OFFSET] = self.track_source_player;
        pkt[CDJ_TRACK_SOURCE_SLOT_OFFSET] = self.track_source_slot;
        pkt[CDJ_TRACK_TYPE_OFFSET] = self.track_type;

        number_to_bytes(self.rekordbox_id, &mut pkt, CDJ_REKORDBOX_ID_OFFSET, 4);

        pkt[CDJ_PLAY_STATE_OFFSET] = self.play_state;
        pkt[CDJ_FIRMWARE_OFFSET..CDJ_FIRMWARE_OFFSET + 4].copy_from_slice(&self.firmware);
        number_to_bytes(self.sync_number, &mut pkt, CDJ_SYNC_NUMBER_OFFSET, 4);

        pkt[CDJ_FLAGS_OFFSET] = self.flags;
        pkt[CDJ_PLAY_STATE_2_OFFSET] = self.play_state_2;
        pkt[CDJ_PLAY_STATE_3_OFFSET] = self.play_state_3;

        // Pitch (3 bytes)
        let pitch_raw = pitch_pct_to_raw(self.pitch_percent);
        let pitch_be = pitch_raw.to_be_bytes();
        pkt[CDJ_PITCH_OFFSET..CDJ_PITCH_OFFSET + CDJ_PITCH_LEN]
            .copy_from_slice(&pitch_be[1..4]);

        // BPM (2 bytes, value × 100)
        let bpm_raw = bpm_to_raw(self.bpm) as u32;
        number_to_bytes(bpm_raw, &mut pkt, CDJ_BPM_OFFSET, 2);

        // Beat number
        number_to_bytes(self.beat_number, &mut pkt, CDJ_BEAT_NUMBER_OFFSET, 4);
        pkt[CDJ_BEAT_WITHIN_BAR_OFFSET] = self.beat_within_bar;

        // Master hand-off = none
        pkt[CDJ_MASTER_HAND_OFF_OFFSET] = 0xFF;

        // Cue countdown sentinel
        pkt[CDJ_CUE_COUNTDOWN_OFFSET] = 0x01;
        pkt[CDJ_CUE_COUNTDOWN_OFFSET + 1] = 0xFF;

        // Packet number
        if pkt.len() >= CDJ_PACKET_NUMBER_OFFSET + 4 {
            number_to_bytes(self.packet_number, &mut pkt, CDJ_PACKET_NUMBER_OFFSET, 4);
        }

        // Media state
        pkt[CDJ_LOCAL_USB_STATE_OFFSET] = self.local_usb_state;
        pkt[CDJ_LOCAL_SD_STATE_OFFSET] = self.local_sd_state;
        pkt[CDJ_IS_BUSY_OFFSET] = 0;

        // CDJ-3000 loop fields
        if let (Some(ls), Some(le), Some(lb)) =
            (self.loop_start, self.loop_end, self.loop_beats)
        {
            // Reverse the encoding: parser does raw * 65536 / 1000, so we
            // store the value that when multiplied gives the desired output.
            let ls_raw = (ls * 1000 / 65536) as u32;
            let le_raw = (le * 1000 / 65536) as u32;
            number_to_bytes(ls_raw, &mut pkt, CDJ_LOOP_START_OFFSET, 4);
            number_to_bytes(le_raw, &mut pkt, CDJ_LOOP_END_OFFSET, 4);
            number_to_bytes(lb as u32, &mut pkt, CDJ_LOOP_BEATS_OFFSET, 2);
        }

        pkt
    }
}

// -----------------------------------------------------------------------
// Mixer status builder
// -----------------------------------------------------------------------

/// Ergonomic builder for mixer status packets.
pub struct MockMixerStatusBuilder {
    name: String,
    device_number: u8,
    bpm: f64,
    pitch_raw: u32,
    beat_within_bar: u8,
    is_master: bool,
    is_synced: bool,
    master_hand_off: u8,
}

impl MockMixerStatusBuilder {
    /// Create a new mixer builder with the given device number.
    pub fn new(device_number: u8) -> Self {
        Self {
            name: "DJM-900NXS2".to_string(),
            device_number,
            bpm: 0.0,
            pitch_raw: 0x100000,
            beat_within_bar: 1,
            is_master: false,
            is_synced: false,
            master_hand_off: 0xFF,
        }
    }

    /// Set the mixer name.
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set the BPM.
    pub fn bpm(mut self, bpm: f64) -> Self {
        self.bpm = bpm;
        self
    }

    /// Mark this mixer as the tempo master.
    pub fn master(mut self) -> Self {
        self.is_master = true;
        self
    }

    /// Enable sync mode.
    pub fn synced(mut self) -> Self {
        self.is_synced = true;
        self
    }

    /// Set beat within bar (1–4).
    pub fn beat_within_bar(mut self, beat: u8) -> Self {
        self.beat_within_bar = beat;
        self
    }

    /// Build the raw mixer status packet bytes.
    pub fn build(self) -> Vec<u8> {
        let mut pkt = vec![0u8; MIXER_MIN_LEN];
        write_magic(&mut pkt);
        pkt[0x0a] = 0x29; // MixerStatus type byte

        write_name(&mut pkt, MIXER_NAME_OFFSET, MIXER_NAME_LEN, &self.name);
        pkt[MIXER_DEVICE_NUMBER_OFFSET] = self.device_number;

        let mut flags: u8 = 0;
        if self.is_master {
            flags |= FLAG_MASTER;
        }
        if self.is_synced {
            flags |= FLAG_SYNCED;
        }
        pkt[MIXER_FLAGS_OFFSET] = flags;

        // Pitch (4 bytes)
        pkt[MIXER_PITCH_OFFSET..MIXER_PITCH_OFFSET + 4]
            .copy_from_slice(&self.pitch_raw.to_be_bytes());

        // BPM (2 bytes)
        let bpm_raw = bpm_to_raw(self.bpm) as u32;
        number_to_bytes(bpm_raw, &mut pkt, MIXER_BPM_OFFSET, 2);

        pkt[MIXER_MASTER_HAND_OFF_OFFSET] = self.master_hand_off;
        pkt[MIXER_BEAT_WITHIN_BAR_OFFSET] = self.beat_within_bar;

        pkt
    }
}

// -----------------------------------------------------------------------
// Beat builder
// -----------------------------------------------------------------------

/// Ergonomic builder for beat packets.
pub struct MockBeatBuilder {
    name: String,
    device_number: u8,
    device_type: u8,
    bpm: f64,
    pitch_percent: f64,
    beat_within_bar: u8,
    timing: [u32; 6],
}

impl MockBeatBuilder {
    /// Create a new beat builder for the given device number.
    pub fn new(device_number: u8) -> Self {
        Self {
            name: "CDJ-2000NXS2".to_string(),
            device_number,
            device_type: 1, // CDJ
            bpm: 128.0,
            pitch_percent: 0.0,
            beat_within_bar: 1,
            timing: [0xFFFFFFFF; 6],
        }
    }

    /// Set the device name.
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set the track BPM (before pitch).
    pub fn bpm(mut self, bpm: f64) -> Self {
        self.bpm = bpm;
        self
    }

    /// Set the pitch percentage (0.0 = normal).
    pub fn pitch(mut self, pitch_percent: f64) -> Self {
        self.pitch_percent = pitch_percent;
        self
    }

    /// Set beat position within bar (1–4).
    pub fn beat_within_bar(mut self, beat: u8) -> Self {
        self.beat_within_bar = beat;
        self
    }

    /// Set all six beat timing fields (ms until next beat, 2nd beat, etc.).
    pub fn timing(
        mut self,
        next_beat: u32,
        second_beat: u32,
        next_bar: u32,
        fourth_beat: u32,
        second_bar: u32,
        eighth_beat: u32,
    ) -> Self {
        self.timing = [next_beat, second_beat, next_bar, fourth_beat, second_bar, eighth_beat];
        self
    }

    /// Build the raw beat packet bytes (exactly 0x60 bytes).
    pub fn build(self) -> Vec<u8> {
        let mut pkt = vec![0u8; BEAT_PACKET_LENGTH];
        write_magic(&mut pkt);
        pkt[0x0a] = 0x28;

        write_name(&mut pkt, BEAT_NAME_OFFSET, BEAT_NAME_LEN, &self.name);
        pkt[BEAT_DEVICE_NUMBER_OFFSET] = self.device_number;
        pkt[BEAT_DEVICE_TYPE_OFFSET] = self.device_type;

        // Timing fields
        let offsets = [
            BEAT_NEXT_BEAT_OFFSET,
            BEAT_SECOND_BEAT_OFFSET,
            BEAT_NEXT_BAR_OFFSET,
            BEAT_FOURTH_BEAT_OFFSET,
            BEAT_SECOND_BAR_OFFSET,
            BEAT_EIGHTH_BEAT_OFFSET,
        ];
        for (off, val) in offsets.iter().zip(self.timing.iter()) {
            pkt[*off..*off + 4].copy_from_slice(&val.to_be_bytes());
        }

        // Pitch (3 bytes at 0x55)
        let pitch_raw = pitch_pct_to_raw(self.pitch_percent);
        let pitch_be = pitch_raw.to_be_bytes();
        pkt[BEAT_PITCH_OFFSET..BEAT_PITCH_OFFSET + 3].copy_from_slice(&pitch_be[1..4]);

        // BPM (2 bytes at 0x5a)
        let bpm_raw = bpm_to_raw(self.bpm);
        pkt[BEAT_BPM_OFFSET..BEAT_BPM_OFFSET + 2].copy_from_slice(&bpm_raw.to_be_bytes());

        pkt[BEAT_WITHIN_BAR_OFFSET] = self.beat_within_bar;

        pkt
    }
}

// -----------------------------------------------------------------------
// PrecisePosition builder
// -----------------------------------------------------------------------

/// Build a precise position packet (type 0x0b, exactly 0x3c bytes).
pub fn mock_precise_position(
    device_number: u8,
    position_ms: u32,
    track_length_s: u32,
    bpm: f64,
    pitch_percent: f64,
) -> Vec<u8> {
    let mut pkt = vec![0u8; PP_PACKET_LENGTH];
    write_magic(&mut pkt);
    pkt[0x0a] = 0x0b;

    write_name(&mut pkt, PP_NAME_OFFSET, PP_NAME_LEN, "CDJ-3000");
    pkt[PP_DEVICE_NUMBER_OFFSET] = device_number;

    pkt[PP_TRACK_LENGTH_OFFSET..PP_TRACK_LENGTH_OFFSET + 4]
        .copy_from_slice(&track_length_s.to_be_bytes());
    pkt[PP_POSITION_OFFSET..PP_POSITION_OFFSET + 4]
        .copy_from_slice(&position_ms.to_be_bytes());

    // Pitch: signed percentage × 100
    let raw_pitch = (pitch_percent * 100.0) as i32;
    pkt[PP_PITCH_OFFSET..PP_PITCH_OFFSET + 4]
        .copy_from_slice(&raw_pitch.to_be_bytes());

    // BPM: effective BPM × 10 (4 bytes)
    let effective_bpm = bpm * (1.0 + pitch_percent / 100.0);
    let raw_bpm = (effective_bpm * 10.0) as u32;
    pkt[PP_BPM_OFFSET..PP_BPM_OFFSET + 4]
        .copy_from_slice(&raw_bpm.to_be_bytes());

    pkt
}

// -----------------------------------------------------------------------
// Channels-on-air builder
// -----------------------------------------------------------------------

/// Build a channels-on-air packet (type 0x03).
///
/// `channels` is a slice of booleans for channels 1–N.
/// Supports 4 or 6 channels.
pub fn mock_channels_on_air(device_number: u8, channels: &[bool]) -> Vec<u8> {
    let num_ch = channels.len().max(4);
    let total_len = OA_CHANNEL_FLAGS_OFFSET + num_ch;
    let mut pkt = vec![0u8; total_len];
    write_magic(&mut pkt);
    pkt[0x0a] = 0x03;

    write_name(&mut pkt, OA_NAME_OFFSET, OA_NAME_LEN, "DJM-900NXS2");
    pkt[OA_DEVICE_NUMBER_OFFSET] = device_number;

    for (i, &on) in channels.iter().enumerate() {
        if OA_CHANNEL_FLAGS_OFFSET + i < pkt.len() {
            pkt[OA_CHANNEL_FLAGS_OFFSET + i] = if on { 0x01 } else { 0x00 };
        }
    }

    pkt
}

// -----------------------------------------------------------------------
// Media details builder
// -----------------------------------------------------------------------

/// Build a media details response packet.
pub fn mock_media_details(
    player: u8,
    slot: u8,
    name: &str,
    track_count: u16,
) -> Vec<u8> {
    let mut pkt = vec![0u8; MD_MIN_SIZE];
    write_magic(&mut pkt);
    pkt[0x0a] = 0x19; // media details type placeholder

    write_name(&mut pkt, MD_NAME_OFFSET, MD_NAME_LEN, "CDJ-2000NXS2");
    pkt[MD_PLAYER_OFFSET] = player;
    pkt[MD_SLOT_OFFSET] = slot;

    // USB media type for USB slot, SD for SD slot
    pkt[MD_MEDIA_TYPE_OFFSET] = match slot {
        2 => 2, // Sd
        3 => 3, // Usb
        _ => slot,
    };

    // UTF-16BE name
    for (i, ch) in name.encode_utf16().enumerate() {
        let off = MD_UTF16_NAME_OFFSET + i * 2;
        if off + 1 < pkt.len() && (off - MD_UTF16_NAME_OFFSET) < MD_UTF16_NAME_LEN {
            let be = ch.to_be_bytes();
            pkt[off] = be[0];
            pkt[off + 1] = be[1];
        }
    }

    // Track count (u32 BE)
    pkt[MD_TRACK_COUNT_OFFSET..MD_TRACK_COUNT_OFFSET + 4]
        .copy_from_slice(&(track_count as u32).to_be_bytes());

    // Playlist count
    pkt[MD_PLAYLIST_COUNT_OFFSET..MD_PLAYLIST_COUNT_OFFSET + 2]
        .copy_from_slice(&4u16.to_be_bytes());

    pkt[MD_COLOR_OFFSET] = 0;
    pkt[MD_REKORDBOX_OFFSET] = 1; // analysed

    // Sizes
    let total: u64 = 32_000_000_000;
    let free: u64 = 16_000_000_000;
    pkt[MD_TOTAL_SIZE_OFFSET..MD_TOTAL_SIZE_OFFSET + 8]
        .copy_from_slice(&total.to_be_bytes());
    pkt[MD_FREE_SPACE_OFFSET..MD_FREE_SPACE_OFFSET + 8]
        .copy_from_slice(&free.to_be_bytes());

    pkt
}
