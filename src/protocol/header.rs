/// The magic 10-byte header that starts every DJ Link UDP packet.
/// ASCII: "Qspt1WmJOL"
pub const MAGIC_HEADER: [u8; 10] = [0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c];

/// Well-known UDP ports
pub const DISCOVERY_PORT: u16 = 50000;
pub const BEAT_PORT: u16 = 50001;
pub const STATUS_PORT: u16 = 50002;

/// TCP port for dbserver discovery
pub const DB_SERVER_QUERY_PORT: u16 = 12523;

/// Minimum packet size (header + type byte)
pub const MIN_PACKET_SIZE: usize = 11;

/// Offset of the packet type byte
pub const PACKET_TYPE_OFFSET: usize = 0x0a;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketType {
    /// Device initial hello/claim stage 1 (0x0a on port 50000)
    DeviceHello,
    /// Device number claim stage 1 (0x00 on port 50000)
    DeviceClaimStage1,
    /// Device number claim stage 2 (0x02 on port 50000)
    DeviceClaimStage2,
    /// Device number claim stage 3 (0x04 on port 50000)
    DeviceClaimStage3,
    /// Device keep-alive announcement (0x06 on port 50000)
    DeviceKeepAlive,
    /// CDJ status update (0x0a on port 50002)
    CdjStatus,
    /// Mixer status update (0x29 on port 50002)
    MixerStatus,
    /// Beat packet (0x28 on port 50001)
    Beat,
    /// Precise position packet (CDJ-3000+) (0x7f on port 50001)
    PrecisePosition,
    /// Sync control
    SyncControl,
    /// Fader start/stop
    FaderStart,
    /// Load track command
    LoadTrack,
    /// Media query
    MediaQuery,
    /// Media response
    MediaResponse,
    /// On-air status
    OnAir,
    /// Unknown type
    Unknown(u8),
}

impl From<u8> for PacketType {
    fn from(value: u8) -> Self {
        match value {
            0x00 => PacketType::DeviceClaimStage1,
            0x02 => PacketType::DeviceClaimStage2,
            0x04 => PacketType::DeviceClaimStage3,
            0x06 => PacketType::DeviceKeepAlive,
            0x0a => PacketType::DeviceHello,
            0x28 => PacketType::Beat,
            0x29 => PacketType::MixerStatus,
            other => PacketType::Unknown(other),
        }
    }
}

impl PacketType {
    /// Disambiguate packet type using the port number.
    ///
    /// Type byte 0x0a means `DeviceHello` on the discovery port (50000)
    /// but `CdjStatus` on the status port (50002).
    pub fn from_u8_on_port(byte: u8, port: u16) -> PacketType {
        match (byte, port) {
            (0x0a, STATUS_PORT) => PacketType::CdjStatus,
            _ => PacketType::from(byte),
        }
    }
}

/// Validate the magic header and extract the packet type byte.
pub fn parse_header(data: &[u8]) -> crate::error::Result<PacketType> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(crate::error::ProDjLinkError::PacketTooShort {
            expected: MIN_PACKET_SIZE,
            actual: data.len(),
        });
    }
    if data[..10] != MAGIC_HEADER {
        return Err(crate::error::ProDjLinkError::InvalidMagic);
    }
    Ok(PacketType::from(data[PACKET_TYPE_OFFSET]))
}

/// Same as [`parse_header`] but uses port to disambiguate overlapping type values.
pub fn parse_header_on_port(data: &[u8], port: u16) -> crate::error::Result<PacketType> {
    if data.len() < MIN_PACKET_SIZE {
        return Err(crate::error::ProDjLinkError::PacketTooShort {
            expected: MIN_PACKET_SIZE,
            actual: data.len(),
        });
    }
    if data[..10] != MAGIC_HEADER {
        return Err(crate::error::ProDjLinkError::InvalidMagic);
    }
    Ok(PacketType::from_u8_on_port(data[PACKET_TYPE_OFFSET], port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(type_byte: u8) -> Vec<u8> {
        let mut pkt = MAGIC_HEADER.to_vec();
        pkt.push(type_byte);
        pkt
    }

    #[test]
    fn parse_valid_header() {
        let pkt = make_packet(0x06);
        let pt = parse_header(&pkt).unwrap();
        assert_eq!(pt, PacketType::DeviceKeepAlive);
    }

    #[test]
    fn parse_beat_packet() {
        let pkt = make_packet(0x28);
        assert_eq!(parse_header(&pkt).unwrap(), PacketType::Beat);
    }

    #[test]
    fn parse_unknown_type() {
        let pkt = make_packet(0xff);
        assert_eq!(parse_header(&pkt).unwrap(), PacketType::Unknown(0xff));
    }

    #[test]
    fn reject_too_short_packet() {
        let short = &MAGIC_HEADER[..5];
        let err = parse_header(short).unwrap_err();
        assert!(
            matches!(err, crate::error::ProDjLinkError::PacketTooShort { expected: 11, actual: 5 })
        );
    }

    #[test]
    fn reject_invalid_magic() {
        let mut pkt = make_packet(0x06);
        pkt[0] = 0x00; // corrupt first byte
        let err = parse_header(&pkt).unwrap_err();
        assert!(matches!(err, crate::error::ProDjLinkError::InvalidMagic));
    }

    #[test]
    fn disambiguate_0x0a_on_discovery_port() {
        let pkt = make_packet(0x0a);
        let pt = parse_header_on_port(&pkt, DISCOVERY_PORT).unwrap();
        assert_eq!(pt, PacketType::DeviceHello);
    }

    #[test]
    fn disambiguate_0x0a_on_status_port() {
        let pkt = make_packet(0x0a);
        let pt = parse_header_on_port(&pkt, STATUS_PORT).unwrap();
        assert_eq!(pt, PacketType::CdjStatus);
    }

    #[test]
    fn port_aware_fallback_for_other_types() {
        let pkt = make_packet(0x29);
        let pt = parse_header_on_port(&pkt, STATUS_PORT).unwrap();
        assert_eq!(pt, PacketType::MixerStatus);
    }
}
