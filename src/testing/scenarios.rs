//! Multi-packet sequences simulating real-world DJ Link conversations.

use super::packets::*;

/// Simulate a device appearing on the network: three keep-alive packets.
pub fn device_discovery_sequence(name: &str, device_number: u8) -> Vec<Vec<u8>> {
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, device_number];
    let ip = [192, 168, 1, device_number];
    (0..3)
        .map(|_| mock_keep_alive(name, device_number, 1, ip, mac))
        .collect()
}

/// Simulate a track being loaded and played through status transitions:
///
/// 1. NoTrack → Loading → Cued → Playing
pub fn track_load_and_play_sequence(device_number: u8, rekordbox_id: u32) -> Vec<Vec<u8>> {
    let base = || {
        MockCdjStatusBuilder::new(device_number)
            .bpm(128.0)
            .usb_loaded()
    };

    vec![
        // 1. No track loaded
        base().packet_number(0).build(),
        // 2. Loading
        {
            let mut b = base()
                .track(rekordbox_id, device_number, 3)
                .packet_number(1)
                .build();
            // Override play_state to Loading (0x02)
            b[0x7b] = 0x02;
            b
        },
        // 3. Cued at start
        base()
            .track(rekordbox_id, device_number, 3)
            .cued()
            .beat(0, 1)
            .packet_number(2)
            .build(),
        // 4. Playing
        base()
            .track(rekordbox_id, device_number, 3)
            .playing()
            .synced()
            .on_air()
            .beat(1, 1)
            .packet_number(3)
            .build(),
    ]
}

/// Simulate a tempo master handoff between two players.
///
/// Returns packets showing player `from_device` yielding master to `to_device`.
pub fn master_handoff_sequence(from_device: u8, to_device: u8) -> Vec<Vec<u8>> {
    vec![
        // 1. from_device is master, playing
        MockCdjStatusBuilder::new(from_device)
            .bpm(128.0)
            .playing()
            .master()
            .synced()
            .on_air()
            .beat(100, 1)
            .packet_number(10)
            .build(),
        // 2. to_device is synced, playing (not yet master)
        MockCdjStatusBuilder::new(to_device)
            .bpm(128.0)
            .playing()
            .synced()
            .on_air()
            .beat(50, 3)
            .packet_number(10)
            .build(),
        // 3. from_device no longer master
        MockCdjStatusBuilder::new(from_device)
            .bpm(128.0)
            .playing()
            .synced()
            .on_air()
            .beat(101, 2)
            .packet_number(11)
            .build(),
        // 4. to_device is now master
        MockCdjStatusBuilder::new(to_device)
            .bpm(128.0)
            .playing()
            .master()
            .synced()
            .on_air()
            .beat(51, 4)
            .packet_number(11)
            .build(),
    ]
}

/// Simulate beat counting through a 4-bar phrase (16 beats).
pub fn four_bar_beat_sequence(device_number: u8, bpm: f64) -> Vec<Vec<u8>> {
    let ms_per_beat = (60_000.0 / bpm) as u32;
    (0..16)
        .map(|i| {
            let beat_in_bar = (i % 4) as u8 + 1;
            let next_beat = ms_per_beat;
            let second_beat = ms_per_beat * 2;
            let next_bar = ms_per_beat * (4 - (i % 4)) as u32;
            let fourth_beat = ms_per_beat * 4;
            let second_bar = ms_per_beat * (8 - (i % 4)) as u32;
            let eighth_beat = ms_per_beat * 8;

            MockBeatBuilder::new(device_number)
                .bpm(bpm)
                .beat_within_bar(beat_in_bar)
                .timing(
                    next_beat,
                    second_beat,
                    next_bar,
                    fourth_beat,
                    second_bar,
                    eighth_beat,
                )
                .build()
        })
        .collect()
}
