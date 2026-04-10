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
pub const LOAD_TRACK_ID_OFFSET: usize = 0x2c;

/// Offset of the sync enable/disable flag within a sync-control packet.
pub const SYNC_FLAG_OFFSET: usize = 0x30;

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

/// Channel action for fader start/stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaderAction {
    Start,
    Stop,
    NoChange,
}

/// Build a fader start/stop command packet.
///
/// Controls up to 4 channels. Each channel can independently start, stop, or
/// be unchanged.
pub fn build_fader_start(
    source_device: DeviceNumber,
    channels: [FaderAction; 4],
) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x08;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, FADER_START_TYPE, source_device, PAYLOAD_LEN);
    for (i, action) in channels.iter().enumerate() {
        buf[0x24 + i] = match action {
            FaderAction::Start => 0x00,
            FaderAction::Stop => 0x01,
            FaderAction::NoChange => 0x02,
        };
    }
    buf
}

/// Build a fader start command targeting a single player.
pub fn build_fader_start_single(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    start: bool,
) -> Vec<u8> {
    let mut channels = [FaderAction::NoChange; 4];
    let idx = (target_device.0 as usize).saturating_sub(1).min(3);
    channels[idx] = if start {
        FaderAction::Start
    } else {
        FaderAction::Stop
    };
    build_fader_start(source_device, channels)
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
    buf[0x24] = source_device.0;
    buf[0x28] = source_player.0;
    buf[0x29] = track_source_slot_to_u8(source_slot);
    buf[0x2a] = track_type_to_u8(track_type);
    number_to_bytes(rekordbox_id, &mut buf, LOAD_TRACK_ID_OFFSET, 4);
    // 0x38: constant 0x32
    if buf.len() > 0x38 {
        buf[0x38] = 0x32;
    }
    // 0x40: target device - 1
    if buf.len() > 0x40 {
        buf[0x40] = target_device.0.saturating_sub(1);
    }
    buf
}

/// Build a sync mode command (enable/disable sync on a player).
pub fn build_sync_command(
    source_device: DeviceNumber,
    target_device: DeviceNumber,
    enable: bool,
) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x0d; // 13 bytes
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, SYNC_CONTROL_TYPE, source_device, PAYLOAD_LEN);
    buf[0x24] = target_device.0;
    buf[0x26] = source_device.0;
    buf[0x2c] = source_device.0;
    buf[SYNC_FLAG_OFFSET] = if enable { SYNC_ON } else { SYNC_OFF };
    buf
}

/// Build a "become tempo master" command.
///
/// Announces that `source_device` wants to become the tempo master.
pub fn build_master_command(source_device: DeviceNumber) -> Vec<u8> {
    const PAYLOAD_LEN: u16 = 0x09;
    let mut buf = vec![0u8; PREFIX_LEN + PAYLOAD_LEN as usize];
    write_prefix(&mut buf, MASTER_COMMAND_TYPE, source_device, PAYLOAD_LEN);
    buf[0x26] = source_device.0;
    if buf.len() > 0x2c {
        buf[0x2c] = source_device.0;
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::bytes_to_number;

    #[test]
    fn fader_start_has_correct_header_and_type() {
        let pkt = build_fader_start(
            DeviceNumber(5),
            [FaderAction::Start, FaderAction::NoChange, FaderAction::NoChange, FaderAction::NoChange],
        );
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], FADER_START_TYPE);
    }

    #[test]
    fn fader_start_channels() {
        let pkt = build_fader_start(
            DeviceNumber(5),
            [FaderAction::Start, FaderAction::Stop, FaderAction::NoChange, FaderAction::Start],
        );
        assert_eq!(pkt[0x21], 5); // source
        assert_eq!(pkt[0x24], 0x00); // channel 1: start
        assert_eq!(pkt[0x25], 0x01); // channel 2: stop
        assert_eq!(pkt[0x26], 0x02); // channel 3: no change
        assert_eq!(pkt[0x27], 0x00); // channel 4: start
    }

    #[test]
    fn fader_start_single_targets_correct_channel() {
        let pkt = build_fader_start_single(DeviceNumber(1), DeviceNumber(3), true);
        assert_eq!(pkt[0x24], 0x02); // channel 1: no change
        assert_eq!(pkt[0x25], 0x02); // channel 2: no change
        assert_eq!(pkt[0x26], 0x00); // channel 3: start
        assert_eq!(pkt[0x27], 0x02); // channel 4: no change

        let pkt = build_fader_start_single(DeviceNumber(1), DeviceNumber(2), false);
        assert_eq!(pkt[0x24], 0x02); // channel 1: no change
        assert_eq!(pkt[0x25], 0x01); // channel 2: stop
        assert_eq!(pkt[0x26], 0x02); // channel 3: no change
        assert_eq!(pkt[0x27], 0x02); // channel 4: no change
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
        assert_eq!(pkt[0x24], 5); // source device (duplicate)
        assert_eq!(pkt[0x28], 2); // source player
        assert_eq!(pkt[0x29], 2); // SdSlot
        assert_eq!(pkt[0x2a], 2); // Unanalyzed
    }

    #[test]
    fn load_track_constant_and_target() {
        let pkt = build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::UsbSlot,
            TrackType::Rekordbox,
            42,
        );
        assert_eq!(pkt[0x38], 0x32);
        assert_eq!(pkt[0x40], 2); // target_device(3) - 1
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
    fn sync_command_payload_fields() {
        let pkt = build_sync_command(DeviceNumber(5), DeviceNumber(2), true);
        assert_eq!(pkt[0x24], 2); // target
        assert_eq!(pkt[0x26], 5); // source (payload[2])
        assert_eq!(pkt[0x2c], 5); // source (payload[8])
        assert_eq!(pkt.len(), PREFIX_LEN + 0x0d);
    }

    #[test]
    fn master_command_has_correct_header_and_type() {
        let pkt = build_master_command(DeviceNumber(7));
        assert_eq!(&pkt[0x00..0x0a], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], MASTER_COMMAND_TYPE);
        assert_eq!(pkt[0x21], 7);
    }

    #[test]
    fn master_command_payload() {
        let pkt = build_master_command(DeviceNumber(3));
        assert_eq!(pkt.len(), PREFIX_LEN + 0x09);
        assert_eq!(pkt[0x26], 3); // payload[2]
        assert_eq!(pkt[0x2c], 3); // payload[8]
    }

    #[test]
    fn device_name_embedded_in_packet() {
        let pkt = build_fader_start_single(DeviceNumber(1), DeviceNumber(2), true);
        let name = crate::util::read_device_name(&pkt, 0x0c, 20);
        assert_eq!(name, "prodjlink-rs");
    }

    #[test]
    fn round_trip_fader_start_fields() {
        let pkt = build_fader_start_single(DeviceNumber(4), DeviceNumber(1), false);
        assert_eq!(&pkt[0..10], &MAGIC_HEADER);
        assert_eq!(pkt[0x0a], FADER_START_TYPE);
        assert_eq!(crate::util::read_device_name(&pkt, 0x0c, 20), "prodjlink-rs");
        assert_eq!(pkt[0x21], 4); // source
        // channel 1 (index 0) should be Stop
        assert_eq!(pkt[0x24], 0x01); // stop
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
        assert_eq!(pkt[0x24], 5); // source device (duplicate)
        assert_eq!(bytes_to_number(&pkt, LOAD_TRACK_ID_OFFSET, 4), 12345);
        assert_eq!(pkt[0x28], 2); // source player
        assert_eq!(pkt[0x29], 3); // UsbSlot
        assert_eq!(pkt[0x2a], 1); // Rekordbox
    }
}
