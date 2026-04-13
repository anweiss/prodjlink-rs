use std::net::Ipv4Addr;
use std::time::Instant;

use crate::device::types::{DeviceNumber, DeviceType};
use crate::error::{ProDjLinkError, Result};
use crate::protocol::header::{MAGIC_HEADER, PACKET_TYPE_OFFSET, PacketType};
use crate::util::{bytes_to_ipv4, bytes_to_mac, read_device_name};

/// Required size for a keep-alive packet (Java requires exactly 0x36 bytes).
const KEEP_ALIVE_MIN_SIZE: usize = 0x36;
const DEVICE_LIBRARY_PLUS_OFFSET: usize = 0x36;

// Keep-alive field offsets (distinct from status-packet offsets on port 50002)
const NAME_OFFSET: usize = 0x0c;
const NAME_MAX_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x24;
const DEVICE_TYPE_OFFSET: usize = 0x25;
const MAC_OFFSET: usize = 0x26;
const IP_OFFSET: usize = 0x2c;
const PEER_COUNT_OFFSET: usize = 0x30;
const DEVICE_TYPE_MIRROR_OFFSET: usize = 0x34;

/// Total size of a keep-alive packet we build.
const KEEP_ALIVE_PACKET_SIZE: usize = 0x36;

// === Claim / Defense Packet Sizes ===
const HELLO_PACKET_SIZE: usize = 0x26;
const CLAIM_STAGE1_PACKET_SIZE: usize = 0x2c;
const CLAIM_STAGE2_PACKET_SIZE: usize = 0x32;
const CLAIM_STAGE3_PACKET_SIZE: usize = 0x26;
const DEFENSE_PACKET_SIZE: usize = 0x29;

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
    /// Number of peers the device can see on the network.
    pub peer_count: u8,
    /// Whether this device is an Opus Quad (all-in-one unit).
    pub is_opus_quad: bool,
    /// Whether this device is an XDJ-AZ.
    pub is_xdj_az: bool,
    /// Whether the device advertises Device Library Plus support.
    pub is_using_device_library_plus: bool,
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
    let peer_count = data[PEER_COUNT_OFFSET];
    let is_opus_quad = name == "OPUS-QUAD";
    let is_xdj_az = name == "XDJ-AZ";
    let is_using_device_library_plus = data
        .get(DEVICE_LIBRARY_PLUS_OFFSET)
        .map_or(false, |&b| b != 0);

    Ok(DeviceAnnouncement {
        name,
        number,
        device_type,
        mac_address,
        ip_address,
        peer_count,
        is_opus_quad,
        is_xdj_az,
        is_using_device_library_plus,
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

    // Packet type (0x06); byte 0x0b stays 0x00
    pkt[PACKET_TYPE_OFFSET] = 0x06;

    // Device name (null-padded to NAME_MAX_LEN bytes)
    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    // Structure marker and keep-alive subtype
    pkt[0x20] = 0x01;
    pkt[0x21] = 0x02;

    // Packet length as u16 BE
    let len_bytes = (KEEP_ALIVE_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);

    // Device number
    pkt[DEVICE_NUMBER_OFFSET] = device_number.0;

    // Device type — always CDJ for a virtual player
    pkt[DEVICE_TYPE_OFFSET] = u8::from(DeviceType::Cdj);
    pkt[DEVICE_TYPE_MIRROR_OFFSET] = u8::from(DeviceType::Cdj);

    // MAC address
    pkt[MAC_OFFSET..MAC_OFFSET + 6].copy_from_slice(&mac_address);

    // IP address
    let octets = ip_address.octets();
    pkt[IP_OFFSET..IP_OFFSET + 4].copy_from_slice(&octets);

    pkt
}

// ---------------------------------------------------------------------------
// Claim & Defense Packet Builders
// ---------------------------------------------------------------------------

/// Build a CDJ-3000-compatible initial announcement packet (type `0x0a`).
///
/// Broadcast three times at 300 ms intervals when a virtual CDJ comes online.
pub fn build_device_hello(name: &str) -> Vec<u8> {
    let mut pkt = vec![0u8; HELLO_PACKET_SIZE];
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
    pkt[PACKET_TYPE_OFFSET] = 0x0a;

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[0x20] = 0x01;
    pkt[0x21] = 0x04; // CDJ-3000 compatible structure type
    let len_bytes = (HELLO_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);
    pkt[0x24] = 0x01; // device type: CDJ
    pkt[0x25] = 0x40; // CDJ-3000 compatibility marker

    pkt
}

/// Build a first-stage device number claim packet (type `0x00`).
///
/// `counter` should be 1, 2, or 3 for the three packets in the series.
pub fn build_claim_stage1(name: &str, mac: [u8; 6], counter: u8) -> Vec<u8> {
    let mut pkt = vec![0u8; CLAIM_STAGE1_PACKET_SIZE];
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
    pkt[PACKET_TYPE_OFFSET] = 0x00;

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[0x20] = 0x01;
    pkt[0x21] = 0x03; // CDJ-3000 compatible
    let len_bytes = (CLAIM_STAGE1_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);
    pkt[0x24] = counter;
    pkt[0x25] = 0x01; // device type: CDJ
    pkt[0x26..0x2c].copy_from_slice(&mac);

    pkt
}

/// Build a second-stage device number claim packet (type `0x02`).
///
/// `counter` should be 1, 2, or 3. `auto_assign` is `true` when the device
/// number was chosen automatically rather than requested by the user.
pub fn build_claim_stage2(
    name: &str,
    mac: [u8; 6],
    ip: Ipv4Addr,
    device_number: u8,
    counter: u8,
    auto_assign: bool,
) -> Vec<u8> {
    let mut pkt = vec![0u8; CLAIM_STAGE2_PACKET_SIZE];
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
    pkt[PACKET_TYPE_OFFSET] = 0x02;

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[0x20] = 0x01;
    pkt[0x21] = 0x03; // CDJ-3000 compatible
    let len_bytes = (CLAIM_STAGE2_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);
    pkt[0x24..0x28].copy_from_slice(&ip.octets());
    pkt[0x28..0x2e].copy_from_slice(&mac);
    pkt[0x2e] = device_number;
    pkt[0x2f] = counter;
    pkt[0x30] = 0x01;
    pkt[0x31] = if auto_assign { 0x01 } else { 0x02 };

    pkt
}

/// Build a third-stage (final) device number claim packet (type `0x04`).
///
/// `counter` should be 1, 2, or 3.
pub fn build_claim_stage3(name: &str, device_number: u8, counter: u8) -> Vec<u8> {
    let mut pkt = vec![0u8; CLAIM_STAGE3_PACKET_SIZE];
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
    pkt[PACKET_TYPE_OFFSET] = 0x04;

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[0x20] = 0x01;
    pkt[0x21] = 0x03; // CDJ-3000 compatible
    let len_bytes = (CLAIM_STAGE3_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);
    pkt[0x24] = device_number;
    pkt[0x25] = counter;

    pkt
}

/// Build a defense packet (type `0x08`) to assert ownership of a device number.
///
/// Sent directly to the IP address of a device that is trying to claim a
/// number we already own.
pub fn build_defense(name: &str, device_number: u8, ip: Ipv4Addr) -> Vec<u8> {
    let mut pkt = vec![0u8; DEFENSE_PACKET_SIZE];
    pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
    pkt[PACKET_TYPE_OFFSET] = 0x08;

    let name_bytes = name.as_bytes();
    let copy_len = name_bytes.len().min(NAME_MAX_LEN);
    pkt[NAME_OFFSET..NAME_OFFSET + copy_len].copy_from_slice(&name_bytes[..copy_len]);

    pkt[0x20] = 0x01;
    pkt[0x21] = 0x02;
    let len_bytes = (DEFENSE_PACKET_SIZE as u16).to_be_bytes();
    pkt[0x22..0x24].copy_from_slice(&len_bytes);
    pkt[0x24] = device_number;
    pkt[0x25..0x29].copy_from_slice(&ip.octets());

    pkt
}

// ---------------------------------------------------------------------------
// Claim / Defense Packet Extractors
// ---------------------------------------------------------------------------

/// Extract the defended device number from a defense packet (type `0x08`).
///
/// Returns `None` if the packet is too short.
/// Assumes the header and type have already been validated.
pub fn extract_defense_device_number(data: &[u8]) -> Option<u8> {
    if data.len() > 0x24 {
        Some(data[0x24])
    } else {
        None
    }
}

/// Extract the claimed device number from a stage-2 claim packet (type `0x02`).
///
/// Returns `None` if the packet is too short.
/// Assumes the header and type have already been validated.
pub fn extract_claim_stage2_device_number(data: &[u8]) -> Option<u8> {
    if data.len() > 0x2e {
        Some(data[0x2e])
    } else {
        None
    }
}

/// Expand a single Opus Quad keep-alive into 4 synthetic player announcements.
///
/// The Opus Quad is an all-in-one unit whose real device number is 9–12
/// (or 17–20 in lighting mode).  This function takes the original
/// announcement and returns four copies with device numbers remapped to
/// 1–4, preserving all other fields.
///
/// If the announcement is *not* from an Opus Quad the returned vec contains
/// only the original announcement unchanged.
pub fn expand_opus_quad_announcement(ann: &DeviceAnnouncement) -> Vec<DeviceAnnouncement> {
    if !ann.is_opus_quad {
        return vec![ann.clone()];
    }

    (1u8..=4)
        .map(|player| {
            let mut synth = ann.clone();
            synth.number = DeviceNumber(player);
            synth
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid keep-alive packet by hand for testing.
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
        pkt[0x20] = 0x01;
        pkt[0x21] = 0x02;
        let len_bytes = (KEEP_ALIVE_PACKET_SIZE as u16).to_be_bytes();
        pkt[0x22..0x24].copy_from_slice(&len_bytes);
        pkt[DEVICE_NUMBER_OFFSET] = device_num;
        pkt[DEVICE_TYPE_OFFSET] = device_type;
        pkt[DEVICE_TYPE_MIRROR_OFFSET] = device_type;
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
        assert!(!ann.is_opus_quad);
        assert!(!ann.is_xdj_az);
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
    fn round_trip_build_byte_layout() {
        let pkt = build_keep_alive(
            "TestCDJ",
            DeviceNumber(3),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
            Ipv4Addr::new(10, 0, 0, 1),
        );
        assert_eq!(pkt[0x0b], 0x00, "byte 0x0b must be 0x00");
        assert_eq!(pkt[0x20], 0x01, "structure marker");
        assert_eq!(pkt[0x21], 0x02, "keep-alive subtype");
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x36], "packet length BE");
        assert_eq!(pkt[DEVICE_NUMBER_OFFSET], 3);
        assert_eq!(pkt[DEVICE_TYPE_OFFSET], u8::from(DeviceType::Cdj));
        assert_eq!(pkt[DEVICE_TYPE_MIRROR_OFFSET], u8::from(DeviceType::Cdj));
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
        let pkt = build_keep_alive("Test", DeviceNumber(1), [0; 6], Ipv4Addr::LOCALHOST);
        assert_eq!(pkt.len(), KEEP_ALIVE_PACKET_SIZE);
    }

    #[test]
    fn name_truncated_to_max_len() {
        let long_name = "A_Very_Long_Device_Name_Exceeding_Twenty";
        let pkt = build_keep_alive(long_name, DeviceNumber(1), [0; 6], Ipv4Addr::LOCALHOST);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert_eq!(ann.name.len(), NAME_MAX_LEN);
        assert_eq!(ann.name, &long_name[..NAME_MAX_LEN]);
    }

    #[test]
    fn parse_peer_count() {
        let mac = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let ip = [10, 0, 0, 1];
        let mut pkt = make_keep_alive_packet("CDJ-2000NXS2", 1, 1, mac, ip);
        pkt[PEER_COUNT_OFFSET] = 4;

        let ann = parse_keep_alive(&pkt).unwrap();
        assert_eq!(ann.peer_count, 4);
    }

    #[test]
    fn detect_opus_quad() {
        let mac = [0x00; 6];
        let ip = [192, 168, 1, 1];
        let pkt = make_keep_alive_packet("OPUS-QUAD", 1, 1, mac, ip);

        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(ann.is_opus_quad);
        assert!(!ann.is_xdj_az);
    }

    #[test]
    fn detect_xdj_az() {
        let mac = [0x00; 6];
        let ip = [192, 168, 1, 2];
        let pkt = make_keep_alive_packet("XDJ-AZ", 2, 1, mac, ip);

        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(!ann.is_opus_quad);
        assert!(ann.is_xdj_az);
    }

    // === Claim / Defense Packet Builder Tests ===

    #[test]
    fn hello_packet_format() {
        let pkt = build_device_hello("TestCDJ");
        assert_eq!(pkt.len(), HELLO_PACKET_SIZE);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
        assert_eq!(pkt[PACKET_TYPE_OFFSET], 0x0a);
        assert_eq!(pkt[0x0b], 0x00);
        // Name should be present
        let name = read_device_name(&pkt, NAME_OFFSET, NAME_MAX_LEN);
        assert_eq!(name, "TestCDJ");
        // CDJ-3000 compatible structure
        assert_eq!(pkt[0x20], 0x01);
        assert_eq!(pkt[0x21], 0x04);
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x26]);
        assert_eq!(pkt[0x24], 0x01);
        assert_eq!(pkt[0x25], 0x40);
    }

    #[test]
    fn claim_stage1_format() {
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let pkt = build_claim_stage1("TestCDJ", mac, 2);
        assert_eq!(pkt.len(), CLAIM_STAGE1_PACKET_SIZE);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
        assert_eq!(pkt[PACKET_TYPE_OFFSET], 0x00);
        assert_eq!(pkt[0x0b], 0x00);
        assert_eq!(pkt[0x20], 0x01);
        assert_eq!(pkt[0x21], 0x03);
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x2c]);
        assert_eq!(pkt[0x24], 2); // counter
        assert_eq!(pkt[0x25], 0x01); // CDJ device type
        assert_eq!(&pkt[0x26..0x2c], &mac);
    }

    #[test]
    fn claim_stage2_format_auto() {
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let ip = Ipv4Addr::new(192, 168, 1, 42);
        let pkt = build_claim_stage2("TestCDJ", mac, ip, 7, 1, true);
        assert_eq!(pkt.len(), CLAIM_STAGE2_PACKET_SIZE);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
        assert_eq!(pkt[PACKET_TYPE_OFFSET], 0x02);
        assert_eq!(pkt[0x0b], 0x00);
        assert_eq!(pkt[0x20], 0x01);
        assert_eq!(pkt[0x21], 0x03);
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x32]);
        assert_eq!(&pkt[0x24..0x28], &[192, 168, 1, 42]); // IP
        assert_eq!(&pkt[0x28..0x2e], &mac);
        assert_eq!(pkt[0x2e], 7); // device number
        assert_eq!(pkt[0x2f], 1); // counter
        assert_eq!(pkt[0x30], 0x01);
        assert_eq!(pkt[0x31], 0x01); // auto-assign
    }

    #[test]
    fn claim_stage2_specific_assign() {
        let pkt = build_claim_stage2("Test", [0; 6], Ipv4Addr::LOCALHOST, 3, 2, false);
        assert_eq!(pkt[0x2e], 3);
        assert_eq!(pkt[0x2f], 2);
        assert_eq!(pkt[0x31], 0x02); // specific assign flag
    }

    #[test]
    fn claim_stage3_format() {
        let pkt = build_claim_stage3("TestCDJ", 7, 3);
        assert_eq!(pkt.len(), CLAIM_STAGE3_PACKET_SIZE);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
        assert_eq!(pkt[PACKET_TYPE_OFFSET], 0x04);
        assert_eq!(pkt[0x0b], 0x00);
        assert_eq!(pkt[0x20], 0x01);
        assert_eq!(pkt[0x21], 0x03);
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x26]);
        assert_eq!(pkt[0x24], 7); // device number
        assert_eq!(pkt[0x25], 3); // counter
    }

    #[test]
    fn defense_packet_format() {
        let ip = Ipv4Addr::new(10, 0, 0, 5);
        let pkt = build_defense("TestCDJ", 7, ip);
        assert_eq!(pkt.len(), DEFENSE_PACKET_SIZE);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
        assert_eq!(pkt[PACKET_TYPE_OFFSET], 0x08);
        assert_eq!(pkt[0x0b], 0x00);
        assert_eq!(pkt[0x20], 0x01);
        assert_eq!(pkt[0x21], 0x02);
        assert_eq!(&pkt[0x22..0x24], &[0x00, 0x29]);
        assert_eq!(pkt[0x24], 7); // device number
        assert_eq!(&pkt[0x25..0x29], &[10, 0, 0, 5]); // IP
    }

    #[test]
    fn extract_defense_device_number_valid() {
        let pkt = build_defense("Test", 9, Ipv4Addr::LOCALHOST);
        assert_eq!(extract_defense_device_number(&pkt), Some(9));
    }

    #[test]
    fn extract_defense_device_number_too_short() {
        let short = &[0u8; 0x24];
        assert_eq!(extract_defense_device_number(short), None);
    }

    #[test]
    fn extract_claim_stage2_device_number_valid() {
        let pkt = build_claim_stage2("Test", [0; 6], Ipv4Addr::LOCALHOST, 12, 1, true);
        assert_eq!(extract_claim_stage2_device_number(&pkt), Some(12));
    }

    #[test]
    fn extract_claim_stage2_device_number_too_short() {
        let short = &[0u8; 0x2e];
        assert_eq!(extract_claim_stage2_device_number(short), None);
    }

    #[test]
    fn hello_name_truncation() {
        let long_name = "A_Very_Long_Device_Name_Exceeding_Twenty";
        let pkt = build_device_hello(long_name);
        let name = read_device_name(&pkt, NAME_OFFSET, NAME_MAX_LEN);
        assert_eq!(name.len(), NAME_MAX_LEN);
    }

    #[test]
    fn claim_stage1_all_counters() {
        let mac = [0x01; 6];
        for counter in 1..=3u8 {
            let pkt = build_claim_stage1("X", mac, counter);
            assert_eq!(pkt[0x24], counter);
        }
    }

    #[test]
    fn claim_stage3_all_counters() {
        for counter in 1..=3u8 {
            let pkt = build_claim_stage3("X", 5, counter);
            assert_eq!(pkt[0x24], 5);
            assert_eq!(pkt[0x25], counter);
        }
    }

    // === Opus Quad Expansion Tests ===

    #[test]
    fn expand_opus_quad_creates_four_players() {
        let mac = [0x00; 6];
        let ip = [192, 168, 1, 1];
        let pkt = make_keep_alive_packet("OPUS-QUAD", 9, 1, mac, ip);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(ann.is_opus_quad);

        let expanded = expand_opus_quad_announcement(&ann);
        assert_eq!(expanded.len(), 4);
        for (i, player) in expanded.iter().enumerate() {
            assert_eq!(player.number, DeviceNumber((i + 1) as u8));
            assert_eq!(player.name, "OPUS-QUAD");
            assert!(player.is_opus_quad);
            assert_eq!(player.ip_address, Ipv4Addr::new(192, 168, 1, 1));
            assert_eq!(player.mac_address, mac);
        }
    }

    #[test]
    fn expand_non_opus_quad_returns_unchanged() {
        let mac = [0xAA; 6];
        let ip = [10, 0, 0, 1];
        let pkt = make_keep_alive_packet("CDJ-3000", 2, 1, mac, ip);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(!ann.is_opus_quad);

        let expanded = expand_opus_quad_announcement(&ann);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].number, DeviceNumber(2));
        assert_eq!(expanded[0].name, "CDJ-3000");
    }

    #[test]
    fn expand_xdj_az_returns_unchanged() {
        let mac = [0x00; 6];
        let ip = [192, 168, 1, 2];
        let pkt = make_keep_alive_packet("XDJ-AZ", 5, 1, mac, ip);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(ann.is_xdj_az);
        assert!(!ann.is_opus_quad);

        let expanded = expand_opus_quad_announcement(&ann);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].name, "XDJ-AZ");
    }

    #[test]
    fn device_library_plus_false_for_standard_packet() {
        let pkt = make_keep_alive_packet("CDJ-3000", 2, 1, [0xAA; 6], [10, 0, 0, 1]);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(!ann.is_using_device_library_plus);
    }

    #[test]
    fn device_library_plus_true_when_flag_set() {
        let mut pkt = make_keep_alive_packet("CDJ-3000", 2, 1, [0xAA; 6], [10, 0, 0, 1]);
        pkt.push(0x01);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(ann.is_using_device_library_plus);
    }

    #[test]
    fn device_library_plus_false_when_flag_zero() {
        let mut pkt = make_keep_alive_packet("CDJ-3000", 2, 1, [0xAA; 6], [10, 0, 0, 1]);
        pkt.push(0x00);
        let ann = parse_keep_alive(&pkt).unwrap();
        assert!(!ann.is_using_device_library_plus);
    }
}
