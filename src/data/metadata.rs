use crate::device::types::{Bpm, DeviceNumber, SlotReference, TrackSourceSlot, TrackType};

/// A searchable item with both a database ID and display label.
/// Used for artist, album, genre, key, label, original artist, and remixer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchableItem {
    /// Database ID for this item (used in dbserver queries).
    pub id: u32,
    /// Display label.
    pub label: String,
}

impl SearchableItem {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}

impl std::fmt::Display for SearchableItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// A reference to a specific track on a specific player/slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataReference {
    /// The player the track is loaded on.
    pub player: DeviceNumber,
    /// The media slot.
    pub slot: TrackSourceSlot,
    /// The rekordbox database ID.
    pub rekordbox_id: u32,
    /// The type of track (rekordbox, unanalyzed, etc.).
    pub track_type: TrackType,
}

impl DataReference {
    pub fn new(player: DeviceNumber, slot: TrackSourceSlot, rekordbox_id: u32) -> Self {
        Self {
            player,
            slot,
            rekordbox_id,
            track_type: TrackType::Rekordbox,
        }
    }

    /// Create a DataReference with an explicit track type.
    pub fn with_track_type(
        player: DeviceNumber,
        slot: TrackSourceSlot,
        rekordbox_id: u32,
        track_type: TrackType,
    ) -> Self {
        Self {
            player,
            slot,
            rekordbox_id,
            track_type,
        }
    }

    /// Get a [`SlotReference`] for this data reference's player and slot.
    pub fn slot_reference(&self) -> SlotReference {
        SlotReference {
            player: self.player,
            slot: self.slot,
        }
    }
}

/// Metadata about a track from the rekordbox database.
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    /// Reference to the track in the database.
    pub data_ref: DataReference,
    /// Track title.
    pub title: String,
    /// Artist.
    pub artist: SearchableItem,
    /// Album.
    pub album: SearchableItem,
    /// Genre.
    pub genre: SearchableItem,
    /// Musical key.
    pub key: SearchableItem,
    /// Record label.
    pub label: SearchableItem,
    /// Original artist.
    pub original_artist: SearchableItem,
    /// Remixer.
    pub remixer: SearchableItem,
    /// Comment field.
    pub comment: String,
    /// Track duration in seconds.
    pub duration: u32,
    /// Original tempo (BPM) of the track.
    pub tempo: Bpm,
    /// Star rating (0-5).
    pub rating: u8,
    /// Color label ID.
    pub color: Option<u8>,
    /// Date added to the collection.
    pub date_added: String,
    /// Artwork ID (for use with art fetcher).
    pub artwork_id: u32,
    /// Release year.
    pub year: u16,
    /// Bit rate in kbps.
    pub bit_rate: u32,
    /// Track type (rekordbox, unanalyzed, etc.).
    pub track_type: TrackType,
}

impl TrackMetadata {
    /// Create a TrackMetadata with default/empty fields.
    pub fn new(data_ref: DataReference) -> Self {
        Self {
            data_ref,
            title: String::new(),
            artist: SearchableItem::new(0, ""),
            album: SearchableItem::new(0, ""),
            genre: SearchableItem::new(0, ""),
            key: SearchableItem::new(0, ""),
            label: SearchableItem::new(0, ""),
            original_artist: SearchableItem::new(0, ""),
            remixer: SearchableItem::new(0, ""),
            comment: String::new(),
            duration: 0,
            tempo: Bpm(0.0),
            rating: 0,
            color: None,
            date_added: String::new(),
            artwork_id: 0,
            year: 0,
            bit_rate: 0,
            track_type: TrackType::Rekordbox,
        }
    }

    /// Build TrackMetadata from dbserver menu item responses.
    ///
    /// The dbserver returns metadata as a series of MenuItem messages,
    /// each tagged with a MenuItemType indicating what field it represents.
    pub fn from_menu_items(
        data_ref: DataReference,
        items: &[crate::dbserver::message::Message],
    ) -> Self {
        use crate::dbserver::message::MenuItemType;

        let mut meta = Self::new(data_ref);

        for item in items {
            // Menu items typically have the type indicator and text fields.
            // The item type is in arg 6 (number field).
            // The text values are in args 3 and 5 (string fields).
            let item_type = item
                .args
                .get(6)
                .and_then(|f| f.as_number().ok())
                .map(|v| MenuItemType::from(v as u16));

            let text1 = item
                .args
                .get(3)
                .and_then(|f| f.as_string().ok())
                .unwrap_or_default()
                .to_string();

            let num1 = item
                .args
                .get(1)
                .and_then(|f| f.as_number().ok())
                .unwrap_or(0);

            // Helper: build a SearchableItem from the numeric ID and text label.
            let searchable = || SearchableItem::new(num1, &text1);

            if let Some(mt) = item_type {
                match mt {
                    MenuItemType::TrackTitle => {
                        meta.title = text1;
                        if let Some(art_id) = item.args.get(8).and_then(|f| f.as_number().ok()) {
                            meta.artwork_id = art_id;
                        }
                    }
                    MenuItemType::Artist => meta.artist = searchable(),
                    MenuItemType::AlbumTitle => meta.album = searchable(),
                    MenuItemType::Genre => meta.genre = searchable(),
                    MenuItemType::Key => meta.key = searchable(),
                    MenuItemType::Label => meta.label = searchable(),
                    MenuItemType::OriginalArtist => meta.original_artist = searchable(),
                    MenuItemType::Remixer => meta.remixer = searchable(),
                    MenuItemType::Comment => meta.comment = text1,
                    MenuItemType::DateAdded => meta.date_added = text1,
                    MenuItemType::Rating => meta.rating = num1 as u8,
                    MenuItemType::Tempo => meta.tempo = Bpm(num1 as f64 / 100.0),
                    MenuItemType::ColorNone => meta.color = None,
                    MenuItemType::ColorPink => meta.color = Some(1),
                    MenuItemType::ColorRed => meta.color = Some(2),
                    MenuItemType::ColorOrange => meta.color = Some(3),
                    MenuItemType::ColorYellow => meta.color = Some(4),
                    MenuItemType::ColorGreen => meta.color = Some(5),
                    MenuItemType::ColorAqua => meta.color = Some(6),
                    MenuItemType::ColorBlue => meta.color = Some(7),
                    MenuItemType::ColorPurple => meta.color = Some(8),
                    MenuItemType::Duration => meta.duration = num1,
                    MenuItemType::BitRate => meta.bit_rate = num1,
                    MenuItemType::Year => meta.year = num1 as u16,
                    _ => {}
                }
            }
        }

        meta
    }
}

/// Build the dbserver request arguments for a metadata query.
pub fn build_metadata_request_args(
    data_ref: &DataReference,
    menu_id: u8,
) -> Vec<crate::dbserver::field::Field> {
    use crate::dbserver::field::Field;
    vec![
        Field::number(menu_id as u32),
        Field::number(u8::from(data_ref.slot) as u32),
        Field::number(data_ref.rekordbox_id),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbserver::field::Field;
    use crate::dbserver::message::{Message, MessageType};

    fn make_data_ref() -> DataReference {
        DataReference::new(DeviceNumber(3), TrackSourceSlot::UsbSlot, 42)
    }

    #[test]
    fn data_reference_fields() {
        let dr = make_data_ref();
        assert_eq!(dr.player, DeviceNumber(3));
        assert_eq!(dr.slot, TrackSourceSlot::UsbSlot);
        assert_eq!(dr.rekordbox_id, 42);
        assert_eq!(dr.track_type, TrackType::Rekordbox);
    }

    #[test]
    fn data_reference_with_track_type() {
        let dr = DataReference::with_track_type(
            DeviceNumber(1),
            TrackSourceSlot::SdSlot,
            99,
            TrackType::Rekordbox,
        );
        assert_eq!(dr.player, DeviceNumber(1));
        assert_eq!(dr.slot, TrackSourceSlot::SdSlot);
        assert_eq!(dr.rekordbox_id, 99);
        assert_eq!(dr.track_type, TrackType::Rekordbox);
    }

    #[test]
    fn data_reference_equality_and_hash() {
        use std::collections::HashSet;
        let a = make_data_ref();
        let b = DataReference::new(DeviceNumber(3), TrackSourceSlot::UsbSlot, 42);
        let c = DataReference::new(DeviceNumber(1), TrackSourceSlot::SdSlot, 99);
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    #[test]
    fn track_metadata_defaults() {
        let meta = TrackMetadata::new(make_data_ref());
        assert_eq!(meta.title, "");
        assert_eq!(meta.artist, SearchableItem::new(0, ""));
        assert_eq!(meta.album, SearchableItem::new(0, ""));
        assert_eq!(meta.genre, SearchableItem::new(0, ""));
        assert_eq!(meta.key, SearchableItem::new(0, ""));
        assert_eq!(meta.label, SearchableItem::new(0, ""));
        assert_eq!(meta.original_artist, SearchableItem::new(0, ""));
        assert_eq!(meta.remixer, SearchableItem::new(0, ""));
        assert_eq!(meta.comment, "");
        assert_eq!(meta.duration, 0);
        assert_eq!(meta.tempo.0, 0.0);
        assert_eq!(meta.rating, 0);
        assert!(meta.color.is_none());
        assert_eq!(meta.artwork_id, 0);
        assert_eq!(meta.date_added, "");
        assert_eq!(meta.year, 0);
        assert_eq!(meta.bit_rate, 0);
        assert_eq!(meta.track_type, TrackType::Rekordbox);
        assert_eq!(meta.data_ref, make_data_ref());
    }

    /// Helper: build a mock MenuItem message with a menu-item-type tag in arg 6,
    /// a string in arg 3, and a number in arg 1.
    fn mock_menu_item(item_type: u16, text: &str, num: u32) -> Message {
        Message::new(
            1,
            MessageType::MenuItem,
            vec![
                Field::number(0),                // arg 0
                Field::number(num),              // arg 1: numeric value
                Field::number(0),                // arg 2
                Field::string(text),             // arg 3: text value
                Field::number(0),                // arg 4
                Field::string(""),               // arg 5
                Field::number(item_type as u32), // arg 6: item type
            ],
        )
    }

    #[test]
    fn from_menu_items_populates_fields() {
        let items = vec![
            mock_menu_item(0x0004, "My Track", 0),     // TrackTitle
            mock_menu_item(0x0007, "DJ Artist", 10),   // Artist
            mock_menu_item(0x0002, "Cool Album", 20),  // AlbumTitle
            mock_menu_item(0x0006, "House", 30),       // Genre
            mock_menu_item(0x0023, "Great track!", 0), // Comment
            mock_menu_item(0x000f, "Am", 40),          // Key
            mock_menu_item(0x002e, "2024-01-15", 0),   // DateAdded
            mock_menu_item(0x000a, "", 4),             // Rating = 4
            mock_menu_item(0x000d, "", 12800),         // Tempo = 128.00 BPM
            mock_menu_item(0x0016, "", 0),             // ColorOrange => color = 3
            mock_menu_item(0x000e, "Cool Label", 50),  // Label
            mock_menu_item(0x0028, "Orig Artist", 60), // OriginalArtist
            mock_menu_item(0x0029, "Remix Guy", 70),   // Remixer
            mock_menu_item(0x000b, "", 240),           // Duration = 240s
            mock_menu_item(0x0010, "", 320),           // BitRate = 320
            mock_menu_item(0x0011, "", 2023),          // Year = 2023
        ];

        let meta = TrackMetadata::from_menu_items(make_data_ref(), &items);

        assert_eq!(meta.title, "My Track");
        assert_eq!(meta.artist, SearchableItem::new(10, "DJ Artist"));
        assert_eq!(meta.album, SearchableItem::new(20, "Cool Album"));
        assert_eq!(meta.genre, SearchableItem::new(30, "House"));
        assert_eq!(meta.comment, "Great track!");
        assert_eq!(meta.key, SearchableItem::new(40, "Am"));
        assert_eq!(meta.date_added, "2024-01-15");
        assert_eq!(meta.rating, 4);
        assert!((meta.tempo.0 - 128.0).abs() < f64::EPSILON);
        assert_eq!(meta.color, Some(3));
        assert_eq!(meta.label, SearchableItem::new(50, "Cool Label"));
        assert_eq!(meta.original_artist, SearchableItem::new(60, "Orig Artist"));
        assert_eq!(meta.remixer, SearchableItem::new(70, "Remix Guy"));
        assert_eq!(meta.duration, 240);
        assert_eq!(meta.bit_rate, 320);
        assert_eq!(meta.year, 2023);
    }

    #[test]
    fn from_menu_items_empty() {
        let meta = TrackMetadata::from_menu_items(make_data_ref(), &[]);
        assert_eq!(meta.title, "");
        assert_eq!(meta.rating, 0);
        assert!(meta.color.is_none());
    }

    #[test]
    fn from_menu_items_ignores_unknown_types() {
        let items = vec![mock_menu_item(0xFFFF, "ignored", 99)];
        let meta = TrackMetadata::from_menu_items(make_data_ref(), &items);
        assert_eq!(meta.title, "");
        assert_eq!(meta.rating, 0);
    }

    #[test]
    fn build_metadata_request_args_values() {
        let dr = make_data_ref();
        let args = build_metadata_request_args(&dr, 8);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 8);
        assert_eq!(args[1].as_number().unwrap(), 3); // UsbSlot = 3
        assert_eq!(args[2].as_number().unwrap(), 42);
    }

    #[test]
    fn searchable_item_new_and_eq() {
        let a = SearchableItem::new(1, "Daft Punk");
        let b = SearchableItem::new(1, "Daft Punk");
        let c = SearchableItem::new(2, "Daft Punk");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.id, 1);
        assert_eq!(a.label, "Daft Punk");
    }

    #[test]
    fn searchable_item_display() {
        let item = SearchableItem::new(5, "Techno");
        assert_eq!(format!("{item}"), "Techno");
    }

    #[test]
    fn searchable_item_hash() {
        use std::collections::HashSet;
        let a = SearchableItem::new(1, "House");
        let b = SearchableItem::new(1, "House");
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }
}
