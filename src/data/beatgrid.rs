use crate::error::{ProDjLinkError, Result};

/// Size of the beat grid header.
const HEADER_SIZE: usize = 20;
/// Size of each beat grid entry.
const ENTRY_SIZE: usize = 16;

/// A single entry in the beat grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatGridEntry {
    /// Beat number within the bar (1-4).
    pub beat_within_bar: u16,
    /// Tempo at this beat in BPM (already divided by 100).
    pub tempo: f64,
    /// Time in milliseconds from the start of the track.
    pub time_ms: u32,
}

/// A beat grid for a track, providing beat-to-time mapping.
#[derive(Debug, Clone)]
pub struct BeatGrid {
    /// The beat entries, in chronological order.
    pub entries: Vec<BeatGridEntry>,
}

impl BeatGrid {
    /// Parse a beat grid from raw binary data (from dbserver BinaryField).
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(ProDjLinkError::Parse(format!(
                "beat grid data too short: {} bytes (need at least {})",
                data.len(),
                HEADER_SIZE
            )));
        }

        let payload = &data[HEADER_SIZE..];
        let entry_count = payload.len() / ENTRY_SIZE;
        let mut entries = Vec::with_capacity(entry_count);

        for i in 0..entry_count {
            let offset = i * ENTRY_SIZE;
            let beat_within_bar = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
            let raw_tempo = u16::from_le_bytes([payload[offset + 2], payload[offset + 3]]);
            let tempo = raw_tempo as f64 / 100.0;
            let time_ms = u32::from_le_bytes([
                payload[offset + 4],
                payload[offset + 5],
                payload[offset + 6],
                payload[offset + 7],
            ]);

            entries.push(BeatGridEntry {
                beat_within_bar,
                tempo,
                time_ms,
            });
        }

        Ok(Self { entries })
    }

    /// Get the number of beats in the grid.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the beat grid contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Find the beat grid entry closest to the given time in milliseconds.
    pub fn beat_at_time(&self, time_ms: u32) -> Option<&BeatGridEntry> {
        if self.entries.is_empty() {
            return None;
        }
        // Binary search for the closest beat
        match self.entries.binary_search_by_key(&time_ms, |e| e.time_ms) {
            Ok(idx) => Some(&self.entries[idx]),
            Err(idx) => {
                if idx == 0 {
                    Some(&self.entries[0])
                } else if idx >= self.entries.len() {
                    self.entries.last()
                } else {
                    // Return the closer of the two adjacent entries
                    let before = &self.entries[idx - 1];
                    let after = &self.entries[idx];
                    if time_ms - before.time_ms <= after.time_ms - time_ms {
                        Some(before)
                    } else {
                        Some(after)
                    }
                }
            }
        }
    }

    /// Get the time in ms of the Nth beat (0-indexed).
    pub fn time_of_beat(&self, beat_index: usize) -> Option<u32> {
        self.entries.get(beat_index).map(|e| e.time_ms)
    }

    /// Get the bar number for a given beat index (0-indexed).
    ///
    /// Handles incomplete first bars (e.g. if the track starts on beat 3).
    /// Returns 1-based bar numbers, or `None` if the index is out of range.
    pub fn bar_number(&self, beat_index: usize) -> Option<u32> {
        if beat_index >= self.entries.len() {
            return None;
        }
        let mut bar = 1u32;
        for i in 1..=beat_index {
            if self.entries[i].beat_within_bar <= self.entries[i - 1].beat_within_bar {
                bar += 1;
            }
        }
        Some(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build test data: 20-byte header + N 16-byte entries.
    /// Each entry is padded to 16 bytes (only first 8 bytes are meaningful).
    fn make_beat_grid_data(entries: &[(u16, u16, u32)]) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];
        for &(beat, raw_tempo, time) in entries {
            let mut entry = vec![0u8; ENTRY_SIZE];
            entry[0..2].copy_from_slice(&beat.to_le_bytes());
            entry[2..4].copy_from_slice(&raw_tempo.to_le_bytes());
            entry[4..8].copy_from_slice(&time.to_le_bytes());
            data.extend_from_slice(&entry);
        }
        data
    }

    #[test]
    fn parse_known_values() {
        // beat_within_bar=1, tempo_raw=12800 (128.00 BPM), time_ms=0
        // beat_within_bar=2, tempo_raw=12800 (128.00 BPM), time_ms=469
        // beat_within_bar=3, tempo_raw=12850 (128.50 BPM), time_ms=937
        // beat_within_bar=4, tempo_raw=12850 (128.50 BPM), time_ms=1404
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 469),
            (3, 12850, 937),
            (4, 12850, 1404),
        ]);

        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert_eq!(grid.len(), 4);
        assert!(!grid.is_empty());

        assert_eq!(grid.entries[0].beat_within_bar, 1);
        assert!((grid.entries[0].tempo - 128.0).abs() < f64::EPSILON);
        assert_eq!(grid.entries[0].time_ms, 0);

        assert_eq!(grid.entries[1].beat_within_bar, 2);
        assert!((grid.entries[1].tempo - 128.0).abs() < f64::EPSILON);
        assert_eq!(grid.entries[1].time_ms, 469);

        assert_eq!(grid.entries[2].beat_within_bar, 3);
        assert!((grid.entries[2].tempo - 128.5).abs() < f64::EPSILON);
        assert_eq!(grid.entries[2].time_ms, 937);

        assert_eq!(grid.entries[3].beat_within_bar, 4);
        assert!((grid.entries[3].tempo - 128.5).abs() < f64::EPSILON);
        assert_eq!(grid.entries[3].time_ms, 1404);
    }

    #[test]
    fn beat_at_time_exact_match() {
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 469),
            (3, 12800, 937),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        let entry = grid.beat_at_time(469).unwrap();
        assert_eq!(entry.beat_within_bar, 2);
        assert_eq!(entry.time_ms, 469);
    }

    #[test]
    fn beat_at_time_closest_before() {
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 500),
            (3, 12800, 1000),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        // 600 is closer to 500 than to 1000
        let entry = grid.beat_at_time(600).unwrap();
        assert_eq!(entry.time_ms, 500);
        assert_eq!(entry.beat_within_bar, 2);
    }

    #[test]
    fn beat_at_time_closest_after() {
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 500),
            (3, 12800, 1000),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        // 800 is closer to 1000 than to 500
        let entry = grid.beat_at_time(800).unwrap();
        assert_eq!(entry.time_ms, 1000);
        assert_eq!(entry.beat_within_bar, 3);
    }

    #[test]
    fn beat_at_time_before_first() {
        let data = make_beat_grid_data(&[(1, 12800, 100)]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        // Time before the first beat returns the first entry
        let entry = grid.beat_at_time(0).unwrap();
        assert_eq!(entry.time_ms, 100);
    }

    #[test]
    fn beat_at_time_after_last() {
        let data = make_beat_grid_data(&[(1, 12800, 100)]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        // Time after the last beat returns the last entry
        let entry = grid.beat_at_time(9999).unwrap();
        assert_eq!(entry.time_ms, 100);
    }

    #[test]
    fn time_of_beat_valid() {
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 469),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        assert_eq!(grid.time_of_beat(0), Some(0));
        assert_eq!(grid.time_of_beat(1), Some(469));
    }

    #[test]
    fn time_of_beat_out_of_bounds() {
        let data = make_beat_grid_data(&[(1, 12800, 0)]);
        let grid = BeatGrid::from_bytes(&data).unwrap();

        assert_eq!(grid.time_of_beat(5), None);
    }

    #[test]
    fn empty_grid() {
        let grid = BeatGrid::from_bytes(&[0u8; HEADER_SIZE]).unwrap();
        assert!(grid.is_empty());
        assert_eq!(grid.len(), 0);
        assert!(grid.beat_at_time(0).is_none());
        assert!(grid.time_of_beat(0).is_none());
    }

    #[test]
    fn header_only_no_entries() {
        // Header with a few trailing bytes (not enough for a full entry)
        let mut data = vec![0u8; HEADER_SIZE + 10];
        data[0] = 0xAA; // arbitrary header content
        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert!(grid.is_empty());
    }

    #[test]
    fn data_shorter_than_header_returns_error() {
        let result = BeatGrid::from_bytes(&[0u8; 10]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too short"));
    }

    // --- bar_number tests ---

    #[test]
    fn bar_number_normal_four_beat_bars() {
        // Two full bars: 1,2,3,4 | 1,2,3,4
        let data = make_beat_grid_data(&[
            (1, 12800, 0),
            (2, 12800, 469),
            (3, 12800, 937),
            (4, 12800, 1404),
            (1, 12800, 1872),
            (2, 12800, 2340),
            (3, 12800, 2808),
            (4, 12800, 3276),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert_eq!(grid.bar_number(0), Some(1)); // beat 1 of bar 1
        assert_eq!(grid.bar_number(1), Some(1)); // beat 2 of bar 1
        assert_eq!(grid.bar_number(2), Some(1)); // beat 3 of bar 1
        assert_eq!(grid.bar_number(3), Some(1)); // beat 4 of bar 1
        assert_eq!(grid.bar_number(4), Some(2)); // beat 1 of bar 2
        assert_eq!(grid.bar_number(7), Some(2)); // beat 4 of bar 2
    }

    #[test]
    fn bar_number_incomplete_first_bar() {
        // Track starts on beat 3: 3,4 | 1,2,3,4
        let data = make_beat_grid_data(&[
            (3, 12800, 0),
            (4, 12800, 469),
            (1, 12800, 937),
            (2, 12800, 1404),
            (3, 12800, 1872),
            (4, 12800, 2340),
        ]);
        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert_eq!(grid.bar_number(0), Some(1)); // beat 3 of bar 1
        assert_eq!(grid.bar_number(1), Some(1)); // beat 4 of bar 1
        assert_eq!(grid.bar_number(2), Some(2)); // beat 1 of bar 2
        assert_eq!(grid.bar_number(5), Some(2)); // beat 4 of bar 2
    }

    #[test]
    fn bar_number_out_of_bounds() {
        let data = make_beat_grid_data(&[(1, 12800, 0), (2, 12800, 469)]);
        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert_eq!(grid.bar_number(2), None);
        assert_eq!(grid.bar_number(100), None);
    }

    #[test]
    fn bar_number_empty_grid() {
        let grid = BeatGrid::from_bytes(&[0u8; HEADER_SIZE]).unwrap();
        assert_eq!(grid.bar_number(0), None);
    }

    #[test]
    fn bar_number_single_beat() {
        let data = make_beat_grid_data(&[(1, 12800, 0)]);
        let grid = BeatGrid::from_bytes(&data).unwrap();
        assert_eq!(grid.bar_number(0), Some(1));
    }
}
