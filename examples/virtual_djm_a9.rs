//! Virtual DJM-A9 mixer simulator for testing DJ Link consumers.
//!
//! Broadcasts keep-alive (port 50000), mixer status (port 50002), and beat
//! packets (port 50001) just like a real DJM-A9 on the network.

use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use clap::Parser;
use tokio::net::UdpSocket;
use tokio::sync::Notify;

use prodjlink_rs::device::types::{Bpm, DeviceNumber, DeviceType, Pitch};
use prodjlink_rs::protocol::announce::build_keep_alive_typed;
use prodjlink_rs::protocol::beat::build_beat;
use prodjlink_rs::protocol::header::{BEAT_PORT, DISCOVERY_PORT, STATUS_PORT};
use prodjlink_rs::protocol::status::{MixerStatusBuilder, build_mixer_status};

/// Virtual DJM-A9 mixer simulator for testing DJ Link consumers.
#[derive(Parser, Debug)]
#[command(name = "virtual-djm-a9")]
struct Args {
    /// Device number (mixers typically use 33)
    #[arg(short = 'n', long, default_value_t = 33)]
    device_number: u8,

    /// Initial BPM
    #[arg(short, long, default_value_t = 128.0)]
    bpm: f64,

    /// Network interface IP to bind (0.0.0.0 for all)
    #[arg(short, long, default_value_t = Ipv4Addr::UNSPECIFIED)]
    interface: Ipv4Addr,
}

struct MixerState {
    device_number: u8,
    bpm: std::sync::atomic::AtomicU64,
    beat_within_bar: AtomicU8,
    is_master: AtomicBool,
    is_synced: AtomicBool,
}

impl MixerState {
    fn new(device_number: u8, bpm: f64) -> Self {
        Self {
            device_number,
            bpm: std::sync::atomic::AtomicU64::new(bpm.to_bits()),
            beat_within_bar: AtomicU8::new(1),
            is_master: AtomicBool::new(false),
            is_synced: AtomicBool::new(false),
        }
    }

    fn bpm(&self) -> f64 {
        f64::from_bits(self.bpm.load(Ordering::Relaxed))
    }

    fn next_beat(&self) -> u8 {
        let current = self.beat_within_bar.load(Ordering::Relaxed);
        let next = if current >= 4 { 1 } else { current + 1 };
        self.beat_within_bar.store(next, Ordering::Relaxed);
        next
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let device_name = "DJM-A9";
    let device_number = DeviceNumber(args.device_number);
    let mac: [u8; 6] = [0x02, 0xD0, 0xA9, 0x00, 0x00, args.device_number];

    let state = Arc::new(MixerState::new(args.device_number, args.bpm));
    let shutdown = Arc::new(Notify::new());

    // Bind sockets
    let discovery_socket = Arc::new(bind_broadcast_socket(0).await?);
    let beat_socket = Arc::new(bind_broadcast_socket(0).await?);
    let status_socket = Arc::new(bind_broadcast_socket(0).await?);

    eprintln!(
        "🎛️  Virtual DJM-A9 started: device={}, bpm={:.1}",
        args.device_number, args.bpm
    );

    let interface = args.interface;

    // Keep-alive loop (port 50000, every 1.5s)
    let ka_shutdown = shutdown.clone();
    let ka_socket = discovery_socket.clone();
    let ka_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let pkt = build_keep_alive_typed(
                        device_name,
                        device_number,
                        mac,
                        interface,
                        DeviceType::Mixer,
                    );
                    let _ = ka_socket.send_to(&pkt, dest).await;
                }
                _ = ka_shutdown.notified() => break,
            }
        }
    });

    // Status broadcast loop (port 50002, every 200ms)
    let st_shutdown = shutdown.clone();
    let st_state = state.clone();
    let st_socket = status_socket.clone();
    let st_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let builder = MixerStatusBuilder {
                        device_name: "DJM-A9".to_string(),
                        device_number,
                        bpm: Bpm(st_state.bpm()),
                        pitch: Pitch(0x100000),
                        beat_within_bar: st_state.beat_within_bar.load(Ordering::Relaxed),
                        is_master: st_state.is_master.load(Ordering::Relaxed),
                        is_synced: st_state.is_synced.load(Ordering::Relaxed),
                        master_hand_off: None,
                    };
                    let pkt = build_mixer_status(&builder);
                    let _ = st_socket.send_to(&pkt, dest).await;
                }
                _ = st_shutdown.notified() => break,
            }
        }
    });

    // Beat broadcast loop (port 50001)
    let bt_shutdown = shutdown.clone();
    let bt_state = state.clone();
    let bt_socket = beat_socket.clone();
    let bt_handle = tokio::spawn(async move {
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), BEAT_PORT);
        loop {
            let bpm = bt_state.bpm();
            let beat_interval_ms = if bpm > 0.0 { 60_000.0 / bpm } else { 500.0 };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs_f64(beat_interval_ms / 1000.0)) => {
                    let beat_within_bar = bt_state.next_beat();
                    let pkt = build_beat(
                        "DJM-A9",
                        device_number,
                        Bpm(bpm),
                        0x100000,
                        beat_within_bar,
                    );
                    let _ = bt_socket.send_to(&pkt, dest).await;
                }
                _ = bt_shutdown.notified() => break,
            }
        }
    });

    // Ctrl+C handler
    let sig_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        sig_shutdown.notify_waiters();
    });

    // Simple status display
    let ui_state = state.clone();
    let ui_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    eprint!(
                        "\r🎛️  DJM-A9 [{}] │ {:.1} BPM │ beat {}  ",
                        ui_state.device_number,
                        ui_state.bpm(),
                        ui_state.beat_within_bar.load(Ordering::Relaxed),
                    );
                    let _ = std::io::stderr().flush();
                }
                _ = ui_shutdown.notified() => break,
            }
        }
    });

    // Wait for shutdown
    shutdown.notified().await;
    eprintln!("\n🎛️  DJM-A9 shutting down...");

    ka_handle.abort();
    st_handle.abort();
    bt_handle.abort();

    Ok(())
}

async fn bind_broadcast_socket(port: u16) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{port}")).await?;
    socket.set_broadcast(true)?;
    Ok(socket)
}
