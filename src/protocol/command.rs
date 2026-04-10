//! Command packet serialization for the Pro DJ Link protocol.
//!
//! Builds command packets sent to CDJs on port 50002 to control playback,
//! load tracks, and manage sync/master state.

use crate::device::types::{DeviceNumber, TrackSourceSlot, TrackType};
use crate::protocol::header::MAGIC_HEADER;
use crate::util::number_to_bytes;

/// Device name embedded in outgoing command packets, null-padded to 20 bytes.
const DEVICE_NAME: &[u8; 20] = b"prodjlink-rs\0\0\0\0\0\0\0\0";

// Packet type bytes for commands (port 50002).
const FADER_START_TYPE: u8 = 0x02;
const LOAD_TRACK_TYPE: u8 = 0x19;
const SYNC_CONTROL_TYPE: u8 = 0x2a;
const MASTER_COMMAND_TYPE: u8 = 0x26;

/// Byte length of the common prefix
/// (magic + type + subtype + name + marker + device + length).
const PREFIX_LEN: usize = 0x24;

/// Offset of the rekordbox track ID within a load-track packet.
pub const LOAD_TRACK_ID_OFFSET: usize = 0x28;

/// Offset of the sync enable/disable flag within a sync-control packet.
pub const SYNC_FLAG_OFFSET: usize = 0x25;

/// Sync-on flag value.
pub const SYNC_ON: u8 = 0x10;

/// Sync-off flag value.
pub const SYNC_OFF: u8 = 0x20;

/// Write the common prefix shared by all command packets.
fn write_prefix(buf: &mut [u8], type_byte: u8, source_device: DeviceNumber, payload_len: u16) {
    buf[0x00..0x0a].copy_from_slice(&MAGIC_HEADER);
    buf[0x0a] = type_byte;
    // 0x0b: subtype, left as 0x00
    buf[0x0c..0x20].copy_from_slice(DEVICE_NAME);
    buf[0x20] = 0x01; // argument count marker
    buf[0x21] = source_device.0;
    number_to_bytes(payload_len as u32, buf, 0x22, 2);
}

fn track_source_slot_to_u8(slot: TrackSourceSlot) -> u8 {
    match slot {
        TrackSourceSlot::NoTrack => 0,
        TrackSourceSlot::CdSlot => 1,
        TrackSourceSlot::SdSlot => 2,
        TrackSourceSlot::UsbSlot => 3,
        TrackSourceSlot::Collection => 4,
        TrackSourceSlot::Unknown(n) => n,
    }
}

fn track_type_to_u8(tt: TrackType) -> u8 {
    match tt {
        TrackType::NoTrack => 0,
        TrackType::Rekordbox => 1,
        TrackType::Unanalyzed => 2,
        TrackType::CdDigitalAudio => 5,
        TrackType::Unknown(n) => n,
    }
}

/// Build a fader start/stop command packet.
///
/// When `start` is true, tells the target to start playback; false to stop.
pub fn build_fader_start(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    start: bool,
) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x04;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, FADER_START_TYPE, source_device, PAYLOAD_LEN);
    buf[0x24] = target_device.0;
    buf[0x25] = if start { 0x00 } else { 0x01 };
    // 0x26-0x27: padding (already 0)
    buf
}

/// Build a load-track command to tell a CDJ to load a specific track.
///
/// This tells `target_device` to load the track identified by `rekordbox_id`
/// from `source_player`'s `source_slot`.
pub fn build_load_track(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    source_player: DeviceNumber,
    source_slot: TrackSourceSlot,
    track_type: TrackType,
    rekordbox_id: u32,
) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x34;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, LOAD_TRACK_TYPE, source_device, PAYLOAD_LEN);
    buf[0x24] = target_device.0;
    // 0x25-0x27: padding (already 0)
    number_to_bytes(rekordbox_id, &mut buf, LOAD_TRACK_ID_OFFSET, 4);
    buf[0x2c] = source_player.0;
    buf[0x2d] = track_source_slot_to_u8(source_slot);
    buf[0x2e] = track_type_to_u8(track_type);
    // TODO: bytes 0x2f-0x57 may contain additional fields observed in
    // protocol captures (e.g. unknown flags). Zero-filled for now.
    buf
}

/// Build a sync mode command (enable/disable sync on a player).
pub fn build_sync_command(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    enable: bool,
) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x04;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, SYNC_CONTROL_TYPE, source_device, PAYLOAD_LEN);
    buf[0x24] = target_device.0;
    buf[SYNC_FLAG_OFFSET] = if enable { SYNC_ON } else { SYNC_OFF };
    // 0x26-0x27: padding (already 0)
    buf
}

/// Build a "become tempo master" command.
///
/// Announces that `source_device` wants to become the tempo master.
pub fn build_master_command(source_device: DeviceNumber) -> Vec<u8> {
    // TODO: the master handoff protocol may require additional payload
    // fields (e.g. current BPM). Using a minimal packet for now.
    const PAYLOAD_LEN: u16 = 0x00;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, MASTER_COMMAND_TYPE, source_device, PAYLOAD_LEN);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::bytes_to_number;

    #[test]
    fn fader_start_has_correct_header_and_type() {
        let pkt = build_fader_start(DeviceNumber(5), DeviceNumber(3), true);
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], FADER_START_TYPE);
    }

    #[test]
    fn fader_start_contains_device_numbers() {
        let pkt = build_fader_start(DeviceNumber(5), DeviceNumber(3), true);
        assert_eq!(pkt[0x21], 5); // source
        assert_eq!(pkt[0x24], 3); // target
    }

    #[test]
    fn fader_start_flag() {
        let start = build_fader_start(DeviceNumber(1), DeviceNumber(2), true);
        assert_eq!(start[0x25], 0x00);

        let stop = build_fader_start(DeviceNumber(1), DeviceNumber(2), false);
        assert_eq!(stop[0x25], 0x01);
    }

    #[test]
    fn load_track_has_correct_header_and_type() {
        let pkt = build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::UsbSlot,
            TrackType::Rekordbox,
            42,
        );
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], LOAD_TRACK_TYPE);
    }

    #[test]
    fn load_track_contains_rekordbox_id() {
        let pkt = build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::UsbSlot,
            TrackType::Rekordbox,
            0xDEAD_BEEF,
        );
        let id = bytes_to_number(&pkt, LOAD_TRACK_ID_OFFSET, 4);
        assert_eq!(id, 0xDEAD_BEEF);
    }

    #[test]
    fn load_track_source_fields() {
        let pkt = build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::SdSlot,
            TrackType::Unanalyzed,
            100,
        );
        assert_eq!(pkt[0x2c], 2); // source player
        assert_eq!(pkt[0x2d], 2); // SdSlot
        assert_eq!(pkt[0x2e], 2); // Unanalyzed
    }

    #[test]
    fn sync_has_correct_header_and_type() {
        let pkt = build_sync_command(DeviceNumber(1), DeviceNumber(2), true);
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], SYNC_CONTROL_TYPE);
    }

    #[test]
    fn sync_enable_disable_flag() {
        let on = build_sync_command(DeviceNumber(1), DeviceNumber(2), true);
        assert_eq!(on[SYNC_FLAG_OFFSET], SYNC_ON);

        let off = build_sync_command(DeviceNumber(1), DeviceNumber(2), false);
        assert_eq!(off[SYNC_FLAG_OFFSET], SYNC_OFF);
    }

    #[test]
    fn master_command_has_correct_header_and_type() {
        let pkt = build_master_command(DeviceNumber(7));
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], MASTER_COMMAND_TYPE);
        assert_eq!(pkt[0x21], 7);
    }

    #[test]
    fn device_name_embedded_in_packet() {
        let pkt = build_fader_start(DeviceNumber(1), DeviceNumber(2), true);
        let name = crate::util::read_device_name(&pkt, 0x0c, 20);
        assert_eq!(name, "prodjlink-rs");
    }

    #[test]
    fn round_trip_fader_start_fields() {
        let pkt = build_fader_start(DeviceNumber(4), DeviceNumber(1), false);
        assert_eq!(&pkt[0..10], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], FADER_START_TYPE);
        assert_eq!(crate::util::read_device_name(&pkt, 0x0c, 20), "prodjlink-rs");
        assert_eq!(pkt[0x21], 4); // source
        assert_eq!(pkt[0x24], 1); // target
        assert_eq!(pkt[0x25], 0x01); // stop
    }

    #[test]
    fn round_trip_load_track_fields() {
        let pkt = build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::UsbSlot,
            TrackType::Rekordbox,
            12345,
        );
        assert_eq!(pkt[0x21], 5);
        assert_eq!(pkt[0x24], 3);
        assert_eq!(bytes_to_number(&pkt, LOAD_TRACK_ID_OFFSET, 4), 12345);
        assert_eq!(pkt[0x2c], 2); // source player
        assert_eq!(pkt[0x2d], 3); // UsbSlot
        assert_eq!(pkt[0x2e], 1); // Rekordbox
    }
}
