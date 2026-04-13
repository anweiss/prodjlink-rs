use crate::dbserver::client::Client;
use crate::dbserver::field::Field;
use crate::dbserver::message::{MenuItemType, Message, MessageType};
use crate::device::types::TrackSourceSlot;
use crate::error::Result;

/// A single item returned from a menu request.
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// The kind of menu item (artist, album, track, folder, …).
    pub item_type: MenuItemType,
    /// Database ID for this item.
    pub id: u32,
    /// Primary display label.
    pub label1: String,
    /// Secondary display label (may be empty).
    pub label2: String,
}

/// Provides methods for browsing rekordbox media libraries on connected players.
///
/// Each method sends a menu request through a [`Client`] and returns the parsed
/// list of [`MenuItem`]s.  The `slot` argument identifies which media slot
/// (USB, SD, CD, …) to browse.
pub struct MenuLoader;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Default sort order sent with every menu request (0 = default/natural).
const DEFAULT_SORT: u32 = 0;

/// Build the standard argument list for a root (unfiltered) menu request.
fn root_args(slot: TrackSourceSlot) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(DEFAULT_SORT, 4),
    ]
}

/// Build args for a menu request with one filter ID.
fn filtered_args_1(slot: TrackSourceSlot, id1: u32) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(DEFAULT_SORT, 4),
        Field::number_with_size(id1, 4),
    ]
}

/// Build args for a menu request with two filter IDs.
fn filtered_args_2(slot: TrackSourceSlot, id1: u32, id2: u32) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(DEFAULT_SORT, 4),
        Field::number_with_size(id1, 4),
        Field::number_with_size(id2, 4),
    ]
}

/// Build args for a menu request with three filter IDs.
fn filtered_args_3(slot: TrackSourceSlot, id1: u32, id2: u32, id3: u32) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(DEFAULT_SORT, 4),
        Field::number_with_size(id1, 4),
        Field::number_with_size(id2, 4),
        Field::number_with_size(id3, 4),
    ]
}

/// Build args for a search request.
fn search_args(slot: TrackSourceSlot, query: &str) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(DEFAULT_SORT, 4),
        Field::string(query),
    ]
}

/// Parse response messages from [`Client::menu_request`] into [`MenuItem`]s.
///
/// The CDJ MenuItem response layout:
/// - arg\[6\]: `MenuItemType` (number → `u16`)
/// - arg\[1\]: item ID (number)
/// - arg\[3\]: label1 (string)
/// - arg\[5\]: label2 (string)
pub(crate) fn parse_menu_items(messages: &[Message]) -> Vec<MenuItem> {
    messages
        .iter()
        .filter_map(|msg| {
            let item_type_raw = msg.arg_number(6).ok()? as u16;
            let item_type = MenuItemType::from(item_type_raw);
            let id = msg.arg_number(1).unwrap_or(0);
            let label1 = msg
                .arg_string(3)
                .map(|s| s.to_owned())
                .unwrap_or_default();
            let label2 = msg
                .arg_string(5)
                .map(|s| s.to_owned())
                .unwrap_or_default();
            Some(MenuItem {
                item_type,
                id,
                label1,
                label2,
            })
        })
        .collect()
}

/// Issue a menu request and parse the response.
async fn menu_request(
    client: &mut Client,
    kind: MessageType,
    args: Vec<Field>,
) -> Result<Vec<MenuItem>> {
    let messages = client.menu_request(kind, args).await?;
    Ok(parse_menu_items(&messages))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl MenuLoader {
    // -- Root (unfiltered) menus -------------------------------------------

    /// Request the root menu from a player's media slot.
    pub async fn root_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RootMenuReq, root_args(slot)).await
    }

    /// Request the artist list.
    pub async fn artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::ArtistMenuReq, root_args(slot)).await
    }

    /// Request the genre list.
    pub async fn genre_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::GenreMenuReq, root_args(slot)).await
    }

    /// Request the album list.
    pub async fn album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::AlbumMenuReq, root_args(slot)).await
    }

    /// Request the key list.
    pub async fn key_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::KeyMenuReq, root_args(slot)).await
    }

    /// Request the BPM range list.
    pub async fn bpm_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::BpmMenuReq, root_args(slot)).await
    }

    /// Request the rating list.
    pub async fn rating_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RatingMenuReq, root_args(slot)).await
    }

    /// Request the color list.
    pub async fn color_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::ColorMenuReq, root_args(slot)).await
    }

    /// Request the label list.
    pub async fn label_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::LabelMenuReq, root_args(slot)).await
    }

    /// Request the original artist list.
    pub async fn original_artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::OriginalArtistMenuReq, root_args(slot)).await
    }

    /// Request the remixer list.
    pub async fn remixer_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RemixerMenuReq, root_args(slot)).await
    }

    /// Request history playlists.
    pub async fn history_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::HistoryMenuReq, root_args(slot)).await
    }

    /// Request tracks by time added.
    pub async fn time_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::TimeMenuReq, root_args(slot)).await
    }

    /// Request tracks by bit rate.
    pub async fn bit_rate_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::BitRateMenuReq, root_args(slot)).await
    }

    /// Request tracks by filename.
    pub async fn filename_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::FilenameMenuReq, root_args(slot)).await
    }

    /// Request year/decade list.
    pub async fn year_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::YearMenuReq, root_args(slot)).await
    }

    // -- Filtered menus (one filter) --------------------------------------

    /// Request albums by a specific artist.
    pub async fn artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForArtistReq,
            filtered_args_1(slot, artist_id),
        )
        .await
    }

    /// Request artists within a genre.
    pub async fn genre_artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        genre_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::ArtistMenuForGenreReq,
            filtered_args_1(slot, genre_id),
        )
        .await
    }

    /// Request tracks by key and distance (neighbors).
    pub async fn key_neighbor_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        key_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::NeighborMenuForKeyReq,
            filtered_args_1(slot, key_id),
        )
        .await
    }

    /// Request tracks in an album.
    pub async fn album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForAlbumReq,
            filtered_args_1(slot, album_id),
        )
        .await
    }

    /// Request playlists (or playlist contents) within a folder.
    pub async fn playlist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        folder_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::PlaylistReq,
            filtered_args_1(slot, folder_id),
        )
        .await
    }

    /// Request folder contents.
    pub async fn folder_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        folder_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::FolderMenuReq,
            filtered_args_1(slot, folder_id),
        )
        .await
    }

    // -- Filtered menus (two filters) -------------------------------------

    /// Request albums by artist within genre.
    pub async fn genre_artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        genre_id: u32,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForGenreAndArtistReq,
            filtered_args_2(slot, genre_id, artist_id),
        )
        .await
    }

    /// Request tracks from a specific album by a specific artist.
    pub async fn artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForArtistAndAlbumReq,
            filtered_args_2(slot, artist_id, album_id),
        )
        .await
    }

    /// Request tracks by genre→artist→album.
    pub async fn genre_artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        genre_id: u32,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForGenreArtistAndAlbumReq,
            filtered_args_3(slot, genre_id, artist_id, album_id),
        )
        .await
    }

    // -- Search -----------------------------------------------------------

    /// Search for tracks by name.
    pub async fn search(
        client: &mut Client,
        slot: TrackSourceSlot,
        query: &str,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::SearchMenuReq, search_args(slot, query)).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbserver::field::Field;
    use crate::dbserver::message::{MenuItemType, Message, MessageType};
    use crate::device::types::TrackSourceSlot;

    /// Build a mock MenuItem response message with the standard arg layout.
    fn mock_menu_item_msg(
        item_type: MenuItemType,
        id: u32,
        label1: &str,
        label2: &str,
    ) -> Message {
        // Standard MenuItem response layout:
        //  arg[0]: parent_id, arg[1]: id, arg[2]: unknown,
        //  arg[3]: label1, arg[4]: unknown, arg[5]: label2,
        //  arg[6]: menu_item_type
        Message::new(
            1,
            MessageType::MenuItem,
            vec![
                Field::number_with_size(0, 4),                                // [0] parent_id
                Field::number_with_size(id, 4),                               // [1] id
                Field::number_with_size(0, 4),                                // [2] unknown
                Field::string(label1),                                        // [3] label1
                Field::number_with_size(0, 4),                                // [4] unknown
                Field::string(label2),                                        // [5] label2
                Field::number_with_size(u16::from(item_type) as u32, 4),      // [6] item type
            ],
        )
    }

    // -- parse_menu_items -------------------------------------------------

    #[test]
    fn parse_menu_items_extracts_fields() {
        let messages = vec![
            mock_menu_item_msg(MenuItemType::Artist, 10, "Daft Punk", ""),
            mock_menu_item_msg(MenuItemType::Artist, 20, "Kraftwerk", ""),
        ];
        let items = parse_menu_items(&messages);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_type, MenuItemType::Artist);
        assert_eq!(items[0].id, 10);
        assert_eq!(items[0].label1, "Daft Punk");
        assert_eq!(items[0].label2, "");
        assert_eq!(items[1].id, 20);
        assert_eq!(items[1].label1, "Kraftwerk");
    }

    #[test]
    fn parse_menu_items_with_two_labels() {
        let messages = vec![mock_menu_item_msg(
            MenuItemType::TrackTitleAndArtist,
            42,
            "Around the World",
            "Daft Punk",
        )];
        let items = parse_menu_items(&messages);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, MenuItemType::TrackTitleAndArtist);
        assert_eq!(items[0].id, 42);
        assert_eq!(items[0].label1, "Around the World");
        assert_eq!(items[0].label2, "Daft Punk");
    }

    #[test]
    fn parse_empty_menu_response() {
        let items = parse_menu_items(&[]);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_skips_messages_without_enough_args() {
        // A message with only 2 args cannot provide arg[6].
        let short_msg = Message::new(
            1,
            MessageType::MenuItem,
            vec![
                Field::number_with_size(0, 4),
                Field::number_with_size(1, 4),
            ],
        );
        let items = parse_menu_items(&[short_msg]);
        assert!(items.is_empty());
    }

    // -- Request argument construction ------------------------------------

    #[test]
    fn root_args_contains_slot_and_sort() {
        let args = root_args(TrackSourceSlot::UsbSlot);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].as_number().unwrap(), 3); // UsbSlot = 3
        assert_eq!(args[1].as_number().unwrap(), DEFAULT_SORT);
    }

    #[test]
    fn filtered_args_1_includes_filter_id() {
        let args = filtered_args_1(TrackSourceSlot::SdSlot, 42);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 2); // SdSlot = 2
        assert_eq!(args[2].as_number().unwrap(), 42);
    }

    #[test]
    fn filtered_args_2_includes_both_ids() {
        let args = filtered_args_2(TrackSourceSlot::UsbSlot, 10, 20);
        assert_eq!(args.len(), 4);
        assert_eq!(args[2].as_number().unwrap(), 10);
        assert_eq!(args[3].as_number().unwrap(), 20);
    }

    #[test]
    fn filtered_args_3_includes_all_ids() {
        let args = filtered_args_3(TrackSourceSlot::UsbSlot, 5, 10, 15);
        assert_eq!(args.len(), 5);
        assert_eq!(args[2].as_number().unwrap(), 5);
        assert_eq!(args[3].as_number().unwrap(), 10);
        assert_eq!(args[4].as_number().unwrap(), 15);
    }

    #[test]
    fn search_args_encodes_query_string() {
        let args = search_args(TrackSourceSlot::UsbSlot, "daft punk");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 3); // UsbSlot
        assert_eq!(args[2].as_string().unwrap(), "daft punk");
    }

    #[test]
    fn search_args_handles_unicode_query() {
        let args = search_args(TrackSourceSlot::UsbSlot, "日本語");
        assert_eq!(args[2].as_string().unwrap(), "日本語");
    }

    // -- Slot wire values ------------------------------------------------

    #[test]
    fn all_slot_wire_values_correct() {
        assert_eq!(
            root_args(TrackSourceSlot::CdSlot)[0].as_number().unwrap(),
            1
        );
        assert_eq!(
            root_args(TrackSourceSlot::SdSlot)[0].as_number().unwrap(),
            2
        );
        assert_eq!(
            root_args(TrackSourceSlot::UsbSlot)[0].as_number().unwrap(),
            3
        );
        assert_eq!(
            root_args(TrackSourceSlot::Collection)[0]
                .as_number()
                .unwrap(),
            4
        );
    }

    // -- Menu item type round-trip in parse --------------------------------

    #[test]
    fn parse_preserves_item_type_variants() {
        let types = [
            MenuItemType::Folder,
            MenuItemType::Artist,
            MenuItemType::AlbumTitle,
            MenuItemType::TrackTitle,
            MenuItemType::Genre,
            MenuItemType::Playlist,
            MenuItemType::Key,
            MenuItemType::GenreMenu,
            MenuItemType::ArtistMenu,
        ];
        for expected_type in &types {
            let msg = mock_menu_item_msg(*expected_type, 1, "test", "");
            let items = parse_menu_items(&[msg]);
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].item_type, *expected_type,
                "round-trip failed for {expected_type:?}"
            );
        }
    }
}
