use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

use crate::device::settings::PlayerSettings;
use crate::device::types::{Bpm, DeviceNumber, TrackSourceSlot, TrackType};
use crate::error::{ProDjLinkError, Result};
use crate::network::finder::{DeviceFinder, FinderEvent};
use crate::network::interface::find_interface_by_ip;
use crate::network::tempo::TempoMaster;
use crate::protocol::announce::{
    build_claim_stage1, build_claim_stage2, build_claim_stage3, build_defense, build_device_hello,
    build_keep_alive,
};
use crate::protocol::beat::{build_beat, build_on_air};
use crate::protocol::command::{self, FaderAction};
use crate::protocol::header::{BEAT_PORT, DISCOVERY_PORT, MAGIC_HEADER, STATUS_PORT};
use crate::protocol::status::{CdjStatusBuilder, CdjStatusFlags, build_cdj_status};

/// Interval between keep-alive packets.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Interval between status broadcast packets.
const STATUS_BROADCAST_INTERVAL: Duration = Duration::from_millis(200);

/// Fader-start command type byte on port 50002.
const FADER_START_TYPE: u8 = 0x02;

/// Load-track command type byte on port 50002.
const LOAD_TRACK_TYPE: u8 = 0x19;

/// Events emitted when an incoming command is received on the status port.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandEvent {
    /// A fader start/stop command was received.
    FaderStart {
        /// Device number of the sender (e.g. the mixer).
        source_device: u8,
        /// Per-channel actions (channels 1–4).
        channels: [FaderAction; 4],
    },
    /// A load-track command was received.
    LoadTrack {
        /// Device number of the sender.
        source_device: u8,
        /// Player the track should be loaded from.
        source_player: u8,
        /// Media slot on the source player.
        source_slot: TrackSourceSlot,
        /// Type of track.
        track_type: TrackType,
        /// rekordbox database ID of the track.
        rekordbox_id: u32,
    },
}

/// Configuration for the virtual CDJ.
#[derive(Debug)]
pub struct VirtualCdjConfig {
    /// Device name to announce (max 20 chars).
    pub name: String,
    /// Desired device number (1-6 typical for CDJs).
    pub device_number: DeviceNumber,
    /// Network interface IP to bind to.
    pub interface_address: Ipv4Addr,
    /// When true, claim a device number in the 1–4 range (like a real CDJ).
    pub use_standard_player_number: bool,
    /// Configurable threshold for comparing tempos.
    pub tempo_epsilon: f64,
}

impl VirtualCdjConfig {
    pub fn new(device_number: u8, interface_address: Ipv4Addr) -> Result<Self> {
        if device_number == 0 {
            return Err(ProDjLinkError::InvalidDeviceNumber(device_number));
        }
        Ok(Self {
            name: "prodjlink-rs".to_string(),
            device_number: DeviceNumber(device_number),
            interface_address,
            use_standard_player_number: false,
            tempo_epsilon: 0.00001,
        })
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_use_standard_player_number(mut self, use_standard: bool) -> Self {
        self.use_standard_player_number = use_standard;
        self
    }

    pub fn with_tempo_epsilon(mut self, epsilon: f64) -> Self {
        self.tempo_epsilon = epsilon;
        self
    }
}

/// A virtual CDJ that appears on the DJ Link network.
pub struct VirtualCdj {
    config: Arc<VirtualCdjConfig>,
    /// Socket for sending keep-alive announcements (port 50000).
    #[allow(dead_code)]
    discovery_socket: Arc<UdpSocket>,
    /// Socket for sending commands (port 50002).
    status_socket: Arc<UdpSocket>,
    /// Socket for sending beat packets (port 50001).
    beat_socket: Arc<UdpSocket>,
    /// The MAC address we're using.
    #[allow(dead_code)]
    mac_address: [u8; 6],
    /// Keep-alive background task.
    keepalive_task: Option<JoinHandle<()>>,
    /// Defense background task — defends our device number against claims.
    defense_task: Option<JoinHandle<()>>,
    /// Command listener background task (port 50002).
    command_task: Option<JoinHandle<()>>,
    /// Broadcast sender for incoming command events.
    command_tx: broadcast::Sender<CommandEvent>,
    /// Tempo master tracker.
    tempo_master: TempoMaster,
    /// Whether this virtual player is "playing".
    playing: Arc<AtomicBool>,
    /// Whether this virtual player is in sync mode.
    synced: Arc<AtomicBool>,
    /// Current playback position in milliseconds.
    playback_position: Arc<AtomicU64>,
    /// Whether we are broadcasting status packets.
    sending_status: Arc<AtomicBool>,
    /// Handle for the status-broadcast background task.
    status_task: Mutex<Option<JoinHandle<()>>>,
    /// Monotonic packet counter for status packets.
    packet_counter: Arc<AtomicU64>,
    /// Timestamp (Instant-based) of the last beat we processed.
    /// Used by the status broadcast loop to avoid sending status packets
    /// too close to beat arrivals — beat timing takes priority.
    last_beat_at: Arc<AtomicU64>,
    opus_quad_mode: bool,
    broadcast_address: Ipv4Addr,
}

impl VirtualCdj {
    /// Start the virtual CDJ with the given configuration.
    ///
    /// Optionally checks the DeviceFinder for number conflicts before claiming.
    pub async fn start(config: VirtualCdjConfig, finder: Option<&DeviceFinder>) -> Result<Self> {
        // Check for device number conflicts
        if let Some(finder) = finder {
            if let Some(existing) = finder.device(config.device_number).await {
                return Err(ProDjLinkError::Parse(format!(
                    "device number {} already in use by {}",
                    config.device_number, existing.name
                )));
            }
        }

        // Look up the real MAC from the network interface; fall back to a
        // locally-administered placeholder if the interface isn't found.
        let mac_address = resolve_mac(config.interface_address, config.device_number.0);

        let discovery_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        discovery_socket.set_broadcast(true)?;
        let discovery_socket = Arc::new(discovery_socket);

        let status_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        status_socket.set_broadcast(true)?;
        let status_socket = Arc::new(status_socket);

        let beat_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        beat_socket.set_broadcast(true)?;
        let beat_socket = Arc::new(beat_socket);

        let (command_tx, _) = broadcast::channel(256);
        let command_task = spawn_command_listener(command_tx.clone());

        let tempo_master = TempoMaster::new(config.device_number);
        let config = Arc::new(config);

        let ka_config = config.clone();
        let ka_socket = discovery_socket.clone();
        let ka_mac = mac_address;
        let keepalive_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(KEEP_ALIVE_INTERVAL);
            let broadcast_addr: SocketAddr =
                SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
            loop {
                interval.tick().await;
                let packet = build_keep_alive(
                    &ka_config.name,
                    ka_config.device_number,
                    ka_mac,
                    ka_config.interface_address,
                );
                let _ = ka_socket.send_to(&packet, broadcast_addr).await;
            }
        });

        Ok(Self {
            config,
            discovery_socket,
            status_socket,
            beat_socket,
            mac_address,
            keepalive_task: Some(keepalive_task),
            defense_task: None,
            command_task,
            command_tx,
            tempo_master,
            playing: Arc::new(AtomicBool::new(false)),
            synced: Arc::new(AtomicBool::new(false)),
            playback_position: Arc::new(AtomicU64::new(0)),
            sending_status: Arc::new(AtomicBool::new(false)),
            status_task: Mutex::new(None),
            packet_counter: Arc::new(AtomicU64::new(0)),
            last_beat_at: Arc::new(AtomicU64::new(0)),
            opus_quad_mode: false,
            broadcast_address: Ipv4Addr::BROADCAST,
        })
    }

    /// Get the device number we're using.
    pub fn device_number(&self) -> DeviceNumber {
        self.config.device_number
    }

    /// Get the device name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn use_standard_player_number(&self) -> bool {
        self.config.use_standard_player_number
    }

    pub fn tempo_epsilon(&self) -> f64 {
        self.config.tempo_epsilon
    }

    pub fn in_opus_quad_compatibility_mode(&self) -> bool {
        self.opus_quad_mode
    }

    pub fn local_address(&self) -> Ipv4Addr {
        self.config.interface_address
    }

    pub fn broadcast_address(&self) -> Ipv4Addr {
        self.broadcast_address
    }

    /// Subscribe to incoming command events (fader start, load track).
    pub fn subscribe_commands(&self) -> broadcast::Receiver<CommandEvent> {
        self.command_tx.subscribe()
    }

    /// Send a fader start/stop command to a target device.
    pub async fn fader_start(&self, target: DeviceNumber, start: bool) -> Result<()> {
        let packet = command::build_fader_start_single(self.config.device_number, target, start);
        self.send_command(&packet).await
    }

    /// Tell a target device to load a specific track.
    pub async fn load_track(
        &self,
        target: DeviceNumber,
        source_player: DeviceNumber,
        source_slot: TrackSourceSlot,
        track_type: TrackType,
        rekordbox_id: u32,
    ) -> Result<()> {
        let packet = command::build_load_track(
            self.config.device_number,
            target,
            source_player,
            source_slot,
            track_type,
            rekordbox_id,
        );
        self.send_command(&packet).await
    }

    /// Enable or disable sync mode on a target device.
    pub async fn set_sync(&self, target: DeviceNumber, enable: bool) -> Result<()> {
        let packet = command::build_sync_command(self.config.device_number, target, enable);
        self.send_beat_command(&packet).await
    }

    /// Request to become the tempo master.
    pub async fn become_master(&self) -> Result<()> {
        let packet = command::build_master_command(self.config.device_number);
        self.send_beat_command(&packet).await
    }

    // --- Tempo Master Integration ---

    /// Get a reference to the tempo master tracker.
    pub fn tempo_master(&self) -> &TempoMaster {
        &self.tempo_master
    }

    /// Request the master role by sending a master_command on the beat port.
    ///
    /// This sends the command packet and optimistically marks us as master.
    /// In a real network, the current master would first yield to us via the
    /// master_handoff byte, and we'd confirm by sending this command.
    pub async fn request_master_role(&self, bpm: Bpm) -> Result<()> {
        let packet = command::build_master_command(self.config.device_number);
        self.send_beat_command(&packet).await?;
        self.tempo_master.set_we_are_master(bpm);
        Ok(())
    }

    /// Yield the master role to another device.
    ///
    /// Marks us as no longer being master. The actual handoff on the wire
    /// is signaled by setting `master_hand_off` in our status packets
    /// (the caller should set the handoff byte in subsequent status broadcasts).
    pub fn yield_master_role(&self) {
        self.tempo_master.resign_master();
    }

    /// Set our reported BPM.
    ///
    /// When we are the tempo master, this immediately updates the master tempo
    /// that gets broadcast in status packets. When we are not the master, the
    /// value is still stored so that status packets reflect our intended tempo
    /// (e.g. for when we later become master).
    pub fn set_tempo(&self, bpm: Bpm) {
        self.tempo_master.set_master_tempo(bpm);
    }

    /// Set our virtual sync mode state (affects status packets we broadcast).
    pub fn set_synced(&self, synced: bool) {
        self.synced.store(synced, Ordering::Relaxed);
    }

    /// Whether our virtual player is currently in sync mode.
    pub fn is_synced(&self) -> bool {
        self.synced.load(Ordering::Relaxed)
    }

    /// Process an incoming CdjStatus to update master tracking state.
    ///
    /// Call this when a status packet arrives from the network. It updates
    /// which device is master, the current BPM, and detects master handoff
    /// requests targeting us.
    pub fn process_cdj_status(&self, status: &crate::protocol::status::CdjStatus) {
        if status.is_master {
            self.tempo_master
                .on_device_is_master(status.device_number, status.bpm);
        }

        // Check if the current master is yielding to us
        if let Some(target) = status.master_hand_off {
            if target == self.config.device_number.0 {
                self.tempo_master
                    .on_master_yielded_to_us(status.device_number);
            }
        }
    }

    /// Process an incoming MixerStatus to update master tracking state.
    pub fn process_mixer_status(&self, status: &crate::protocol::status::MixerStatus) {
        if status.is_master {
            self.tempo_master
                .on_device_is_master(status.device_number, status.bpm);
        }

        if let Some(target) = status.master_hand_off {
            if target == self.config.device_number.0 {
                self.tempo_master
                    .on_master_yielded_to_us(status.device_number);
            }
        }
    }

    /// Process an incoming Beat packet to update master tempo.
    ///
    /// Uses the effective (pitch-adjusted) tempo so the tracked master BPM
    /// reflects what the audience actually hears.
    pub fn process_beat(&self, beat: &crate::protocol::beat::Beat) {
        // Record beat arrival time so the status broadcast loop can avoid
        // sending packets too close to beats (beat timing takes priority).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_beat_at.store(now, Ordering::Release);
        self.tempo_master
            .on_beat(beat.device_number, Bpm(beat.effective_tempo()));
    }

    /// Process any [`DeviceUpdate`] to update master tracking state.
    pub fn process_device_update(&self, update: &crate::protocol::status::DeviceUpdate) {
        match update {
            crate::protocol::status::DeviceUpdate::Cdj(s) => self.process_cdj_status(s),
            crate::protocol::status::DeviceUpdate::Mixer(s) => self.process_mixer_status(s),
        }
    }

    // -------------------------------------------------------------------
    // On-air / Beat / Status broadcasting
    // -------------------------------------------------------------------

    /// Broadcast an on-air packet indicating which channels are currently
    /// on-air. `channels[0]` is channel 1, etc.
    pub async fn send_on_air_command(&self, channels: &[bool; 4]) -> Result<()> {
        let packet = build_on_air(&self.config.name, self.config.device_number, channels);
        self.send_beat_command(&packet).await
    }

    /// Broadcast a beat packet with the given BPM and beat-within-bar (1–4).
    pub async fn send_beat(&self, bpm: Bpm, beat_within_bar: u8) -> Result<()> {
        // 0x100000 is the "normal speed" pitch (no adjustment).
        let pitch: u32 = 0x100000;
        let packet = build_beat(
            &self.config.name,
            self.config.device_number,
            bpm,
            pitch,
            beat_within_bar,
        );
        self.send_beat_command(&packet).await
    }

    /// Start or stop the background status-broadcast loop.
    ///
    /// When `sending` is `true` a background task sends a CDJ-style status
    /// packet to the status port every 200 ms, reading the current BPM and
    /// master state from the [`TempoMaster`] watch channel. Calling with
    /// `false` signals the task to exit and awaits its join handle.
    pub async fn set_sending_status(&self, sending: bool) {
        let was_sending = self.sending_status.swap(sending, Ordering::SeqCst);

        if sending && !was_sending {
            let playing = Arc::clone(&self.playing);
            let synced = Arc::clone(&self.synced);
            let sending_flag = Arc::clone(&self.sending_status);
            let counter = Arc::clone(&self.packet_counter);
            let last_beat = Arc::clone(&self.last_beat_at);
            let config = self.config.clone();
            let socket = self.status_socket.clone();
            let master_watch = self.tempo_master.watch();

            let handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(STATUS_BROADCAST_INTERVAL);
                let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
                /// Skip status packets within this window after a beat arrival
                /// so beat timing takes priority on the network.
                const BEAT_GUARD_MS: u64 = 50;

                loop {
                    interval.tick().await;

                    if !sending_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    // Beat timing guard: skip if a beat arrived very recently
                    let beat_ts = last_beat.load(Ordering::Acquire);
                    if beat_ts > 0 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        if now.saturating_sub(beat_ts) < BEAT_GUARD_MS {
                            continue;
                        }
                    }

                    let seq = counter.fetch_add(1, Ordering::Relaxed);
                    let master_state = master_watch.borrow().clone();

                    let flags = CdjStatusFlags {
                        playing: playing.load(Ordering::Relaxed),
                        master: master_state.we_are_master,
                        synced: synced.load(Ordering::Relaxed),
                        on_air: true,
                        bpm_sync: false,
                    };
                    let builder = CdjStatusBuilder {
                        device_name: config.name.clone(),
                        device_number: config.device_number,
                        flags,
                        bpm: master_state.master_tempo,
                        packet_number: seq as u32,
                        ..CdjStatusBuilder::default()
                    };
                    let packet = build_cdj_status(&builder);
                    let _ = socket.send_to(&packet, broadcast_addr).await;
                }
            });

            let mut task = self.status_task.lock().await;
            *task = Some(handle);
        } else if !sending && was_sending {
            let mut task = self.status_task.lock().await;
            if let Some(h) = task.take() {
                h.abort();
            }
        }
    }

    /// Whether the background status-broadcast loop is active.
    pub fn is_sending_status(&self) -> bool {
        self.sending_status.load(Ordering::Relaxed)
    }

    // -------------------------------------------------------------------
    // Playback state helpers
    // -------------------------------------------------------------------

    /// Set our virtual playing state (affects status packets we broadcast).
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }

    /// Whether our virtual player is currently "playing".
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Current playback position in milliseconds.
    pub fn playback_position(&self) -> u64 {
        self.playback_position.load(Ordering::Relaxed)
    }

    /// Update the current playback position (ms).
    pub fn adjust_playback_position(&self, position: u64) {
        self.playback_position.store(position, Ordering::Relaxed);
    }

    // -------------------------------------------------------------------
    // Settings
    // -------------------------------------------------------------------

    /// Send a load-settings command to a specific target device.
    pub async fn send_load_settings_command(
        &self,
        target: DeviceNumber,
        settings: &PlayerSettings,
    ) -> Result<()> {
        let packet = settings.build_settings_packet(self.config.device_number.0, target.0);
        self.send_command(&packet).await
    }

    /// Send a command packet via broadcast on the status port (50002).
    ///
    /// Used for fader start (0x02) and load track (0x19) commands.
    async fn send_command(&self, packet: &[u8]) -> Result<()> {
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), STATUS_PORT);
        self.status_socket.send_to(packet, broadcast_addr).await?;
        Ok(())
    }

    /// Send a command packet via broadcast on the beat port (50001).
    ///
    /// Used for sync control (0x2a) and master handoff (0x26) commands.
    async fn send_beat_command(&self, packet: &[u8]) -> Result<()> {
        let broadcast_addr = SocketAddr::new(Ipv4Addr::BROADCAST.into(), BEAT_PORT);
        self.beat_socket.send_to(packet, broadcast_addr).await?;
        Ok(())
    }

    /// Stop the virtual CDJ and its keep-alive loop.
    pub fn stop(mut self) {
        // Signal the status broadcast loop to exit.
        self.sending_status.store(false, Ordering::SeqCst);

        if let Some(task) = self.keepalive_task.take() {
            task.abort();
        }
        if let Some(task) = self.defense_task.take() {
            task.abort();
        }
        if let Some(task) = self.command_task.take() {
            task.abort();
        }
        // Abort status task if running.
        if let Ok(mut guard) = self.status_task.try_lock() {
            if let Some(h) = guard.take() {
                h.abort();
            }
        }
    }

    /// Start the virtual CDJ with the full 3-stage device number claim protocol.
    ///
    /// Runs the claim handshake before starting the keep-alive loop. After
    /// claiming, a background defense task monitors for conflicting claims.
    pub async fn start_claimed(config: VirtualCdjConfig, finder: &DeviceFinder) -> Result<Self> {
        let mac_address = resolve_mac(config.interface_address, config.device_number.0);

        let discovery_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        discovery_socket.set_broadcast(true)?;
        let discovery_socket = Arc::new(discovery_socket);

        // Run the claim protocol before starting keep-alive
        run_claim_protocol(
            &discovery_socket,
            finder,
            Ipv4Addr::BROADCAST,
            config.device_number.0,
            &config.name,
            mac_address,
            config.interface_address,
            true, // auto_assign flag
        )
        .await?;

        let status_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        status_socket.set_broadcast(true)?;
        let status_socket = Arc::new(status_socket);

        let beat_socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
        beat_socket.set_broadcast(true)?;
        let beat_socket = Arc::new(beat_socket);

        let (command_tx, _) = broadcast::channel(256);
        let command_task = spawn_command_listener(command_tx.clone());

        let tempo_master = TempoMaster::new(config.device_number);
        let config = Arc::new(config);

        // Start keep-alive loop
        let ka_config = config.clone();
        let ka_socket = discovery_socket.clone();
        let ka_mac = mac_address;
        let keepalive_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(KEEP_ALIVE_INTERVAL);
            let broadcast_addr: SocketAddr =
                SocketAddr::new(Ipv4Addr::BROADCAST.into(), DISCOVERY_PORT);
            loop {
                interval.tick().await;
                let packet = build_keep_alive(
                    &ka_config.name,
                    ka_config.device_number,
                    ka_mac,
                    ka_config.interface_address,
                );
                let _ = ka_socket.send_to(&packet, broadcast_addr).await;
            }
        });

        // Start defense task
        let def_socket = discovery_socket.clone();
        let def_events = finder.subscribe();
        let def_number = config.device_number.0;
        let def_name = config.name.clone();
        let def_ip = config.interface_address;
        let defense_task = tokio::spawn(async move {
            defense_loop(def_socket, def_events, def_number, def_name, def_ip).await;
        });

        Ok(Self {
            config,
            discovery_socket,
            status_socket,
            beat_socket,
            mac_address,
            keepalive_task: Some(keepalive_task),
            defense_task: Some(defense_task),
            command_task,
            command_tx,
            tempo_master,
            playing: Arc::new(AtomicBool::new(false)),
            synced: Arc::new(AtomicBool::new(false)),
            playback_position: Arc::new(AtomicU64::new(0)),
            sending_status: Arc::new(AtomicBool::new(false)),
            status_task: Mutex::new(None),
            packet_counter: Arc::new(AtomicU64::new(0)),
            last_beat_at: Arc::new(AtomicU64::new(0)),
            opus_quad_mode: false,
            broadcast_address: Ipv4Addr::BROADCAST,
        })
    }

    /// Start with automatic device number assignment.
    ///
    /// Watches the network for 4 seconds via the [`DeviceFinder`], picks the
    /// first unused number in the preferred range, and claims it using the
    /// 3-stage protocol.
    ///
    /// When `use_player_numbers` is `true`, prefers numbers 1–4 (standard CDJ
    /// range) before falling back to 7–15. Otherwise only tries 7–15, avoiding
    /// channels 5–6 which cause CDJ-3000 issues.
    pub async fn start_auto(
        name: impl Into<String>,
        interface_address: Ipv4Addr,
        finder: &DeviceFinder,
        use_player_numbers: bool,
    ) -> Result<Self> {
        let name = name.into();

        // Watch the network for 4 seconds
        tokio::time::sleep(Duration::from_secs(4)).await;

        let devices = finder.devices().await;
        let used: HashSet<u8> = devices.iter().map(|d| d.number.0).collect();
        let candidates = candidate_device_numbers(&used, use_player_numbers);

        if candidates.is_empty() {
            return Err(ProDjLinkError::NoAvailableDeviceNumber);
        }

        for &device_number in &candidates {
            let config =
                VirtualCdjConfig::new(device_number, interface_address)?.with_name(name.clone());
            match Self::start_claimed(config, finder).await {
                Ok(vcdj) => return Ok(vcdj),
                Err(ProDjLinkError::DeviceNumberInUse(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(ProDjLinkError::NoAvailableDeviceNumber)
    }
}

// ---------------------------------------------------------------------------
// Claim Protocol Helpers
// ---------------------------------------------------------------------------

/// Execute the 3-stage device number claim protocol on the DJ Link network.
///
/// Sends the hello → stage 1 → stage 2 → stage 3 packet series on the
/// broadcast address, checking for defense packets between each send.
/// Returns `Err(DeviceNumberInUse)` if another device defends the number.
async fn run_claim_protocol(
    socket: &UdpSocket,
    finder: &DeviceFinder,
    broadcast_addr: Ipv4Addr,
    device_number: u8,
    name: &str,
    mac: [u8; 6],
    ip: Ipv4Addr,
    auto_assign: bool,
) -> Result<()> {
    let mut events = finder.subscribe();
    let dest = SocketAddr::new(broadcast_addr.into(), DISCOVERY_PORT);

    // Phase 1: Broadcast DeviceHello 3 times, 300 ms apart
    let hello = build_device_hello(name);
    for _ in 0..3 {
        socket.send_to(&hello, dest).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // Phase 2: Stage 1 claim — 3 packets, 300 ms apart, watch for defense
    for i in 1..=3u8 {
        let pkt = build_claim_stage1(name, mac, i);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    // Phase 3: Stage 2 claim — 3 packets, 300 ms apart, watch for defense
    for i in 1..=3u8 {
        let pkt = build_claim_stage2(name, mac, ip, device_number, i, auto_assign);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    // Phase 4: Stage 3 (final) claim — 3 packets, 300 ms apart
    for i in 1..=3u8 {
        let pkt = build_claim_stage3(name, device_number, i);
        socket.send_to(&pkt, dest).await?;
        if wait_for_defense(&mut events, Duration::from_millis(300), device_number).await {
            return Err(ProDjLinkError::DeviceNumberInUse(device_number));
        }
    }

    Ok(())
}

/// Wait up to `duration` for a defense event matching `device_number`.
///
/// Returns `true` if a defense was received, `false` on timeout.
async fn wait_for_defense(
    events: &mut broadcast::Receiver<FinderEvent>,
    duration: Duration,
    device_number: u8,
) -> bool {
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            result = events.recv() => {
                match result {
                    Ok(FinderEvent::DefenseReceived { device_number: dn })
                        if dn == device_number =>
                    {
                        return true;
                    }
                    Err(broadcast::error::RecvError::Closed) => return false,
                    _ => continue, // other event or Lagged — keep waiting
                }
            }
            () = &mut sleep => {
                return false;
            }
        }
    }
}

/// Background loop that defends our device number against incoming claims.
///
/// Listens for [`FinderEvent::ClaimReceived`] events and responds with a
/// defense packet sent directly to the claimer's IP address.
async fn defense_loop(
    socket: Arc<UdpSocket>,
    mut events: broadcast::Receiver<FinderEvent>,
    device_number: u8,
    name: String,
    ip: Ipv4Addr,
) {
    loop {
        match events.recv().await {
            Ok(FinderEvent::ClaimReceived {
                device_number: dn,
                source_ip,
            }) => {
                if dn == device_number {
                    let pkt = build_defense(&name, device_number, ip);
                    let target = SocketAddr::new(source_ip.into(), DISCOVERY_PORT);
                    let _ = socket.send_to(&pkt, target).await;
                    tracing::info!(
                        device_number,
                        %source_ip,
                        "Defended device number against incoming claim"
                    );
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            _ => {} // ignore other events
        }
    }
}

/// Return candidate device numbers in priority order, excluding numbers
/// already in use on the network.
///
/// When `use_player_numbers` is `true`, tries 1–4 first, then 7–15.
/// Otherwise tries 7–15 only. Channels 5–6 are always skipped to avoid
/// CDJ-3000 issues.
fn candidate_device_numbers(used: &HashSet<u8>, use_player_numbers: bool) -> Vec<u8> {
    let mut candidates = Vec::new();
    if use_player_numbers {
        for n in 1..=4 {
            if !used.contains(&n) {
                candidates.push(n);
            }
        }
    }
    for n in 7..=15 {
        if !used.contains(&n) {
            candidates.push(n);
        }
    }
    candidates
}

// ---------------------------------------------------------------------------
// MAC Address Resolution
// ---------------------------------------------------------------------------

/// Resolve the MAC address for a given interface IP.
///
/// Falls back to a locally-administered placeholder if the interface is not
/// found (e.g. when binding to `UNSPECIFIED` or in test environments).
fn resolve_mac(interface_address: Ipv4Addr, device_number: u8) -> [u8; 6] {
    if interface_address.is_unspecified() || interface_address.is_loopback() {
        return [0x02, 0x00, 0x00, 0x00, 0x00, device_number];
    }
    find_interface_by_ip(interface_address)
        .map(|iface| iface.mac)
        .unwrap_or([0x02, 0x00, 0x00, 0x00, 0x00, device_number])
}

// ---------------------------------------------------------------------------
// Command Reception Helpers
// ---------------------------------------------------------------------------

/// Create a UDP socket with `SO_REUSEADDR` + `SO_REUSEPORT` bound to the
/// given port, suitable for sharing port 50002 with the StatusListener.
fn create_reuse_port_socket(port: u16) -> std::io::Result<tokio::net::UdpSocket> {
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
    tokio::net::UdpSocket::from_std(std_socket)
}

/// Spawn a background task that listens for incoming command packets on the
/// status port (50002) and emits [`CommandEvent`]s.
///
/// Returns `Some(JoinHandle)` if the socket could be bound, or `None` if
/// port 50002 is unavailable (the VirtualCdj still works, just without
/// command reception).
fn spawn_command_listener(tx: broadcast::Sender<CommandEvent>) -> Option<JoinHandle<()>> {
    let socket = match create_reuse_port_socket(STATUS_PORT) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::warn!("Could not bind command listener on port {STATUS_PORT}: {e}");
            return None;
        }
    };

    Some(tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    let data = &buf[..len];
                    if let Some(event) = try_parse_command(data) {
                        let _ = tx.send(event);
                    }
                }
                Err(_) => break,
            }
        }
    }))
}

/// Try to parse a raw packet as an incoming command.
///
/// Returns `None` if the packet is not a recognized command or is malformed.
fn try_parse_command(data: &[u8]) -> Option<CommandEvent> {
    // Minimum: 10-byte magic header + type byte
    if data.len() < 11 {
        return None;
    }
    if data[..10] != MAGIC_HEADER {
        return None;
    }
    match data[0x0a] {
        FADER_START_TYPE => parse_incoming_fader_start(data),
        LOAD_TRACK_TYPE => parse_incoming_load_track(data),
        _ => None,
    }
}

/// Parse an incoming fader-start command packet (type `0x02` on port 50002).
fn parse_incoming_fader_start(data: &[u8]) -> Option<CommandEvent> {
    if data.len() < 0x28 {
        return None;
    }
    let source_device = data[0x21];
    let channels = [
        byte_to_fader_action(data[0x24]),
        byte_to_fader_action(data[0x25]),
        byte_to_fader_action(data[0x26]),
        byte_to_fader_action(data[0x27]),
    ];
    Some(CommandEvent::FaderStart {
        source_device,
        channels,
    })
}

/// Parse an incoming load-track command packet (type `0x19` on port 50002).
fn parse_incoming_load_track(data: &[u8]) -> Option<CommandEvent> {
    if data.len() < 0x30 {
        return None;
    }
    let source_device = data[0x21];
    let source_player = data[0x28];
    let source_slot = TrackSourceSlot::from(data[0x29]);
    let track_type = TrackType::from(data[0x2a]);
    let rekordbox_id = crate::util::bytes_to_number(data, 0x2c, 4);
    Some(CommandEvent::LoadTrack {
        source_device,
        source_player,
        source_slot,
        track_type,
        rekordbox_id,
    })
}

fn byte_to_fader_action(b: u8) -> FaderAction {
    match b {
        0x00 => FaderAction::Start,
        0x01 => FaderAction::Stop,
        _ => FaderAction::NoChange,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_valid_device_number() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        assert_eq!(cfg.device_number, DeviceNumber(5));
        assert_eq!(cfg.name, "prodjlink-rs");
        assert_eq!(cfg.interface_address, Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn config_rejects_zero_device_number() {
        let err = VirtualCdjConfig::new(0, Ipv4Addr::LOCALHOST).unwrap_err();
        assert!(matches!(err, ProDjLinkError::InvalidDeviceNumber(0)));
    }

    #[test]
    fn config_with_name_builder() {
        let cfg = VirtualCdjConfig::new(1, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_name("MyPlayer");
        assert_eq!(cfg.name, "MyPlayer");
        assert_eq!(cfg.device_number, DeviceNumber(1));
    }

    #[tokio::test]
    async fn start_and_accessors() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        assert_eq!(vcdj.device_number(), DeviceNumber(4));
        assert_eq!(vcdj.name(), "prodjlink-rs");

        vcdj.stop();
    }

    #[tokio::test]
    async fn start_with_custom_name() {
        let cfg = VirtualCdjConfig::new(2, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_name("TestCDJ");
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        assert_eq!(vcdj.name(), "TestCDJ");
        assert_eq!(vcdj.device_number(), DeviceNumber(2));

        vcdj.stop();
    }

    // === Claim Protocol / Candidate Number Tests ===

    #[test]
    fn candidate_numbers_non_standard_default() {
        let used = HashSet::new();
        let candidates = candidate_device_numbers(&used, false);
        // Should start at 7, not include 1-6
        assert_eq!(candidates[0], 7);
        assert!(!candidates.contains(&5));
        assert!(!candidates.contains(&6));
        assert!(!candidates.iter().any(|&n| n <= 4));
        assert_eq!(candidates.len(), 9); // 7..=15
    }

    #[test]
    fn candidate_numbers_standard_prefers_1_to_4() {
        let used = HashSet::new();
        let candidates = candidate_device_numbers(&used, true);
        assert_eq!(&candidates[..4], &[1, 2, 3, 4]);
        // 7-15 follow as fallback
        assert_eq!(candidates[4], 7);
    }

    #[test]
    fn candidate_numbers_skips_used() {
        let used: HashSet<u8> = [1, 3, 7, 9].into_iter().collect();
        let candidates = candidate_device_numbers(&used, true);
        assert_eq!(candidates[0], 2);
        assert_eq!(candidates[1], 4);
        assert_eq!(candidates[2], 8);
        assert!(!candidates.contains(&1));
        assert!(!candidates.contains(&3));
        assert!(!candidates.contains(&7));
        assert!(!candidates.contains(&9));
    }

    #[test]
    fn candidate_numbers_avoids_5_and_6() {
        let used = HashSet::new();
        for use_player in [true, false] {
            let candidates = candidate_device_numbers(&used, use_player);
            assert!(!candidates.contains(&5));
            assert!(!candidates.contains(&6));
        }
    }

    #[test]
    fn candidate_numbers_all_taken() {
        let used: HashSet<u8> = (1..=15).collect();
        assert!(candidate_device_numbers(&used, false).is_empty());
        assert!(candidate_device_numbers(&used, true).is_empty());
    }

    #[test]
    fn candidate_numbers_non_standard_with_some_taken() {
        let used: HashSet<u8> = [7, 8, 10].into_iter().collect();
        let candidates = candidate_device_numbers(&used, false);
        assert_eq!(candidates[0], 9);
        assert_eq!(candidates[1], 11);
    }

    #[tokio::test]
    async fn wait_for_defense_returns_false_on_timeout() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        // Don't send any defense events
        drop(tx);
        let result = wait_for_defense(&mut rx, Duration::from_millis(50), 7).await;
        // Channel closed before timeout, should return false
        assert!(!result);
    }

    #[tokio::test]
    async fn wait_for_defense_detects_matching_defense() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        let _ = tx.send(FinderEvent::DefenseReceived { device_number: 7 });
        let result = wait_for_defense(&mut rx, Duration::from_millis(500), 7).await;
        assert!(result);
    }

    #[tokio::test]
    async fn wait_for_defense_ignores_non_matching_defense() {
        let (tx, mut rx) = broadcast::channel::<FinderEvent>(16);
        // Send defense for a different number
        let _ = tx.send(FinderEvent::DefenseReceived { device_number: 8 });
        drop(tx); // Close channel so the test terminates
        let result = wait_for_defense(&mut rx, Duration::from_millis(50), 7).await;
        assert!(!result);
    }

    // === Tempo Master Integration Tests ===

    #[tokio::test]
    async fn vcdj_has_tempo_master() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        assert!(vcdj.tempo_master().master_device().is_none());
        assert!(!vcdj.tempo_master().we_are_master());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_process_cdj_status_master() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        let status = crate::protocol::status::CdjStatus {
            name: "CDJ-2000".to_string(),
            device_number: DeviceNumber(3),
            device_type: crate::device::types::DeviceType::Cdj,
            track_source_player: DeviceNumber(3),
            track_source_slot: crate::device::types::TrackSourceSlot::UsbSlot,
            track_type: crate::device::types::TrackType::Rekordbox,
            rekordbox_id: 1,
            play_state: crate::device::types::PlayState::Playing,
            play_state_2: crate::device::types::PlayState2::Moving,
            play_state_3: crate::device::types::PlayState3::ForwardCdj,
            is_playing_flag: true,
            is_master: true,
            is_synced: true,
            is_bpm_synced: false,
            is_on_air: true,
            bpm: Bpm(128.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_number: Some(crate::device::types::BeatNumber(1)),
            beat_within_bar: 1,
            firmware_version: "1A01".to_string(),
            sync_number: 0,
            master_hand_off: None,
            loop_start: None,
            loop_end: None,
            loop_beats: None,
            packet_length: 0xd4,
            is_busy: false,
            track_number: 1,
            cue_countdown: None,
            packet_number: 0,
            local_usb_state: 4,
            local_sd_state: 0,
            link_media_available: false,
            local_disc_state: 0,
            disc_track_count: 0,
            timestamp: std::time::Instant::now(),
        };

        vcdj.process_cdj_status(&status);
        assert_eq!(vcdj.tempo_master().master_device(), Some(DeviceNumber(3)));
        assert!((vcdj.tempo_master().master_tempo().0 - 128.0).abs() < f64::EPSILON);
        assert!(!vcdj.tempo_master().we_are_master());

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_process_cdj_status_handoff_to_us() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        let mut rx = vcdj.tempo_master().subscribe();

        let status = crate::protocol::status::CdjStatus {
            name: "CDJ-2000".to_string(),
            device_number: DeviceNumber(3),
            device_type: crate::device::types::DeviceType::Cdj,
            track_source_player: DeviceNumber(3),
            track_source_slot: crate::device::types::TrackSourceSlot::UsbSlot,
            track_type: crate::device::types::TrackType::Rekordbox,
            rekordbox_id: 1,
            play_state: crate::device::types::PlayState::Playing,
            play_state_2: crate::device::types::PlayState2::Moving,
            play_state_3: crate::device::types::PlayState3::ForwardCdj,
            is_playing_flag: true,
            is_master: true,
            is_synced: true,
            is_bpm_synced: false,
            is_on_air: true,
            bpm: Bpm(128.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_number: Some(crate::device::types::BeatNumber(1)),
            beat_within_bar: 1,
            firmware_version: "1A01".to_string(),
            sync_number: 0,
            master_hand_off: Some(5), // yielding to us (device 5)
            loop_start: None,
            loop_end: None,
            loop_beats: None,
            packet_length: 0xd4,
            is_busy: false,
            track_number: 1,
            cue_countdown: None,
            packet_number: 0,
            local_usb_state: 4,
            local_sd_state: 0,
            link_media_available: false,
            local_disc_state: 0,
            disc_track_count: 0,
            timestamp: std::time::Instant::now(),
        };

        vcdj.process_cdj_status(&status);

        // Should have received MasterChanged + MasterYieldedToUs events
        let mut got_yield = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, crate::network::tempo::TempoMasterEvent::MasterYieldedToUs { from_device } if from_device == DeviceNumber(3))
            {
                got_yield = true;
            }
        }
        assert!(got_yield);

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_request_master_role() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        vcdj.request_master_role(Bpm(130.0)).await.unwrap();
        assert!(vcdj.tempo_master().we_are_master());
        assert!((vcdj.tempo_master().master_tempo().0 - 130.0).abs() < f64::EPSILON);

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_yield_master_role() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        vcdj.request_master_role(Bpm(128.0)).await.unwrap();
        assert!(vcdj.tempo_master().we_are_master());

        vcdj.yield_master_role();
        assert!(!vcdj.tempo_master().we_are_master());

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_set_tempo_when_master() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        vcdj.request_master_role(Bpm(128.0)).await.unwrap();
        vcdj.set_tempo(Bpm(135.0));
        assert!((vcdj.tempo_master().master_tempo().0 - 135.0).abs() < f64::EPSILON);

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_set_tempo_always_stores_bpm() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        // Even when not master, set_tempo stores the BPM for status packets
        vcdj.set_tempo(Bpm(200.0));
        assert!((vcdj.tempo_master().master_tempo().0 - 200.0).abs() < f64::EPSILON);

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_process_mixer_status_master() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        let status = crate::protocol::status::MixerStatus {
            name: "DJM-900".to_string(),
            device_number: DeviceNumber(33),
            bpm: Bpm(126.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_within_bar: 1,
            is_master: true,
            is_synced: true,
            master_hand_off: None,
            timestamp: std::time::Instant::now(),
        };

        vcdj.process_mixer_status(&status);
        assert_eq!(vcdj.tempo_master().master_device(), Some(DeviceNumber(33)));
        assert!((vcdj.tempo_master().master_tempo().0 - 126.0).abs() < f64::EPSILON);

        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_process_beat_updates_master_tempo() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();

        // First set device 3 as master
        let status = crate::protocol::status::CdjStatus {
            name: "CDJ".to_string(),
            device_number: DeviceNumber(3),
            device_type: crate::device::types::DeviceType::Cdj,
            track_source_player: DeviceNumber(3),
            track_source_slot: crate::device::types::TrackSourceSlot::UsbSlot,
            track_type: crate::device::types::TrackType::Rekordbox,
            rekordbox_id: 1,
            play_state: crate::device::types::PlayState::Playing,
            play_state_2: crate::device::types::PlayState2::Moving,
            play_state_3: crate::device::types::PlayState3::ForwardCdj,
            is_playing_flag: true,
            is_master: true,
            is_synced: true,
            is_bpm_synced: false,
            is_on_air: true,
            bpm: Bpm(128.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_number: Some(crate::device::types::BeatNumber(1)),
            beat_within_bar: 1,
            firmware_version: "".to_string(),
            sync_number: 0,
            master_hand_off: None,
            loop_start: None,
            loop_end: None,
            loop_beats: None,
            packet_length: 0xd4,
            is_busy: false,
            track_number: 0,
            cue_countdown: None,
            packet_number: 0,
            local_usb_state: 0,
            local_sd_state: 0,
            link_media_available: false,
            local_disc_state: 0,
            disc_track_count: 0,
            timestamp: std::time::Instant::now(),
        };
        vcdj.process_cdj_status(&status);

        // Now process a beat from the same master with different BPM
        let beat = crate::protocol::beat::Beat {
            name: "CDJ".to_string(),
            device_number: DeviceNumber(3),
            device_type: crate::device::types::DeviceType::Cdj,
            bpm: Bpm(130.5),
            pitch: crate::device::types::Pitch(0x100000),
            next_beat: None,
            second_beat: None,
            next_bar: None,
            fourth_beat: None,
            second_bar: None,
            eighth_beat: None,
            beat_within_bar: 2,
            timestamp: std::time::Instant::now(),
        };
        vcdj.process_beat(&beat);
        assert!((vcdj.tempo_master().master_tempo().0 - 130.5).abs() < f64::EPSILON);

        vcdj.stop();
    }

    // === Command Reception Tests ===

    #[test]
    fn try_parse_command_rejects_short_packet() {
        assert!(try_parse_command(&[0u8; 5]).is_none());
    }

    #[test]
    fn try_parse_command_rejects_bad_magic() {
        let mut pkt = [0u8; 0x30];
        pkt[0x0a] = FADER_START_TYPE;
        assert!(try_parse_command(&pkt).is_none());
    }

    #[test]
    fn try_parse_command_rejects_unknown_type() {
        let mut pkt = [0u8; 0x30];
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = 0xFF;
        assert!(try_parse_command(&pkt).is_none());
    }

    #[test]
    fn round_trip_fader_start_command() {
        // Build a fader-start packet using the protocol builder, then parse it
        let pkt = command::build_fader_start(
            DeviceNumber(5),
            [
                FaderAction::Start,
                FaderAction::Stop,
                FaderAction::NoChange,
                FaderAction::Start,
            ],
        );
        let event = try_parse_command(&pkt).expect("should parse fader_start");
        match event {
            CommandEvent::FaderStart {
                source_device,
                channels,
            } => {
                assert_eq!(source_device, 5);
                assert_eq!(channels[0], FaderAction::Start);
                assert_eq!(channels[1], FaderAction::Stop);
                assert_eq!(channels[2], FaderAction::NoChange);
                assert_eq!(channels[3], FaderAction::Start);
            }
            _ => panic!("expected FaderStart"),
        }
    }

    #[test]
    fn round_trip_fader_start_single() {
        let pkt = command::build_fader_start_single(DeviceNumber(1), DeviceNumber(3), false);
        let event = try_parse_command(&pkt).unwrap();
        match event {
            CommandEvent::FaderStart {
                source_device,
                channels,
            } => {
                assert_eq!(source_device, 1);
                assert_eq!(channels[0], FaderAction::NoChange);
                assert_eq!(channels[1], FaderAction::NoChange);
                assert_eq!(channels[2], FaderAction::Stop);
                assert_eq!(channels[3], FaderAction::NoChange);
            }
            _ => panic!("expected FaderStart"),
        }
    }

    #[test]
    fn round_trip_load_track_command() {
        let pkt = command::build_load_track(
            DeviceNumber(5),
            DeviceNumber(3),
            DeviceNumber(2),
            TrackSourceSlot::UsbSlot,
            TrackType::Rekordbox,
            12345,
        );
        let event = try_parse_command(&pkt).expect("should parse load_track");
        match event {
            CommandEvent::LoadTrack {
                source_device,
                source_player,
                source_slot,
                track_type,
                rekordbox_id,
            } => {
                assert_eq!(source_device, 5);
                assert_eq!(source_player, 2);
                assert_eq!(source_slot, TrackSourceSlot::UsbSlot);
                assert_eq!(track_type, TrackType::Rekordbox);
                assert_eq!(rekordbox_id, 12345);
            }
            _ => panic!("expected LoadTrack"),
        }
    }

    #[test]
    fn load_track_with_various_slots() {
        for (slot, expected_byte) in [
            (TrackSourceSlot::SdSlot, 2u8),
            (TrackSourceSlot::CdSlot, 1),
            (TrackSourceSlot::Collection, 4),
        ] {
            let pkt = command::build_load_track(
                DeviceNumber(1),
                DeviceNumber(2),
                DeviceNumber(3),
                slot,
                TrackType::Unanalyzed,
                999,
            );
            let event = try_parse_command(&pkt).unwrap();
            if let CommandEvent::LoadTrack { source_slot, .. } = event {
                assert_eq!(u8::from(source_slot), expected_byte);
            } else {
                panic!("expected LoadTrack");
            }
        }
    }

    #[test]
    fn fader_start_too_short_returns_none() {
        // Packet with valid header but too short for fader_start payload
        let mut pkt = vec![0u8; 0x25]; // need at least 0x28
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = FADER_START_TYPE;
        assert!(try_parse_command(&pkt).is_none());
    }

    #[test]
    fn load_track_too_short_returns_none() {
        let mut pkt = vec![0u8; 0x2e]; // need at least 0x30
        pkt[..10].copy_from_slice(&MAGIC_HEADER);
        pkt[0x0a] = LOAD_TRACK_TYPE;
        assert!(try_parse_command(&pkt).is_none());
    }

    #[test]
    fn byte_to_fader_action_values() {
        assert_eq!(byte_to_fader_action(0x00), FaderAction::Start);
        assert_eq!(byte_to_fader_action(0x01), FaderAction::Stop);
        assert_eq!(byte_to_fader_action(0x02), FaderAction::NoChange);
        assert_eq!(byte_to_fader_action(0xFF), FaderAction::NoChange);
    }

    #[test]
    fn command_event_is_debug_clone_eq() {
        let event = CommandEvent::FaderStart {
            source_device: 1,
            channels: [FaderAction::Start; 4],
        };
        let cloned = event.clone();
        assert_eq!(event, cloned);
        let _ = format!("{:?}", event);
    }

    #[test]
    fn resolve_mac_loopback_returns_placeholder() {
        let mac = resolve_mac(Ipv4Addr::LOCALHOST, 5);
        assert_eq!(mac, [0x02, 0x00, 0x00, 0x00, 0x00, 5]);
    }

    #[test]
    fn resolve_mac_unspecified_returns_placeholder() {
        let mac = resolve_mac(Ipv4Addr::UNSPECIFIED, 7);
        assert_eq!(mac, [0x02, 0x00, 0x00, 0x00, 0x00, 7]);
    }

    #[tokio::test]
    async fn subscribe_commands_returns_receiver() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        let _rx = vcdj.subscribe_commands();
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_playing_default_false() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        assert!(!vcdj.is_playing());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_set_and_get_playing() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        vcdj.set_playing(true);
        assert!(vcdj.is_playing());
        vcdj.set_playing(false);
        assert!(!vcdj.is_playing());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_playback_position_default_zero() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        assert_eq!(vcdj.playback_position(), 0);
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_adjust_and_get_playback_position() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        vcdj.adjust_playback_position(42000);
        assert_eq!(vcdj.playback_position(), 42000);
        vcdj.adjust_playback_position(0);
        assert_eq!(vcdj.playback_position(), 0);
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_sending_status_default_false() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        assert!(!vcdj.is_sending_status());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_set_sending_status_toggle() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        vcdj.set_sending_status(true).await;
        assert!(vcdj.is_sending_status());
        vcdj.set_sending_status(false).await;
        assert!(!vcdj.is_sending_status());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_send_on_air_command() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        let result = vcdj.send_on_air_command(&[true, false, true, false]).await;
        assert!(result.is_ok());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_send_beat() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        let result = vcdj.send_beat(Bpm(128.0), 1).await;
        assert!(result.is_ok());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_send_load_settings_command() {
        use crate::device::settings::PlayerSettings;
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        let settings = PlayerSettings::default();
        let result = vcdj
            .send_load_settings_command(DeviceNumber(1), &settings)
            .await;
        assert!(result.is_ok());
        vcdj.stop();
    }

    #[test]
    fn config_defaults() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        assert!(!cfg.use_standard_player_number);
        assert!((cfg.tempo_epsilon - 0.00001).abs() < f64::EPSILON);
    }

    #[test]
    fn config_builder_methods() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_use_standard_player_number(true)
            .with_tempo_epsilon(0.001);
        assert!(cfg.use_standard_player_number);
        assert!((cfg.tempo_epsilon - 0.001).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn vcdj_accessors() {
        let cfg = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST)
            .unwrap()
            .with_use_standard_player_number(true)
            .with_tempo_epsilon(0.005);
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        assert!(vcdj.use_standard_player_number());
        assert!((vcdj.tempo_epsilon() - 0.005).abs() < f64::EPSILON);
        assert!(!vcdj.in_opus_quad_compatibility_mode());
        assert_eq!(vcdj.local_address(), Ipv4Addr::LOCALHOST);
        assert_eq!(vcdj.broadcast_address(), Ipv4Addr::BROADCAST);
        vcdj.stop();
    }

    // === Synced State Tests ===

    #[tokio::test]
    async fn vcdj_synced_default_false() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        assert!(!vcdj.is_synced());
        vcdj.stop();
    }

    #[tokio::test]
    async fn vcdj_set_and_get_synced() {
        let cfg = VirtualCdjConfig::new(4, Ipv4Addr::LOCALHOST).unwrap();
        let vcdj = VirtualCdj::start(cfg, None).await.unwrap();
        vcdj.set_synced(true);
        assert!(vcdj.is_synced());
        vcdj.set_synced(false);
        assert!(!vcdj.is_synced());
        vcdj.stop();
    }

    // === Status Broadcast Content Tests ===

    /// Helper: build the status packet that the broadcast loop would produce,
    /// given the current VirtualCdj state. This tests the same logic the
    /// spawned task uses without requiring actual network I/O.
    fn build_status_from_state(
        config: &VirtualCdjConfig,
        playing: bool,
        synced: bool,
        master_state: &crate::network::tempo::MasterState,
        seq: u32,
    ) -> Vec<u8> {
        let flags = CdjStatusFlags {
            playing,
            master: master_state.we_are_master,
            synced,
            on_air: true,
            bpm_sync: false,
        };
        let builder = CdjStatusBuilder {
            device_name: config.name.clone(),
            device_number: config.device_number,
            flags,
            bpm: master_state.master_tempo,
            packet_number: seq,
            ..CdjStatusBuilder::default()
        };
        build_cdj_status(&builder)
    }

    #[test]
    fn status_broadcast_reflects_master_tempo() {
        use crate::protocol::status::parse_cdj_status;

        let config = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let master_state = crate::network::tempo::MasterState {
            master_device: Some(DeviceNumber(5)),
            master_tempo: Bpm(128.5),
            we_are_master: true,
            updated_at: std::time::Instant::now(),
        };

        let packet = build_status_from_state(&config, true, true, &master_state, 42);
        let status = parse_cdj_status(&packet).expect("should parse status packet");

        assert!((status.bpm.0 - 128.5).abs() < 0.01);
        assert!(status.is_master);
        assert!(status.is_synced);
        assert!(status.is_playing_flag);
    }

    #[test]
    fn status_broadcast_defaults_when_not_master() {
        use crate::protocol::status::parse_cdj_status;

        let config = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        let master_state = crate::network::tempo::MasterState {
            master_device: None,
            master_tempo: Bpm(0.0),
            we_are_master: false,
            updated_at: std::time::Instant::now(),
        };

        let packet = build_status_from_state(&config, false, false, &master_state, 0);
        let status = parse_cdj_status(&packet).expect("should parse status packet");

        assert!(!status.is_master);
        assert!(!status.is_synced);
        assert!(!status.is_playing_flag);
    }

    #[test]
    fn status_broadcast_includes_set_tempo_bpm() {
        use crate::protocol::status::parse_cdj_status;

        let config = VirtualCdjConfig::new(5, Ipv4Addr::LOCALHOST).unwrap();
        // Simulate: we called set_tempo(135.0) before becoming master
        let master_state = crate::network::tempo::MasterState {
            master_device: None,
            master_tempo: Bpm(135.0),
            we_are_master: false,
            updated_at: std::time::Instant::now(),
        };

        let packet = build_status_from_state(&config, false, false, &master_state, 1);
        let status = parse_cdj_status(&packet).expect("should parse status packet");

        // BPM should reflect set_tempo value even when not master
        assert!((status.bpm.0 - 135.0).abs() < 0.01);
    }
}
