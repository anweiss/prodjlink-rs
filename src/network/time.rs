use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use tokio::sync::broadcast;

use crate::device::types::*;
use crate::protocol::beat::{Beat, PrecisePosition};
use crate::protocol::status::CdjStatus;

const DEFAULT_SLACK_MS: u64 = 50;
const BROADCAST_CAPACITY: usize = 64;

/// A position update event broadcast whenever a player's tracked position changes.
#[derive(Debug, Clone)]
pub struct PositionUpdate {
    /// The player that was updated.
    pub player: DeviceNumber,
    /// The position in milliseconds at the time of the update.
    pub position_ms: u64,
    /// Whether the player was playing at the time of the update.
    pub playing: bool,
    /// Whether the position came from a precise position packet (CDJ-3000+).
    pub precise: bool,
}

/// Internal per-player position tracking state.
struct PlayerPosition {
    /// Last known position in milliseconds.
    position_ms: u64,
    /// Whether the player was playing at the last update.
    playing: bool,
    /// When the last position update was received.
    timestamp: Instant,
    /// Effective tempo (BPM × pitch multiplier) at the last update.
    effective_tempo: f64,
    /// Pitch multiplier at the last update (1.0 = normal speed).
    pitch_multiplier: f64,
    /// Whether the last position came from a precise position packet.
    precise: bool,
}

impl PlayerPosition {
    /// Advance position forward based on elapsed time and current tempo.
    ///
    /// After calling, `self.position_ms` reflects the interpolated position
    /// at `now`, and `self.timestamp` is reset to `now`.
    fn snap_forward(&mut self, now: Instant) {
        if self.playing && self.pitch_multiplier > 0.0 {
            if let Some(elapsed) = now.checked_duration_since(self.timestamp) {
                let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
                if elapsed_ms > 0.0 {
                    let new_pos = self.position_ms as f64 + elapsed_ms * self.pitch_multiplier;
                    self.position_ms = new_pos.max(0.0) as u64;
                }
            }
        }
        self.timestamp = now;
    }
}

/// Tracks the playback position of all devices on the network.
///
/// Reconstructs precise playback time by combining data from CDJ status
/// packets, beat packets, and precise position packets (CDJ-3000+).
///
/// Between updates the position is interpolated forward based on the
/// player's effective tempo and pitch multiplier.
///
/// # Usage
///
/// ```rust,no_run
/// use prodjlink_rs::network::time::TimeFinder;
/// use prodjlink_rs::DeviceNumber;
///
/// let tf = TimeFinder::new();
///
/// // Feed it events from the network (status, beats, precise positions)
/// // then query the current position:
/// if let Some(ms) = tf.get_time_for(DeviceNumber(1)) {
///     println!("Player 1 is at {ms} ms");
/// }
/// ```
pub struct TimeFinder {
    /// Per-device position tracking state.
    positions: RwLock<HashMap<u8, PlayerPosition>>,
    /// Configurable slack for position interpolation (default 50 ms).
    slack_ms: AtomicU64,
    /// Broadcast channel for position update events.
    tx: broadcast::Sender<PositionUpdate>,
}

impl TimeFinder {
    /// Create a new `TimeFinder` with the default 50 ms slack.
    pub fn new() -> Self {
        Self::with_slack(DEFAULT_SLACK_MS)
    }

    /// Create a new `TimeFinder` with a custom slack value (in milliseconds).
    pub fn with_slack(slack_ms: u64) -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            positions: RwLock::new(HashMap::new()),
            slack_ms: AtomicU64::new(slack_ms),
            tx,
        }
    }

    /// Get the current interpolated position for a device (in milliseconds).
    ///
    /// If the player is playing, the position is extrapolated forward from
    /// the last known position using `elapsed_time × pitch_multiplier`.
    /// If paused, returns the last known position unchanged.
    /// Returns `None` if no data is available for the device.
    pub fn get_time_for(&self, device: DeviceNumber) -> Option<u64> {
        let positions = self.positions.read().unwrap();
        let pos = positions.get(&device.0)?;

        if pos.playing && pos.pitch_multiplier > 0.0 {
            let elapsed_ms = pos
                .timestamp
                .elapsed()
                .as_secs_f64()
                * 1000.0;
            let interpolated = pos.position_ms as f64 + elapsed_ms * pos.pitch_multiplier;
            Some(interpolated.max(0.0) as u64)
        } else {
            Some(pos.position_ms)
        }
    }

    /// Get the last stored position update for a device **without** interpolation.
    pub fn get_latest_position(&self, device: DeviceNumber) -> Option<PositionUpdate> {
        let positions = self.positions.read().unwrap();
        let pos = positions.get(&device.0)?;
        Some(PositionUpdate {
            player: device,
            position_ms: pos.position_ms,
            playing: pos.playing,
            precise: pos.precise,
        })
    }

    /// Process an incoming CDJ status update.
    ///
    /// Extracts playing state, BPM, pitch, and (optionally) beat number to
    /// update the position estimate for the reporting player.
    ///
    /// If the player already has a precise position (from a CDJ-3000 precise
    /// position packet), only the playing state and tempo are updated — the
    /// position itself is left to the higher-quality precise source.
    pub fn on_cdj_status(&self, status: &CdjStatus) {
        let playing = status.is_playing();
        let pitch_multiplier = status.pitch.to_multiplier();
        let effective_tempo = status.bpm.0 * pitch_multiplier;

        // Approximate position from beat number and BPM.
        let estimated_position_ms = if let Some(beat_num) = status.beat_number {
            if status.bpm.0 > 0.0 {
                let ms_per_beat = 60_000.0 / status.bpm.0;
                (beat_num.0.saturating_sub(1) as f64 * ms_per_beat) as u64
            } else {
                0
            }
        } else {
            0
        };

        let update;
        {
            let mut positions = self.positions.write().unwrap();
            let entry = positions
                .entry(status.device_number.0)
                .or_insert_with(|| PlayerPosition {
                    position_ms: 0,
                    playing: false,
                    timestamp: status.timestamp,
                    effective_tempo: 0.0,
                    pitch_multiplier: 1.0,
                    precise: false,
                });

            if entry.precise {
                // Snap position forward before changing state so that the
                // interpolation base stays consistent.
                entry.snap_forward(status.timestamp);
                entry.playing = playing;
                entry.effective_tempo = effective_tempo;
                entry.pitch_multiplier = pitch_multiplier;
            } else {
                entry.position_ms = estimated_position_ms;
                entry.playing = playing;
                entry.timestamp = status.timestamp;
                entry.effective_tempo = effective_tempo;
                entry.pitch_multiplier = pitch_multiplier;
                entry.precise = false;
            }

            update = PositionUpdate {
                player: status.device_number,
                position_ms: entry.position_ms,
                playing,
                precise: entry.precise,
            };
        }

        let _ = self.tx.send(update);
    }

    /// Process a precise position packet (CDJ-3000 and newer).
    ///
    /// These packets provide exact millisecond positions (~every 30 ms)
    /// and are the highest-quality position source available.
    pub fn on_precise_position(&self, pos: &PrecisePosition) {
        let pitch_multiplier = pos.pitch.to_multiplier();
        let position_ms = pos.position_ms as u64;

        let update;
        {
            let mut positions = self.positions.write().unwrap();
            let entry = positions
                .entry(pos.device_number.0)
                .or_insert_with(|| PlayerPosition {
                    position_ms: 0,
                    playing: false,
                    timestamp: pos.timestamp,
                    effective_tempo: 0.0,
                    pitch_multiplier: 1.0,
                    precise: true,
                });

            // Preserve playing state — precise position packets do not carry
            // an explicit playing flag, so we keep whatever was last set by
            // a CDJ status update.
            let playing = entry.playing;

            entry.position_ms = position_ms;
            entry.timestamp = pos.timestamp;
            entry.effective_tempo = pos.effective_bpm.0;
            entry.pitch_multiplier = pitch_multiplier;
            entry.precise = true;

            update = PositionUpdate {
                player: pos.device_number,
                position_ms,
                playing,
                precise: true,
            };
        }

        let _ = self.tx.send(update);
    }

    /// Process a beat packet.
    ///
    /// Beat packets act as timing anchors — we snap the interpolated
    /// position forward and reset the timestamp so that future
    /// interpolation starts from a freshly confirmed point.
    pub fn on_beat(&self, beat: &Beat) {
        let pitch_multiplier = beat.pitch.to_multiplier();
        let effective_tempo = beat.effective_tempo();

        let update;
        {
            let mut positions = self.positions.write().unwrap();
            let entry = positions
                .entry(beat.device_number.0)
                .or_insert_with(|| PlayerPosition {
                    position_ms: 0,
                    // Receiving beats implies playback.
                    playing: true,
                    timestamp: beat.timestamp,
                    effective_tempo,
                    pitch_multiplier,
                    precise: false,
                });

            entry.snap_forward(beat.timestamp);
            entry.effective_tempo = effective_tempo;
            entry.pitch_multiplier = pitch_multiplier;

            update = PositionUpdate {
                player: beat.device_number,
                position_ms: entry.position_ms,
                playing: entry.playing,
                precise: entry.precise,
            };
        }

        let _ = self.tx.send(update);
    }

    /// Subscribe to position update events.
    pub fn subscribe(&self) -> broadcast::Receiver<PositionUpdate> {
        self.tx.subscribe()
    }

    /// Get the current slack value in milliseconds.
    pub fn slack(&self) -> u64 {
        self.slack_ms.load(Ordering::Relaxed)
    }

    /// Set the slack value in milliseconds.
    pub fn set_slack(&self, ms: u64) {
        self.slack_ms.store(ms, Ordering::Relaxed);
    }

    /// Remove tracking data for a device that has left the network.
    pub fn remove_player(&self, device: DeviceNumber) {
        self.positions.write().unwrap().remove(&device.0);
    }
}

impl Default for TimeFinder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::status::{build_cdj_status, parse_cdj_status, CdjStatusBuilder, CdjStatusFlags};
    use std::time::Duration;

    /// Helper: build and parse a CdjStatus with the given parameters.
    fn make_cdj_status(
        device_number: u8,
        bpm: f64,
        pitch: Pitch,
        beat_number: Option<u32>,
        playing: bool,
    ) -> CdjStatus {
        let params = CdjStatusBuilder {
            device_number: DeviceNumber(device_number),
            bpm: Bpm(bpm),
            pitch,
            beat_number,
            flags: CdjStatusFlags {
                playing,
                ..CdjStatusFlags::default()
            },
            ..CdjStatusBuilder::default()
        };
        let pkt = build_cdj_status(&params);
        parse_cdj_status(&pkt).expect("valid status packet")
    }

    /// Helper: build a minimal PrecisePosition for testing.
    fn make_precise_position(
        device_number: u8,
        position_ms: u32,
        pitch: Pitch,
        effective_bpm: f64,
    ) -> PrecisePosition {
        PrecisePosition {
            name: "test-cdj".into(),
            device_number: DeviceNumber(device_number),
            track_length: 300,
            position_ms,
            pitch,
            effective_bpm: Bpm(effective_bpm),
            timestamp: Instant::now(),
        }
    }

    /// Helper: build a minimal Beat for testing.
    fn make_beat(device_number: u8, bpm: f64, pitch: Pitch) -> Beat {
        Beat {
            name: "test-cdj".into(),
            device_number: DeviceNumber(device_number),
            device_type: DeviceType::Cdj,
            bpm: Bpm(bpm),
            pitch,
            next_beat: Some(500),
            second_beat: Some(1000),
            next_bar: Some(2000),
            fourth_beat: Some(2000),
            second_bar: Some(4000),
            eighth_beat: Some(4000),
            beat_within_bar: 1,
            timestamp: Instant::now(),
        }
    }

    // -----------------------------------------------------------------------
    // 1. Position interpolation while playing
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn interpolation_while_playing() {
        let tf = TimeFinder::new();
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(1), true);
        tf.on_cdj_status(&status);

        // Sleep briefly so elapsed time > 0
        tokio::time::sleep(Duration::from_millis(50)).await;

        let pos = tf.get_time_for(DeviceNumber(1)).unwrap();
        // At 120 BPM, normal pitch (1.0×), position should have advanced ~50 ms
        assert!(pos > 0, "position should advance while playing");
    }

    // -----------------------------------------------------------------------
    // 2. Position stays fixed while paused
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn position_fixed_while_paused() {
        let tf = TimeFinder::new();
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), false);
        tf.on_cdj_status(&status);

        let pos1 = tf.get_time_for(DeviceNumber(1)).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let pos2 = tf.get_time_for(DeviceNumber(1)).unwrap();

        assert_eq!(pos1, pos2, "position must not change while paused");
    }

    // -----------------------------------------------------------------------
    // 3. Precise position overrides estimated position
    // -----------------------------------------------------------------------
    #[test]
    fn precise_overrides_estimated() {
        let tf = TimeFinder::new();

        // First, set an estimated position via status
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), true);
        tf.on_cdj_status(&status);
        let before = tf.get_latest_position(DeviceNumber(1)).unwrap();
        assert!(!before.precise);

        // Now send a precise position
        let pp = make_precise_position(1, 42_000, Pitch(0x100000), 120.0);
        tf.on_precise_position(&pp);

        let after = tf.get_latest_position(DeviceNumber(1)).unwrap();
        assert!(after.precise);
        assert_eq!(after.position_ms, 42_000);
    }

    // -----------------------------------------------------------------------
    // 4. CDJ status updates position
    // -----------------------------------------------------------------------
    #[test]
    fn cdj_status_updates_position() {
        let tf = TimeFinder::new();
        // Beat 5 at 120 BPM → (5-1) × 500ms = 2000ms
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), false);
        tf.on_cdj_status(&status);

        let pos = tf.get_latest_position(DeviceNumber(1)).unwrap();
        assert_eq!(pos.position_ms, 2000);
    }

    // -----------------------------------------------------------------------
    // 5. Beat updates position (timing anchor)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn beat_updates_position() {
        let tf = TimeFinder::new();

        // Start with a known position via status
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), true);
        tf.on_cdj_status(&status);
        let pos_before = tf.get_latest_position(DeviceNumber(1)).unwrap().position_ms;

        // Wait a bit, then send a beat — should snap forward
        tokio::time::sleep(Duration::from_millis(50)).await;
        let beat = make_beat(1, 120.0, Pitch(0x100000));
        tf.on_beat(&beat);

        let pos_after = tf.get_latest_position(DeviceNumber(1)).unwrap().position_ms;
        assert!(
            pos_after > pos_before,
            "beat should snap position forward (was {pos_before}, now {pos_after})"
        );
    }

    // -----------------------------------------------------------------------
    // 6. Player removal
    // -----------------------------------------------------------------------
    #[test]
    fn remove_player() {
        let tf = TimeFinder::new();
        let status = make_cdj_status(2, 128.0, Pitch(0x100000), Some(1), true);
        tf.on_cdj_status(&status);
        assert!(tf.get_time_for(DeviceNumber(2)).is_some());

        tf.remove_player(DeviceNumber(2));
        assert!(tf.get_time_for(DeviceNumber(2)).is_none());
    }

    // -----------------------------------------------------------------------
    // 7. Slack configuration
    // -----------------------------------------------------------------------
    #[test]
    fn slack_default_and_custom() {
        let tf = TimeFinder::new();
        assert_eq!(tf.slack(), 50);

        tf.set_slack(100);
        assert_eq!(tf.slack(), 100);

        let tf2 = TimeFinder::with_slack(200);
        assert_eq!(tf2.slack(), 200);
    }

    // -----------------------------------------------------------------------
    // 8. Subscription receives events
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn subscription_receives_events() {
        let tf = TimeFinder::new();
        let mut rx = tf.subscribe();

        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(1), true);
        tf.on_cdj_status(&status);

        let update = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel not closed");

        assert_eq!(update.player, DeviceNumber(1));
        assert!(update.playing);
    }

    // -----------------------------------------------------------------------
    // 9. Multiple players tracked independently
    // -----------------------------------------------------------------------
    #[test]
    fn multiple_players_independent() {
        let tf = TimeFinder::new();

        // Player 1 at beat 5 (2000 ms at 120 BPM)
        let s1 = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), false);
        tf.on_cdj_status(&s1);

        // Player 2 at beat 9 (4000 ms at 120 BPM)
        let s2 = make_cdj_status(2, 120.0, Pitch(0x100000), Some(9), false);
        tf.on_cdj_status(&s2);

        let p1 = tf.get_latest_position(DeviceNumber(1)).unwrap();
        let p2 = tf.get_latest_position(DeviceNumber(2)).unwrap();

        assert_eq!(p1.position_ms, 2000);
        assert_eq!(p2.position_ms, 4000);
    }

    // -----------------------------------------------------------------------
    // 10. Pitch-adjusted interpolation
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn pitch_adjusted_interpolation() {
        let tf = TimeFinder::new();

        // +6% pitch → multiplier ≈ 1.06
        let pitch = Pitch::from_percentage(6.0);
        assert!((pitch.to_multiplier() - 1.06).abs() < 0.001);

        let status = make_cdj_status(1, 120.0, pitch, Some(1), true);
        tf.on_cdj_status(&status);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = tf.get_time_for(DeviceNumber(1)).unwrap();
        // At 1.06× for ~100 ms → expect ~106 ms of advancement.
        // Allow generous tolerance because sleep is imprecise.
        assert!(
            pos >= 80 && pos <= 200,
            "position {pos} should reflect ~1.06× speed over ~100 ms"
        );
    }

    // -----------------------------------------------------------------------
    // 11. Position doesn't go negative
    // -----------------------------------------------------------------------
    #[test]
    fn position_does_not_go_negative() {
        let tf = TimeFinder::new();

        // Start at position 0, paused
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(1), false);
        tf.on_cdj_status(&status);

        let pos = tf.get_time_for(DeviceNumber(1)).unwrap();
        assert_eq!(pos, 0, "position at beat 1 with 0 elapsed should be 0");

        // Even with an explicit position of 0, playing shouldn't go negative
        let pp = make_precise_position(1, 0, Pitch(0x100000), 120.0);
        tf.on_precise_position(&pp);
        let pos = tf.get_time_for(DeviceNumber(1)).unwrap();
        assert_eq!(pos, 0, "position should not be negative");
    }

    // -----------------------------------------------------------------------
    // 12. Unknown player returns None
    // -----------------------------------------------------------------------
    #[test]
    fn unknown_player_returns_none() {
        let tf = TimeFinder::new();
        assert!(tf.get_time_for(DeviceNumber(42)).is_none());
        assert!(tf.get_latest_position(DeviceNumber(42)).is_none());
    }

    // -----------------------------------------------------------------------
    // Additional: Default trait implementation
    // -----------------------------------------------------------------------
    #[test]
    fn default_trait() {
        let tf = TimeFinder::default();
        assert_eq!(tf.slack(), DEFAULT_SLACK_MS);
    }

    // -----------------------------------------------------------------------
    // Additional: Precise position preserves playing state from status
    // -----------------------------------------------------------------------
    #[test]
    fn precise_preserves_playing_state() {
        let tf = TimeFinder::new();

        // Set playing via status
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(1), true);
        tf.on_cdj_status(&status);

        // Send precise position — playing should still be true
        let pp = make_precise_position(1, 5000, Pitch(0x100000), 120.0);
        tf.on_precise_position(&pp);

        let update = tf.get_latest_position(DeviceNumber(1)).unwrap();
        assert!(update.playing, "precise position should preserve playing state");
    }

    // -----------------------------------------------------------------------
    // Additional: CDJ status does not overwrite precise position
    // -----------------------------------------------------------------------
    #[test]
    fn status_does_not_overwrite_precise_position() {
        let tf = TimeFinder::new();

        // Set precise position
        let pp = make_precise_position(1, 42_000, Pitch(0x100000), 120.0);
        tf.on_precise_position(&pp);

        // Now send a CDJ status with a different beat-derived position
        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(5), true);
        tf.on_cdj_status(&status);

        let pos = tf.get_latest_position(DeviceNumber(1)).unwrap();
        assert!(pos.precise, "should still be marked precise");
        // The position should NOT be 2000 (beat-derived) — it should be
        // close to 42000 (the precise value, possibly slightly advanced
        // by snap_forward).
        assert!(
            pos.position_ms >= 42_000,
            "status must not overwrite precise position (got {})",
            pos.position_ms
        );
    }

    // -----------------------------------------------------------------------
    // Additional: Beat creates entry for new device
    // -----------------------------------------------------------------------
    #[test]
    fn beat_creates_new_entry() {
        let tf = TimeFinder::new();
        assert!(tf.get_time_for(DeviceNumber(3)).is_none());

        let beat = make_beat(3, 128.0, Pitch(0x100000));
        tf.on_beat(&beat);

        let pos = tf.get_time_for(DeviceNumber(3));
        assert!(pos.is_some(), "beat should create an entry for new device");
    }

    // -----------------------------------------------------------------------
    // Additional: Multiple subscriptions are independent
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn multiple_subscribers() {
        let tf = TimeFinder::new();
        let mut rx1 = tf.subscribe();
        let mut rx2 = tf.subscribe();

        let status = make_cdj_status(1, 120.0, Pitch(0x100000), Some(1), true);
        tf.on_cdj_status(&status);

        let u1 = tokio::time::timeout(Duration::from_millis(100), rx1.recv())
            .await
            .expect("rx1 timeout")
            .expect("rx1 recv");
        let u2 = tokio::time::timeout(Duration::from_millis(100), rx2.recv())
            .await
            .expect("rx2 timeout")
            .expect("rx2 recv");

        assert_eq!(u1.player, u2.player);
    }
}
