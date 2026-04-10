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
}
