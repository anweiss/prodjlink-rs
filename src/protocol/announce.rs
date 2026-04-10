use std::net::Ipv4Addr;
use std::time::Instant;

use crate::device::types::{DeviceNumber, DeviceType};
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{PacketType, MAGIC_HEADER, PACKET_TYPE_OFFSET};
use crate::util::{bytes_to_ipv4, bytes_to_mac, read_device_name};

/// Minimum size for a keep-alive packet (through IP address at 0x2a..0x2e).
const KEEP_ALIVE_MIN_SIZE: usize = 0x2e;

// Keep-alive field offsets
const NAME_OFFSET: usize = 0x0c;
const NAME_MAX_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x21;
const DEVICE_TYPE_OFFSET: usize = 0x23;
const MAC_OFFSET: usize = 0x24;
const IP_OFFSET: usize = 0x2a;

/// Total size of a keep-alive packet we build.
const KEEP_ALIVE_PACKET_SIZE: usize = 0x36;

/// A device announcement received on the discovery port.
#[derive(Debug, Clone)]
pub struct DeviceAnnouncement {
    /// The name reported by the device (e.g. "CDJ-2000NXS2").
    pub name: String,
    /// The device number (player number).
    pub number: DeviceNumber,
    /// The type of device.
    pub device_type: DeviceType,
    /// MAC address of the device.
    pub mac_address: [u8; 6],
    /// IP address of the device.
    pub ip_address: Ipv4Addr,
    /// When this announcement was last received.
    pub last_seen: Instant,
}

/// Parse a keep-alive packet (type 0x06) from raw bytes.
pub fn parse_keep_alive(data: &[u8]) -> Result<DeviceAnnouncement> {
    let pkt_type = crate::protocol::header::parse_header(data)?;

    if pkt_type != PacketType::DeviceKeepAlive {
        let raw_type = data[PACKET_TYPE_OFFSET];
        return Err(ProDjLinkError::InvalidPacketType(raw_type));
    }

    if data.len() < KEEP_ALIVE_MIN_SIZE {
        return Err(ProDjLinkError::PacketTooShort {
            expected: KEEP_ALIVE_MIN_SIZE,
            actual: data.len(),
        });
    }

    let name = read_device_name(data, NAME_OFFSET, NAME_MAX_LEN);
    let number = DeviceNumber::from(data[DEVICE_NUMBER_OFFSET]);
    let device_type = DeviceType::from(data[DEVICE_TYPE_OFFSET]);
    let mac_address = bytes_to_mac(data, MAC_OFFSET);
    let ip_address = bytes_to_ipv4(data, IP_OFFSET);

    Ok(DeviceAnnouncement {
        name,
        number,
        device_type,
        mac_address,
        ip_address,
        last_seen: Instant::now(),
    })
}

/// Build a keep-alive packet for a virtual CDJ to send.
///
/// This allows our software to appear on the DJ Link network as a CDJ device.
pub fn build_keep_alive(
    name: &str,
    device_number: DeviceNumber,
    mac_address: [u8; 6],
    ip_address: Ipv4Addr,
) -> Vec<u8> {
    let mut pkt = vec![0u8; KEEP_ALIVE_PACKET_SIZE];

    // Magic header
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);

    // Packet type
    pkt[PACKET_TYPE_OFFSET] = 0x06;

    // Name length field (byte 0x0b) — name subtype marker used by protocol
    pkt[0x0b] = NAME_MAX_LEN as u8;

    // Device name (null-padded to NAME_MAX_LEN bytes)
    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // Packet structure length marker at 0x20
    pkt[0x20] = 0x01;

    // Device number
    pkt[DEVICE_NUMBER_OFFSET] = device_number.0;

    // Device type — always CDJ for a virtual player
    pkt[DEVICE_TYPE_OFFSET] = u8::from(DeviceType::Cdj);

    // MAC address
    pkt[MAC_OFFSET..MAC_OFFSET + 6].copy_from_slice(&mac_address);

    // IP address
    let octets = ip_address.octets();
    pkt[IP_OFFSET..IP_OFFSET + 4].copy_from_slice(&octets);

    pkt
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid keep-alive packet by hand for testing.
    fn make_keep_alive_packet(
        name: &str,
        device_num: u8,
        device_type: u8,
        mac: [u8; 6],
        ip: [u8; 4],
    ) -> Vec<u8> {
        let mut pkt = vec![0u8; KEEP_ALIVE_PACKET_SIZE];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[PACKET_TYPE_OFFSET] = 0x06;
        let nb = name.as_bytes();
        let copy_len = nb.len().min(NAME_MAX_LEN);
        pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&nb[..copy_len]);
        pkt[DEVICE_NUMBER_OFFSET] = device_num;
        pkt[DEVICE_TYPE_OFFSET] = device_type;
        pkt[MAC_OFFSET..MAC_OFFSET + 6].copy_from_slice(&mac);
        pkt[IP_OFFSET..IP_OFFSET + 4].copy_from_slice(&ip);
        pkt
    }

    #[test]
    fn parse_handcrafted_keep_alive() {
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let ip = [192, 168, 1, 42];
        let pkt = make_keep_alive_packet("CDJ-2000NXS2", 3, 1, mac, ip);

        let ann = parse_keep_alive(&pkt).unwrap();
        assert_eq!(ann.name, "CDJ-2000NXS2");
        assert_eq!(ann.number, DeviceNumber(3));
        assert_eq!(ann.device_type, DeviceType::Cdj);
        assert_eq!(ann.mac_address, mac);
        assert_eq!(ann.ip_address, Ipv4Addr::new(192, 168, 1, 42));
    }

    #[test]
    fn round_trip_build_then_parse() {
        let name = "VirtualCDJ";
        let number = DeviceNumber(5);
        let mac = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB];
        let ip = Ipv4Addr::new(10, 0, 0, 7);

        let pkt = build_keep_alive(name, number, mac, ip);
        let ann = parse_keep_alive(&pkt).unwrap();

        assert_eq!(ann.name, name);
        assert_eq!(ann.number, number);
        assert_eq!(ann.device_type, DeviceType::Cdj);
        assert_eq!(ann.mac_address, mac);
        assert_eq!(ann.ip_address, ip);
    }

    #[test]
    fn reject_too_short_packet() {
        let mut pkt = MAGIC_HEADER.to_vec();
        pkt.push(0x06); // type byte — valid header but way too short
        let err = parse_keep_alive(&pkt).unwrap_err();
        assert!(matches!(
            err,
            ProDjLinkError::PacketTooShort {
                expected: KEEP_ALIVE_MIN_SIZE,
                actual: 11,
            }
        ));
    }

    #[test]
    fn reject_wrong_packet_type() {
        let mut pkt = vec![0u8; KEEP_ALIVE_PACKET_SIZE];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[PACKET_TYPE_OFFSET] = 0x0a; // DeviceHello, not KeepAlive
        let err = parse_keep_alive(&pkt).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidPacketType(0x0a)));
    }

    #[test]
    fn parse_mixer_device_type() {
        let mac = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let ip = [172, 16, 0, 1];
        let pkt = make_keep_alive_packet("DJM-900NXS2", 33, 2, mac, ip);

        let ann = parse_keep_alive(&pkt).unwrap();
        assert_eq!(ann.device_type, DeviceType::Mixer);
        assert_eq!(ann.number, DeviceNumber(33));
    }

    #[test]
    fn build_packet_size() {
        let pkt = build_keep_alive(
            "Test",
            DeviceNumber(1),
            [0; 6],
            Ipv4Addr::LOCALHOST,
        );
        assert_eq!(pkt.len(), KEEP_ALIVE_PACKET_SIZE);
    }

    #[test]
    fn name_truncated_to_max_len() {
        let long_name = "A_Very_Long_Device_Name_Exceeding_Twenty";
        let pkt = build_keep_alive(
            long_name,
            DeviceNumber(1),
            [0; 6],
            Ipv4Addr::LOCALHOST,
        );
        let ann = parse_keep_alive(&pkt).unwrap();
        assert_eq!(ann.name.len(), NAME_MAX_LEN);
        assert_eq!(ann.name, &long_name[..NAME_MAX_LEN]);
    }
}
