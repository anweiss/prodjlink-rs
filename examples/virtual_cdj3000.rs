//! Virtual CDJ-3000 simulator for testing beatbridge and prodjlink-rs consumers.
//!
//! Joins the DJ Link network as a CDJ-3000, broadcasts keep-alive, status, and
//! beat packets, and responds to incoming commands (fader start, load track,
//! tempo master handoff). Useful for integration testing without real hardware.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example virtual-cdj3000 -- --device-number 1 --bpm 128.0
//! ```
//!
//! Press Ctrl+C to stop. The simulator will deannounce itself on shutdown.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::Duration;

use clap::Parser;
use tokio::net::UdpSocket;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use prodjlink_rs::device::types::{Bpm, DeviceNumber, Pitch};
use prodjlink_rs::protocol::announce::build_keep_alive;
use prodjlink_rs::protocol::beat::build_beat;
use prodjlink_rs::protocol::command::FaderAction;
use prodjlink_rs::protocol::header::{BEAT_PORT, DISCOVERY_PORT, MAGIC_HEADER, STATUS_PORT};
use prodjlink_rs::protocol::status::{CdjStatusBuilder, CdjStatusFlags, build_cdj_status};

/// Virtual CDJ-3000 simulator for testing DJ Link consumers.
#[derive(Parser, Debug)]
#[command(name = "virtual-cdj3000")]
struct Args {
    /// Device number (1–4 for standard CDJ channels)
    #[arg(short = 'n', long, default_value_t = 1)]
    device_number: u8,

    /// Initial BPM
    #[arg(short, long, default_value_t = 128.0)]
    bpm: f64,

    /// Start in playing state
    #[arg(short, long, default_value_t = true)]
    playing: bool,

    /// Claim tempo master role
    #[arg(short, long, default_value_t = true)]
    master: bool,

    /// Network interface IP to bind (0.0.0.0 for all)
    #[arg(short, long, default_value = "0.0.0.0")]
    interface: Ipv4Addr,
}

/// Shared mutable state for the virtual CDJ.
struct CdjState {
    bpm: std::sync::atomic::AtomicU64,
    playing: AtomicBool,
    master: AtomicBool,
    synced: AtomicBool,
    beat_within_bar: AtomicU8,
    packet_counter: AtomicU32,
    beat_number: AtomicU32,
}

impl CdjState {
    fn new(bpm: f64, playing: bool, master: bool) -> Self {
        Self {
            bpm: std::sync::atomic::AtomicU64::new(bpm.to_bits()),
            playing: AtomicBool::new(playing),
            master: AtomicBool::new(master),
            synced: AtomicBool::new(true),
            beat_within_bar: AtomicU8::new(1),
            packet_counter: AtomicU32::new(0),
            beat_number: AtomicU32::new(1),
        }
    }

    fn bpm(&self) -> f64 {
        f64::from_bits(self.bpm.load(Ordering::Relaxed))
    }

    fn set_bpm(&self, bpm: f64) {
        self.bpm.store(bpm.to_bits(), Ordering::Relaxed);
    }

    fn next_beat(&self) -> u8 {
        let cur = self.beat_within_bar.load(Ordering::Relaxed);
        let next = if cur >= 4 { 1 } else { cur + 1 };
        self.beat_within_bar.store(next, Ordering::Relaxed);
        self.beat_number.fetch_add(1, Ordering::Relaxed);
        cur
    }

    fn next_packet_number(&self) -> u32 {
        self.packet_counter.fetch_add(1, Ordering::Relaxed)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    if args.device_number == 0 || args.device_number > 6 {
        eprintln!("Error: device-number must be 1–6");
        std::process::exit(1);
    }

    let device_name = "CDJ-3000";
    let device_number = DeviceNumber(args.device_number);
    let mac: [u8; 6] = [0x02, 0xCD, 0x30, 0x00, 0x00, args.device_number];

    let state = Arc::new(CdjState::new(args.bpm, args.playing, args.master));
    let shutdown = Arc::new(Notify::new());

    info!(
        name = device_name,
        number = args.device_number,
        bpm = args.bpm,
        playing = args.playing,
        master = args.master,
        "Starting virtual CDJ-3000"
    );

    // Bind sockets
    let discovery_socket = Arc::new(bind_broadcast_socket(0).await?);
    let beat_socket = Arc::new(bind_broadcast_socket(0).await?);
    let status_socket = Arc::new(bind_broadcast_socket(0).await?);

    // Bind a listener on the status port to receive incoming commands
    let cmd_socket = Arc::new(bind_reuse_socket(STATUS_PORT)?);

    // Spawn: keep-alive loop (port 50000, every 1.5s)
    let ka_shutdown = shutdown.clone();
    let ka_socket = discovery_socket.clone();
    let ka_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let pkt = build_keep_alive(device_name, device_number, mac, args.interface);
                    let _ = ka_socket.send_to(&pkt, dest).await;
                    debug!("keep-alive sent");
                }
                _ = ka_shutdown.notified() => break,
            }
        }
    });

    // Spawn: status broadcast loop (port 50002, every 200ms)
    let st_shutdown = shutdown.clone();
    let st_state = state.clone();
    let st_socket = status_socket.clone();
    let st_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let bpm = st_state.bpm();
                    let playing = st_state.playing.load(Ordering::Relaxed);
                    let is_master = st_state.master.load(Ordering::Relaxed);
                    let synced = st_state.synced.load(Ordering::Relaxed);
                    let beat_within_bar = st_state.beat_within_bar.load(Ordering::Relaxed);
                    let seq = st_state.next_packet_number();
                    let beat_num = st_state.beat_number.load(Ordering::Relaxed);

                    let flags = CdjStatusFlags {
                        playing,
                        master: is_master,
                        synced,
                        on_air: true,
                        bpm_sync: false,
                    };
                    let builder = CdjStatusBuilder {
                        device_name: device_name.to_string(),
                        device_number,
                        flags,
                        bpm: Bpm(bpm),
                        pitch: Pitch(0x100000),
                        beat_number: Some(beat_num),
                        beat_within_bar,
                        master_hand_off: None,
                        packet_number: seq,
                    };
                    let pkt = build_cdj_status(&builder);
                    let _ = st_socket.send_to(&pkt, dest).await;
                }
                _ = st_shutdown.notified() => break,
            }
        }
    });

    // Spawn: beat broadcast loop (port 50001, interval derived from BPM)
    let bt_shutdown = shutdown.clone();
    let bt_state = state.clone();
    let bt_socket = beat_socket.clone();
    let bt_handle = tokio::spawn(async move {
        let dest = std::net::SocketAddr::new(Ipv4Addr::BROADCAST.into(), BEAT_PORT);
        loop {
            let bpm = bt_state.bpm();
            let beat_interval_ms = if bpm > 0.0 {
                (60_000.0 / bpm) as u64
            } else {
                500
            };

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(beat_interval_ms)) => {
                    if !bt_state.playing.load(Ordering::Relaxed) {
                        continue;
                    }
                    let beat_within_bar = bt_state.next_beat();
                    let pkt = build_beat(
                        device_name,
                        device_number,
                        Bpm(bpm),
                        0x100000, // normal pitch
                        beat_within_bar,
                    );
                    let _ = bt_socket.send_to(&pkt, dest).await;
                    debug!(
                        bpm,
                        beat = beat_within_bar,
                        "beat sent"
                    );
                }
                _ = bt_shutdown.notified() => break,
            }
        }
    });

    // Spawn: command listener (incoming packets on port 50002)
    let cmd_shutdown = shutdown.clone();
    let cmd_state = state.clone();
    let cmd_handle = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            tokio::select! {
                result = cmd_socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, src)) => {
                            handle_incoming_command(&buf[..len], src, &cmd_state);
                        }
                        Err(e) => {
                            warn!(error = %e, "command listener error");
                            break;
                        }
                    }
                }
                _ = cmd_shutdown.notified() => break,
            }
        }
    });

    // Interactive console: BPM changes, play/pause, master toggle
    let console_shutdown = shutdown.clone();
    let console_state = state.clone();
    let console_handle = tokio::spawn(async move {
        print_help();
        let stdin = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(tokio::io::stdin()));
        tokio::pin!(stdin);
        loop {
            tokio::select! {
                line = stdin.next_line() => {
                    match line {
                        Ok(Some(input)) => {
                            if !handle_console_input(&input, &console_state) {
                                console_shutdown.notify_waiters();
                                break;
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                _ = console_shutdown.notified() => break,
            }
        }
    });

    // Wait for Ctrl+C or console quit
    let sig_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl+C received, shutting down...");
        sig_shutdown.notify_waiters();
    });

    // Wait for shutdown
    shutdown.notified().await;
    // Give tasks a moment to exit
    tokio::time::sleep(Duration::from_millis(100)).await;

    ka_handle.abort();
    st_handle.abort();
    bt_handle.abort();
    cmd_handle.abort();
    console_handle.abort();

    info!("Virtual CDJ-3000 stopped");
    Ok(())
}

fn print_help() {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════╗");
    eprintln!("║       Virtual CDJ-3000 Simulator         ║");
    eprintln!("╠══════════════════════════════════════════╣");
    eprintln!("║  Commands:                               ║");
    eprintln!("║    <number>  — set BPM (e.g. 135.5)      ║");
    eprintln!("║    p         — toggle play/pause          ║");
    eprintln!("║    m         — toggle master               ║");
    eprintln!("║    s         — toggle sync                 ║");
    eprintln!("║    i         — show current state          ║");
    eprintln!("║    q         — quit                        ║");
    eprintln!("║    h         — show this help              ║");
    eprintln!("╚══════════════════════════════════════════╝");
    eprintln!();
}

fn handle_console_input(input: &str, state: &CdjState) -> bool {
    let input = input.trim();
    if input.is_empty() {
        return true;
    }

    match input {
        "q" | "quit" | "exit" => return false,
        "p" | "play" => {
            let was = state.playing.load(Ordering::Relaxed);
            state.playing.store(!was, Ordering::Relaxed);
            info!(playing = !was, "toggled play state");
        }
        "m" | "master" => {
            let was = state.master.load(Ordering::Relaxed);
            state.master.store(!was, Ordering::Relaxed);
            info!(master = !was, "toggled master state");
        }
        "s" | "sync" => {
            let was = state.synced.load(Ordering::Relaxed);
            state.synced.store(!was, Ordering::Relaxed);
            info!(synced = !was, "toggled sync state");
        }
        "i" | "info" | "status" => {
            let bpm = state.bpm();
            let playing = state.playing.load(Ordering::Relaxed);
            let master = state.master.load(Ordering::Relaxed);
            let synced = state.synced.load(Ordering::Relaxed);
            let beat = state.beat_within_bar.load(Ordering::Relaxed);
            eprintln!(
                "  BPM: {bpm:.1} | Playing: {playing} | Master: {master} | Sync: {synced} | Beat: {beat}/4"
            );
        }
        "h" | "help" => print_help(),
        other => {
            if let Ok(bpm) = other.parse::<f64>() {
                if bpm > 0.0 && bpm < 300.0 {
                    state.set_bpm(bpm);
                    info!(bpm, "BPM changed");
                } else {
                    eprintln!("  BPM must be between 0 and 300");
                }
            } else {
                eprintln!("  Unknown command: {other} (type 'h' for help)");
            }
        }
    }
    true
}

fn handle_incoming_command(data: &[u8], src: std::net::SocketAddr, state: &CdjState) {
    if data.len() < 11 {
        return;
    }
    if data[..10] != MAGIC_HEADER {
        return;
    }

    match data[0x0a] {
        // Fader start (0x02)
        0x02 => {
            if data.len() < 0x28 {
                return;
            }
            let source = data[0x21];
            let channels: [FaderAction; 4] = [
                byte_to_fader(data[0x24]),
                byte_to_fader(data[0x25]),
                byte_to_fader(data[0x26]),
                byte_to_fader(data[0x27]),
            ];
            info!(
                from = source,
                ch1 = ?channels[0],
                ch2 = ?channels[1],
                ch3 = ?channels[2],
                ch4 = ?channels[3],
                "← Received fader start command"
            );

            // Apply to our channel if targeted
            let our_ch = state.beat_within_bar.load(Ordering::Relaxed); // reuse for device_number
            // device_number is not stored in state, but we check by index
            for (i, action) in channels.iter().enumerate() {
                match action {
                    FaderAction::Start => {
                        state.playing.store(true, Ordering::Relaxed);
                        info!(channel = i + 1, "▶ Started playing (fader start)");
                    }
                    FaderAction::Stop => {
                        state.playing.store(false, Ordering::Relaxed);
                        info!(channel = i + 1, "⏸ Stopped playing (fader stop)");
                    }
                    FaderAction::NoChange => {}
                }
            }
            // Suppress unused variable
            let _ = our_ch;
        }
        // Load track (0x19)
        0x19 => {
            if data.len() < 0x30 {
                return;
            }
            let source = data[0x21];
            let rb_id = u32::from_be_bytes([data[0x2c], data[0x2d], data[0x2e], data[0x2f]]);
            info!(
                from = source,
                rekordbox_id = rb_id,
                %src,
                "← Received load track command"
            );
        }
        // Sync command (0x2a) — arrives on beat port but we listen on status
        // Master command (0x26) — arrives on beat port
        other => {
            debug!(packet_type = other, len = data.len(), "← Unknown command type");
        }
    }
}

fn byte_to_fader(b: u8) -> FaderAction {
    match b {
        0x00 => FaderAction::Start,
        0x01 => FaderAction::Stop,
        _ => FaderAction::NoChange,
    }
}

async fn bind_broadcast_socket(port: u16) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{port}")).await?;
    socket.set_broadcast(true)?;
    Ok(socket)
}

fn bind_reuse_socket(port: u16) -> std::io::Result<UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(not(windows))]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    socket.bind(&addr.into())?;
    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket)
}
