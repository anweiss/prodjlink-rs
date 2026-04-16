//! Round-trip and integration tests for the testing module.

use crate::protocol::announce::parse_keep_alive;
use crate::protocol::beat::{parse_beat, parse_channels_on_air, parse_precise_position};
use crate::protocol::media::parse_media_details;
use crate::protocol::status::{parse_cdj_status, parse_mixer_status, parse_status};

use super::fixtures;
use super::packets::*;
use super::scenarios;

// -----------------------------------------------------------------------
// Keep-alive round-trip
// -----------------------------------------------------------------------

#[test]
fn keep_alive_round_trip() {
    let pkt = mock_keep_alive(
        "CDJ-2000NXS2",
        2,
        1, // CDJ
        [192, 168, 1, 50],
        [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
    );
    let ann = parse_keep_alive(&pkt).expect("should parse keep-alive");
    assert_eq!(ann.name, "CDJ-2000NXS2");
    assert_eq!(ann.number.0, 2);
    assert_eq!(ann.mac_address, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    assert_eq!(ann.ip_address, std::net::Ipv4Addr::new(192, 168, 1, 50));
}

#[test]
fn keep_alive_mixer_round_trip() {
    let pkt = mock_keep_alive("DJM-900NXS2", 33, 2, [10, 0, 0, 1], [1, 2, 3, 4, 5, 6]);
    let ann = parse_keep_alive(&pkt).expect("should parse mixer keep-alive");
    assert_eq!(ann.name, "DJM-900NXS2");
    assert_eq!(ann.number.0, 33);
}

// -----------------------------------------------------------------------
// CDJ status round-trip
// -----------------------------------------------------------------------

#[test]
fn cdj_status_playing_round_trip() {
    let pkt = MockCdjStatusBuilder::new(2)
        .name("CDJ-2000NXS2")
        .bpm(128.0)
        .playing()
        .synced()
        .on_air()
        .track(42, 2, 3)
        .beat(97, 3)
        .usb_loaded()
        .packet_number(5)
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse CDJ status");
    assert_eq!(s.name, "CDJ-2000NXS2");
    assert_eq!(s.device_number.0, 2);
    assert!((s.bpm.0 - 128.0).abs() < 0.01);
    assert!(s.is_playing());
    assert!(s.is_synced);
    assert!(s.is_on_air);
    assert!(!s.is_master);
    assert_eq!(s.rekordbox_id, 42);
    assert_eq!(s.beat_number, Some(crate::device::types::BeatNumber(97)));
    assert_eq!(s.beat_within_bar, 3);
    assert!(s.is_local_usb_loaded());
    assert_eq!(s.packet_number, 5);
}

#[test]
fn cdj_status_cued_round_trip() {
    let pkt = MockCdjStatusBuilder::new(3)
        .bpm(126.0)
        .cued()
        .track(77, 3, 3)
        .beat(0, 1)
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse cued CDJ status");
    assert!(s.is_cued());
    assert!(!s.is_playing());
    assert_eq!(s.play_state, crate::device::types::PlayState::Cued);
}

#[test]
fn cdj_status_looping_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1)
        .bpm(174.0)
        .looping()
        .synced()
        .on_air()
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse looping CDJ status");
    assert!(s.is_looping());
    assert_eq!(s.play_state, crate::device::types::PlayState::Looping);
}

#[test]
fn cdj_status_paused_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1).bpm(130.0).paused().build();

    let s = parse_cdj_status(&pkt).expect("should parse paused CDJ status");
    assert!(s.is_paused());
    assert!(!s.is_playing());
}

#[test]
fn cdj_status_master_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1)
        .bpm(128.0)
        .playing()
        .master()
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse master CDJ status");
    assert!(s.is_master);
    assert!(s.is_playing());
}

#[test]
fn cdj_status_cdj3000_loop_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1)
        .name("CDJ-3000")
        .bpm(174.0)
        .looping()
        .cdj3000_loop(65536, 131072, 4)
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse CDJ-3000 loop status");
    assert!(s.is_looping());
    assert!(s.loop_start.is_some());
    assert!(s.loop_end.is_some());
    assert_eq!(s.loop_beats, Some(4));
}

#[test]
fn cdj_status_sd_loaded_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1).sd_loaded().build();

    let s = parse_cdj_status(&pkt).expect("should parse SD loaded status");
    assert!(s.is_local_sd_loaded());
}

#[test]
fn cdj_status_pitch_round_trip() {
    let pkt = MockCdjStatusBuilder::new(1)
        .bpm(128.0)
        .pitch(6.0)
        .playing()
        .build();

    let s = parse_cdj_status(&pkt).expect("should parse pitched CDJ status");
    // Pitch should be approximately +6%
    let pct = s.pitch.to_percentage();
    assert!((pct - 6.0).abs() < 0.1, "expected ~6%, got {pct}%");
}

// -----------------------------------------------------------------------
// CDJ status via parse_status dispatcher
// -----------------------------------------------------------------------

#[test]
fn cdj_status_via_parse_status() {
    let pkt = MockCdjStatusBuilder::new(2).bpm(128.0).playing().build();

    let update = parse_status(&pkt).expect("should parse via dispatcher");
    assert!(matches!(
        update,
        crate::protocol::status::DeviceUpdate::Cdj(_)
    ));
}

// -----------------------------------------------------------------------
// Mixer status round-trip
// -----------------------------------------------------------------------

#[test]
fn mixer_status_round_trip() {
    let pkt = MockMixerStatusBuilder::new(33)
        .name("DJM-900NXS2")
        .bpm(128.0)
        .master()
        .synced()
        .beat_within_bar(1)
        .build();

    let s = parse_mixer_status(&pkt).expect("should parse mixer status");
    assert_eq!(s.name, "DJM-900NXS2");
    assert_eq!(s.device_number.0, 33);
    assert!((s.bpm.0 - 128.0).abs() < 0.01);
    assert!(s.is_master);
    assert!(s.is_synced);
    assert_eq!(s.beat_within_bar, 1);
    assert!(s.master_hand_off.is_none());
}

#[test]
fn mixer_status_via_parse_status() {
    let pkt = MockMixerStatusBuilder::new(33).bpm(128.0).master().build();

    let update = parse_status(&pkt).expect("should parse mixer via dispatcher");
    assert!(matches!(
        update,
        crate::protocol::status::DeviceUpdate::Mixer(_)
    ));
}

// -----------------------------------------------------------------------
// Beat round-trip
// -----------------------------------------------------------------------

#[test]
fn beat_round_trip() {
    let pkt = MockBeatBuilder::new(2)
        .name("CDJ-2000NXS2")
        .bpm(128.0)
        .beat_within_bar(3)
        .timing(468, 937, 1406, 1875, 2812, 3750)
        .build();

    let beat = parse_beat(&pkt).expect("should parse beat");
    assert_eq!(beat.name, "CDJ-2000NXS2");
    assert_eq!(beat.device_number.0, 2);
    assert!((beat.bpm.0 - 128.0).abs() < 0.01);
    assert_eq!(beat.beat_within_bar, 3);
    assert_eq!(beat.next_beat, Some(468));
    assert_eq!(beat.second_beat, Some(937));
    assert_eq!(beat.next_bar, Some(1406));
    assert_eq!(beat.fourth_beat, Some(1875));
    assert_eq!(beat.second_bar, Some(2812));
    assert_eq!(beat.eighth_beat, Some(3750));
}

#[test]
fn beat_sentinel_timing() {
    // Default timing is 0xFFFFFFFF (sentinel → None)
    let pkt = MockBeatBuilder::new(1)
        .bpm(128.0)
        .beat_within_bar(1)
        .build();

    let beat = parse_beat(&pkt).expect("should parse beat with sentinel timing");
    assert_eq!(beat.next_beat, None);
    assert_eq!(beat.second_beat, None);
}

#[test]
fn beat_pitched() {
    let pkt = MockBeatBuilder::new(1)
        .bpm(125.0)
        .pitch(6.0)
        .beat_within_bar(4)
        .build();

    let beat = parse_beat(&pkt).expect("should parse pitched beat");
    assert!((beat.bpm.0 - 125.0).abs() < 0.01);
    let pct = beat.pitch.to_percentage();
    assert!((pct - 6.0).abs() < 0.1, "expected ~6%, got {pct}%");
    // effective_tempo should be ~132.5
    assert!((beat.effective_tempo() - 132.5).abs() < 0.5);
}

// -----------------------------------------------------------------------
// PrecisePosition round-trip
// -----------------------------------------------------------------------

#[test]
fn precise_position_round_trip() {
    let pkt = mock_precise_position(1, 60000, 240, 128.0, 0.0);
    let pp = parse_precise_position(&pkt).expect("should parse precise position");
    assert_eq!(pp.device_number.0, 1);
    assert_eq!(pp.position_ms, 60000);
    assert_eq!(pp.track_length, 240);
    assert!((pp.effective_bpm.0 - 128.0).abs() < 0.1);
    assert!((pp.pitch.to_percentage()).abs() < 0.1);
}

#[test]
fn precise_position_pitched() {
    let pkt = mock_precise_position(2, 30000, 180, 128.0, 6.0);
    let pp = parse_precise_position(&pkt).expect("should parse pitched precise position");
    let pct = pp.pitch.to_percentage();
    assert!((pct - 6.0).abs() < 0.1, "expected ~6%, got {pct}%");
    // Effective BPM = 128 * 1.06 = 135.68
    assert!((pp.effective_bpm.0 - 135.68).abs() < 0.2);
}

// -----------------------------------------------------------------------
// Channels-on-air round-trip
// -----------------------------------------------------------------------

#[test]
fn channels_on_air_round_trip() {
    let pkt = mock_channels_on_air(33, &[true, false, true, false]);
    let oa = parse_channels_on_air(&pkt).expect("should parse channels-on-air");
    assert_eq!(oa.device_number.0, 33);
    assert!(*oa.channels.get(&1).unwrap());
    assert!(!(*oa.channels.get(&2).unwrap()));
    assert!(*oa.channels.get(&3).unwrap());
    assert!(!(*oa.channels.get(&4).unwrap()));
}

#[test]
fn channels_on_air_6ch() {
    let pkt = mock_channels_on_air(33, &[true, true, false, false, true, false]);
    let oa = parse_channels_on_air(&pkt).expect("should parse 6-channel on-air");
    assert_eq!(oa.channels.len(), 6);
    assert!(*oa.channels.get(&5).unwrap());
    assert!(!(*oa.channels.get(&6).unwrap()));
}

// -----------------------------------------------------------------------
// Media details round-trip
// -----------------------------------------------------------------------

#[test]
fn media_details_round_trip() {
    let pkt = mock_media_details(2, 3, "MY_USB", 1024);
    let md = parse_media_details(&pkt).expect("should parse media details");
    assert_eq!(md.player.0, 2);
    assert_eq!(md.name, "MY_USB");
    assert_eq!(md.track_count, 1024);
    assert!(md.is_rekordbox);
}

// -----------------------------------------------------------------------
// Golden fixture tests
// -----------------------------------------------------------------------

#[test]
fn fixture_cdj_2000nxs2_playing() {
    let pkt = fixtures::cdj_2000nxs2_playing();
    let s = parse_cdj_status(&pkt).expect("fixture should parse");
    assert_eq!(s.name, "CDJ-2000NXS2");
    assert!(s.is_playing());
    assert!(s.is_synced);
    assert!(s.is_on_air);
    assert!((s.bpm.0 - 128.0).abs() < 0.01);
    assert_eq!(s.beat_within_bar, 3);
    assert_eq!(s.rekordbox_id, 42);
}

#[test]
fn fixture_cdj_3000_looping() {
    let pkt = fixtures::cdj_3000_looping();
    let s = parse_cdj_status(&pkt).expect("fixture should parse");
    assert_eq!(s.name, "CDJ-3000");
    assert!(s.is_looping());
    assert!((s.bpm.0 - 174.0).abs() < 0.01);
    assert_eq!(s.loop_beats, Some(4));
    assert!(s.loop_start.is_some());
}

#[test]
fn fixture_cdj_2000nxs2_cued() {
    let pkt = fixtures::cdj_2000nxs2_cued();
    let s = parse_cdj_status(&pkt).expect("fixture should parse");
    assert!(s.is_cued());
    assert!(!s.is_playing());
}

#[test]
fn fixture_djm_900nxs2_master() {
    let pkt = fixtures::djm_900nxs2_master();
    let s = parse_mixer_status(&pkt).expect("fixture should parse");
    assert_eq!(s.name, "DJM-900NXS2");
    assert!(s.is_master);
    assert!((s.bpm.0 - 128.0).abs() < 0.01);
}

#[test]
fn fixture_djm_a9_status() {
    let pkt = fixtures::djm_a9_status();
    let s = parse_mixer_status(&pkt).expect("fixture should parse");
    assert_eq!(s.name, "DJM-A9");
    assert!(s.is_master);
}

#[test]
fn fixture_opus_quad_keepalive() {
    let pkt = fixtures::opus_quad_keepalive();
    let ann = parse_keep_alive(&pkt).expect("fixture should parse");
    assert_eq!(ann.name, "OPUS-QUAD");
    assert!(ann.is_opus_quad);
}

#[test]
fn fixture_cdj_900_pre_nexus() {
    let pkt = fixtures::cdj_900_pre_nexus();
    let s = parse_cdj_status(&pkt).expect("fixture should parse");
    assert_eq!(s.name, "CDJ-900");
    // Pre-nexus: packet_length == 0xCC < 0xd4, uses fallback is_playing
    assert_eq!(s.packet_length, 0xCC);
    assert!(s.is_playing());
}

#[test]
fn fixture_beat_128bpm_downbeat() {
    let pkt = fixtures::beat_128bpm_downbeat();
    let beat = parse_beat(&pkt).expect("fixture should parse");
    assert!((beat.bpm.0 - 128.0).abs() < 0.01);
    assert_eq!(beat.beat_within_bar, 1);
}

#[test]
fn fixture_precise_position_mid_track() {
    let pkt = fixtures::precise_position_mid_track();
    let pp = parse_precise_position(&pkt).expect("fixture should parse");
    assert_eq!(pp.position_ms, 60000);
    assert_eq!(pp.track_length, 240);
}

#[test]
fn fixture_channels_on_air_1_3() {
    let pkt = fixtures::channels_on_air_1_3();
    let oa = parse_channels_on_air(&pkt).expect("fixture should parse");
    assert!(*oa.channels.get(&1).unwrap());
    assert!(!(*oa.channels.get(&2).unwrap()));
    assert!(*oa.channels.get(&3).unwrap());
    assert!(!(*oa.channels.get(&4).unwrap()));
}

// -----------------------------------------------------------------------
// Scenario tests
// -----------------------------------------------------------------------

#[test]
fn scenario_device_discovery() {
    let packets = scenarios::device_discovery_sequence("CDJ-2000NXS2", 2);
    assert_eq!(packets.len(), 3);
    for pkt in &packets {
        let ann = parse_keep_alive(pkt).expect("discovery packet should parse");
        assert_eq!(ann.name, "CDJ-2000NXS2");
        assert_eq!(ann.number.0, 2);
    }
}

#[test]
fn scenario_track_load_and_play() {
    let packets = scenarios::track_load_and_play_sequence(2, 42);
    assert_eq!(packets.len(), 4);

    // All should parse
    for pkt in &packets {
        parse_cdj_status(pkt).expect("sequence packet should parse");
    }

    // First: no track
    let s0 = parse_cdj_status(&packets[0]).unwrap();
    assert_eq!(s0.play_state, crate::device::types::PlayState::NoTrack);

    // Second: loading
    let s1 = parse_cdj_status(&packets[1]).unwrap();
    assert_eq!(s1.play_state, crate::device::types::PlayState::Loading);
    assert_eq!(s1.rekordbox_id, 42);

    // Third: cued
    let s2 = parse_cdj_status(&packets[2]).unwrap();
    assert!(s2.is_cued());

    // Fourth: playing
    let s3 = parse_cdj_status(&packets[3]).unwrap();
    assert!(s3.is_playing());
}

#[test]
fn scenario_master_handoff() {
    let packets = scenarios::master_handoff_sequence(1, 2);
    assert_eq!(packets.len(), 4);

    // All should parse
    for pkt in &packets {
        parse_cdj_status(pkt).expect("handoff packet should parse");
    }

    // 1: device 1 is master
    let s0 = parse_cdj_status(&packets[0]).unwrap();
    assert!(s0.is_master);
    assert_eq!(s0.device_number.0, 1);

    // 2: device 2 is not yet master
    let s1 = parse_cdj_status(&packets[1]).unwrap();
    assert!(!s1.is_master);
    assert_eq!(s1.device_number.0, 2);

    // 3: device 1 no longer master
    let s2 = parse_cdj_status(&packets[2]).unwrap();
    assert!(!s2.is_master);

    // 4: device 2 is now master
    let s3 = parse_cdj_status(&packets[3]).unwrap();
    assert!(s3.is_master);
    assert_eq!(s3.device_number.0, 2);
}

#[test]
fn scenario_four_bar_beat_sequence() {
    let packets = scenarios::four_bar_beat_sequence(1, 128.0);
    assert_eq!(packets.len(), 16);

    for (i, pkt) in packets.iter().enumerate() {
        let beat = parse_beat(pkt).expect("beat sequence packet should parse");
        let expected_bar_pos = (i % 4) as u8 + 1;
        assert_eq!(beat.beat_within_bar, expected_bar_pos, "beat {i}");
        assert!((beat.bpm.0 - 128.0).abs() < 0.01);
    }
}

// -----------------------------------------------------------------------
// Error handling tests
// -----------------------------------------------------------------------

#[test]
fn malformed_magic_rejected() {
    let mut pkt = MockCdjStatusBuilder::new(1).bpm(128.0).build();
    pkt[0] = 0xFF; // corrupt magic
    // parse_status validates the magic header (parse_cdj_status does not)
    let err = parse_status(&pkt).unwrap_err();
    assert!(matches!(err, crate::error::ProDjLinkError::InvalidMagic));
}

#[test]
fn truncated_cdj_status_rejected() {
    let pkt = MockCdjStatusBuilder::new(1).bpm(128.0).build();
    let err = parse_cdj_status(&pkt[..0x50]).unwrap_err();
    assert!(matches!(
        err,
        crate::error::ProDjLinkError::PacketTooShort { .. }
    ));
}

#[test]
fn truncated_beat_rejected() {
    let pkt = MockBeatBuilder::new(1).bpm(128.0).build();
    let err = parse_beat(&pkt[..0x30]).unwrap_err();
    assert!(matches!(
        err,
        crate::error::ProDjLinkError::PacketTooShort { .. }
    ));
}

#[test]
fn truncated_keep_alive_rejected() {
    let pkt = mock_keep_alive("X", 1, 1, [0; 4], [0; 6]);
    let err = parse_keep_alive(&pkt[..0x20]).unwrap_err();
    assert!(matches!(
        err,
        crate::error::ProDjLinkError::PacketTooShort { .. }
    ));
}

#[test]
fn unknown_play_state_variant() {
    let mut pkt = MockCdjStatusBuilder::new(1).bpm(128.0).build();
    pkt[0x7b] = 0xEE; // unknown play state
    let s = parse_cdj_status(&pkt).expect("should still parse");
    assert_eq!(s.play_state, crate::device::types::PlayState::Unknown(0xEE));
}

#[test]
fn unknown_play_state_2_variant() {
    let mut pkt = MockCdjStatusBuilder::new(1).bpm(128.0).build();
    pkt[0x8b] = 0x01; // unknown play state 2
    let s = parse_cdj_status(&pkt).expect("should still parse");
    assert_eq!(
        s.play_state_2,
        crate::device::types::PlayState2::Unknown(0x01)
    );
}

#[test]
fn unknown_play_state_3_variant() {
    let mut pkt = MockCdjStatusBuilder::new(1).bpm(128.0).build();
    pkt[0x9d] = 0xFF; // unknown play state 3
    let s = parse_cdj_status(&pkt).expect("should still parse");
    assert_eq!(
        s.play_state_3,
        crate::device::types::PlayState3::Unknown(0xFF)
    );
}

#[test]
fn pre_nexus_is_playing_fallback() {
    // Build a 0xCC packet (pre-nexus, < 0xd4) with playing + moving
    let pkt = MockCdjStatusBuilder::new(1)
        .name("CDJ-900")
        .bpm(120.0)
        .playing()
        .build();

    assert_eq!(pkt.len(), 0xCC);
    let s = parse_cdj_status(&pkt).unwrap();
    // Pre-nexus uses fallback: PlayState::Playing + PlayState2::Moving
    assert!(s.is_playing());
}

#[test]
fn pre_nexus_not_playing_when_stopped() {
    let mut pkt = MockCdjStatusBuilder::new(1)
        .name("CDJ-900")
        .bpm(120.0)
        .playing()
        .build();

    // Override play_state_2 to Stopped
    pkt[0x8b] = 0x6e; // Stopped
    let s = parse_cdj_status(&pkt).unwrap();
    assert_eq!(s.packet_length, 0xCC);
    // Fallback: Playing + Stopped → not playing
    assert!(!s.is_playing());
}
