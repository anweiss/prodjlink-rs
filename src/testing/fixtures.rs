//! Golden packet fixtures representing common real-world scenarios.
//!
//! Every fixture produces a byte-for-byte realistic packet that round-trips
//! through the library's parsers.

use super::packets::*;

/// A CDJ-2000NXS2 playing a track at 128 BPM, beat 3 of 4, synced, on-air.
pub fn cdj_2000nxs2_playing() -> Vec<u8> {
    MockCdjStatusBuilder::new(2)
        .name("CDJ-2000NXS2")
        .bpm(128.0)
        .playing()
        .synced()
        .on_air()
        .track(42, 2, 3) // rekordbox id 42, from player 2, USB slot
        .beat(97, 3)
        .usb_loaded()
        .packet_number(1)
        .build()
}

/// A CDJ-3000 playing at 174 BPM with an active 4-beat loop.
pub fn cdj_3000_looping() -> Vec<u8> {
    MockCdjStatusBuilder::new(1)
        .name("CDJ-3000")
        .bpm(174.0)
        .looping()
        .synced()
        .on_air()
        .track(100, 1, 3) // rekordbox id 100, from player 1, USB slot
        .beat(256, 2)
        .usb_loaded()
        .cdj3000_loop(65536, 131072, 4) // loop start/end in native format, 4 beats
        .packet_number(50)
        .build()
}

/// A CDJ-2000NXS2 paused/cued at a cue point.
pub fn cdj_2000nxs2_cued() -> Vec<u8> {
    MockCdjStatusBuilder::new(3)
        .name("CDJ-2000NXS2")
        .bpm(126.0)
        .cued()
        .track(77, 3, 3) // USB slot
        .beat(0, 1)
        .usb_loaded()
        .build()
}

/// A DJM-900NXS2 mixer status as tempo master at 128 BPM.
pub fn djm_900nxs2_master() -> Vec<u8> {
    MockMixerStatusBuilder::new(33)
        .name("DJM-900NXS2")
        .bpm(128.0)
        .master()
        .synced()
        .beat_within_bar(1)
        .build()
}

/// A DJM-A9 mixer status.
pub fn djm_a9_status() -> Vec<u8> {
    MockMixerStatusBuilder::new(33)
        .name("DJM-A9")
        .bpm(130.0)
        .master()
        .synced()
        .beat_within_bar(3)
        .build()
}

/// An Opus Quad keep-alive packet.
pub fn opus_quad_keepalive() -> Vec<u8> {
    mock_keep_alive(
        "OPUS-QUAD",
        9, // Opus Quad uses device numbers 9–12
        1, // CDJ type
        [192, 168, 1, 10],
        [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E],
    )
}

/// A pre-nexus CDJ-900 status packet (shorter format, exactly 0xCC bytes).
///
/// Pre-nexus packets use PlayState + PlayState2 fallback for `is_playing()`.
pub fn cdj_900_pre_nexus() -> Vec<u8> {
    // Build a minimal 0xCC-byte packet (pre-nexus threshold is < 0xd4)
    MockCdjStatusBuilder::new(4)
        .name("CDJ-900")
        .bpm(120.0)
        .playing()
        .track(10, 4, 3) // USB slot
        .beat(50, 2)
        .usb_loaded()
        .build()
}

/// A beat packet at 128 BPM, beat 1 of bar (downbeat).
pub fn beat_128bpm_downbeat() -> Vec<u8> {
    MockBeatBuilder::new(2)
        .name("CDJ-2000NXS2")
        .bpm(128.0)
        .beat_within_bar(1)
        .timing(468, 937, 1406, 1875, 2812, 3750)
        .build()
}

/// A precise position packet mid-track.
pub fn precise_position_mid_track() -> Vec<u8> {
    mock_precise_position(
        1,     // device number
        60000, // 60 seconds in
        240,   // 4-minute track
        128.0, // 128 BPM
        0.0,   // no pitch adjustment
    )
}

/// Channels-on-air with channels 1 and 3 active.
pub fn channels_on_air_1_3() -> Vec<u8> {
    mock_channels_on_air(33, &[true, false, true, false])
}
