use crate::device::types::{Bpm, DeviceNumber, TrackSourceSlot};

/// A reference to a specific track on a specific player/slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataReference {
    /// The player the track is loaded on.
    pub player: DeviceNumber,
    /// The media slot.
    pub slot: TrackSourceSlot,
    /// The rekordbox database ID.
    pub rekordbox_id: u32,
}

impl DataReference {
    pub fn new(player: DeviceNumber, slot: TrackSourceSlot, rekordbox_id: u32) -> Self {
        Self {
            player,
            slot,
            rekordbox_id,
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
    /// Artist name.
    pub artist: String,
    /// Album title.
    pub album: String,
    /// Genre.
    pub genre: String,
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
    /// Musical key.
    pub key: String,
    /// Artwork ID (for use with art fetcher).
    pub artwork_id: u32,
    /// Date added to the collection.
    pub date_added: String,
}

impl TrackMetadata {
    /// Create a TrackMetadata with default/empty fields.
    pub fn new(data_ref: DataReference) -> Self {
        Self {
            data_ref,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            genre: String::new(),
            comment: String::new(),
            duration: 0,
            tempo: Bpm(0.0),
            rating: 0,
            color: None,
            key: String::new(),
            artwork_id: 0,
            date_added: String::new(),
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

            if let Some(mt) = item_type {
                match mt {
                    MenuItemType::TrackTitle => meta.title = text1,
                    MenuItemType::Artist => meta.artist = text1,
                    MenuItemType::AlbumTitle => meta.album = text1,
                    MenuItemType::Genre => meta.genre = text1,
                    MenuItemType::Comment => meta.comment = text1,
                    MenuItemType::Key => meta.key = text1,
                    MenuItemType::DateAdded => meta.date_added = text1,
                    MenuItemType::Rating => meta.rating = num1 as u8,
                    MenuItemType::Tempo => meta.tempo = Bpm(num1 as f64 / 100.0),
                    MenuItemType::ColorLabel => meta.color = Some(num1 as u8),
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
        assert_eq!(meta.artist, "");
        assert_eq!(meta.album, "");
        assert_eq!(meta.genre, "");
        assert_eq!(meta.comment, "");
        assert_eq!(meta.duration, 0);
        assert_eq!(meta.tempo.0, 0.0);
        assert_eq!(meta.rating, 0);
        assert!(meta.color.is_none());
        assert_eq!(meta.key, "");
        assert_eq!(meta.artwork_id, 0);
        assert_eq!(meta.date_added, "");
        assert_eq!(meta.data_ref, make_data_ref());
    }

    /// Helper: build a mock MenuItem message with a menu-item-type tag in arg 6,
    /// a string in arg 3, and a number in arg 1.
    fn mock_menu_item(item_type: u16, text: &str, num: u32) -> Message {
        Message::new(
            1,
            MessageType::MenuItem,
            vec![
                Field::number(0),                    // arg 0
                Field::number(num),                  // arg 1: numeric value
                Field::number(0),                    // arg 2
                Field::string(text),                 // arg 3: text value
                Field::number(0),                    // arg 4
                Field::string(""),                   // arg 5
                Field::number(item_type as u32),     // arg 6: item type
            ],
        )
    }

    #[test]
    fn from_menu_items_populates_fields() {
        let items = vec![
            mock_menu_item(0x0001, "My Track", 0),
            mock_menu_item(0x0002, "DJ Artist", 0),
            mock_menu_item(0x0003, "Cool Album", 0),
            mock_menu_item(0x0006, "House", 0),
            mock_menu_item(0x0009, "Great track!", 0),
            mock_menu_item(0x000e, "Am", 0),
            mock_menu_item(0x0010, "2024-01-15", 0),
            mock_menu_item(0x000b, "", 4),       // rating = 4
            mock_menu_item(0x000a, "", 12800),   // tempo = 128.00 BPM
            mock_menu_item(0x000d, "", 3),       // color = 3
        ];

        let meta = TrackMetadata::from_menu_items(make_data_ref(), &items);

        assert_eq!(meta.title, "My Track");
        assert_eq!(meta.artist, "DJ Artist");
        assert_eq!(meta.album, "Cool Album");
        assert_eq!(meta.genre, "House");
        assert_eq!(meta.comment, "Great track!");
        assert_eq!(meta.key, "Am");
        assert_eq!(meta.date_added, "2024-01-15");
        assert_eq!(meta.rating, 4);
        assert!((meta.tempo.0 - 128.0).abs() < f64::EPSILON);
        assert_eq!(meta.color, Some(3));
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
}
