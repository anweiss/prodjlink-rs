use std::net::Ipv4Addr;

// --- Port constants ---

pub const DISCOVERY_PORT: u16 = 50000;
pub const BEAT_PORT: u16 = 50001;
pub const STATUS_PORT: u16 = 50002;
pub const DB_SERVER_QUERY_PORT: u16 = 12523;

// --- Pitch constants ---

/// The pitch value representing normal (100%) playback speed.
pub const PITCH_NORMAL: i32 = 0x100000;

// --- Byte manipulation ---

/// Read a big-endian unsigned integer from a byte slice at the given offset.
/// Supports 1, 2, 3, or 4 byte reads.
pub fn bytes_to_number(data: &[u8], offset: usize, len: usize) -> u32 {
    match len {
        1 => data[offset] as u32,
        2 => u16::from_be_bytes([data[offset], data[offset + 1]]) as u32,
        3 => {
            (data[offset] as u32) << 16
                | (data[offset + 1] as u32) << 8
                | (data[offset + 2] as u32)
        }
        4 => u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]),
        _ => panic!("bytes_to_number: len must be 1..=4, got {len}"),
    }
}

/// Read a little-endian unsigned integer from a byte slice at the given offset.
/// Supports 1, 2, 3, or 4 byte reads.
pub fn bytes_to_number_le(data: &[u8], offset: usize, len: usize) -> u32 {
    match len {
        1 => data[offset] as u32,
        2 => u16::from_le_bytes([data[offset], data[offset + 1]]) as u32,
        3 => {
            (data[offset] as u32)
                | (data[offset + 1] as u32) << 8
                | (data[offset + 2] as u32) << 16
        }
        4 => u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]),
        _ => panic!("bytes_to_number_le: len must be 1..=4, got {len}"),
    }
}

/// Write a big-endian integer to a byte buffer at the given offset.
/// Supports 1, 2, 3, or 4 byte writes.
pub fn number_to_bytes(value: u32, buf: &mut [u8], offset: usize, len: usize) {
    let be = value.to_be_bytes();
    // The last `len` bytes of the 4-byte big-endian representation are the
    // significant ones (e.g. for len=2 we want bytes [2] and [3]).
    buf[offset..offset + len].copy_from_slice(&be[4 - len..]);
}

/// Read a null-terminated ASCII string from a byte slice.
/// Reads up to `max_len` bytes starting at `offset`, stopping at the first
/// null byte or when `max_len` bytes have been consumed.
pub fn read_device_name(data: &[u8], offset: usize, max_len: usize) -> String {
    let end = (offset + max_len).min(data.len());
    let slice = &data[offset..end];
    let nul_pos = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..nul_pos]).into_owned()
}

// --- Tempo / Pitch math ---

/// Convert raw pitch integer to a percentage (-100.0 to +100.0 range around 0).
/// `pitch == PITCH_NORMAL` → 0%, `pitch == 2 * PITCH_NORMAL` → +100%.
pub fn pitch_to_percentage(pitch: i32) -> f64 {
    ((pitch as f64) - (PITCH_NORMAL as f64)) / (PITCH_NORMAL as f64) * 100.0
}

/// Convert raw pitch to a speed multiplier (1.0 = normal speed).
pub fn pitch_to_multiplier(pitch: i32) -> f64 {
    (pitch as f64) / (PITCH_NORMAL as f64)
}

/// Convert a percentage back to a raw pitch value.
pub fn percentage_to_pitch(pct: f64) -> i32 {
    ((pct / 100.0) * (PITCH_NORMAL as f64) + (PITCH_NORMAL as f64)) as i32
}

// --- Network helpers ---

/// Convert 4 bytes at the given offset to an IPv4 address.
pub fn bytes_to_ipv4(data: &[u8], offset: usize) -> Ipv4Addr {
    Ipv4Addr::new(
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    )
}

/// Convert 6 bytes at the given offset to a MAC address as `[u8; 6]`.
pub fn bytes_to_mac(data: &[u8], offset: usize) -> [u8; 6] {
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&data[offset..offset + 6]);
    mac
}

/// Format a MAC address as a colon-separated hex string.
pub fn format_mac(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

// --- Half-frame / time conversion ---

/// Convert a half-frame position (1/150s units, used in cue lists) to milliseconds.
pub fn half_frame_to_time(half_frame: u32) -> u64 {
    (half_frame as u64) * 100 / 15
}

/// Convert milliseconds to half-frame position.
pub fn time_to_half_frame(ms: u64) -> u32 {
    ((ms * 15 + 50) / 100) as u32
}

/// Convert milliseconds to half-frame position (rounded up).
pub fn time_to_half_frame_rounded(ms: u64) -> u32 {
    ((ms * 15 + 99) / 100) as u32
}

// --- Phrase colour constants (from Java Util.java) ---

/// Phrase visualization colour constants (RGB tuples) — low/mid/high intensities.
pub const PHRASE_LOW_RED: (u8, u8, u8) = (0xc8, 0x00, 0x00);
pub const PHRASE_MID_RED: (u8, u8, u8) = (0xff, 0x00, 0x00);
pub const PHRASE_HIGH_RED: (u8, u8, u8) = (0xff, 0x50, 0x50);
pub const PHRASE_LOW_PINK: (u8, u8, u8) = (0xc8, 0x00, 0xc8);
pub const PHRASE_MID_PINK: (u8, u8, u8) = (0xff, 0x00, 0xff);
pub const PHRASE_HIGH_PINK: (u8, u8, u8) = (0xff, 0x60, 0xff);
pub const PHRASE_LOW_BLUE: (u8, u8, u8) = (0x00, 0x40, 0xc8);
pub const PHRASE_MID_BLUE: (u8, u8, u8) = (0x00, 0x64, 0xff);
pub const PHRASE_HIGH_BLUE: (u8, u8, u8) = (0x50, 0xb4, 0xff);
pub const PHRASE_LOW_GREEN: (u8, u8, u8) = (0x00, 0xc8, 0x00);
pub const PHRASE_MID_GREEN: (u8, u8, u8) = (0x00, 0xff, 0x00);
pub const PHRASE_HIGH_GREEN: (u8, u8, u8) = (0x60, 0xff, 0x60);
pub const PHRASE_LOW_PURPLE: (u8, u8, u8) = (0x60, 0x00, 0xd8);
pub const PHRASE_MID_PURPLE: (u8, u8, u8) = (0x80, 0x00, 0xff);
pub const PHRASE_HIGH_PURPLE: (u8, u8, u8) = (0xb4, 0x64, 0xff);

/// Map a phrase mood and intensity to a display colour.
///
/// `mood` selects the hue family (1 = red, 2 = orange/pink, 3 = blue,
/// 4 = green, 5 = purple; other values fall back to white).
/// `intensity` selects brightness: 0 → low, 1 → mid, ≥2 → high.
pub fn phrase_color(mood: u8, intensity: u8) -> (u8, u8, u8) {
    match mood {
        1 => match intensity {
            0 => PHRASE_LOW_RED,
            1 => PHRASE_MID_RED,
            _ => PHRASE_HIGH_RED,
        },
        2 => match intensity {
            0 => PHRASE_LOW_PINK,
            1 => PHRASE_MID_PINK,
            _ => PHRASE_HIGH_PINK,
        },
        3 => match intensity {
            0 => PHRASE_LOW_BLUE,
            1 => PHRASE_MID_BLUE,
            _ => PHRASE_HIGH_BLUE,
        },
        4 => match intensity {
            0 => PHRASE_LOW_GREEN,
            1 => PHRASE_MID_GREEN,
            _ => PHRASE_HIGH_GREEN,
        },
        5 => match intensity {
            0 => PHRASE_LOW_PURPLE,
            1 => PHRASE_MID_PURPLE,
            _ => PHRASE_HIGH_PURPLE,
        },
        _ => (0xff, 0xff, 0xff), // unknown mood → white
    }
}

/// Human-readable label for a phrase mood value.
pub fn phrase_label(mood: u8) -> &'static str {
    match mood {
        1 => "High",
        2 => "Mid",
        3 => "Low",
        4 => "Verse",
        5 => "Chorus",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- bytes_to_number ---

    #[test]
    fn bytes_to_number_1_byte() {
        let data = [0x00, 0xAB, 0x00];
        assert_eq!(bytes_to_number(&data, 1, 1), 0xAB);
    }

    #[test]
    fn bytes_to_number_2_bytes() {
        let data = [0x01, 0x02];
        assert_eq!(bytes_to_number(&data, 0, 2), 0x0102);
    }

    #[test]
    fn bytes_to_number_3_bytes() {
        let data = [0xFF, 0x01, 0x02, 0x03, 0xFF];
        assert_eq!(bytes_to_number(&data, 1, 3), 0x010203);
    }

    #[test]
    fn bytes_to_number_4_bytes() {
        let data = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(bytes_to_number(&data, 0, 4), 0xDEADBEEF);
    }

    // --- bytes_to_number_le ---

    #[test]
    fn bytes_to_number_le_2_bytes() {
        // LE: 0x02 0x01 → 0x0102
        let data = [0x02, 0x01];
        assert_eq!(bytes_to_number_le(&data, 0, 2), 0x0102);
    }

    #[test]
    fn bytes_to_number_le_4_bytes() {
        // LE representation of 0xDEADBEEF: [0xEF, 0xBE, 0xAD, 0xDE]
        let data = [0xEF, 0xBE, 0xAD, 0xDE];
        assert_eq!(bytes_to_number_le(&data, 0, 4), 0xDEADBEEF);
    }

    // --- number_to_bytes ---

    #[test]
    fn number_to_bytes_round_trip() {
        let mut buf = [0u8; 4];
        number_to_bytes(0xDEADBEEF, &mut buf, 0, 4);
        assert_eq!(bytes_to_number(&buf, 0, 4), 0xDEADBEEF);
    }

    #[test]
    fn number_to_bytes_2_byte() {
        let mut buf = [0u8; 6];
        number_to_bytes(0x1234, &mut buf, 2, 2);
        assert_eq!(buf, [0x00, 0x00, 0x12, 0x34, 0x00, 0x00]);
    }

    // --- pitch conversions ---

    #[test]
    fn pitch_normal_is_zero_pct() {
        assert!((pitch_to_percentage(PITCH_NORMAL) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pitch_double_is_100_pct() {
        assert!((pitch_to_percentage(2 * PITCH_NORMAL) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pitch_multiplier_normal() {
        assert!((pitch_to_multiplier(PITCH_NORMAL) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pitch_round_trip() {
        let pct = 8.4;
        let pitch = percentage_to_pitch(pct);
        let back = pitch_to_percentage(pitch);
        // Allow small floating-point truncation error from the i32 cast
        assert!((back - pct).abs() < 0.01, "expected ~{pct}, got {back}");
    }

    #[test]
    fn pitch_round_trip_negative() {
        let pct = -6.0;
        let pitch = percentage_to_pitch(pct);
        let back = pitch_to_percentage(pitch);
        assert!((back - pct).abs() < 0.01, "expected ~{pct}, got {back}");
    }

    // --- network helpers ---

    #[test]
    fn ipv4_conversion() {
        let data = [0x00, 192, 168, 1, 100, 0x00];
        assert_eq!(bytes_to_ipv4(&data, 1), Ipv4Addr::new(192, 168, 1, 100));
    }

    #[test]
    fn mac_conversion() {
        let data = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        assert_eq!(bytes_to_mac(&data, 0), [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn mac_format() {
        let mac = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB];
        assert_eq!(format_mac(&mac), "01:23:45:67:89:ab");
    }

    // --- read_device_name ---

    #[test]
    fn read_device_name_with_null() {
        let mut data = [0u8; 20];
        let name = b"CDJ-2000";
        data[4..4 + name.len()].copy_from_slice(name);
        // byte after name is already 0 (null terminator)
        assert_eq!(read_device_name(&data, 4, 16), "CDJ-2000");
    }

    #[test]
    fn read_device_name_no_null() {
        let data = b"ABCDEFGHIJ";
        assert_eq!(read_device_name(data, 0, 10), "ABCDEFGHIJ");
    }

    #[test]
    fn read_device_name_max_len_limits() {
        let data = b"Hello World";
        assert_eq!(read_device_name(data, 0, 5), "Hello");
    }

    // --- half_frame / time conversions ---

    #[test]
    fn half_frame_to_time_zero() {
        assert_eq!(half_frame_to_time(0), 0);
    }

    #[test]
    fn half_frame_to_time_known_value() {
        // 150 half-frames = 1 second = 1000 ms
        // 150 * 100 / 15 = 1000
        assert_eq!(half_frame_to_time(150), 1000);
    }

    #[test]
    fn half_frame_to_time_one() {
        // 1 half-frame = 100/15 = 6 ms (integer division)
        assert_eq!(half_frame_to_time(1), 6);
    }

    #[test]
    fn time_to_half_frame_zero() {
        assert_eq!(time_to_half_frame(0), 0);
    }

    #[test]
    fn time_to_half_frame_one_second() {
        // 1000 ms → (1000*15 + 50) / 100 = 15050/100 = 150
        assert_eq!(time_to_half_frame(1000), 150);
    }

    #[test]
    fn time_to_half_frame_rounded_one_second() {
        assert_eq!(time_to_half_frame_rounded(1000), 150);
    }

    #[test]
    fn time_to_half_frame_rounded_small() {
        // 1 ms → (1*15 + 99) / 100 = 114/100 = 1
        assert_eq!(time_to_half_frame_rounded(1), 1);
    }

    #[test]
    fn half_frame_round_trip() {
        let ms = 5000u64;
        let hf = time_to_half_frame(ms);
        let back = half_frame_to_time(hf);
        // Allow ±1 ms rounding error
        assert!((back as i64 - ms as i64).unsigned_abs() <= 1);
    }

    // --- phrase_color ---

    #[test]
    fn phrase_color_red_variants() {
        assert_eq!(phrase_color(1, 0), PHRASE_LOW_RED);
        assert_eq!(phrase_color(1, 1), PHRASE_MID_RED);
        assert_eq!(phrase_color(1, 2), PHRASE_HIGH_RED);
    }

    #[test]
    fn phrase_color_pink_variants() {
        assert_eq!(phrase_color(2, 0), PHRASE_LOW_PINK);
        assert_eq!(phrase_color(2, 1), PHRASE_MID_PINK);
        assert_eq!(phrase_color(2, 2), PHRASE_HIGH_PINK);
    }

    #[test]
    fn phrase_color_blue_variants() {
        assert_eq!(phrase_color(3, 0), PHRASE_LOW_BLUE);
        assert_eq!(phrase_color(3, 1), PHRASE_MID_BLUE);
        assert_eq!(phrase_color(3, 2), PHRASE_HIGH_BLUE);
    }

    #[test]
    fn phrase_color_green_variants() {
        assert_eq!(phrase_color(4, 0), PHRASE_LOW_GREEN);
        assert_eq!(phrase_color(4, 1), PHRASE_MID_GREEN);
        assert_eq!(phrase_color(4, 2), PHRASE_HIGH_GREEN);
    }

    #[test]
    fn phrase_color_purple_variants() {
        assert_eq!(phrase_color(5, 0), PHRASE_LOW_PURPLE);
        assert_eq!(phrase_color(5, 1), PHRASE_MID_PURPLE);
        assert_eq!(phrase_color(5, 2), PHRASE_HIGH_PURPLE);
    }

    #[test]
    fn phrase_color_unknown_mood_returns_white() {
        assert_eq!(phrase_color(0, 0), (0xff, 0xff, 0xff));
        assert_eq!(phrase_color(99, 1), (0xff, 0xff, 0xff));
    }

    #[test]
    fn phrase_color_high_intensity_clamp() {
        // intensity ≥ 2 always returns high
        assert_eq!(phrase_color(1, 5), PHRASE_HIGH_RED);
        assert_eq!(phrase_color(3, 255), PHRASE_HIGH_BLUE);
    }

    // --- phrase_label ---

    #[test]
    fn phrase_label_known_moods() {
        assert_eq!(phrase_label(1), "High");
        assert_eq!(phrase_label(2), "Mid");
        assert_eq!(phrase_label(3), "Low");
        assert_eq!(phrase_label(4), "Verse");
        assert_eq!(phrase_label(5), "Chorus");
    }

    #[test]
    fn phrase_label_unknown_mood() {
        assert_eq!(phrase_label(0), "Unknown");
        assert_eq!(phrase_label(6), "Unknown");
        assert_eq!(phrase_label(255), "Unknown");
    }
}
