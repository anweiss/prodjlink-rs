use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::protocol::beat::{
    parse_beat, parse_channels_on_air, parse_precise_position, Beat, ChannelsOnAir,
    PrecisePosition,
};
use crate::protocol::header::{parse_header, PacketType, BEAT_PORT};

/// Events emitted by the BeatFinder.
#[derive(Debug, Clone)]
pub enum BeatEvent {
    /// A new beat was received from a player.
    NewBeat(Beat),
    /// A precise position update was received (CDJ-3000+).
    PrecisePosition(PrecisePosition),
}

/// Async service that listens for beat timing packets on the DJ Link network.
pub struct BeatFinder {
    event_tx: broadcast::Sender<BeatEvent>,
    on_air_tx: broadcast::Sender<ChannelsOnAir>,
    recv_task: JoinHandle<()>,
}

impl BeatFinder {
    /// Start the beat finder, binding to UDP port 50001.
    pub async fn start() -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", BEAT_PORT)).await?;
        Self::start_with_socket(socket)
    }

    /// Start the beat finder using a pre-bound socket (useful for testing).
    pub(crate) fn start_with_socket(socket: UdpSocket) -> Result<Self> {
        let socket = Arc::new(socket);
        let (event_tx, _) = broadcast::channel(512);
        let (on_air_tx, _) = broadcast::channel(64);

        let recv_tx = event_tx.clone();
        let on_air_tx_clone = on_air_tx.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, _)) => {
                        let data = &buf[..len];
                        if let Ok(ptype) = parse_header(data) {
                            match ptype {
                                PacketType::Beat => {
                                    if let Ok(b) = parse_beat(data) {
                                        let _ = recv_tx.send(BeatEvent::NewBeat(b));
                                    }
                                }
                                PacketType::PrecisePosition => {
                                    if let Ok(pp) = parse_precise_position(data) {
                                        let _ =
                                            recv_tx.send(BeatEvent::PrecisePosition(pp));
                                    }
                                }
                                PacketType::OnAir => {
                                    if let Ok(oa) = parse_channels_on_air(data) {
                                        let _ = on_air_tx_clone.send(oa);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            event_tx,
            on_air_tx,
            recv_task,
        })
    }

    /// Subscribe to beat events.
    pub fn subscribe(&self) -> broadcast::Receiver<BeatEvent> {
        self.event_tx.subscribe()
    }

    /// Subscribe to channels-on-air updates from mixers.
    pub fn subscribe_on_air(&self) -> broadcast::Receiver<ChannelsOnAir> {
        self.on_air_tx.subscribe()
    }

    /// Stop the beat finder.
    pub fn stop(self) {
        self.recv_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::header::MAGIC_HEADER;

    #[test]
    fn beat_event_enum_construction() {
        use std::time::Instant;

        use crate::device::types::{Bpm, DeviceNumber, DeviceType, Pitch};

        let beat = Beat {
            name: "CDJ-3000".into(),
            device_number: DeviceNumber(1),
            device_type: DeviceType::Cdj,
            bpm: Bpm(128.0),
            pitch: Pitch(0x100000),
            next_beat: Some(0),
            second_beat: Some(0),
            next_bar: Some(0),
            fourth_beat: Some(0),
            second_bar: Some(0),
            eighth_beat: Some(0),
            beat_within_bar: 1,
            timestamp: Instant::now(),
        };
        let evt = BeatEvent::NewBeat(beat);
        assert!(matches!(evt, BeatEvent::NewBeat(_)));

        let pp = PrecisePosition {
            name: "CDJ-3000".into(),
            device_number: DeviceNumber(2),
            track_length: 300,
            position_ms: 1000,
            pitch: Pitch(0x100000),
            effective_bpm: Bpm(140.0),
            timestamp: Instant::now(),
        };
        let evt = BeatEvent::PrecisePosition(pp);
        assert!(matches!(evt, BeatEvent::PrecisePosition(_)));
    }

    #[test]
    fn beat_finder_has_expected_api() {
        // Compile-time check: BeatFinder exposes subscribe, subscribe_on_air, and stop.
        fn _assert_subscribe(bf: &BeatFinder) -> broadcast::Receiver<BeatEvent> {
            bf.subscribe()
        }
        fn _assert_subscribe_on_air(bf: &BeatFinder) -> broadcast::Receiver<ChannelsOnAir> {
            bf.subscribe_on_air()
        }
        fn _assert_stop(bf: BeatFinder) {
            bf.stop();
        }
    }

    /// Build a minimal beat packet suitable for the loopback test.
    fn make_beat_packet(device_num: u8, bpm_hundredths: u16) -> Vec<u8> {
        let mut pkt = vec![0u8; 0x60];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x28; // Beat type
        pkt[0x21] = device_num;
        pkt[0x23] = 0x01; // CDJ
        pkt[0x5a..0x5c].copy_from_slice(&bpm_hundredths.to_be_bytes());
        pkt[0x5c] = 3; // beat 3 of 4
        pkt
    }

    /// Build a minimal precise-position packet for the loopback test.
    fn make_precise_position_packet(device_num: u8) -> Vec<u8> {
        let mut pkt = vec![0u8; 0x3c]; // exact size
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x0b; // PrecisePosition type
        pkt[0x21] = device_num;
        // BPM at 0x38 (4 bytes): 1250 → 125.0 effective BPM
        pkt[0x38..0x3c].copy_from_slice(&1250u32.to_be_bytes());
        pkt
    }

    #[tokio::test]
    async fn send_receive_beat_on_loopback() {
        // Bind to an OS-assigned port so we don't conflict with BEAT_PORT.
        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return, // skip if we can't bind
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe();

        // Send a beat packet to the finder's socket.
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = make_beat_packet(3, 12800);
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel error");

        match evt {
            BeatEvent::NewBeat(b) => {
                assert_eq!(b.device_number, crate::device::types::DeviceNumber(3));
                assert!((b.bpm.0 - 128.0).abs() < f64::EPSILON);
                assert_eq!(b.beat_within_bar, 3);
            }
            other => panic!("expected NewBeat, got {:?}", other),
        }

        finder.stop();
    }

    #[tokio::test]
    async fn send_receive_precise_position_on_loopback() {
        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = make_precise_position_packet(4);
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel error");

        match evt {
            BeatEvent::PrecisePosition(pp) => {
                assert_eq!(pp.device_number, crate::device::types::DeviceNumber(4));
                assert!((pp.effective_bpm.0 - 125.0).abs() < f64::EPSILON);
            }
            other => panic!("expected PrecisePosition, got {:?}", other),
        }

        finder.stop();
    }

    #[tokio::test]
    async fn ignores_unknown_packet_types() {
        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe();

        // Send a keep-alive packet (type 0x06) — should be ignored.
        let mut pkt = vec![0u8; 0x60];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x06;

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(&pkt, local_addr).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(result.is_err(), "should not have received an event");

        finder.stop();
    }

    /// Build a minimal channels-on-air packet for the loopback test.
    fn make_on_air_packet(device_num: u8, flags: &[u8]) -> Vec<u8> {
        let total_len = 0x24 + flags.len();
        let mut pkt = vec![0u8; total_len];
        pkt[..MAGIC_HEADER.len()].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0x03; // OnAir type
        pkt[0x21] = device_num;
        pkt[0x24..0x24 + flags.len()].copy_from_slice(flags);
        pkt
    }

    #[tokio::test]
    async fn send_receive_on_air_on_loopback() {
        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe_on_air();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = make_on_air_packet(33, &[0x01, 0x00, 0x01, 0x00]);
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for on-air event")
            .expect("channel error");

        assert_eq!(evt.device_number, crate::device::types::DeviceNumber(33));
        assert_eq!(evt.channels[&1], true);
        assert_eq!(evt.channels[&2], false);
        assert_eq!(evt.channels[&3], true);
        assert_eq!(evt.channels[&4], false);

        finder.stop();
    }

    #[tokio::test]
    async fn on_air_does_not_appear_on_beat_channel() {
        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut beat_rx = finder.subscribe();

        // Send an on-air packet — it should NOT show up on the beat channel.
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = make_on_air_packet(33, &[0x01, 0x01, 0x01, 0x01]);
        sender.send_to(&pkt, local_addr).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), beat_rx.recv()).await;
        assert!(result.is_err(), "on-air should not appear on beat channel");

        finder.stop();
    }
}
