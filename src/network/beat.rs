use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::protocol::beat::{
    Beat, ChannelsOnAir, FaderStartEvent, MasterHandoffEvent, PrecisePosition, SyncEvent,
    parse_beat, parse_channels_on_air, parse_fader_start, parse_master_handoff,
    parse_precise_position, parse_sync,
};
use crate::protocol::header::{BEAT_PORT, PacketType, parse_header_on_port};

/// Time window within which an XDJ-XZ is considered visible on the network.
const XDJ_XZ_ACTIVE_WINDOW: Duration = Duration::from_secs(1);

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
    sync_tx: broadcast::Sender<SyncEvent>,
    handoff_tx: broadcast::Sender<MasterHandoffEvent>,
    fader_start_tx: broadcast::Sender<FaderStartEvent>,
    recv_task: JoinHandle<()>,
    last_xdj_xz_seen: Arc<Mutex<Option<Instant>>>,
}

impl BeatFinder {
    /// Start the beat finder, binding to UDP port 50001.
    pub async fn start() -> Result<Self> {
        let socket = super::create_reuseport_socket(BEAT_PORT)?;
        Self::start_with_socket(socket)
    }

    /// Start the beat finder using a pre-bound socket (useful for testing).
    pub(crate) fn start_with_socket(socket: UdpSocket) -> Result<Self> {
        let socket = Arc::new(socket);
        let (event_tx, _) = broadcast::channel(512);
        let (on_air_tx, _) = broadcast::channel(64);
        let (sync_tx, _) = broadcast::channel(64);
        let (handoff_tx, _) = broadcast::channel(64);
        let (fader_start_tx, _) = broadcast::channel(64);

        let recv_tx = event_tx.clone();
        let on_air_tx_clone = on_air_tx.clone();
        let sync_tx_clone = sync_tx.clone();
        let handoff_tx_clone = handoff_tx.clone();
        let fader_start_tx_clone = fader_start_tx.clone();
        let last_xdj_xz_seen = Arc::new(Mutex::new(None::<Instant>));
        let xdj_xz_clone = last_xdj_xz_seen.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            while let Ok((len, _)) = socket.recv_from(&mut buf).await {
                let data = &buf[..len];
                if let Ok(ptype) = parse_header_on_port(data, BEAT_PORT) {
                    match ptype {
                        PacketType::Beat => {
                            if let Ok(b) = parse_beat(data) {
                                if is_xdj_xz_name(&b.name) {
                                    *xdj_xz_clone.lock().unwrap() = Some(Instant::now());
                                }
                                let _ = recv_tx.send(BeatEvent::NewBeat(b));
                            }
                        }
                        PacketType::PrecisePosition => {
                            if let Ok(pp) = parse_precise_position(data) {
                                let _ = recv_tx.send(BeatEvent::PrecisePosition(pp));
                            }
                        }
                        PacketType::OnAir => {
                            if let Ok(oa) = parse_channels_on_air(data) {
                                let _ = on_air_tx_clone.send(oa);
                            }
                        }
                        PacketType::SyncControl => {
                            if let Ok(se) = parse_sync(data) {
                                let _ = sync_tx_clone.send(se);
                            }
                        }
                        PacketType::MasterHandoff => {
                            if let Ok(mh) = parse_master_handoff(data) {
                                let _ = handoff_tx_clone.send(mh);
                            }
                        }
                        PacketType::FaderStart => {
                            if let Ok(fs) = parse_fader_start(data) {
                                let _ = fader_start_tx_clone.send(fs);
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(Self {
            event_tx,
            on_air_tx,
            sync_tx,
            handoff_tx,
            fader_start_tx,
            recv_task,
            last_xdj_xz_seen,
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

    /// Subscribe to sync control events.
    pub fn subscribe_sync(&self) -> broadcast::Receiver<SyncEvent> {
        self.sync_tx.subscribe()
    }

    /// Subscribe to master-handoff request events.
    pub fn subscribe_master_handoff(&self) -> broadcast::Receiver<MasterHandoffEvent> {
        self.handoff_tx.subscribe()
    }

    /// Subscribe to fader-start command events.
    pub fn subscribe_fader_start(&self) -> broadcast::Receiver<FaderStartEvent> {
        self.fader_start_tx.subscribe()
    }

    /// Returns `true` if an XDJ-XZ has sent a beat packet recently enough to
    /// be considered visible on the Pro DJ Link network.
    pub fn can_see_xdj_xz_in_pro_dj_link_mode(&self) -> bool {
        self.last_xdj_xz_seen
            .lock()
            .unwrap()
            .is_some_and(|ts| ts.elapsed() < XDJ_XZ_ACTIVE_WINDOW)
    }

    /// Stop the beat finder.
    pub fn stop(self) {
        self.recv_task.abort();
    }
}

/// The XDJ-XZ identifies itself as "XDJ-AZ" on the network.
fn is_xdj_xz_name(name: &str) -> bool {
    name == "XDJ-XZ" || name == "XDJ-AZ"
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
        // Compile-time check: BeatFinder exposes all subscribe methods and stop.
        fn _assert_subscribe(bf: &BeatFinder) -> broadcast::Receiver<BeatEvent> {
            bf.subscribe()
        }
        fn _assert_subscribe_on_air(bf: &BeatFinder) -> broadcast::Receiver<ChannelsOnAir> {
            bf.subscribe_on_air()
        }
        fn _assert_subscribe_sync(bf: &BeatFinder) -> broadcast::Receiver<SyncEvent> {
            bf.subscribe_sync()
        }
        fn _assert_subscribe_handoff(bf: &BeatFinder) -> broadcast::Receiver<MasterHandoffEvent> {
            bf.subscribe_master_handoff()
        }
        fn _assert_subscribe_fader(bf: &BeatFinder) -> broadcast::Receiver<FaderStartEvent> {
            bf.subscribe_fader_start()
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

        let result = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
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
        assert!(evt.channels[&1]);
        assert!(!evt.channels[&2]);
        assert!(evt.channels[&3]);
        assert!(!evt.channels[&4]);

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

    #[tokio::test]
    async fn send_receive_sync_on_loopback() {
        use crate::protocol::command::build_sync_command;

        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe_sync();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = build_sync_command(
            crate::device::types::DeviceNumber(1),
            crate::device::types::DeviceNumber(2),
            true,
        );
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for sync event")
            .expect("channel error");

        assert_eq!(evt.device_number, crate::device::types::DeviceNumber(1));
        assert!(evt.sync_enabled);

        finder.stop();
    }

    #[tokio::test]
    async fn send_receive_master_handoff_on_loopback() {
        use crate::protocol::command::build_master_command;

        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe_master_handoff();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = build_master_command(crate::device::types::DeviceNumber(3));
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for master handoff event")
            .expect("channel error");

        assert_eq!(evt.device_number, crate::device::types::DeviceNumber(3));
        assert_eq!(evt.target_device, crate::device::types::DeviceNumber(3));

        finder.stop();
    }

    #[tokio::test]
    async fn send_receive_fader_start_on_loopback() {
        use crate::protocol::command::{FaderAction, build_fader_start};

        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut rx = finder.subscribe_fader_start();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = build_fader_start(
            crate::device::types::DeviceNumber(5),
            [
                FaderAction::Start,
                FaderAction::Stop,
                FaderAction::NoChange,
                FaderAction::Start,
            ],
        );
        sender.send_to(&pkt, local_addr).await.unwrap();

        let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for fader start event")
            .expect("channel error");

        assert_eq!(evt.device_number, crate::device::types::DeviceNumber(5));
        assert_eq!(evt.channels[0], FaderAction::Start);
        assert_eq!(evt.channels[1], FaderAction::Stop);
        assert_eq!(evt.channels[2], FaderAction::NoChange);
        assert_eq!(evt.channels[3], FaderAction::Start);

        finder.stop();
    }

    #[tokio::test]
    async fn sync_does_not_appear_on_beat_channel() {
        use crate::protocol::command::build_sync_command;

        let socket = match UdpSocket::bind("127.0.0.1:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let local_addr = socket.local_addr().unwrap();

        let finder = BeatFinder::start_with_socket(socket).unwrap();
        let mut beat_rx = finder.subscribe();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pkt = build_sync_command(
            crate::device::types::DeviceNumber(1),
            crate::device::types::DeviceNumber(2),
            true,
        );
        sender.send_to(&pkt, local_addr).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(200), beat_rx.recv()).await;
        assert!(result.is_err(), "sync should not appear on beat channel");

        finder.stop();
    }

    #[test]
    fn is_xdj_xz_name_matches() {
        assert!(is_xdj_xz_name("XDJ-XZ"));
        assert!(is_xdj_xz_name("XDJ-AZ"));
        assert!(!is_xdj_xz_name("CDJ-3000"));
        assert!(!is_xdj_xz_name(""));
    }

    #[tokio::test]
    async fn can_see_xdj_xz_initially_false() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let finder = BeatFinder::start_with_socket(sock).unwrap();
        assert!(!finder.can_see_xdj_xz_in_pro_dj_link_mode());
        finder.stop();
    }

    #[tokio::test]
    async fn xdj_xz_seen_after_beat() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local_addr = sock.local_addr().unwrap();
        let finder = BeatFinder::start_with_socket(sock).unwrap();
        let mut rx = finder.subscribe();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut pkt = make_beat_packet(1, 12800);
        // Write device name "XDJ-AZ" at offset 0x0b so parse_beat sees it.
        let name = b"XDJ-AZ";
        pkt[0x0b..0x0b + name.len()].copy_from_slice(name);
        sender.send_to(&pkt, local_addr).await.unwrap();

        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(finder.can_see_xdj_xz_in_pro_dj_link_mode());
        finder.stop();
    }
}
