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

/// Parse a Nexus-format binary cue list (36 bytes per entry).
pub fn parse_nexus_entries(data: &[u8]) -> Result<Vec<CueEntry>, String> {
    // TODO: implement Nexus binary format parsing
    // Each entry is 36 bytes with fields at known offsets
    let _ = data;
    Ok(Vec::new())
}

/// Parse an Nxs2-format binary cue list (variable length entries with comments and colors).
pub fn parse_nxs2_entries(data: &[u8]) -> Result<Vec<CueEntry>, String> {
    // TODO: implement Nxs2 binary format parsing
    let _ = data;
    Ok(Vec::new())
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

    // --- parsing stubs ---

    #[test]
    fn parse_nexus_entries_stub() {
        let entries = parse_nexus_entries(&[]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_nxs2_entries_stub() {
        let entries = parse_nxs2_entries(&[]).unwrap();
        assert!(entries.is_empty());
    }
}
