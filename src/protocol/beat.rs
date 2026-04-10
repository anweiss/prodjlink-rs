use std::time::Instant;

use crate::device::types::{BeatNumber, Bpm, DeviceNumber, DeviceType, Pitch};
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{parse_header, PacketType};
use crate::util::{bytes_to_number, read_device_name};

/// Minimum length for a beat packet (need at least through byte 0x5f).
const BEAT_MIN_LENGTH: usize = 0x60;

/// Minimum length for a precise-position packet (header + device number).
const PRECISE_POSITION_MIN_LENGTH: usize = 0x28;

// -- Beat packet offsets --
const DEVICE_NAME_OFFSET: usize = 0x0c;
const DEVICE_NAME_MAX_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x21;
const DEVICE_TYPE_OFFSET: usize = 0x23;
const PITCH_OFFSET: usize = 0x54;
const BPM_OFFSET: usize = 0x5a;
const BEAT_WITHIN_BAR_OFFSET: usize = 0x5f;

/// A beat timing packet received on the beat port (type 0x28).
#[derive(Debug, Clone)]
pub struct Beat {
    pub name: String,
    pub device_number: DeviceNumber,
    pub device_type: DeviceType,
    /// The current BPM as reported by the device (pitch-adjusted).
    pub bpm: Bpm,
    /// Raw pitch value.
    pub pitch: Pitch,
    /// Position within the current bar (1-4), or 0 if unknown.
    pub beat_within_bar: u8,
    /// When this beat was received.
    pub timestamp: Instant,
}

/// Precise playback position from CDJ-3000 and newer (type 0x7f).
#[derive(Debug, Clone)]
pub struct PrecisePosition {
    pub device_number: DeviceNumber,
    /// Playback position in milliseconds.
    pub position_ms: u32,
    pub bpm: Bpm,
    pub beat_number: BeatNumber,
    pub playing: bool,
    pub timestamp: Instant,
}

/// Parse a beat packet (type 0x28) from raw bytes.
pub fn parse_beat(data: &[u8]) -> Result<Beat> {
    let pkt_type = parse_header(data)?;
    if pkt_type != PacketType::Beat {
        return Err(ProDjLinkError::Parse(format!(
            "expected Beat (0x28) packet, got {:?}",
            pkt_type
        )));
    }

    if data.len() < BEAT_MIN_LENGTH {
        return Err(ProDjLinkError::PacketTooShort {
            expected: BEAT_MIN_LENGTH,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, DEVICE_NAME_OFFSET, DEVICE_NAME_MAX_LEN);
    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);
    let device_type = DeviceType::from(data[DEVICE_TYPE_OFFSET]);
    let pitch = Pitch(bytes_to_number(data, PITCH_OFFSET, 4) as i32);
    let raw_bpm = bytes_to_number(data, BPM_OFFSET, 2);
    let bpm = Bpm(raw_bpm as f64 / 100.0);
    let beat_within_bar = data[BEAT_WITHIN_BAR_OFFSET];

    Ok(Beat {
        name,
        device_number,
        device_type,
        bpm,
        pitch,
        beat_within_bar,
        timestamp: Instant::now(),
    })
}

/// Parse a precise position packet (type 0x7f) from raw bytes.
pub fn parse_precise_position(data: &[u8]) -> Result<PrecisePosition> {
    let pkt_type = parse_header(data)?;
    if pkt_type != PacketType::Unknown(0x7f) {
        return Err(ProDjLinkError::Parse(format!(
            "expected PrecisePosition (0x7f) packet, got {:?}",
            pkt_type
        )));
    }

    if data.len() < PRECISE_POSITION_MIN_LENGTH {
        return Err(ProDjLinkError::PacketTooShort {
            expected: PRECISE_POSITION_MIN_LENGTH,
            actual: data.len(),
        });
    }

    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);

    // Precise-position fields — offsets based on PrecisePosition.java.
    // Fall back to zero / false when the packet is too short.
    let position_ms = if data.len() > 0x2c {
        bytes_to_number(data, 0x28, 4)
    } else {
        0
    };

    let raw_bpm = if data.len() > 0x2e {
        bytes_to_number(data, 0x2c, 2)
    } else {
        0
    };
    let bpm = Bpm(raw_bpm as f64 / 100.0);

    let beat_number = if data.len() > 0x34 {
        BeatNumber(bytes_to_number(data, 0x30, 4))
    } else {
        BeatNumber(0)
    };

    let playing = if data.len() > 0x27 {
        data[0x27] != 0
    } else {
        false
    };

    Ok(PrecisePosition {
        device_number,
        position_ms,
        bpm,
        beat_number,
        playing,
        timestamp: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::header::MAGIC_HEADER;

    /// Build a minimal 0x60-byte beat packet with the given fields.
    fn make_beat_packet(
        name: &str,
        device_num: u8,
        device_type: u8,
        bpm_hundredths: u16,
        pitch_raw: u32,
        beat_in_bar: u8,
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; BEAT_MIN_LENGTH];

        // Magic header
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        // Packet type
        pkt[0x0a] = 0x28;
        // Device name (null-terminated, up to 20 bytes)
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(DEVICE_NAME_MAX_LEN);
        pkt[DEVICE_NAME_OFFSET..DEVICE_NAME_OFFSET + copy_len]
            .copy_from_slice(&name_bytes[..copy_len]);
        // Device number & type
        pkt[DEVICE_NUMBER_OFFSET] = device_num;
        pkt[DEVICE_TYPE_OFFSET] = device_type;
        // Pitch (4 bytes big-endian)
        pkt[PITCH_OFFSET..PITCH_OFFSET + 4].copy_from_slice(&pitch_raw.to_be_bytes());
        // BPM (2 bytes big-endian, value × 100)
        pkt[BPM_OFFSET..BPM_OFFSET + 2].copy_from_slice(&bpm_hundredths.to_be_bytes());
        // Beat within bar
        pkt[BEAT_WITHIN_BAR_OFFSET] = beat_in_bar;

        pkt
    }

    fn make_precise_position_packet(
        device_num: u8,
        position_ms: u32,
        bpm_hundredths: u16,
        beat_number: u32,
        playing: bool,
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; 0x35];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x7f;
        pkt[DEVICE_NUMBER_OFFSET] = device_num;
        pkt[0x27] = if playing { 1 } else { 0 };
        pkt[0x28..0x2c].copy_from_slice(&position_ms.to_be_bytes());
        pkt[0x2c..0x2e].copy_from_slice(&bpm_hundredths.to_be_bytes());
        pkt[0x30..0x34].copy_from_slice(&beat_number.to_be_bytes());
        pkt
    }

    // -- parse_beat tests --

    #[test]
    fn parse_beat_valid() {
        // 128.00 BPM → raw 12800, pitch normal (0x100000), beat 3
        let pkt = make_beat_packet("CDJ-2000NXS2", 2, 1, 12800, 0x100000, 3);
        let beat = parse_beat(&pkt).unwrap();

        assert_eq!(beat.name, "CDJ-2000NXS2");
        assert_eq!(beat.device_number, DeviceNumber(2));
        assert_eq!(beat.device_type, DeviceType::Cdj);
        assert!((beat.bpm.0 - 128.0).abs() < f64::EPSILON);
        assert_eq!(beat.pitch, Pitch(0x100000));
        assert_eq!(beat.beat_within_bar, 3);
    }

    #[test]
    fn parse_beat_zero_bpm() {
        let pkt = make_beat_packet("DJM-900NXS2", 33, 2, 0, 0x100000, 1);
        let beat = parse_beat(&pkt).unwrap();

        assert!((beat.bpm.0).abs() < f64::EPSILON);
        assert_eq!(beat.device_type, DeviceType::Mixer);
    }

    #[test]
    fn parse_beat_unknown_beat_position() {
        let pkt = make_beat_packet("CDJ-3000", 1, 1, 14050, 0x100000, 0);
        let beat = parse_beat(&pkt).unwrap();

        assert_eq!(beat.beat_within_bar, 0);
        assert!((beat.bpm.0 - 140.50).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_beat_pitched() {
        // BPM 125.00, pitch at +6% → raw pitch 0x10F5C2 (approx)
        let pitch_raw = ((1.06f64) * 0x100000 as f64) as u32;
        let pkt = make_beat_packet("CDJ-2000NXS2", 3, 1, 12500, pitch_raw, 4);
        let beat = parse_beat(&pkt).unwrap();

        assert!((beat.bpm.0 - 125.0).abs() < f64::EPSILON);
        let pct = beat.pitch.to_percentage();
        assert!((pct - 6.0).abs() < 0.01, "expected ~6%, got {pct}%");
    }

    #[test]
    fn parse_beat_too_short() {
        let pkt = make_beat_packet("X", 1, 1, 12800, 0, 1);
        let short = &pkt[..0x30]; // truncated
        let err = parse_beat(short).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: BEAT_MIN_LENGTH,
                ..
            }
        ));
    }

    #[test]
    fn parse_beat_wrong_type() {
        let mut pkt = make_beat_packet("X", 1, 1, 12800, 0, 1);
        pkt[0x0a] = 0x06; // DeviceKeepAlive, not Beat
        let err = parse_beat(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::Parse(_)));
    }

    #[test]
    fn parse_beat_invalid_magic() {
        let mut pkt = make_beat_packet("X", 1, 1, 12800, 0, 1);
        pkt[0] = 0xFF;
        let err = parse_beat(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidMagic));
    }

    // -- parse_precise_position tests --

    #[test]
    fn parse_precise_position_valid() {
        let pkt = make_precise_position_packet(4, 65432, 13500, 97, true);
        let pp = parse_precise_position(&pkt).unwrap();

        assert_eq!(pp.device_number, DeviceNumber(4));
        assert_eq!(pp.position_ms, 65432);
        assert!((pp.bpm.0 - 135.0).abs() < f64::EPSILON);
        assert_eq!(pp.beat_number, BeatNumber(97));
        assert!(pp.playing);
    }

    #[test]
    fn parse_precise_position_not_playing() {
        let pkt = make_precise_position_packet(1, 0, 0, 0, false);
        let pp = parse_precise_position(&pkt).unwrap();

        assert!(!pp.playing);
        assert!((pp.bpm.0).abs() < f64::EPSILON);
        assert_eq!(pp.position_ms, 0);
    }

    #[test]
    fn parse_precise_position_wrong_type() {
        let mut pkt = make_precise_position_packet(1, 0, 0, 0, false);
        pkt[0x0a] = 0x28; // Beat, not PrecisePosition
        let err = parse_precise_position(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::Parse(_)));
    }
}
