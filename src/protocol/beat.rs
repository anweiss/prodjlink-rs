use std::time::Instant;

use crate::device::types::{Bpm, DeviceNumber, DeviceType, Pitch};
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{parse_header, PacketType};
use crate::util::{bytes_to_number, read_device_name};

/// Beat packets are exactly 0x60 bytes.
const BEAT_PACKET_LENGTH: usize = 0x60;

/// PrecisePosition packets are exactly 0x3c (60) bytes.
const PRECISE_POSITION_LENGTH: usize = 0x3c;

// -- Beat packet offsets (verified against Beat.java) --
const DEVICE_NAME_OFFSET: usize = 0x0b;
const DEVICE_NAME_MAX_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x21;
const DEVICE_TYPE_OFFSET: usize = 0x23;
/// 3-byte pitch at 0x55 (Beat.java: `Util.bytesToNumber(packetBytes, 0x55, 3)`)
const BEAT_PITCH_OFFSET: usize = 0x55;
const BEAT_PITCH_LEN: usize = 3;
/// 2-byte BPM at 0x5a (hundredths)
const BEAT_BPM_OFFSET: usize = 0x5a;
/// Beat within bar at 0x5c (Beat.java: `packetBytes[0x5c]`)
const BEAT_WITHIN_BAR_OFFSET: usize = 0x5c;

// -- PrecisePosition offsets (verified against PrecisePosition.java) --
const PP_TRACK_LENGTH_OFFSET: usize = 0x24;
const PP_POSITION_OFFSET: usize = 0x28;
const PP_PITCH_OFFSET: usize = 0x2c;
const PP_BPM_OFFSET: usize = 0x38;

/// A beat timing packet received on the beat port (type 0x28).
#[derive(Debug, Clone)]
pub struct Beat {
    pub name: String,
    pub device_number: DeviceNumber,
    pub device_type: DeviceType,
    /// Track BPM (before pitch adjustment), from 2-byte value / 100.
    pub bpm: Bpm,
    /// Raw pitch value (0–2097152 range; 0x100000 = normal speed).
    pub pitch: Pitch,
    /// Position within the current bar (1–4), or 0 if unknown.
    pub beat_within_bar: u8,
    pub timestamp: Instant,
}

/// Precise playback position from CDJ-3000 and newer (type 0x0b, sent ~30 ms).
#[derive(Debug, Clone)]
pub struct PrecisePosition {
    pub name: String,
    pub device_number: DeviceNumber,
    /// Track length in seconds.
    pub track_length: u32,
    /// Playback position in milliseconds.
    pub position_ms: u32,
    /// Raw pitch converted to standard range (0–2097152).
    pub pitch: Pitch,
    /// Effective BPM (already pitch-adjusted).
    pub effective_bpm: Bpm,
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

    if data.len() < BEAT_PACKET_LENGTH {
        return Err(ProDjLinkError::PacketTooShort {
            expected: BEAT_PACKET_LENGTH,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, DEVICE_NAME_OFFSET, DEVICE_NAME_MAX_LEN);
    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);
    let device_type = DeviceType::from(data[DEVICE_TYPE_OFFSET]);
    let pitch = Pitch(bytes_to_number(data, BEAT_PITCH_OFFSET, BEAT_PITCH_LEN) as i32);
    let raw_bpm = bytes_to_number(data, BEAT_BPM_OFFSET, 2);
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

/// Parse a precise position packet (type 0x0b) from raw bytes.
///
/// CDJ-3000 and newer send these ~every 30 ms while a track is loaded.
pub fn parse_precise_position(data: &[u8]) -> Result<PrecisePosition> {
    let pkt_type = parse_header(data)?;
    if pkt_type != PacketType::PrecisePosition {
        return Err(ProDjLinkError::Parse(format!(
            "expected PrecisePosition (0x0b) packet, got {:?}",
            pkt_type
        )));
    }

    if data.len() != PRECISE_POSITION_LENGTH {
        return Err(ProDjLinkError::PacketTooShort {
            expected: PRECISE_POSITION_LENGTH,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, DEVICE_NAME_OFFSET, DEVICE_NAME_MAX_LEN);
    let device_number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);

    let track_length = bytes_to_number(data, PP_TRACK_LENGTH_OFFSET, 4);
    let position_ms = bytes_to_number(data, PP_POSITION_OFFSET, 4);

    // Pitch is a signed 4-byte value representing effective tempo percentage × 100.
    // Convert from signed big-endian and then to standard pitch range (0–2097152).
    let raw_pitch = i32::from_be_bytes([
        data[PP_PITCH_OFFSET],
        data[PP_PITCH_OFFSET + 1],
        data[PP_PITCH_OFFSET + 2],
        data[PP_PITCH_OFFSET + 3],
    ]);
    let percentage = raw_pitch as f64 / 100.0;
    let pitch = Pitch::from_percentage(percentage);

    // BPM is a 4-byte value; multiply by 10 gives standard "bpm × 100" integer,
    // so dividing by 10 yields effective BPM directly.
    let raw_bpm = bytes_to_number(data, PP_BPM_OFFSET, 4);
    let effective_bpm = Bpm(raw_bpm as f64 / 10.0);

    Ok(PrecisePosition {
        name,
        device_number,
        track_length,
        position_ms,
        pitch,
        effective_bpm,
        timestamp: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::header::MAGIC_HEADER;

    /// Build a beat packet with correct offsets (verified against Beat.java).
    fn make_beat_packet(
        name: &str,
        device_num: u8,
        device_type: u8,
        bpm_hundredths: u16,
        pitch_raw: u32,
        beat_in_bar: u8,
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; BEAT_PACKET_LENGTH];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x28;

        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(DEVICE_NAME_MAX_LEN);
        pkt[DEVICE_NAME_OFFSET..DEVICE_NAME_OFFSET + copy_len]
            .copy_from_slice(&name_bytes[..copy_len]);

        pkt[DEVICE_NUMBER_OFFSET] = device_num;
        pkt[DEVICE_TYPE_OFFSET] = device_type;

        // 3-byte pitch at 0x55 (big-endian, lower 3 bytes of u32)
        let pitch_be = pitch_raw.to_be_bytes();
        pkt[BEAT_PITCH_OFFSET..BEAT_PITCH_OFFSET + 3].copy_from_slice(&pitch_be[1..4]);

        pkt[BEAT_BPM_OFFSET..BEAT_BPM_OFFSET + 2].copy_from_slice(&bpm_hundredths.to_be_bytes());
        pkt[BEAT_WITHIN_BAR_OFFSET] = beat_in_bar;
        pkt
    }

    /// Build a PrecisePosition packet (verified against PrecisePosition.java).
    fn make_precise_position_packet(
        name: &str,
        device_num: u8,
        track_length: u32,
        position_ms: u32,
        raw_pitch_pct100: i32,
        raw_bpm_tenths: u32,
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; PRECISE_POSITION_LENGTH];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x0b;

        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(DEVICE_NAME_MAX_LEN);
        pkt[DEVICE_NAME_OFFSET..DEVICE_NAME_OFFSET + copy_len]
            .copy_from_slice(&name_bytes[..copy_len]);

        pkt[DEVICE_NUMBER_OFFSET] = device_num;

        pkt[PP_TRACK_LENGTH_OFFSET..PP_TRACK_LENGTH_OFFSET + 4]
            .copy_from_slice(&track_length.to_be_bytes());
        pkt[PP_POSITION_OFFSET..PP_POSITION_OFFSET + 4]
            .copy_from_slice(&position_ms.to_be_bytes());
        pkt[PP_PITCH_OFFSET..PP_PITCH_OFFSET + 4]
            .copy_from_slice(&raw_pitch_pct100.to_be_bytes());
        pkt[PP_BPM_OFFSET..PP_BPM_OFFSET + 4]
            .copy_from_slice(&raw_bpm_tenths.to_be_bytes());
        pkt
    }

    // -- parse_beat tests --

    #[test]
    fn parse_beat_valid() {
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
    fn parse_beat_cdj3000() {
        let pkt = make_beat_packet("CDJ-3000", 1, 1, 14050, 0x100000, 0);
        let beat = parse_beat(&pkt).unwrap();
        assert_eq!(beat.beat_within_bar, 0);
        assert!((beat.bpm.0 - 140.50).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_beat_pitched() {
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
        let short = &pkt[..0x30];
        let err = parse_beat(short).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: BEAT_PACKET_LENGTH,
                ..
            }
        ));
    }

    #[test]
    fn parse_beat_wrong_type() {
        let mut pkt = make_beat_packet("X", 1, 1, 12800, 0, 1);
        pkt[0x0a] = 0x06;
        let err = parse_beat(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::Parse(_)));
    }

    // -- parse_precise_position tests --

    #[test]
    fn parse_precise_position_valid() {
        // +0% pitch (raw = 0), 128.5 effective BPM (raw = 1285)
        let pkt = make_precise_position_packet("CDJ-3000", 2, 300, 45000, 0, 1285);
        let pp = parse_precise_position(&pkt).unwrap();

        assert_eq!(pp.name, "CDJ-3000");
        assert_eq!(pp.device_number, DeviceNumber(2));
        assert_eq!(pp.track_length, 300);
        assert_eq!(pp.position_ms, 45000);
        assert!((pp.effective_bpm.0 - 128.5).abs() < f64::EPSILON);
        assert!((pp.pitch.to_percentage()).abs() < 0.01);
    }

    #[test]
    fn parse_precise_position_pitched() {
        // +6% pitch → raw = 600 (6.00 × 100)
        let pkt = make_precise_position_packet("CDJ-3000", 1, 240, 10000, 600, 1362);
        let pp = parse_precise_position(&pkt).unwrap();

        assert!((pp.pitch.to_percentage() - 6.0).abs() < 0.01);
        assert!((pp.effective_bpm.0 - 136.2).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_precise_position_negative_pitch() {
        // -3.5% pitch → raw = -350
        let pkt = make_precise_position_packet("CDJ-3000", 3, 180, 5000, -350, 1238);
        let pp = parse_precise_position(&pkt).unwrap();

        assert!((pp.pitch.to_percentage() - (-3.5)).abs() < 0.01);
    }

    #[test]
    fn parse_precise_position_wrong_length() {
        let mut pkt = vec![0u8; 0x30]; // too short
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x0b;
        let err = parse_precise_position(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::PacketTooShort { .. }));
    }

    #[test]
    fn parse_precise_position_wrong_type() {
        let mut pkt = vec![0u8; PRECISE_POSITION_LENGTH];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x28;
        let err = parse_precise_position(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::Parse(_)));
    }
}
