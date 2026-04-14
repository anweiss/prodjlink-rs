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
//! All keys are instant (no Enter required). Press `q` or Ctrl+C to stop.

use std::fmt::Write as _;
use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{self, ClearType};
use crossterm::{cursor, execute};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

use unicode_width::UnicodeWidthStr;

use prodjlink_rs::device::types::{Bpm, DeviceNumber, Pitch};
use prodjlink_rs::protocol::announce::build_keep_alive;
use prodjlink_rs::protocol::beat::{Beat, build_beat, parse_beat};
use prodjlink_rs::protocol::command::FaderAction;
use prodjlink_rs::protocol::header::{BEAT_PORT, DISCOVERY_PORT, MAGIC_HEADER, STATUS_PORT};
use prodjlink_rs::protocol::status::{
    CdjStatusBuilder, CdjStatusFlags, DeviceUpdate, build_cdj_status, parse_status,
};

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

// ── Shared state ────────────────────────────────────────────────────────────

struct CdjState {
    device_number: u8,
    bpm: std::sync::atomic::AtomicU64,
    playing: AtomicBool,
    master: AtomicBool,
    synced: AtomicBool,
    beat_within_bar: AtomicU8,
    packet_counter: AtomicU32,
    beat_number: AtomicU32,
    /// Device number of the current network tempo master (0 = none known).
    master_device: AtomicU8,
    /// Rolling log of recent events (newest first). Protected by a std Mutex
    /// because we only hold it briefly from sync contexts.
    event_log: std::sync::Mutex<Vec<LogEntry>>,
    /// Phase reference from the current master's most recent beat.
    phase_ref: std::sync::Mutex<Option<PhaseRef>>,
    /// Wakes the beat loop when the phase reference changes.
    phase_notify: Notify,
}

struct LogEntry {
    time: Instant,
    message: String,
}

/// Reference point for phase-aligning our beats with the master's beat grid.
#[derive(Clone)]
struct PhaseRef {
    /// When the master beat was received.
    time: Instant,
    /// The master's beat_within_bar at that moment (1–4).
    beat_within_bar: u8,
}

const MAX_LOG_ENTRIES: usize = 8;

impl CdjState {
    fn new(device_number: u8, bpm: f64, playing: bool, master: bool) -> Self {
        Self {
            device_number,
            bpm: std::sync::atomic::AtomicU64::new(bpm.to_bits()),
            playing: AtomicBool::new(playing),
            master: AtomicBool::new(master),
            synced: AtomicBool::new(true),
            beat_within_bar: AtomicU8::new(1),
            packet_counter: AtomicU32::new(0),
            beat_number: AtomicU32::new(1),
            master_device: AtomicU8::new(0),
            event_log: std::sync::Mutex::new(Vec::new()),
            phase_ref: std::sync::Mutex::new(None),
            phase_notify: Notify::new(),
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

    fn push_event(&self, msg: String) {
        if let Ok(mut log) = self.event_log.lock() {
            log.insert(
                0,
                LogEntry {
                    time: Instant::now(),
                    message: msg,
                },
            );
            log.truncate(MAX_LOG_ENTRIES);
        }
    }

    /// Update the phase reference from an incoming master beat.
    fn set_phase_ref(&self, beat: &Beat) {
        if beat.beat_within_bar < 1 || beat.beat_within_bar > 4 {
            return; // Not meaningful for bar alignment.
        }
        if let Ok(mut pr) = self.phase_ref.lock() {
            *pr = Some(PhaseRef {
                time: beat.timestamp,
                beat_within_bar: beat.beat_within_bar,
            });
        }
        self.phase_notify.notify_waiters();
    }

    /// Clear the phase reference (master changed, sync off, etc.).
    fn clear_phase_ref(&self) {
        if let Ok(mut pr) = self.phase_ref.lock() {
            *pr = None;
        }
        self.phase_notify.notify_waiters();
    }

    /// Whether we have an active (non-stale) phase lock with the master.
    fn has_phase_lock(&self) -> bool {
        self.phase_ref
            .lock()
            .ok()
            .and_then(|pr| pr.as_ref().map(|p| p.time.elapsed().as_secs() < 5))
            .unwrap_or(false)
    }
}

// ── Beat phase bar ──────────────────────────────────────────────────────────

fn beat_phase_bar(beat: u8) -> String {
    let mut bar = String::with_capacity(9);
    bar.push('[');
    for i in 1..=4u8 {
        if i == beat {
            bar.push_str("\u{2588}");
        } else {
            bar.push('.');
        }
    }
    bar.push(']');
    bar
}

// ── Render ──────────────────────────────────────────────────────────────────

const INNER_W: usize = 49;

/// Format a content row padded to exactly INNER_W display columns.
fn row(content: &str) -> String {
    let display_w = UnicodeWidthStr::width(content);
    let padding = INNER_W.saturating_sub(display_w);
    format!("\u{2502}{content}{:padding$}\u{2502}", "")
}

fn sep(left: char, right: char) -> String {
    let bar: String = "\u{2500}".repeat(INNER_W);
    format!("{left}{bar}{right}")
}

fn render_display(state: &CdjState, start: Instant) -> String {
    let bpm = state.bpm();
    let playing = state.playing.load(Ordering::Relaxed);
    let master = state.master.load(Ordering::Relaxed);
    let synced = state.synced.load(Ordering::Relaxed);
    let beat = state.beat_within_bar.load(Ordering::Relaxed);
    let beat_num = state.beat_number.load(Ordering::Relaxed);
    let master_dev = state.master_device.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs();

    let play_icon = if playing { "\u{25b6}" } else { "\u{23f8}" };
    let master_str = if master { "MASTER" } else { "      " };
    let sync_str = if synced { "SYNC" } else { "    " };
    let phase_locked = state.has_phase_lock();
    let following_str = if synced && !master && master_dev > 0 {
        let phase_str = if phase_locked { " \u{2713}" } else { " ..." };
        format!("  following P{}{}", master_dev, phase_str)
    } else {
        String::new()
    };

    let mut out = String::with_capacity(1024);

    // Header
    let _ = writeln!(out, "{}", sep('\u{250c}', '\u{2510}'));
    let _ = writeln!(
        out,
        "{}",
        row(&format!(
            "  CDJ-3000  |  Player {}  |  {:>4}s uptime",
            state.device_number, elapsed
        ))
    );
    let _ = writeln!(out, "{}", sep('\u{251c}', '\u{2524}'));

    // Main status
    let _ = writeln!(out, "{}", row(""));
    let _ = writeln!(
        out,
        "{}",
        row(&format!(
            "  {play_icon}  {bpm:>6.1} BPM   {master_str}  {sync_str}{following_str}"
        ))
    );
    let _ = writeln!(out, "{}", row(""));
    let _ = writeln!(
        out,
        "{}",
        row(&format!(
            "  Beat: {}/4  {}  #{:<8}",
            beat,
            beat_phase_bar(beat),
            beat_num
        ))
    );
    let _ = writeln!(out, "{}", row(""));
    let _ = writeln!(out, "{}", sep('\u{251c}', '\u{2524}'));

    // Controls — one binding per line
    let _ = writeln!(out, "{}", row("  \u{2191}/\u{2193}   BPM +/-1"));
    let _ = writeln!(out, "{}", row("  \u{2190}/\u{2192}   BPM +/-0.1"));
    let _ = writeln!(out, "{}", row("  p     play / pause"));
    let _ = writeln!(out, "{}", row("  m     master"));
    let _ = writeln!(out, "{}", row("  s     sync"));
    let _ = writeln!(out, "{}", row("  1-9   BPM preset (x20)"));
    let _ = writeln!(out, "{}", row("  q     quit"));
    let _ = writeln!(out, "{}", sep('\u{251c}', '\u{2524}'));

    // Event log
    if let Ok(log) = state.event_log.lock() {
        if log.is_empty() {
            let _ = writeln!(out, "{}", row("  (no events yet)"));
        }
        let now = Instant::now();
        for entry in log.iter().take(6) {
            let age = now.duration_since(entry.time).as_secs();
            let dim = if age > 5 { "." } else { "\u{2190}" };
            let msg: String = entry.message.chars().take(42).collect();
            let _ = writeln!(out, "{}", row(&format!(" {dim} {msg}")));
        }
    }

    let _ = writeln!(out, "{}", sep('\u{2514}', '\u{2518}'));
    out
}

// ── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.device_number == 0 || args.device_number > 6 {
        eprintln!("Error: device-number must be 1–6");
        std::process::exit(1);
    }

    let device_name = "CDJ-3000";
    let device_number = DeviceNumber(args.device_number);
    let mac: [u8; 6] = [0x02, 0xCD, 0x30, 0x00, 0x00, args.device_number];

    let state = Arc::new(CdjState::new(
        args.device_number,
        args.bpm,
        args.playing,
        args.master,
    ));
    let shutdown = Arc::new(Notify::new());
    let start_time = Instant::now();

    // Enable raw mode for instant key input
    terminal::enable_raw_mode()?;
    // Hide cursor
    let mut stdout = std::io::stdout();
    execute!(stdout, cursor::Hide)?;

    // Bind sockets
    let discovery_socket = Arc::new(bind_broadcast_socket(0).await?);
    let beat_socket = Arc::new(bind_broadcast_socket(0).await?);
    let status_socket = Arc::new(bind_broadcast_socket(0).await?);
    let cmd_socket = Arc::new(bind_reuse_socket(STATUS_PORT)?);
    let beat_listen_socket = Arc::new(bind_reuse_socket(BEAT_PORT)?);

    state.push_event(format!(
        "Started as Player {} @ {:.1} BPM",
        args.device_number, args.bpm
    ));

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
                        device_name: "CDJ-3000".to_string(),
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
            let synced = bt_state.synced.load(Ordering::Relaxed);
            let we_are_master = bt_state.master.load(Ordering::Relaxed);

            let beat_interval_ms = if bpm > 0.0 { 60_000.0 / bpm } else { 500.0 };

            // When synced to a master, phase-align our beats to the master's
            // beat grid.  Otherwise free-run at the current BPM.
            let (sleep_dur, target_beat) = if synced && !we_are_master {
                phase_aligned_sleep(&bt_state, beat_interval_ms)
            } else {
                (Duration::from_secs_f64(beat_interval_ms / 1000.0), None)
            };

            tokio::select! {
                _ = tokio::time::sleep(sleep_dur) => {
                    if !bt_state.playing.load(Ordering::Relaxed) {
                        continue;
                    }
                    let beat_within_bar = if let Some(target) = target_beat {
                        // Phase-locked: override our bar position to match the
                        // master's beat grid.
                        let next = if target >= 4 { 1 } else { target + 1 };
                        bt_state.beat_within_bar.store(next, Ordering::Relaxed);
                        bt_state.beat_number.fetch_add(1, Ordering::Relaxed);
                        target
                    } else {
                        bt_state.next_beat()
                    };
                    let bpm = bt_state.bpm();
                    let pkt = build_beat(
                        "CDJ-3000",
                        device_number,
                        Bpm(bpm),
                        0x100000,
                        beat_within_bar,
                    );
                    let _ = bt_socket.send_to(&pkt, dest).await;
                }
                // Phase reference changed — recalculate timing immediately.
                _ = bt_state.phase_notify.notified() => continue,
                _ = bt_shutdown.notified() => break,
            }
        }
    });

    // Spawn: command listener (incoming packets on port 50002)
    let cmd_shutdown = shutdown.clone();
    let cmd_state = state.clone();
    let cmd_device_number = args.device_number;
    let cmd_handle = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            tokio::select! {
                result = cmd_socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, _src)) => {
                            handle_incoming_command(&buf[..len], cmd_device_number, &cmd_state);
                        }
                        Err(_) => break,
                    }
                }
                _ = cmd_shutdown.notified() => break,
            }
        }
    });

    // Spawn: beat listener (incoming beats on port 50001 for sync)
    let bl_shutdown = shutdown.clone();
    let bl_state = state.clone();
    let bl_device_number = args.device_number;
    let bl_handle = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            tokio::select! {
                result = beat_listen_socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, _src)) => {
                            handle_incoming_beat(&buf[..len], bl_device_number, &bl_state);
                        }
                        Err(_) => break,
                    }
                }
                _ = bl_shutdown.notified() => break,
            }
        }
    });

    // Spawn: Ctrl+C handler
    let sig_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        sig_shutdown.notify_waiters();
    });

    // Main loop: render display + read keys (non-blocking)
    let ui_state = state.clone();
    let ui_shutdown = shutdown.clone();
    let ui_handle = tokio::task::spawn_blocking(move || {
        let mut last_render = Instant::now() - Duration::from_secs(1);
        loop {
            // Check for shutdown
            // (Notify doesn't have try_wait, so we check a flag via the display interval)

            // Render at ~10 Hz
            if last_render.elapsed() >= Duration::from_millis(100) {
                let display = render_display(&ui_state, start_time);
                // In raw mode, \n only moves down without carriage return.
                // Replace \n with \r\n so each line starts at column 0.
                let display = display.replace('\n', "\r\n");
                let mut stdout = std::io::stdout();
                let _ = execute!(
                    stdout,
                    cursor::MoveTo(0, 0),
                    terminal::Clear(ClearType::All)
                );
                let _ = stdout.write_all(display.as_bytes());
                let _ = stdout.flush();
                last_render = Instant::now();
            }

            // Poll for key events (50ms timeout keeps render responsive)
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                })) = event::read()
                {
                    // Only handle Press events (avoid double-fire on Release)
                    if kind != KeyEventKind::Press {
                        continue;
                    }

                    // Ctrl+C
                    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                        break;
                    }

                    match code {
                        KeyCode::Char('q') => break,

                        KeyCode::Char('p') => {
                            let was = ui_state.playing.load(Ordering::Relaxed);
                            ui_state.playing.store(!was, Ordering::Relaxed);
                            ui_state.push_event(if !was {
                                "\u{25b6} Playing".into()
                            } else {
                                "\u{23f8} Paused".into()
                            });
                        }

                        KeyCode::Char('m') => {
                            let was = ui_state.master.load(Ordering::Relaxed);
                            ui_state.master.store(!was, Ordering::Relaxed);
                            if !was {
                                // Becoming master — we drive the grid, not follow it.
                                ui_state.clear_phase_ref();
                            }
                            ui_state.push_event(if !was {
                                "\u{2605} Master ON".into()
                            } else {
                                "\u{2606} Master OFF".into()
                            });
                        }

                        KeyCode::Char('s') => {
                            let was = ui_state.synced.load(Ordering::Relaxed);
                            ui_state.synced.store(!was, Ordering::Relaxed);
                            if was {
                                // Sync turned off — drop phase lock.
                                ui_state.clear_phase_ref();
                            }
                            ui_state.push_event(if !was {
                                "Sync ON".into()
                            } else {
                                "Sync OFF".into()
                            });
                        }

                        // Arrow keys: fine BPM adjustment
                        KeyCode::Up => {
                            let new_bpm = (ui_state.bpm() + 1.0).min(300.0);
                            ui_state.set_bpm(new_bpm);
                            ui_state.push_event(format!("BPM \u{2192} {new_bpm:.1}"));
                        }
                        KeyCode::Down => {
                            let new_bpm = (ui_state.bpm() - 1.0).max(20.0);
                            ui_state.set_bpm(new_bpm);
                            ui_state.push_event(format!("BPM \u{2192} {new_bpm:.1}"));
                        }
                        KeyCode::Right => {
                            let new_bpm = (ui_state.bpm() + 0.1).min(300.0);
                            ui_state.set_bpm(new_bpm);
                            ui_state.push_event(format!("BPM \u{2192} {new_bpm:.1}"));
                        }
                        KeyCode::Left => {
                            let new_bpm = (ui_state.bpm() - 0.1).max(20.0);
                            ui_state.set_bpm(new_bpm);
                            ui_state.push_event(format!("BPM \u{2192} {new_bpm:.1}"));
                        }

                        // Number keys: BPM presets (1=80, 2=100, 3=120, ..., 9=240)
                        KeyCode::Char(c @ '1'..='9') => {
                            let n = (c as u8 - b'0') as f64;
                            let preset = 60.0 + n * 20.0;
                            ui_state.set_bpm(preset);
                            ui_state.push_event(format!("BPM preset \u{2192} {preset:.0}"));
                        }

                        _ => {}
                    }
                }
            }
        }
        ui_shutdown.notify_waiters();
    });

    // Wait for UI task to exit (triggered by key press or Ctrl+C)
    let _ = ui_handle.await;

    // Clean up terminal
    let mut stdout = std::io::stdout();
    let _ = execute!(
        stdout,
        cursor::Show,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    );
    terminal::disable_raw_mode()?;

    eprintln!("Virtual CDJ-3000 stopped.");

    // Abort background tasks
    ka_handle.abort();
    st_handle.abort();
    bt_handle.abort();
    cmd_handle.abort();
    bl_handle.abort();

    Ok(())
}

// ── Command handling ────────────────────────────────────────────────────────

fn handle_incoming_command(data: &[u8], our_device: u8, state: &CdjState) {
    if data.len() < 11 || data[..10] != MAGIC_HEADER {
        return;
    }

    match data[0x0a] {
        // CDJ status (0x0a on status port) — sync BPM with the network master
        0x0a => {
            if let Ok(DeviceUpdate::Cdj(status)) = parse_status(data) {
                let source = status.device_number.0;
                // Ignore our own packets
                if source == our_device {
                    return;
                }

                if status.is_master {
                    let prev_master = state.master_device.swap(source, Ordering::Relaxed);
                    let synced = state.synced.load(Ordering::Relaxed);
                    let we_are_master = state.master.load(Ordering::Relaxed);

                    // If we had claimed master but another device is master, yield
                    if we_are_master {
                        state.master.store(false, Ordering::Relaxed);
                        state.push_event(format!("P{source} is master, yielding"));
                    }

                    // When synced, follow the master's BPM
                    if synced && !we_are_master && status.bpm.0 > 0.0 {
                        let old_bpm = state.bpm();
                        let new_bpm = status.bpm.0;
                        if (old_bpm - new_bpm).abs() > 0.05 {
                            state.set_bpm(new_bpm);
                            state.push_event(format!("Sync P{source} \u{2192} {new_bpm:.1} BPM"));
                        }
                    }

                    if prev_master != source {
                        state.clear_phase_ref();
                        state.push_event(format!("P{source} became master"));
                    }
                }
            }
        }

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

            // Only react to the action targeting our channel (1-indexed)
            if our_device >= 1 && our_device <= 4 {
                match channels[(our_device - 1) as usize] {
                    FaderAction::Start => {
                        state.playing.store(true, Ordering::Relaxed);
                        state.push_event(format!("\u{2190} Fader START from device {source}"));
                    }
                    FaderAction::Stop => {
                        state.playing.store(false, Ordering::Relaxed);
                        state.push_event(format!("\u{2190} Fader STOP from device {source}"));
                    }
                    FaderAction::NoChange => {}
                }
            }
        }
        // Load track (0x19)
        0x19 => {
            if data.len() < 0x30 {
                return;
            }
            let source = data[0x21];
            let rb_id = u32::from_be_bytes([data[0x2c], data[0x2d], data[0x2e], data[0x2f]]);
            state.push_event(format!("\u{2190} Load track #{rb_id} from device {source}"));
        }
        _ => {}
    }
}

fn byte_to_fader(b: u8) -> FaderAction {
    match b {
        0x00 => FaderAction::Start,
        0x01 => FaderAction::Stop,
        _ => FaderAction::NoChange,
    }
}

fn handle_incoming_beat(data: &[u8], our_device: u8, state: &CdjState) {
    if let Ok(beat) = parse_beat(data) {
        let source = beat.device_number.0;
        if source == our_device {
            return;
        }

        // If the beat came from the known master and we're synced, follow BPM
        // and phase-lock to the master's beat grid.
        let master_dev = state.master_device.load(Ordering::Relaxed);
        let synced = state.synced.load(Ordering::Relaxed);
        let we_are_master = state.master.load(Ordering::Relaxed);

        if synced && !we_are_master && source == master_dev && beat.bpm.0 > 0.0 {
            let old_bpm = state.bpm();
            let new_bpm = beat.bpm.0;
            if (old_bpm - new_bpm).abs() > 0.05 {
                state.set_bpm(new_bpm);
            }
            // Update phase reference so our beat loop aligns with the master.
            state.set_phase_ref(&beat);
        }
    }
}

// ── Phase alignment ─────────────────────────────────────────────────────────

/// Calculate the sleep duration and target `beat_within_bar` to align with the
/// master's beat grid.  Returns `(sleep_duration, Some(beat))` when a valid
/// phase reference is available, or a free-running interval otherwise.
fn phase_aligned_sleep(state: &CdjState, beat_interval_ms: f64) -> (Duration, Option<u8>) {
    let phase = state.phase_ref.lock().ok().and_then(|pr| pr.clone());

    let Some(phase) = phase else {
        // No phase reference yet — free-run.
        return (Duration::from_secs_f64(beat_interval_ms / 1000.0), None);
    };

    // If the reference is stale (>4 beat intervals old) fall back to
    // free-running so we don't drift if the master disappears.
    let staleness = phase.time.elapsed();
    if staleness > Duration::from_secs_f64(beat_interval_ms * 4.0 / 1000.0) {
        return (Duration::from_secs_f64(beat_interval_ms / 1000.0), None);
    }

    let elapsed_ms = staleness.as_secs_f64() * 1000.0;

    // Find the next grid point after *now*.
    //   Master beat grid: phase.time + k * beat_interval_ms  for k = 0, 1, 2, …
    //   k=0 is the reference beat that already happened.
    let k_float = elapsed_ms / beat_interval_ms;
    let next_k = (k_float.floor() as i64) + 1;
    let time_to_next_ms = (next_k as f64) * beat_interval_ms - elapsed_ms;

    // What beat_within_bar will align with the master at that grid point?
    // The master played `phase.beat_within_bar` at k=0, so at k=next_k it
    // would play ((ref_beat - 1 + next_k) % 4) + 1.
    let ref_beat = phase.beat_within_bar.max(1) as i64;
    let target_beat = (((ref_beat - 1 + next_k) % 4 + 4) % 4 + 1) as u8;

    let sleep = Duration::from_secs_f64(time_to_next_ms.max(0.5) / 1000.0);
    (sleep, Some(target_beat))
}

// ── Socket helpers ──────────────────────────────────────────────────────────

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
