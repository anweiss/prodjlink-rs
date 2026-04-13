//! Tempo master tracking and handoff protocol.
//!
//! Tracks which device on the DJ Link network is the current tempo master,
//! what BPM it is broadcasting, and supports master handoff negotiation.
//!
//! The tempo master is the device whose BPM all synced devices follow.
//! Only one device is the master at a time. When a device wants to become
//! master, it sets the MASTER flag (0x20) in its status; the current master
//! sees this and yields by setting the master_handoff byte; the new master
//! then sends a master_command packet on the beat port (50001) to confirm.

use std::fmt;
use std::time::Instant;

use tokio::sync::{broadcast, watch};

use crate::device::types::{Bpm, DeviceNumber};

// ---------------------------------------------------------------------------
// Master state
// ---------------------------------------------------------------------------

/// Snapshot of the current tempo master state.
#[derive(Debug, Clone)]
pub struct MasterState {
    /// Which device is the current tempo master, or `None` if unknown.
    pub master_device: Option<DeviceNumber>,
    /// The BPM being broadcast by the tempo master.
    pub master_tempo: Bpm,
    /// Whether *we* (the VirtualCdj) are the tempo master.
    pub we_are_master: bool,
    /// When this state was last updated.
    pub updated_at: Instant,
}

impl Default for MasterState {
    fn default() -> Self {
        Self {
            master_device: None,
            master_tempo: Bpm(0.0),
            we_are_master: false,
            updated_at: Instant::now(),
        }
    }
}

impl fmt::Display for MasterState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.master_device {
            Some(d) => write!(f, "Master: device {} @ {} BPM", d, self.master_tempo),
            None => write!(f, "Master: none"),
        }
    }
}

// ---------------------------------------------------------------------------
// Change events
// ---------------------------------------------------------------------------

/// Events emitted when the tempo master state changes.
#[derive(Debug, Clone)]
pub enum TempoMasterEvent {
    /// A new device became the tempo master.
    MasterChanged {
        old: Option<DeviceNumber>,
        new: Option<DeviceNumber>,
    },
    /// The master tempo (BPM) changed.
    TempoChanged {
        device: DeviceNumber,
        old_bpm: Bpm,
        new_bpm: Bpm,
    },
    /// The current master is yielding to us — we should acknowledge.
    MasterYieldedToUs { from_device: DeviceNumber },
    /// We became the tempo master.
    WeBecameMaster,
    /// We are no longer the tempo master.
    WeResignedMaster,
}

// ---------------------------------------------------------------------------
// TempoMaster
// ---------------------------------------------------------------------------

/// Tracks tempo master state and emits change events.
///
/// Use [`subscribe()`](TempoMaster::subscribe) to receive
/// [`TempoMasterEvent`] notifications, and [`state()`](TempoMaster::state)
/// to read the current master snapshot at any time.
pub struct TempoMaster {
    /// Our device number.
    our_device: DeviceNumber,
    /// Current state (watch channel for reads, sender for writes).
    state_tx: watch::Sender<MasterState>,
    state_rx: watch::Receiver<MasterState>,
    /// Broadcast channel for change events.
    event_tx: broadcast::Sender<TempoMasterEvent>,
}

impl TempoMaster {
    /// Create a new tempo master tracker for the given virtual CDJ device number.
    pub fn new(our_device: DeviceNumber) -> Self {
        let initial = MasterState::default();
        let (state_tx, state_rx) = watch::channel(initial);
        let (event_tx, _) = broadcast::channel(64);
        Self {
            our_device,
            state_tx,
            state_rx,
            event_tx,
        }
    }

    /// Get the current master state.
    pub fn state(&self) -> MasterState {
        self.state_rx.borrow().clone()
    }

    /// Get a watch receiver for continuous state observation.
    pub fn watch(&self) -> watch::Receiver<MasterState> {
        self.state_rx.clone()
    }

    /// Subscribe to master change events.
    pub fn subscribe(&self) -> broadcast::Receiver<TempoMasterEvent> {
        self.event_tx.subscribe()
    }

    /// Get the current master device, if any.
    pub fn master_device(&self) -> Option<DeviceNumber> {
        self.state_rx.borrow().master_device
    }

    /// Get the current master tempo.
    pub fn master_tempo(&self) -> Bpm {
        self.state_rx.borrow().master_tempo
    }

    /// Whether we are the current tempo master.
    pub fn we_are_master(&self) -> bool {
        self.state_rx.borrow().we_are_master
    }

    /// Update state when we see a device claiming to be master (via status packet).
    ///
    /// Called when a CdjStatus or MixerStatus arrives with the MASTER flag set.
    pub fn on_device_is_master(&self, device: DeviceNumber, bpm: Bpm) {
        let current = self.state_rx.borrow().clone();
        let is_us = device == self.our_device;

        // Emit events for changes
        if current.master_device != Some(device) {
            let _ = self.event_tx.send(TempoMasterEvent::MasterChanged {
                old: current.master_device,
                new: Some(device),
            });

            if is_us && !current.we_are_master {
                let _ = self.event_tx.send(TempoMasterEvent::WeBecameMaster);
            } else if !is_us && current.we_are_master {
                let _ = self.event_tx.send(TempoMasterEvent::WeResignedMaster);
            }
        }

        if current.master_device == Some(device) && (current.master_tempo.0 - bpm.0).abs() > 0.001 {
            let _ = self.event_tx.send(TempoMasterEvent::TempoChanged {
                device,
                old_bpm: current.master_tempo,
                new_bpm: bpm,
            });
        }

        self.state_tx.send_modify(|s| {
            s.master_device = Some(device);
            s.master_tempo = bpm;
            s.we_are_master = is_us;
            s.updated_at = Instant::now();
        });
    }

    /// Update tempo from a beat packet (beats also carry BPM).
    ///
    /// Only updates the tempo if the beat comes from the current master.
    pub fn on_beat(&self, device: DeviceNumber, bpm: Bpm) {
        let current = self.state_rx.borrow().clone();
        if current.master_device == Some(device) && (current.master_tempo.0 - bpm.0).abs() > 0.001 {
            let _ = self.event_tx.send(TempoMasterEvent::TempoChanged {
                device,
                old_bpm: current.master_tempo,
                new_bpm: bpm,
            });
            self.state_tx.send_modify(|s| {
                s.master_tempo = bpm;
                s.updated_at = Instant::now();
            });
        }
    }

    /// Called when a status packet shows that no device is master.
    ///
    /// This happens when the previous master disappears from the network.
    pub fn on_no_master(&self) {
        let current = self.state_rx.borrow().clone();
        if current.master_device.is_some() {
            let _ = self.event_tx.send(TempoMasterEvent::MasterChanged {
                old: current.master_device,
                new: None,
            });
            if current.we_are_master {
                let _ = self.event_tx.send(TempoMasterEvent::WeResignedMaster);
            }
            self.state_tx.send_modify(|s| {
                s.master_device = None;
                s.master_tempo = Bpm(0.0);
                s.we_are_master = false;
                s.updated_at = Instant::now();
            });
        }
    }

    /// Called when we observe a master_handoff byte targeting our device.
    ///
    /// The current master is yielding to us. The VirtualCdj should
    /// respond by sending a master_command on port 50001.
    pub fn on_master_yielded_to_us(&self, from_device: DeviceNumber) {
        let _ = self
            .event_tx
            .send(TempoMasterEvent::MasterYieldedToUs { from_device });
    }

    /// Called when we successfully claim the master role.
    pub fn set_we_are_master(&self, bpm: Bpm) {
        let current = self.state_rx.borrow().clone();
        if !current.we_are_master {
            let _ = self.event_tx.send(TempoMasterEvent::MasterChanged {
                old: current.master_device,
                new: Some(self.our_device),
            });
            let _ = self.event_tx.send(TempoMasterEvent::WeBecameMaster);
        }
        self.state_tx.send_modify(|s| {
            s.master_device = Some(self.our_device);
            s.master_tempo = bpm;
            s.we_are_master = true;
            s.updated_at = Instant::now();
        });
    }

    /// Update master tempo (used when we are master and change our BPM).
    pub fn set_master_tempo(&self, bpm: Bpm) {
        let current = self.state_rx.borrow().clone();
        if (current.master_tempo.0 - bpm.0).abs() > 0.001 {
            if let Some(device) = current.master_device {
                let _ = self.event_tx.send(TempoMasterEvent::TempoChanged {
                    device,
                    old_bpm: current.master_tempo,
                    new_bpm: bpm,
                });
            }
            self.state_tx.send_modify(|s| {
                s.master_tempo = bpm;
                s.updated_at = Instant::now();
            });
        }
    }

    /// Called when we resign the master role (e.g. yielding to another device).
    pub fn resign_master(&self) {
        let current = self.state_rx.borrow().clone();
        if current.we_are_master {
            let _ = self.event_tx.send(TempoMasterEvent::WeResignedMaster);
            self.state_tx.send_modify(|s| {
                s.we_are_master = false;
                s.updated_at = Instant::now();
            });
        }
    }

    /// Get our device number.
    pub fn our_device(&self) -> DeviceNumber {
        self.our_device
    }
}

impl fmt::Debug for TempoMaster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TempoMaster")
            .field("our_device", &self.our_device)
            .field("state", &*self.state_rx.borrow())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(n: u8) -> DeviceNumber {
        DeviceNumber(n)
    }

    #[test]
    fn initial_state_is_no_master() {
        let tm = TempoMaster::new(dev(5));
        let state = tm.state();
        assert!(state.master_device.is_none());
        assert!((state.master_tempo.0).abs() < f64::EPSILON);
        assert!(!state.we_are_master);
    }

    #[test]
    fn on_device_is_master_sets_state() {
        let tm = TempoMaster::new(dev(5));
        tm.on_device_is_master(dev(3), Bpm(128.0));

        let state = tm.state();
        assert_eq!(state.master_device, Some(dev(3)));
        assert!((state.master_tempo.0 - 128.0).abs() < f64::EPSILON);
        assert!(!state.we_are_master);
    }

    #[test]
    fn on_device_is_master_we_are_master() {
        let tm = TempoMaster::new(dev(5));
        tm.on_device_is_master(dev(5), Bpm(130.0));

        let state = tm.state();
        assert_eq!(state.master_device, Some(dev(5)));
        assert!(state.we_are_master);
    }

    #[test]
    fn master_changed_event() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        tm.on_device_is_master(dev(3), Bpm(128.0));
        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, TempoMasterEvent::MasterChanged { old: None, new: Some(d) } if d == dev(3))
        );
    }

    #[test]
    fn tempo_changed_event() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        // Set initial master
        tm.on_device_is_master(dev(3), Bpm(128.0));
        // Drain the MasterChanged event
        let _ = rx.try_recv();

        // Change tempo from same master
        tm.on_device_is_master(dev(3), Bpm(130.0));
        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, TempoMasterEvent::TempoChanged { device, old_bpm, new_bpm }
                if device == dev(3)
                && (old_bpm.0 - 128.0).abs() < f64::EPSILON
                && (new_bpm.0 - 130.0).abs() < f64::EPSILON
            )
        );
    }

    #[test]
    fn master_handoff_between_devices() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        // Device 3 is master
        tm.on_device_is_master(dev(3), Bpm(128.0));
        let _ = rx.try_recv(); // drain MasterChanged

        // Device 1 becomes master
        tm.on_device_is_master(dev(1), Bpm(128.0));
        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, TempoMasterEvent::MasterChanged { old: Some(o), new: Some(n) }
                if o == dev(3) && n == dev(1)
            )
        );
    }

    #[test]
    fn on_no_master_clears_state() {
        let tm = TempoMaster::new(dev(5));
        tm.on_device_is_master(dev(3), Bpm(128.0));

        let mut rx = tm.subscribe();
        tm.on_no_master();

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, TempoMasterEvent::MasterChanged { old: Some(o), new: None }
                if o == dev(3)
            )
        );

        let state = tm.state();
        assert!(state.master_device.is_none());
        assert!(!state.we_are_master);
    }

    #[test]
    fn on_beat_updates_tempo_for_current_master() {
        let tm = TempoMaster::new(dev(5));
        tm.on_device_is_master(dev(3), Bpm(128.0));

        let mut rx = tm.subscribe();
        tm.on_beat(dev(3), Bpm(130.0));

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TempoMasterEvent::TempoChanged { .. }));
        assert!((tm.master_tempo().0 - 130.0).abs() < f64::EPSILON);
    }

    #[test]
    fn on_beat_ignores_non_master() {
        let tm = TempoMaster::new(dev(5));
        tm.on_device_is_master(dev(3), Bpm(128.0));

        tm.on_beat(dev(1), Bpm(999.0));
        assert!((tm.master_tempo().0 - 128.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_we_are_master_emits_events() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        tm.set_we_are_master(Bpm(126.0));

        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, TempoMasterEvent::MasterChanged { .. }));

        let ev2 = rx.try_recv().unwrap();
        assert!(matches!(ev2, TempoMasterEvent::WeBecameMaster));

        let state = tm.state();
        assert_eq!(state.master_device, Some(dev(5)));
        assert!(state.we_are_master);
    }

    #[test]
    fn resign_master_emits_event() {
        let tm = TempoMaster::new(dev(5));
        tm.set_we_are_master(Bpm(128.0));

        let mut rx = tm.subscribe();
        tm.resign_master();

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TempoMasterEvent::WeResignedMaster));
        assert!(!tm.we_are_master());
    }

    #[test]
    fn resign_when_not_master_is_noop() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        tm.resign_master();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn set_master_tempo_when_master() {
        let tm = TempoMaster::new(dev(5));
        tm.set_we_are_master(Bpm(128.0));

        let mut rx = tm.subscribe();
        tm.set_master_tempo(Bpm(132.0));

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TempoMasterEvent::TempoChanged { .. }));
        assert!((tm.master_tempo().0 - 132.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_master_tempo_same_value_is_noop() {
        let tm = TempoMaster::new(dev(5));
        tm.set_we_are_master(Bpm(128.0));

        let mut rx = tm.subscribe();
        tm.set_master_tempo(Bpm(128.0));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn on_master_yielded_to_us_emits_event() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        tm.on_master_yielded_to_us(dev(3));

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, TempoMasterEvent::MasterYieldedToUs { from_device } if from_device == dev(3))
        );
    }

    #[test]
    fn watch_channel_reflects_current_state() {
        let tm = TempoMaster::new(dev(5));
        let rx = tm.watch();

        assert!(rx.borrow().master_device.is_none());

        tm.on_device_is_master(dev(2), Bpm(140.0));
        assert_eq!(rx.borrow().master_device, Some(dev(2)));
        assert!((rx.borrow().master_tempo.0 - 140.0).abs() < f64::EPSILON);
    }

    #[test]
    fn we_become_master_then_other_takes_over() {
        let tm = TempoMaster::new(dev(5));
        let mut rx = tm.subscribe();

        // We become master
        tm.on_device_is_master(dev(5), Bpm(128.0));
        assert!(tm.we_are_master());
        let _ = rx.try_recv(); // MasterChanged
        let _ = rx.try_recv(); // WeBecameMaster

        // Another device takes over
        tm.on_device_is_master(dev(3), Bpm(128.0));
        assert!(!tm.we_are_master());

        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, TempoMasterEvent::MasterChanged { .. }));

        let ev2 = rx.try_recv().unwrap();
        assert!(matches!(ev2, TempoMasterEvent::WeResignedMaster));
    }

    #[test]
    fn display_formatting() {
        let tm = TempoMaster::new(dev(5));
        assert_eq!(tm.state().to_string(), "Master: none");

        tm.on_device_is_master(dev(3), Bpm(128.0));
        assert_eq!(tm.state().to_string(), "Master: device 3 @ 128.00 BPM");
    }

    #[test]
    fn master_device_accessor() {
        let tm = TempoMaster::new(dev(5));
        assert!(tm.master_device().is_none());

        tm.on_device_is_master(dev(2), Bpm(120.0));
        assert_eq!(tm.master_device(), Some(dev(2)));
    }

    #[test]
    fn our_device_accessor() {
        let tm = TempoMaster::new(dev(7));
        assert_eq!(tm.our_device(), dev(7));
    }
}
