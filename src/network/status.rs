use std::collections::HashMap;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::device::types::DeviceNumber;
use crate::error::Result;
use crate::protocol::header::STATUS_PORT;
use crate::protocol::status::{parse_status, DeviceUpdate};

/// Async service that listens for CDJ and mixer status updates on UDP port 50002.
///
/// Incoming packets are parsed into [`DeviceUpdate`] values, broadcast to all
/// subscribers, and cached so the latest status per device is always available.
pub struct StatusListener {
    /// Latest status per device number.
    latest: Arc<RwLock<HashMap<u8, DeviceUpdate>>>,
    event_tx: broadcast::Sender<DeviceUpdate>,
    recv_task: JoinHandle<()>,
}

impl StatusListener {
    /// Bind to the status port and begin receiving device updates.
    pub async fn start() -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", STATUS_PORT)).await?;
        let socket = Arc::new(socket);
        let (event_tx, _) = broadcast::channel(512);
        let latest: Arc<RwLock<HashMap<u8, DeviceUpdate>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let recv_tx = event_tx.clone();
        let recv_latest = latest.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, _)) => {
                        if let Ok(update) = parse_status(&buf[..len]) {
                            let key = match &update {
                                DeviceUpdate::Cdj(s) => s.device_number.0,
                                DeviceUpdate::Mixer(s) => s.device_number.0,
                            };
                            recv_latest.write().await.insert(key, update.clone());
                            let _ = recv_tx.send(update);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            latest,
            event_tx,
            recv_task,
        })
    }

    /// Subscribe to a broadcast stream of status update events.
    pub fn subscribe(&self) -> broadcast::Receiver<DeviceUpdate> {
        self.event_tx.subscribe()
    }

    /// Get the latest status for a specific device.
    pub async fn latest(&self, device: DeviceNumber) -> Option<DeviceUpdate> {
        self.latest.read().await.get(&device.0).cloned()
    }

    /// Get all latest statuses.
    pub async fn all_latest(&self) -> Vec<DeviceUpdate> {
        self.latest.read().await.values().cloned().collect()
    }

    /// Stop the listener, aborting its background receive task.
    pub fn stop(self) {
        self.recv_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::status::{CdjStatus, MixerStatus};
    use std::time::Instant;

    fn make_cdj_update(num: u8) -> DeviceUpdate {
        DeviceUpdate::Cdj(CdjStatus {
            name: format!("CDJ-{num}"),
            device_number: DeviceNumber(num),
            device_type: crate::device::types::DeviceType::Cdj,
            track_source_player: DeviceNumber(1),
            track_source_slot: crate::device::types::TrackSourceSlot::UsbSlot,
            track_type: crate::device::types::TrackType::Rekordbox,
            rekordbox_id: 42,
            play_state: crate::device::types::PlayState::Playing,
            is_playing: true,
            is_master: false,
            is_synced: true,
            is_bpm_synced: false,
            is_on_air: true,
            bpm: crate::device::types::Bpm(128.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_number: Some(crate::device::types::BeatNumber(1)),
            beat_within_bar: 1,
            firmware_version: "1A01".to_string(),
            sync_number: 0,
            master_hand_off: None,
            loop_start: None,
            loop_end: None,
            loop_beats: None,
            timestamp: Instant::now(),
        })
    }

    fn make_mixer_update(num: u8) -> DeviceUpdate {
        DeviceUpdate::Mixer(MixerStatus {
            name: format!("DJM-{num}"),
            device_number: DeviceNumber(num),
            bpm: crate::device::types::Bpm(128.0),
            pitch: crate::device::types::Pitch(0x100000),
            beat_within_bar: 1,
            is_master: true,
            is_synced: true,
            master_hand_off: None,
            timestamp: Instant::now(),
        })
    }

    // ------------------------------------------------------------------
    // DeviceUpdate variant matching
    // ------------------------------------------------------------------

    #[test]
    fn device_update_matches_cdj() {
        let update = make_cdj_update(3);
        assert!(matches!(update, DeviceUpdate::Cdj(_)));
        if let DeviceUpdate::Cdj(s) = &update {
            assert_eq!(s.device_number, DeviceNumber(3));
        }
    }

    #[test]
    fn device_update_matches_mixer() {
        let update = make_mixer_update(33);
        assert!(matches!(update, DeviceUpdate::Mixer(_)));
        if let DeviceUpdate::Mixer(s) = &update {
            assert_eq!(s.device_number, DeviceNumber(33));
        }
    }

    #[test]
    fn device_key_extraction_cdj() {
        let update = make_cdj_update(2);
        let key = match &update {
            DeviceUpdate::Cdj(s) => s.device_number.0,
            DeviceUpdate::Mixer(s) => s.device_number.0,
        };
        assert_eq!(key, 2);
    }

    #[test]
    fn device_key_extraction_mixer() {
        let update = make_mixer_update(33);
        let key = match &update {
            DeviceUpdate::Cdj(s) => s.device_number.0,
            DeviceUpdate::Mixer(s) => s.device_number.0,
        };
        assert_eq!(key, 33);
    }

    // ------------------------------------------------------------------
    // Latest map logic (unit-tested without networking)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn latest_map_insert_and_retrieve() {
        let map: Arc<RwLock<HashMap<u8, DeviceUpdate>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let cdj = make_cdj_update(1);
        let mixer = make_mixer_update(33);

        {
            let mut w = map.write().await;
            w.insert(1, cdj.clone());
            w.insert(33, mixer.clone());
        }

        let r = map.read().await;
        assert_eq!(r.len(), 2);
        assert!(matches!(r.get(&1), Some(DeviceUpdate::Cdj(_))));
        assert!(matches!(r.get(&33), Some(DeviceUpdate::Mixer(_))));
    }

    #[tokio::test]
    async fn latest_map_overwrite() {
        let map: Arc<RwLock<HashMap<u8, DeviceUpdate>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let cdj1 = make_cdj_update(1);
        let cdj1_v2 = make_cdj_update(1);

        {
            let mut w = map.write().await;
            w.insert(1, cdj1);
            w.insert(1, cdj1_v2);
        }

        let r = map.read().await;
        assert_eq!(r.len(), 1);
    }

    #[tokio::test]
    async fn latest_map_missing_key_returns_none() {
        let map: Arc<RwLock<HashMap<u8, DeviceUpdate>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let r = map.read().await;
        assert!(r.get(&99).is_none());
    }

    // ------------------------------------------------------------------
    // Broadcast channel semantics
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn broadcast_channel_delivers_updates() {
        let (tx, mut rx) = broadcast::channel::<DeviceUpdate>(16);
        let update = make_cdj_update(2);
        tx.send(update.clone()).unwrap();

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, DeviceUpdate::Cdj(_)));
        if let DeviceUpdate::Cdj(s) = received {
            assert_eq!(s.device_number, DeviceNumber(2));
        }
    }

    #[tokio::test]
    async fn broadcast_channel_multiple_subscribers() {
        let (tx, mut rx1) = broadcast::channel::<DeviceUpdate>(16);
        let mut rx2 = tx.subscribe();

        let update = make_mixer_update(33);
        tx.send(update).unwrap();

        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert!(matches!(r1, DeviceUpdate::Mixer(_)));
        assert!(matches!(r2, DeviceUpdate::Mixer(_)));
    }
}
