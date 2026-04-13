use crate::dbserver::client::Client;
use crate::dbserver::field::Field;
use crate::dbserver::message::{MenuItemType, Message, MessageType};
use crate::device::types::TrackSourceSlot;
use crate::error::Result;

/// Sort order for menu requests.
///
/// These correspond to the protocol-level sort order values defined in the
/// Pioneer dbserver protocol (originally in Java's `Message.java`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SortOrder {
    /// Natural / default ordering.
    Default = 0,
    Album = 1,
    Artist = 2,
    Bpm = 3,
    Genre = 4,
    Key = 5,
    Rating = 6,
    Color = 7,
    Duration = 8,
    DateAdded = 9,
    /// Sort by name / title.
    Name = 10,
    Bitrate = 11,
}

impl SortOrder {
    /// Return the protocol-level numeric value for this sort order.
    pub fn to_protocol_value(self) -> u32 {
        self as u32
    }
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Default
    }
}

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

/// Resolve an optional sort order to its protocol value.
fn sort_value(sort: Option<SortOrder>) -> u32 {
    sort.unwrap_or_default().to_protocol_value()
}

/// Build the standard argument list for a root (unfiltered) menu request.
fn root_args(slot: TrackSourceSlot, sort: Option<SortOrder>) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(sort_value(sort), 4),
    ]
}

/// Build args for a menu request with one filter ID.
fn filtered_args_1(slot: TrackSourceSlot, sort: Option<SortOrder>, id1: u32) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(sort_value(sort), 4),
        Field::number_with_size(id1, 4),
    ]
}

/// Build args for a menu request with two filter IDs.
fn filtered_args_2(
    slot: TrackSourceSlot,
    sort: Option<SortOrder>,
    id1: u32,
    id2: u32,
) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(sort_value(sort), 4),
        Field::number_with_size(id1, 4),
        Field::number_with_size(id2, 4),
    ]
}

/// Build args for a menu request with three filter IDs.
fn filtered_args_3(
    slot: TrackSourceSlot,
    sort: Option<SortOrder>,
    id1: u32,
    id2: u32,
    id3: u32,
) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(sort_value(sort), 4),
        Field::number_with_size(id1, 4),
        Field::number_with_size(id2, 4),
        Field::number_with_size(id3, 4),
    ]
}

/// Build args for a search request.
fn search_args(slot: TrackSourceSlot, sort: Option<SortOrder>, query: &str) -> Vec<Field> {
    vec![
        Field::number_with_size(u8::from(slot) as u32, 4),
        Field::number_with_size(sort_value(sort), 4),
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
            let label1 = msg.arg_string(3).map(|s| s.to_owned()).unwrap_or_default();
            let label2 = msg.arg_string(5).map(|s| s.to_owned()).unwrap_or_default();
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
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RootMenuReq, root_args(slot, sort)).await
    }

    /// Request all tracks (root track listing).
    pub async fn track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::TrackMenuReq, root_args(slot, sort)).await
    }

    /// Request the artist list.
    pub async fn artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::ArtistMenuReq, root_args(slot, sort)).await
    }

    /// Request the genre list.
    pub async fn genre_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::GenreMenuReq, root_args(slot, sort)).await
    }

    /// Request the album list.
    pub async fn album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::AlbumMenuReq, root_args(slot, sort)).await
    }

    /// Request the key list.
    pub async fn key_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::KeyMenuReq, root_args(slot, sort)).await
    }

    /// Request the BPM range list.
    pub async fn bpm_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::BpmMenuReq, root_args(slot, sort)).await
    }

    /// Request the rating list.
    pub async fn rating_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RatingMenuReq, root_args(slot, sort)).await
    }

    /// Request the color list.
    pub async fn color_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::ColorMenuReq, root_args(slot, sort)).await
    }

    /// Request the label list.
    pub async fn label_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::LabelMenuReq, root_args(slot, sort)).await
    }

    /// Request the original artist list.
    pub async fn original_artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::OriginalArtistMenuReq,
            root_args(slot, sort),
        )
        .await
    }

    /// Request the remixer list.
    pub async fn remixer_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RemixerMenuReq, root_args(slot, sort)).await
    }

    /// Request history playlists.
    pub async fn history_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::HistoryMenuReq, root_args(slot, sort)).await
    }

    /// Request tracks by time added.
    pub async fn time_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::TimeMenuReq, root_args(slot, sort)).await
    }

    /// Request tracks by bit rate.
    pub async fn bit_rate_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::BitRateMenuReq, root_args(slot, sort)).await
    }

    /// Request tracks by filename.
    pub async fn filename_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::FilenameMenuReq, root_args(slot, sort)).await
    }

    /// Request year/decade list.
    pub async fn year_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::YearMenuReq, root_args(slot, sort)).await
    }

    // -- Filtered menus (one filter) --------------------------------------

    /// Request albums by a specific artist.
    pub async fn artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForArtistReq,
            filtered_args_1(slot, sort, artist_id),
        )
        .await
    }

    /// Request artists within a genre.
    pub async fn genre_artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        genre_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::ArtistMenuForGenreReq,
            filtered_args_1(slot, sort, genre_id),
        )
        .await
    }

    /// Request tracks by key and distance (neighbors).
    pub async fn key_neighbor_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        key_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::NeighborMenuForKeyReq,
            filtered_args_1(slot, sort, key_id),
        )
        .await
    }

    /// Request tracks in an album.
    pub async fn album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForAlbumReq,
            filtered_args_1(slot, sort, album_id),
        )
        .await
    }

    /// Request playlists (or playlist contents) within a folder.
    pub async fn playlist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        folder_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::PlaylistReq,
            filtered_args_1(slot, sort, folder_id),
        )
        .await
    }

    /// Request folder contents.
    pub async fn folder_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        folder_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::FolderMenuReq,
            filtered_args_1(slot, sort, folder_id),
        )
        .await
    }

    /// Request tracks within a specific history entry.
    pub async fn history_playlist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        history_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForHistoryReq,
            filtered_args_1(slot, sort, history_id),
        )
        .await
    }

    /// Request albums by an original artist.
    pub async fn original_artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForOriginalArtistReq,
            filtered_args_1(slot, sort, artist_id),
        )
        .await
    }

    /// Request albums by a remixer.
    pub async fn remixer_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        remixer_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForRemixerReq,
            filtered_args_1(slot, sort, remixer_id),
        )
        .await
    }

    /// Request artists on a label.
    pub async fn label_artist_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        label_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::ArtistMenuForLabelReq,
            filtered_args_1(slot, sort, label_id),
        )
        .await
    }

    /// Request tracks within a BPM range.
    pub async fn bpm_range_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        bpm_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::BpmRangeReq,
            filtered_args_1(slot, sort, bpm_id),
        )
        .await
    }

    /// Request tracks filtered by rating.
    pub async fn rating_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        rating_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForRatingReq,
            filtered_args_1(slot, sort, rating_id),
        )
        .await
    }

    /// Request tracks filtered by color.
    pub async fn color_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        color_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForColorReq,
            filtered_args_1(slot, sort, color_id),
        )
        .await
    }

    /// Request tracks filtered by time/date added.
    pub async fn time_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        time_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForTimeReq,
            filtered_args_1(slot, sort, time_id),
        )
        .await
    }

    /// Request tracks filtered by bit rate.
    pub async fn bit_rate_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        bit_rate_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForBitRateReq,
            filtered_args_1(slot, sort, bit_rate_id),
        )
        .await
    }

    /// Request years within a decade.
    pub async fn decade_year_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        decade_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::YearMenuForDecadeReq,
            filtered_args_1(slot, sort, decade_id),
        )
        .await
    }

    // -- Filtered menus (two filters) -------------------------------------

    /// Request albums by artist within genre.
    pub async fn genre_artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        genre_id: u32,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForGenreAndArtistReq,
            filtered_args_2(slot, sort, genre_id, artist_id),
        )
        .await
    }

    /// Request tracks from a specific album by a specific artist.
    pub async fn artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForArtistAndAlbumReq,
            filtered_args_2(slot, sort, artist_id, album_id),
        )
        .await
    }

    /// Request tracks in an album by an original artist.
    pub async fn original_artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForOriginalArtistAndAlbumReq,
            filtered_args_2(slot, sort, artist_id, album_id),
        )
        .await
    }

    /// Request tracks in an album by a remixer.
    pub async fn remixer_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        remixer_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForRemixerAndAlbumReq,
            filtered_args_2(slot, sort, remixer_id, album_id),
        )
        .await
    }

    /// Request albums by an artist on a label.
    pub async fn label_artist_album_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        label_id: u32,
        artist_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::AlbumMenuForLabelAndArtistReq,
            filtered_args_2(slot, sort, label_id, artist_id),
        )
        .await
    }

    /// Request tracks for a specific decade and year.
    pub async fn year_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        decade_id: u32,
        year_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForDecadeAndYearReq,
            filtered_args_2(slot, sort, decade_id, year_id),
        )
        .await
    }

    // -- Filtered menus (three filters) -----------------------------------

    /// Request tracks by genre->artist->album.
    pub async fn genre_artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        genre_id: u32,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForGenreArtistAndAlbumReq,
            filtered_args_3(slot, sort, genre_id, artist_id, album_id),
        )
        .await
    }

    /// Request tracks by label->artist->album.
    pub async fn label_artist_album_track_menu(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        label_id: u32,
        artist_id: u32,
        album_id: u32,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::TrackMenuForLabelArtistAndAlbumReq,
            filtered_args_3(slot, sort, label_id, artist_id, album_id),
        )
        .await
    }

    // -- Search -----------------------------------------------------------

    /// Search for tracks by name.
    pub async fn search(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
        query: &str,
    ) -> Result<Vec<MenuItem>> {
        menu_request(
            client,
            MessageType::SearchMenuReq,
            search_args(slot, sort, query),
        )
        .await
    }

    /// Request the next page of search results (pagination).
    pub async fn more_search_results(
        client: &mut Client,
        slot: TrackSourceSlot,
        sort: Option<SortOrder>,
    ) -> Result<Vec<MenuItem>> {
        menu_request(client, MessageType::RenderMenuReq, root_args(slot, sort)).await
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

    fn mock_menu_item_msg(item_type: MenuItemType, id: u32, label1: &str, label2: &str) -> Message {
        Message::new(
            1,
            MessageType::MenuItem,
            vec![
                Field::number_with_size(0, 4),
                Field::number_with_size(id, 4),
                Field::number_with_size(0, 4),
                Field::string(label1),
                Field::number_with_size(0, 4),
                Field::string(label2),
                Field::number_with_size(u16::from(item_type) as u32, 4),
            ],
        )
    }

    // -- SortOrder --------------------------------------------------------

    #[test]
    fn sort_order_default_is_zero() {
        assert_eq!(SortOrder::default(), SortOrder::Default);
        assert_eq!(SortOrder::Default.to_protocol_value(), 0);
    }

    #[test]
    fn sort_order_protocol_values() {
        assert_eq!(SortOrder::Album.to_protocol_value(), 1);
        assert_eq!(SortOrder::Artist.to_protocol_value(), 2);
        assert_eq!(SortOrder::Bpm.to_protocol_value(), 3);
        assert_eq!(SortOrder::Genre.to_protocol_value(), 4);
        assert_eq!(SortOrder::Key.to_protocol_value(), 5);
        assert_eq!(SortOrder::Rating.to_protocol_value(), 6);
        assert_eq!(SortOrder::Color.to_protocol_value(), 7);
        assert_eq!(SortOrder::Duration.to_protocol_value(), 8);
        assert_eq!(SortOrder::DateAdded.to_protocol_value(), 9);
        assert_eq!(SortOrder::Name.to_protocol_value(), 10);
        assert_eq!(SortOrder::Bitrate.to_protocol_value(), 11);
    }

    #[test]
    fn sort_order_equality_and_clone() {
        let a = SortOrder::Bpm;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn sort_order_debug_format() {
        let s = format!("{:?}", SortOrder::Artist);
        assert!(s.contains("Artist"));
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
        let short_msg = Message::new(
            1,
            MessageType::MenuItem,
            vec![Field::number_with_size(0, 4), Field::number_with_size(1, 4)],
        );
        let items = parse_menu_items(&[short_msg]);
        assert!(items.is_empty());
    }

    // -- Request argument construction ------------------------------------

    #[test]
    fn root_args_contains_slot_and_default_sort() {
        let args = root_args(TrackSourceSlot::UsbSlot, None);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].as_number().unwrap(), 3);
        assert_eq!(args[1].as_number().unwrap(), 0);
    }

    #[test]
    fn root_args_with_explicit_sort() {
        let args = root_args(TrackSourceSlot::UsbSlot, Some(SortOrder::Artist));
        assert_eq!(args[1].as_number().unwrap(), 2);
    }

    #[test]
    fn filtered_args_1_includes_filter_id() {
        let args = filtered_args_1(TrackSourceSlot::SdSlot, None, 42);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 2);
        assert_eq!(args[1].as_number().unwrap(), 0);
        assert_eq!(args[2].as_number().unwrap(), 42);
    }

    #[test]
    fn filtered_args_1_with_sort() {
        let args = filtered_args_1(TrackSourceSlot::UsbSlot, Some(SortOrder::Bpm), 10);
        assert_eq!(args[1].as_number().unwrap(), 3);
        assert_eq!(args[2].as_number().unwrap(), 10);
    }

    #[test]
    fn filtered_args_2_includes_both_ids() {
        let args = filtered_args_2(TrackSourceSlot::UsbSlot, None, 10, 20);
        assert_eq!(args.len(), 4);
        assert_eq!(args[2].as_number().unwrap(), 10);
        assert_eq!(args[3].as_number().unwrap(), 20);
    }

    #[test]
    fn filtered_args_3_includes_all_ids() {
        let args = filtered_args_3(TrackSourceSlot::UsbSlot, None, 5, 10, 15);
        assert_eq!(args.len(), 5);
        assert_eq!(args[2].as_number().unwrap(), 5);
        assert_eq!(args[3].as_number().unwrap(), 10);
        assert_eq!(args[4].as_number().unwrap(), 15);
    }

    #[test]
    fn search_args_encodes_query_string() {
        let args = search_args(TrackSourceSlot::UsbSlot, None, "daft punk");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 3);
        assert_eq!(args[2].as_string().unwrap(), "daft punk");
    }

    #[test]
    fn search_args_handles_unicode_query() {
        let args = search_args(TrackSourceSlot::UsbSlot, None, "\u{65e5}\u{672c}\u{8a9e}");
        assert_eq!(args[2].as_string().unwrap(), "\u{65e5}\u{672c}\u{8a9e}");
    }

    #[test]
    fn search_args_with_sort() {
        let args = search_args(TrackSourceSlot::UsbSlot, Some(SortOrder::Name), "test");
        assert_eq!(args[1].as_number().unwrap(), 10);
    }

    // -- Slot wire values ------------------------------------------------

    #[test]
    fn all_slot_wire_values_correct() {
        assert_eq!(
            root_args(TrackSourceSlot::CdSlot, None)[0]
                .as_number()
                .unwrap(),
            1
        );
        assert_eq!(
            root_args(TrackSourceSlot::SdSlot, None)[0]
                .as_number()
                .unwrap(),
            2
        );
        assert_eq!(
            root_args(TrackSourceSlot::UsbSlot, None)[0]
                .as_number()
                .unwrap(),
            3
        );
        assert_eq!(
            root_args(TrackSourceSlot::Collection, None)[0]
                .as_number()
                .unwrap(),
            4
        );
    }

    // -- Menu item type round-trip ----------------------------------------

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

    // -- sort_value helper ------------------------------------------------

    #[test]
    fn sort_value_none_returns_default() {
        assert_eq!(sort_value(None), 0);
    }

    #[test]
    fn sort_value_some_returns_protocol_value() {
        assert_eq!(sort_value(Some(SortOrder::Rating)), 6);
        assert_eq!(sort_value(Some(SortOrder::Bitrate)), 11);
    }

    // -- New method arg patterns ------------------------------------------

    #[test]
    fn filtered_args_2_with_sort_for_label_artist() {
        let args = filtered_args_2(TrackSourceSlot::UsbSlot, Some(SortOrder::Album), 100, 200);
        assert_eq!(args.len(), 4);
        assert_eq!(args[0].as_number().unwrap(), 3);
        assert_eq!(args[1].as_number().unwrap(), 1);
        assert_eq!(args[2].as_number().unwrap(), 100);
        assert_eq!(args[3].as_number().unwrap(), 200);
    }

    #[test]
    fn filtered_args_3_with_sort_for_label_artist_album() {
        let args = filtered_args_3(
            TrackSourceSlot::SdSlot,
            Some(SortOrder::DateAdded),
            10,
            20,
            30,
        );
        assert_eq!(args.len(), 5);
        assert_eq!(args[0].as_number().unwrap(), 2);
        assert_eq!(args[1].as_number().unwrap(), 9);
        assert_eq!(args[2].as_number().unwrap(), 10);
        assert_eq!(args[3].as_number().unwrap(), 20);
        assert_eq!(args[4].as_number().unwrap(), 30);
    }
}
