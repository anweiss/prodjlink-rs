/// The type of a cue entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueType {
    /// A memory point (not assigned to a hot cue button).
    MemoryPoint,
    /// A hot cue (assigned to button 1-8).
    HotCue,
    /// A loop (has start and end positions).
    Loop,
}

/// Color information for a cue point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CueColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl CueColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self {
            red: r,
            green: g,
            blue: b,
        }
    }
}

/// A single cue point entry.
#[derive(Debug, Clone)]
pub struct CueEntry {
    /// What type of cue this is.
    pub cue_type: CueType,
    /// Hot cue button number (1-8), or None if this is a memory point.
    pub hot_cue_number: Option<u8>,
    /// Position in milliseconds from the start of the track.
    pub position_ms: u32,
    /// End position for loops (None for non-loop cues).
    pub loop_end_ms: Option<u32>,
    /// User comment (from extended cue list).
    pub comment: Option<String>,
    /// Color assigned to this cue (from extended cue list or Nexus colors).
    pub color: Option<CueColor>,
    /// Color ID for the cue point (if available).
    pub color_id: Option<u8>,
    /// Embedded RGB color (hot cues on Nxs2+).
    pub color_rgb: Option<(u8, u8, u8)>,
}

impl CueEntry {
    /// Whether this entry represents a loop.
    pub fn is_loop(&self) -> bool {
        self.cue_type == CueType::Loop
    }

    /// Whether this is a hot cue (assigned to a button).
    pub fn is_hot_cue(&self) -> bool {
        self.cue_type == CueType::HotCue
    }

    /// Whether this is a plain memory point.
    pub fn is_memory_point(&self) -> bool {
        self.cue_type == CueType::MemoryPoint
    }

    /// The cue position in milliseconds.
    pub fn time_ms(&self) -> Option<u64> {
        Some(self.position_ms as u64)
    }

    /// The loop end position in milliseconds (for loop cues).
    ///
    /// Returns `None` for non-loop cues.
    pub fn loop_time_ms(&self) -> Option<u64> {
        self.loop_end_ms.map(|ms| ms as u64)
    }
}

/// A complete cue list for a track.
#[derive(Debug, Clone)]
pub struct CueList {
    /// All cue entries in order.
    pub entries: Vec<CueEntry>,
}

impl CueList {
    pub fn new(entries: Vec<CueEntry>) -> Self {
        Self { entries }
    }

    /// Get all hot cues.
    pub fn hot_cues(&self) -> Vec<&CueEntry> {
        self.entries.iter().filter(|e| e.is_hot_cue()).collect()
    }

    /// Get all memory points.
    pub fn memory_points(&self) -> Vec<&CueEntry> {
        self.entries.iter().filter(|e| e.is_memory_point()).collect()
    }

    /// Get all loops.
    pub fn loops(&self) -> Vec<&CueEntry> {
        self.entries.iter().filter(|e| e.is_loop()).collect()
    }

    /// Get a specific hot cue by button number (1-8).
    pub fn hot_cue(&self, number: u8) -> Option<&CueEntry> {
        self.entries
            .iter()
            .find(|e| e.hot_cue_number == Some(number))
    }

    /// Total number of cue entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of hot cues only.
    pub fn hot_cue_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_hot_cue()).count()
    }

    /// Count of memory points only.
    pub fn memory_point_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_memory_point()).count()
    }

    /// Parse a cue list from dbserver menu item responses.
    pub fn from_menu_items(items: &[crate::dbserver::message::Message]) -> Self {
        let mut entries = Vec::new();

        for item in items {
            let hot_cue_num = item.args.get(1).and_then(|f| f.as_number().ok());
            let position = item
                .args
                .get(3)
                .and_then(|f| f.as_number().ok())
                .unwrap_or(0);
            let loop_end = item.args.get(4).and_then(|f| f.as_number().ok());
            let comment = item
                .args
                .get(5)
                .and_then(|f| f.as_string().ok())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let is_loop = loop_end.is_some() && loop_end != Some(0);
            let is_hot_cue = hot_cue_num.is_some() && hot_cue_num != Some(0);

            let cue_type = if is_loop {
                CueType::Loop
            } else if is_hot_cue {
                CueType::HotCue
            } else {
                CueType::MemoryPoint
            };

            entries.push(CueEntry {
                cue_type,
                hot_cue_number: if is_hot_cue {
                    hot_cue_num.map(|n| n as u8)
                } else {
                    None
                },
                position_ms: position,
                loop_end_ms: if is_loop { loop_end } else { None },
                comment,
                color: None,
                color_id: None,
                color_rgb: None,
            });
        }

        Self::new(entries)
    }

    /// Find the cue entry just before the given position.
    pub fn entry_before(&self, position_ms: u32) -> Option<&CueEntry> {
        self.entries
            .iter()
            .filter(|e| e.position_ms < position_ms)
            .max_by_key(|e| e.position_ms)
    }

    /// Find the cue entry at or just after the given position.
    pub fn entry_after(&self, position_ms: u32) -> Option<&CueEntry> {
        self.entries
            .iter()
            .filter(|e| e.position_ms >= position_ms)
            .min_by_key(|e| e.position_ms)
    }
}

/// Standard rekordbox color palette for hot cues.
///
/// Maps a color code (used as index) to its `(R, G, B)` value.
/// Index 0 represents "no color" (`None` when looked up via
/// [`rekordbox_color`]).  Codes 0x01–0x3E correspond to the 4×4 color
/// grids available in the rekordbox hot-cue configuration UI.
///
/// Values are transcribed from the `findRekordboxColor` method in
/// Deep Symmetry's beat-link `CueList.java`.
pub const REKORDBOX_COLORS: &[(u8, u8, u8)] = &[
    (0x00, 0x00, 0x00), // 0x00 — no color
    (0x30, 0x5a, 0xff), // 0x01
    (0x50, 0x73, 0xff), // 0x02
    (0x50, 0x8c, 0xff), // 0x03
    (0x50, 0xa0, 0xff), // 0x04
    (0x50, 0xb4, 0xff), // 0x05
    (0x50, 0xb0, 0xf2), // 0x06
    (0x50, 0xae, 0xe8), // 0x07
    (0x45, 0xac, 0xdb), // 0x08
    (0x00, 0xe0, 0xff), // 0x09
    (0x19, 0xda, 0xf0), // 0x0a
    (0x32, 0xd2, 0xe6), // 0x0b
    (0x21, 0xb4, 0xb9), // 0x0c
    (0x20, 0xaa, 0xa0), // 0x0d
    (0x1f, 0xa3, 0x92), // 0x0e
    (0x19, 0xa0, 0x8c), // 0x0f
    (0x14, 0xa5, 0x84), // 0x10
    (0x14, 0xaa, 0x7d), // 0x11
    (0x10, 0xb1, 0x76), // 0x12
    (0x30, 0xd2, 0x6e), // 0x13
    (0x37, 0xde, 0x5a), // 0x14
    (0x3c, 0xeb, 0x50), // 0x15
    (0x28, 0xe2, 0x14), // 0x16
    (0x7d, 0xc1, 0x3d), // 0x17
    (0x8c, 0xc8, 0x32), // 0x18
    (0x9b, 0xd7, 0x23), // 0x19
    (0xa5, 0xe1, 0x16), // 0x1a
    (0xa5, 0xdc, 0x0a), // 0x1b
    (0xaa, 0xd2, 0x08), // 0x1c
    (0xb4, 0xc8, 0x05), // 0x1d
    (0xb4, 0xbe, 0x04), // 0x1e
    (0xba, 0xb4, 0x04), // 0x1f
    (0xc3, 0xaf, 0x04), // 0x20
    (0xe1, 0xaa, 0x00), // 0x21
    (0xff, 0xa0, 0x00), // 0x22
    (0xff, 0x96, 0x00), // 0x23
    (0xff, 0x8c, 0x00), // 0x24
    (0xff, 0x75, 0x00), // 0x25
    (0xe0, 0x64, 0x1b), // 0x26
    (0xe0, 0x46, 0x1e), // 0x27
    (0xe0, 0x30, 0x1e), // 0x28
    (0xe0, 0x28, 0x23), // 0x29
    (0xe6, 0x28, 0x28), // 0x2a
    (0xff, 0x37, 0x6f), // 0x2b
    (0xff, 0x2d, 0x6f), // 0x2c
    (0xff, 0x12, 0x7b), // 0x2d
    (0xf5, 0x1e, 0x8c), // 0x2e
    (0xeb, 0x2d, 0xa0), // 0x2f
    (0xe6, 0x37, 0xb4), // 0x30
    (0xde, 0x44, 0xcf), // 0x31
    (0xde, 0x44, 0x8d), // 0x32
    (0xe6, 0x30, 0xb4), // 0x33
    (0xe6, 0x19, 0xdc), // 0x34
    (0xe6, 0x00, 0xff), // 0x35
    (0xdc, 0x00, 0xff), // 0x36
    (0xcc, 0x00, 0xff), // 0x37
    (0xb4, 0x32, 0xff), // 0x38
    (0xb9, 0x3c, 0xff), // 0x39
    (0xc5, 0x42, 0xff), // 0x3a
    (0xaa, 0x5a, 0xff), // 0x3b
    (0xaa, 0x72, 0xff), // 0x3c
    (0x82, 0x72, 0xff), // 0x3d
    (0x64, 0x73, 0xff), // 0x3e
];

/// Look up the RGB color for a rekordbox color code.
///
/// Returns `None` for code 0 (no color) or unrecognised codes.
pub fn rekordbox_color(color_code: u8) -> Option<(u8, u8, u8)> {
    if color_code == 0 {
        return None;
    }
    REKORDBOX_COLORS.get(color_code as usize).copied()
}

/// Convert a position in half-frame units to milliseconds.
///
/// Half-frames are 1/150 of a second (75 frames/s × 2 = 150 half-frames/s).
/// Formula: `ms = half_frames * 100 / 15` (equivalent to `* 1000 / 150`).
pub fn half_frames_to_ms(half_frames: u32) -> u32 {
    (half_frames as u64 * 100 / 15) as u32
}

/// Read a little-endian `u32` from `data` at the given `offset`.
fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset + 4;
    if end > data.len() {
        return Err(format!(
            "not enough data: need 4 bytes at offset {offset}, have {}",
            data.len()
        ));
    }
    let bytes: [u8; 4] = data[offset..end].try_into().unwrap();
    Ok(u32::from_le_bytes(bytes))
}

/// Read a little-endian `u16` from `data` at the given `offset`.
fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, String> {
    let end = offset + 2;
    if end > data.len() {
        return Err(format!(
            "not enough data: need 2 bytes at offset {offset}, have {}",
            data.len()
        ));
    }
    let bytes: [u8; 2] = data[offset..end].try_into().unwrap();
    Ok(u16::from_le_bytes(bytes))
}

/// Safely read a single byte, returning 0 if the index is out of bounds.
fn safely_fetch_byte(data: &[u8], index: usize) -> u8 {
    if index < data.len() {
        data[index]
    } else {
        0
    }
}

/// Parse a Nexus-format binary cue list (36 bytes per entry).
///
/// The Nexus binary cue format uses fixed 36-byte records with positions
/// stored in half-frame units (little-endian). Layout per entry:
///
/// | Offset | Size | Field |
/// |--------|------|-------|
/// | 0 | 1 | Loop flag (non-zero → loop) |
/// | 1 | 1 | Cue flag (non-zero → entry is populated) |
/// | 2 | 1 | Hot cue number (0 = memory point, 1+ = hot cue) |
/// | 12–15 | 4 | Cue position in half-frames (LE u32) |
/// | 16–19 | 4 | Loop end position in half-frames (LE u32) |
pub fn parse_nexus_entries(data: &[u8]) -> Result<Vec<CueEntry>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let entry_count = data.len() / 36;
    let mut entries = Vec::with_capacity(entry_count);

    for i in 0..entry_count {
        let offset = i * 36;

        let is_loop_flag = data[offset];
        let cue_flag = data[offset + 1];
        let hot_cue_number = data[offset + 2];

        // Skip empty entries
        if cue_flag == 0 && hot_cue_number == 0 {
            continue;
        }

        let position_hf = read_u32_le(data, offset + 12)?;
        let position_ms = half_frames_to_ms(position_hf);

        let is_loop = is_loop_flag != 0;
        let is_hot_cue = hot_cue_number != 0;

        let loop_end_ms = if is_loop {
            let end_hf = read_u32_le(data, offset + 16)?;
            Some(half_frames_to_ms(end_hf))
        } else {
            None
        };

        let cue_type = if is_loop {
            CueType::Loop
        } else if is_hot_cue {
            CueType::HotCue
        } else {
            CueType::MemoryPoint
        };

        entries.push(CueEntry {
            cue_type,
            hot_cue_number: if is_hot_cue { Some(hot_cue_number) } else { None },
            position_ms,
            loop_end_ms,
            comment: None,
            color: None,
            color_id: None,
            color_rgb: None,
        });
    }

    Ok(entries)
}

/// Parse an Nxs2-format binary cue list (variable-length entries with
/// comments and colors).
///
/// Each entry begins with a 4-byte little-endian size field, followed by:
///
/// | Offset | Size | Field |
/// |--------|------|-------|
/// | 0–3 | 4 | Entry size in bytes (LE u32) |
/// | 4 | 1 | Hot cue number (0 = memory point) |
/// | 6 | 1 | Cue flag (1 = cue, 2 = loop) |
/// | 12–15 | 4 | Position in milliseconds (LE u32) |
/// | 16–19 | 4 | Loop end in milliseconds (LE u32) |
/// | 0x22 | 1 | Color ID (memory points only) |
/// | 0x48–0x49 | 2 | Comment size (LE u16, present when entry_size > 0x49) |
/// | 0x4a.. | var | UTF-16LE comment string |
/// | cs+0x4e | 1 | Color code (hot cues; cs = comment_size) |
/// | cs+0x4f | 3 | Embedded RGB color (hot cues) |
pub fn parse_nxs2_entries(data: &[u8]) -> Result<Vec<CueEntry>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    let mut offset = 0;

    while offset + 4 <= data.len() {
        let entry_size = read_u32_le(data, offset)? as usize;
        if entry_size == 0 || offset + entry_size > data.len() {
            break;
        }

        // Need at least enough bytes for the basic fields (through offset+19).
        if entry_size < 20 {
            offset += entry_size;
            continue;
        }

        let hot_cue_number = data[offset + 4];
        let cue_flag = data[offset + 6];

        // Skip empty entries
        if cue_flag == 0 && hot_cue_number == 0 {
            offset += entry_size;
            continue;
        }

        // Position stored in milliseconds (LE u32).
        let position_ms = read_u32_le(data, offset + 12)?;

        // --- Comment ---
        let mut comment = None;
        let mut comment_size: usize = 0;
        if entry_size > 0x49 {
            comment_size = read_u16_le(data, offset + 0x48)? as usize;
        }
        if comment_size > 2 {
            let comment_start = offset + 0x4a;
            let comment_byte_len = comment_size - 2; // exclude UTF-16 null terminator
            if comment_start + comment_byte_len <= data.len() {
                let u16_count = comment_byte_len / 2;
                let mut u16s = Vec::with_capacity(u16_count);
                for j in 0..u16_count {
                    let lo = data[comment_start + j * 2] as u16;
                    let hi = data[comment_start + j * 2 + 1] as u16;
                    u16s.push((hi << 8) | lo);
                }
                let decoded =
                    String::from_utf16(&u16s).map_err(|e| format!("invalid UTF-16: {e}"))?;
                let trimmed = decoded.trim().to_string();
                if !trimmed.is_empty() {
                    comment = Some(trimmed);
                }
            }
        }

        let is_loop = cue_flag == 2;
        let is_hot_cue = hot_cue_number != 0;

        let loop_end_ms = if is_loop {
            Some(read_u32_le(data, offset + 16)?)
        } else {
            None
        };

        // --- Color ---
        let (color_id, color_rgb, color) = if is_hot_cue {
            let color_base = offset + comment_size + 0x4e;
            let color_code = safely_fetch_byte(data, color_base);
            let red = safely_fetch_byte(data, color_base + 1);
            let green = safely_fetch_byte(data, color_base + 2);
            let blue = safely_fetch_byte(data, color_base + 3);

            let rgb = if red == 0 && green == 0 && blue == 0 {
                None
            } else {
                Some((red, green, blue))
            };
            let clr = rgb.map(|(r, g, b)| CueColor::new(r, g, b));
            let cid = if color_code != 0 {
                Some(color_code)
            } else {
                None
            };

            (cid, rgb, clr)
        } else {
            // Memory point / loop: color_id at offset + 0x22.
            let cid = if offset + 0x22 < data.len() {
                let c = data[offset + 0x22];
                if c != 0 { Some(c) } else { None }
            } else {
                None
            };
            (cid, None, None)
        };

        let cue_type = if is_loop {
            CueType::Loop
        } else if is_hot_cue {
            CueType::HotCue
        } else {
            CueType::MemoryPoint
        };

        entries.push(CueEntry {
            cue_type,
            hot_cue_number: if is_hot_cue {
                Some(hot_cue_number)
            } else {
                None
            },
            position_ms,
            loop_end_ms,
            comment,
            color,
            color_id,
            color_rgb,
        });

        offset += entry_size;
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbserver::message::{Message, MessageType};
    use crate::dbserver::Field;

    fn make_memory_point(position: u32) -> CueEntry {
        CueEntry {
            cue_type: CueType::MemoryPoint,
            hot_cue_number: None,
            position_ms: position,
            loop_end_ms: None,
            comment: None,
            color: None,
            color_id: None,
            color_rgb: None,
        }
    }

    fn make_hot_cue(number: u8, position: u32) -> CueEntry {
        CueEntry {
            cue_type: CueType::HotCue,
            hot_cue_number: Some(number),
            position_ms: position,
            loop_end_ms: None,
            comment: None,
            color: None,
            color_id: None,
            color_rgb: None,
        }
    }

    fn make_loop(position: u32, end: u32) -> CueEntry {
        CueEntry {
            cue_type: CueType::Loop,
            hot_cue_number: None,
            position_ms: position,
            loop_end_ms: Some(end),
            comment: None,
            color: None,
            color_id: None,
            color_rgb: None,
        }
    }

    // --- CueEntry type predicates ---

    #[test]
    fn memory_point_predicates() {
        let entry = make_memory_point(1000);
        assert!(entry.is_memory_point());
        assert!(!entry.is_hot_cue());
        assert!(!entry.is_loop());
    }

    #[test]
    fn hot_cue_predicates() {
        let entry = make_hot_cue(1, 2000);
        assert!(entry.is_hot_cue());
        assert!(!entry.is_memory_point());
        assert!(!entry.is_loop());
    }

    #[test]
    fn loop_predicates() {
        let entry = make_loop(3000, 4000);
        assert!(entry.is_loop());
        assert!(!entry.is_memory_point());
        assert!(!entry.is_hot_cue());
    }

    // --- CueColor ---

    #[test]
    fn cue_color_new() {
        let color = CueColor::new(255, 128, 0);
        assert_eq!(color.red, 255);
        assert_eq!(color.green, 128);
        assert_eq!(color.blue, 0);
    }

    // --- CueList filtering ---

    fn sample_cue_list() -> CueList {
        CueList::new(vec![
            make_memory_point(0),
            make_hot_cue(1, 1000),
            make_hot_cue(2, 2000),
            make_loop(3000, 4000),
            make_memory_point(5000),
            make_hot_cue(3, 6000),
        ])
    }

    #[test]
    fn hot_cues_filter() {
        let list = sample_cue_list();
        let hot = list.hot_cues();
        assert_eq!(hot.len(), 3);
        assert!(hot.iter().all(|e| e.is_hot_cue()));
    }

    #[test]
    fn memory_points_filter() {
        let list = sample_cue_list();
        let mps = list.memory_points();
        assert_eq!(mps.len(), 2);
        assert!(mps.iter().all(|e| e.is_memory_point()));
    }

    #[test]
    fn loops_filter() {
        let list = sample_cue_list();
        let loops = list.loops();
        assert_eq!(loops.len(), 1);
        assert!(loops.iter().all(|e| e.is_loop()));
        assert_eq!(loops[0].loop_end_ms, Some(4000));
    }

    #[test]
    fn hot_cue_lookup_found() {
        let list = sample_cue_list();
        let cue = list.hot_cue(2).unwrap();
        assert_eq!(cue.position_ms, 2000);
        assert_eq!(cue.hot_cue_number, Some(2));
    }

    #[test]
    fn hot_cue_lookup_not_found() {
        let list = sample_cue_list();
        assert!(list.hot_cue(8).is_none());
    }

    #[test]
    fn len_and_is_empty() {
        let list = sample_cue_list();
        assert_eq!(list.len(), 6);
        assert!(!list.is_empty());
    }

    #[test]
    fn empty_cue_list() {
        let list = CueList::new(vec![]);
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(list.hot_cues().is_empty());
        assert!(list.memory_points().is_empty());
        assert!(list.loops().is_empty());
        assert!(list.hot_cue(1).is_none());
    }

    // --- hot_cue_count / memory_point_count ---

    #[test]
    fn hot_cue_count() {
        let list = sample_cue_list();
        assert_eq!(list.hot_cue_count(), 3);
    }

    #[test]
    fn memory_point_count() {
        let list = sample_cue_list();
        assert_eq!(list.memory_point_count(), 2);
    }

    #[test]
    fn counts_on_empty_list() {
        let list = CueList::new(vec![]);
        assert_eq!(list.hot_cue_count(), 0);
        assert_eq!(list.memory_point_count(), 0);
    }

    // --- time_ms / loop_time_ms ---

    #[test]
    fn time_ms_accessor() {
        let entry = make_memory_point(4200);
        assert_eq!(entry.time_ms(), Some(4200));
    }

    #[test]
    fn loop_time_ms_for_loop() {
        let entry = make_loop(1000, 2000);
        assert_eq!(entry.loop_time_ms(), Some(2000));
    }

    #[test]
    fn loop_time_ms_none_for_non_loop() {
        let entry = make_hot_cue(1, 500);
        assert_eq!(entry.loop_time_ms(), None);
    }

    // --- from_menu_items ---

    fn mock_menu_item(args: Vec<Field>) -> Message {
        Message::new(1, MessageType::MenuItem, args)
    }

    #[test]
    fn from_menu_items_memory_point() {
        // args[1] = 0 (no hot cue), args[3] = position, args[4] = 0 (no loop)
        let msg = mock_menu_item(vec![
            Field::number(0),    // arg 0: unused
            Field::number(0),    // arg 1: hot cue number (0 = none)
            Field::number(0),    // arg 2: unused
            Field::number(1500), // arg 3: position ms
            Field::number(0),    // arg 4: loop end (0 = none)
        ]);
        let list = CueList::from_menu_items(&[msg]);
        assert_eq!(list.len(), 1);
        let entry = &list.entries[0];
        assert!(entry.is_memory_point());
        assert_eq!(entry.position_ms, 1500);
        assert!(entry.hot_cue_number.is_none());
        assert!(entry.loop_end_ms.is_none());
    }

    #[test]
    fn from_menu_items_hot_cue() {
        let msg = mock_menu_item(vec![
            Field::number(0),    // arg 0
            Field::number(3),    // arg 1: hot cue button 3
            Field::number(0),    // arg 2
            Field::number(5000), // arg 3: position ms
            Field::number(0),    // arg 4: no loop
        ]);
        let list = CueList::from_menu_items(&[msg]);
        assert_eq!(list.len(), 1);
        let entry = &list.entries[0];
        assert!(entry.is_hot_cue());
        assert_eq!(entry.hot_cue_number, Some(3));
        assert_eq!(entry.position_ms, 5000);
    }

    #[test]
    fn from_menu_items_loop() {
        let msg = mock_menu_item(vec![
            Field::number(0),     // arg 0
            Field::number(0),     // arg 1: no hot cue
            Field::number(0),     // arg 2
            Field::number(10000), // arg 3: position ms
            Field::number(12000), // arg 4: loop end ms
        ]);
        let list = CueList::from_menu_items(&[msg]);
        assert_eq!(list.len(), 1);
        let entry = &list.entries[0];
        assert!(entry.is_loop());
        assert_eq!(entry.position_ms, 10000);
        assert_eq!(entry.loop_end_ms, Some(12000));
    }

    #[test]
    fn from_menu_items_with_comment() {
        let msg = mock_menu_item(vec![
            Field::number(0),        // arg 0
            Field::number(1),        // arg 1: hot cue 1
            Field::number(0),        // arg 2
            Field::number(8000),     // arg 3: position
            Field::number(0),        // arg 4: no loop
            Field::string("Drop!"),  // arg 5: comment
        ]);
        let list = CueList::from_menu_items(&[msg]);
        assert_eq!(list.len(), 1);
        let entry = &list.entries[0];
        assert_eq!(entry.comment.as_deref(), Some("Drop!"));
    }

    #[test]
    fn from_menu_items_empty_comment_ignored() {
        let msg = mock_menu_item(vec![
            Field::number(0),
            Field::number(0),
            Field::number(0),
            Field::number(500),
            Field::number(0),
            Field::string(""), // empty comment should be None
        ]);
        let list = CueList::from_menu_items(&[msg]);
        assert!(list.entries[0].comment.is_none());
    }

    #[test]
    fn from_menu_items_multiple() {
        let items = vec![
            mock_menu_item(vec![
                Field::number(0),
                Field::number(0),
                Field::number(0),
                Field::number(0),
                Field::number(0),
            ]),
            mock_menu_item(vec![
                Field::number(0),
                Field::number(1),
                Field::number(0),
                Field::number(2000),
                Field::number(0),
            ]),
            mock_menu_item(vec![
                Field::number(0),
                Field::number(0),
                Field::number(0),
                Field::number(4000),
                Field::number(6000),
            ]),
        ];
        let list = CueList::from_menu_items(&items);
        assert_eq!(list.len(), 3);
        assert_eq!(list.memory_points().len(), 1);
        assert_eq!(list.hot_cues().len(), 1);
        assert_eq!(list.loops().len(), 1);
    }

    #[test]
    fn from_menu_items_empty() {
        let list = CueList::from_menu_items(&[]);
        assert!(list.is_empty());
    }

    #[test]
    fn from_menu_items_sparse_args() {
        // Message with fewer args than expected — should still parse gracefully
        let msg = mock_menu_item(vec![Field::number(0)]);
        let list = CueList::from_menu_items(&[msg]);
        assert_eq!(list.len(), 1);
        let entry = &list.entries[0];
        assert!(entry.is_memory_point());
        assert_eq!(entry.position_ms, 0);
    }

    // --- color fields ---

    #[test]
    fn cue_entry_color_fields_default_none() {
        let entry = make_memory_point(100);
        assert!(entry.color_id.is_none());
        assert!(entry.color_rgb.is_none());
    }

    #[test]
    fn cue_entry_color_fields_populated() {
        let entry = CueEntry {
            cue_type: CueType::HotCue,
            hot_cue_number: Some(1),
            position_ms: 500,
            loop_end_ms: None,
            comment: None,
            color: Some(CueColor::new(255, 0, 0)),
            color_id: Some(3),
            color_rgb: Some((255, 0, 0)),
        };
        assert_eq!(entry.color_id, Some(3));
        assert_eq!(entry.color_rgb, Some((255, 0, 0)));
    }

    // --- entry_before / entry_after ---

    #[test]
    fn entry_before_finds_closest() {
        let list = sample_cue_list();
        let entry = list.entry_before(2500).unwrap();
        assert_eq!(entry.position_ms, 2000);
    }

    #[test]
    fn entry_before_none_when_at_start() {
        let list = sample_cue_list();
        assert!(list.entry_before(0).is_none());
    }

    #[test]
    fn entry_after_finds_closest() {
        let list = sample_cue_list();
        let entry = list.entry_after(2500).unwrap();
        assert_eq!(entry.position_ms, 3000);
    }

    #[test]
    fn entry_after_exact_match() {
        let list = sample_cue_list();
        let entry = list.entry_after(1000).unwrap();
        assert_eq!(entry.position_ms, 1000);
    }

    #[test]
    fn entry_after_none_past_end() {
        let list = sample_cue_list();
        assert!(list.entry_after(10000).is_none());
    }

    #[test]
    fn entry_before_and_after_on_empty() {
        let list = CueList::new(vec![]);
        assert!(list.entry_before(500).is_none());
        assert!(list.entry_after(500).is_none());
    }

    // --- half_frames_to_ms ---

    #[test]
    fn half_frames_to_ms_zero() {
        assert_eq!(half_frames_to_ms(0), 0);
    }

    #[test]
    fn half_frames_to_ms_one_second() {
        // 150 half-frames = 1000 ms
        assert_eq!(half_frames_to_ms(150), 1000);
    }

    #[test]
    fn half_frames_to_ms_fractional() {
        // 1 half-frame = 100/15 = 6 ms (integer division)
        assert_eq!(half_frames_to_ms(1), 6);
        // 3 half-frames = 300/15 = 20 ms
        assert_eq!(half_frames_to_ms(3), 20);
    }

    // --- Nexus binary format parsing ---

    /// Build a 36-byte Nexus entry. Positions are in half-frames, little-endian.
    fn make_nexus_entry(
        is_loop: u8,
        cue_flag: u8,
        hot_cue: u8,
        position_hf: u32,
        loop_end_hf: u32,
    ) -> [u8; 36] {
        let mut buf = [0u8; 36];
        buf[0] = is_loop;
        buf[1] = cue_flag;
        buf[2] = hot_cue;
        buf[12..16].copy_from_slice(&position_hf.to_le_bytes());
        buf[16..20].copy_from_slice(&loop_end_hf.to_le_bytes());
        buf
    }

    #[test]
    fn parse_nexus_empty_data() {
        let entries = parse_nexus_entries(&[]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_nexus_memory_point() {
        // cue_flag=1 (populated), hot_cue=0 (memory point), is_loop=0
        // position = 150 half-frames = 1000 ms
        let entry = make_nexus_entry(0, 1, 0, 150, 0);
        let entries = parse_nexus_entries(&entry).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_memory_point());
        assert_eq!(entries[0].position_ms, 1000);
        assert!(entries[0].hot_cue_number.is_none());
        assert!(entries[0].loop_end_ms.is_none());
        assert!(entries[0].comment.is_none());
        assert!(entries[0].color.is_none());
    }

    #[test]
    fn parse_nexus_hot_cue() {
        // hot_cue=3 (hot cue C), position = 300 hf = 2000 ms
        let entry = make_nexus_entry(0, 0, 3, 300, 0);
        let entries = parse_nexus_entries(&entry).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hot_cue());
        assert_eq!(entries[0].hot_cue_number, Some(3));
        assert_eq!(entries[0].position_ms, 2000);
        assert!(entries[0].loop_end_ms.is_none());
    }

    #[test]
    fn parse_nexus_loop() {
        // is_loop=1, cue_flag=1, hot_cue=0 (memory loop)
        // start = 150 hf (1000 ms), end = 300 hf (2000 ms)
        let entry = make_nexus_entry(1, 1, 0, 150, 300);
        let entries = parse_nexus_entries(&entry).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_loop());
        assert_eq!(entries[0].position_ms, 1000);
        assert_eq!(entries[0].loop_end_ms, Some(2000));
        assert!(entries[0].hot_cue_number.is_none());
    }

    #[test]
    fn parse_nexus_hot_cue_loop() {
        // is_loop=1, cue_flag=0, hot_cue=2 (hot cue B loop)
        let entry = make_nexus_entry(1, 0, 2, 450, 600);
        let entries = parse_nexus_entries(&entry).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_loop());
        assert_eq!(entries[0].hot_cue_number, Some(2));
        assert_eq!(entries[0].position_ms, half_frames_to_ms(450));
        assert_eq!(entries[0].loop_end_ms, Some(half_frames_to_ms(600)));
    }

    #[test]
    fn parse_nexus_skips_empty_entries() {
        // cue_flag=0, hot_cue=0 → empty, should be skipped
        let entry = make_nexus_entry(0, 0, 0, 150, 0);
        let entries = parse_nexus_entries(&entry).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_nexus_multiple_entries() {
        let e1 = make_nexus_entry(0, 1, 0, 150, 0); // memory point
        let e2 = make_nexus_entry(0, 0, 1, 300, 0); // hot cue A
        let e3 = make_nexus_entry(0, 0, 0, 0, 0); // empty (skipped)
        let e4 = make_nexus_entry(1, 1, 0, 450, 600); // loop

        let mut data = Vec::new();
        data.extend_from_slice(&e1);
        data.extend_from_slice(&e2);
        data.extend_from_slice(&e3);
        data.extend_from_slice(&e4);

        let entries = parse_nexus_entries(&data).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_memory_point());
        assert!(entries[1].is_hot_cue());
        assert_eq!(entries[1].hot_cue_number, Some(1));
        assert!(entries[2].is_loop());
        assert_eq!(entries[2].loop_end_ms, Some(half_frames_to_ms(600)));
    }

    #[test]
    fn parse_nexus_ignores_trailing_bytes() {
        // 36 bytes + 10 trailing bytes (less than another entry)
        let entry = make_nexus_entry(0, 1, 0, 150, 0);
        let mut data = entry.to_vec();
        data.extend_from_slice(&[0u8; 10]);
        let entries = parse_nexus_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
    }

    // --- Nxs2 binary format parsing ---

    /// Build a minimal Nxs2 entry. Positions are in milliseconds, little-endian.
    /// `extra` is appended after the 0x48-byte fixed region.
    fn make_nxs2_entry(
        hot_cue: u8,
        cue_flag: u8,
        position_ms: u32,
        loop_end_ms: u32,
        color_id: u8,
        comment: Option<&str>,
        color_code: u8,
        rgb: Option<(u8, u8, u8)>,
    ) -> Vec<u8> {
        // Fixed region: at least 0x4a bytes to hold comment_size field,
        // plus comment data, plus color data for hot cues.
        let comment_bytes: Vec<u8> = match comment {
            Some(s) => {
                let utf16: Vec<u16> = s.encode_utf16().collect();
                let mut bytes = Vec::new();
                for ch in &utf16 {
                    bytes.push(*ch as u8); // lo
                    bytes.push((*ch >> 8) as u8); // hi
                }
                bytes
            }
            None => Vec::new(),
        };
        // comment_size includes the 2-byte null terminator
        let comment_size = if comment.is_some() {
            (comment_bytes.len() + 2) as u16
        } else {
            0u16
        };

        // The color info for hot cues lives at `comment_size + 0x4e` (4 bytes),
        // so the entry must be at least `comment_size + 0x52` bytes.
        // Without color (memory points), 0x4a + comment data is enough.
        let cs = comment_size as usize;
        let entry_size = if hot_cue != 0 {
            cs + 0x52 // room for color at cs+0x4e..cs+0x51
        } else {
            0x4a + cs // fixed header + comment data
        };

        let mut buf = vec![0u8; entry_size];
        buf[0..4].copy_from_slice(&(entry_size as u32).to_le_bytes());
        buf[4] = hot_cue;
        buf[6] = cue_flag;
        buf[12..16].copy_from_slice(&position_ms.to_le_bytes());
        buf[16..20].copy_from_slice(&loop_end_ms.to_le_bytes());
        buf[0x22] = color_id;

        // Comment size field at 0x48
        buf[0x48..0x4a].copy_from_slice(&comment_size.to_le_bytes());

        // Comment data at 0x4a
        if !comment_bytes.is_empty() {
            buf[0x4a..0x4a + comment_bytes.len()].copy_from_slice(&comment_bytes);
            // Null terminator (2 zero bytes) already present from vec initialization
        }

        // Color info for hot cues: at offset + comment_size + 0x4e
        if hot_cue != 0 {
            let color_base = comment_size as usize + 0x4e;
            if color_base + 3 < buf.len() {
                buf[color_base] = color_code;
                if let Some((r, g, b)) = rgb {
                    buf[color_base + 1] = r;
                    buf[color_base + 2] = g;
                    buf[color_base + 3] = b;
                }
            }
        }

        buf
    }

    #[test]
    fn parse_nxs2_empty_data() {
        let entries = parse_nxs2_entries(&[]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_nxs2_memory_point() {
        let data = make_nxs2_entry(0, 1, 5000, 0, 0, None, 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_memory_point());
        assert_eq!(entries[0].position_ms, 5000);
        assert!(entries[0].loop_end_ms.is_none());
        assert!(entries[0].comment.is_none());
    }

    #[test]
    fn parse_nxs2_memory_point_with_color_id() {
        let data = make_nxs2_entry(0, 1, 3000, 0, 5, None, 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].color_id, Some(5));
    }

    #[test]
    fn parse_nxs2_hot_cue() {
        let data = make_nxs2_entry(1, 1, 8000, 0, 0, None, 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hot_cue());
        assert_eq!(entries[0].hot_cue_number, Some(1));
        assert_eq!(entries[0].position_ms, 8000);
    }

    #[test]
    fn parse_nxs2_loop() {
        let data = make_nxs2_entry(0, 2, 10000, 12000, 0, None, 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_loop());
        assert_eq!(entries[0].position_ms, 10000);
        assert_eq!(entries[0].loop_end_ms, Some(12000));
    }

    #[test]
    fn parse_nxs2_with_comment() {
        let data = make_nxs2_entry(0, 1, 1500, 0, 0, Some("Drop!"), 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].comment.as_deref(), Some("Drop!"));
    }

    #[test]
    fn parse_nxs2_hot_cue_with_rgb() {
        let data = make_nxs2_entry(2, 1, 4000, 0, 0, None, 0x22, Some((255, 160, 0)));
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_hot_cue());
        assert_eq!(entries[0].hot_cue_number, Some(2));
        assert_eq!(entries[0].color_id, Some(0x22));
        assert_eq!(entries[0].color_rgb, Some((255, 160, 0)));
        let c = entries[0].color.as_ref().unwrap();
        assert_eq!(c.red, 255);
        assert_eq!(c.green, 160);
        assert_eq!(c.blue, 0);
    }

    #[test]
    fn parse_nxs2_hot_cue_zero_rgb_is_none() {
        let data = make_nxs2_entry(3, 1, 6000, 0, 0, None, 0x01, Some((0, 0, 0)));
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        // (0,0,0) embedded color → None
        assert!(entries[0].color_rgb.is_none());
        assert!(entries[0].color.is_none());
        // But color_id is still set from color_code
        assert_eq!(entries[0].color_id, Some(0x01));
    }

    #[test]
    fn parse_nxs2_with_comment_and_rgb() {
        let data = make_nxs2_entry(1, 1, 2000, 0, 0, Some("Verse"), 0x0a, Some((25, 218, 240)));
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].comment.as_deref(), Some("Verse"));
        assert_eq!(entries[0].color_rgb, Some((25, 218, 240)));
        assert_eq!(entries[0].color_id, Some(0x0a));
    }

    #[test]
    fn parse_nxs2_skips_empty_entries() {
        let data = make_nxs2_entry(0, 0, 1000, 0, 0, None, 0, None);
        let entries = parse_nxs2_entries(&data).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_nxs2_multiple_entries() {
        let e1 = make_nxs2_entry(0, 1, 1000, 0, 0, None, 0, None);
        let e2 = make_nxs2_entry(1, 1, 5000, 0, 0, None, 0, None);
        let e3 = make_nxs2_entry(0, 2, 8000, 10000, 0, None, 0, None);

        let mut data = Vec::new();
        data.extend_from_slice(&e1);
        data.extend_from_slice(&e2);
        data.extend_from_slice(&e3);

        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_memory_point());
        assert!(entries[1].is_hot_cue());
        assert!(entries[2].is_loop());
    }

    #[test]
    fn parse_nxs2_hot_cue_loop_with_color() {
        let data = make_nxs2_entry(4, 2, 3000, 5000, 0, None, 0x15, Some((60, 235, 80)));
        let entries = parse_nxs2_entries(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_loop());
        assert_eq!(entries[0].hot_cue_number, Some(4));
        assert_eq!(entries[0].position_ms, 3000);
        assert_eq!(entries[0].loop_end_ms, Some(5000));
        assert_eq!(entries[0].color_rgb, Some((60, 235, 80)));
    }

    // --- rekordbox color table tests ---

    #[test]
    fn rekordbox_color_valid_code() {
        // Code 0x01 → (0x30, 0x5a, 0xff)
        assert_eq!(rekordbox_color(0x01), Some((0x30, 0x5a, 0xff)));
        // Code 0x22 → (0xff, 0xa0, 0x00)
        assert_eq!(rekordbox_color(0x22), Some((0xff, 0xa0, 0x00)));
        // Code 0x3e → (0x64, 0x73, 0xff)
        assert_eq!(rekordbox_color(0x3e), Some((0x64, 0x73, 0xff)));
    }

    #[test]
    fn rekordbox_color_no_color_returns_none() {
        assert_eq!(rekordbox_color(0x00), None);
    }

    #[test]
    fn rekordbox_color_out_of_range_returns_none() {
        assert_eq!(rekordbox_color(0x3f), None);
        assert_eq!(rekordbox_color(0xff), None);
    }

    #[test]
    fn rekordbox_colors_table_length() {
        // 0x00 through 0x3e = 63 entries
        assert_eq!(REKORDBOX_COLORS.len(), 63);
    }
}
