use bytes::{Buf, BufMut, BytesMut};

use super::Field;
use crate::error::{ProDjLinkError, Result};

/// Magic bytes at the start of every dbserver message.
pub const MESSAGE_START: u32 = 0x872349ae;
/// Maximum number of arguments per message.
pub const MAX_ARGS: usize = 12;

// ---------------------------------------------------------------------------
// ANLZ file type and tag constants (from Java Message.java)
// ---------------------------------------------------------------------------

/// ANLZ file type for .DAT files (standard analysis).
pub const ANLZ_FILE_TYPE_DAT: &str = "DAT";
/// ANLZ file type for .EXT files (extended analysis, Nexus).
pub const ANLZ_FILE_TYPE_EXT: &str = "EXT";
/// ANLZ file type for .2EX files (extended analysis, Nexus 2).
pub const ANLZ_FILE_TYPE_2EX: &str = "2EX";

/// ANLZ tag type for colour waveform preview data.
pub const ANLZ_TAG_COLOR_WAVEFORM_PREVIEW: &str = "PWV4";
/// ANLZ tag type for colour waveform detail data.
pub const ANLZ_TAG_COLOR_WAVEFORM_DETAIL: &str = "PWV5";
/// ANLZ tag type for three-band waveform preview data.
pub const ANLZ_TAG_3BAND_WAVEFORM_PREVIEW: &str = "PWV6";
/// ANLZ tag type for three-band waveform detail data.
pub const ANLZ_TAG_3BAND_WAVEFORM_DETAIL: &str = "PWV7";
/// ANLZ tag type for song structure (phrases) data.
pub const ANLZ_TAG_SONG_STRUCTURE: &str = "PSSI";
/// ANLZ tag type for cue point comments.
pub const ANLZ_TAG_CUE_COMMENT: &str = "PCO2";

/// Known dbserver message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    // Lifecycle
    SetupReq,
    InvalidData,
    TeardownReq,

    // Root menu requests
    RootMenuReq,
    GenreMenuReq,
    ArtistMenuReq,
    AlbumMenuReq,
    TrackMenuReq,
    BpmMenuReq,
    RatingMenuReq,
    YearMenuReq,
    LabelMenuReq,
    ColorMenuReq,
    TimeMenuReq,
    BitRateMenuReq,
    HistoryMenuReq,
    FilenameMenuReq,
    KeyMenuReq,

    // Filtered menu requests (single filter)
    ArtistMenuForGenreReq,
    AlbumMenuForArtistReq,
    TrackMenuForAlbumReq,
    PlaylistReq,
    BpmRangeReq,
    TrackMenuForRatingReq,
    YearMenuForDecadeReq,
    ArtistMenuForLabelReq,
    TrackMenuForColorReq,
    TrackMenuForTimeReq,
    TrackMenuForBitRateReq,
    TrackMenuForHistoryReq,
    NeighborMenuForKeyReq,

    // Filtered menu requests (two filters)
    AlbumMenuForGenreAndArtistReq,
    TrackMenuForArtistAndAlbumReq,
    TrackMenuForBpmAndDistanceReq,
    TrackMenuForDecadeAndYearReq,
    AlbumMenuForLabelAndArtistReq,
    TrackMenuForKeyAndDistanceReq,

    // Search and multi-filter
    SearchMenuReq,
    TrackMenuForGenreArtistAndAlbumReq,
    OriginalArtistMenuReq,
    TrackMenuForLabelArtistAndAlbumReq,

    // Original artist / remixer chain
    AlbumMenuForOriginalArtistReq,
    TrackMenuForOriginalArtistAndAlbumReq,
    RemixerMenuReq,
    AlbumMenuForRemixerReq,
    TrackMenuForRemixerAndAlbumReq,

    // Data requests
    MetadataReq,
    AlbumArtReq,
    WaveformPreviewReq,
    FolderMenuReq,
    CueListReq,
    UnanalyzedMetadataReq,
    BeatGridReq,
    WaveformDetailReq,
    CueListExtReq,
    AnlzTagReq,
    RenderMenuReq,

    // Responses
    MenuAvailable,
    MenuHeader,
    AlbumArtResponse,
    Unavailable,
    MenuItem,
    MenuFooter,
    WaveformPreviewResponse,
    BeatGridResponse,
    CueListResponse,
    WaveformDetailResponse,
    CueListExtResponse,
    AnlzTagResponse,

    Unknown(u16),
}

impl From<u16> for MessageType {
    fn from(value: u16) -> Self {
        match value {
            // Lifecycle
            0x0000 => MessageType::SetupReq,
            0x0001 => MessageType::InvalidData,
            0x0100 => MessageType::TeardownReq,

            // Root menu requests
            0x1000 => MessageType::RootMenuReq,
            0x1001 => MessageType::GenreMenuReq,
            0x1002 => MessageType::ArtistMenuReq,
            0x1003 => MessageType::AlbumMenuReq,
            0x1004 => MessageType::TrackMenuReq,
            0x1006 => MessageType::BpmMenuReq,
            0x1007 => MessageType::RatingMenuReq,
            0x1008 => MessageType::YearMenuReq,
            0x100a => MessageType::LabelMenuReq,
            0x100d => MessageType::ColorMenuReq,
            0x1010 => MessageType::TimeMenuReq,
            0x1011 => MessageType::BitRateMenuReq,
            0x1012 => MessageType::HistoryMenuReq,
            0x1013 => MessageType::FilenameMenuReq,
            0x1014 => MessageType::KeyMenuReq,

            // Filtered menu requests (single filter)
            0x1101 => MessageType::ArtistMenuForGenreReq,
            0x1102 => MessageType::AlbumMenuForArtistReq,
            0x1103 => MessageType::TrackMenuForAlbumReq,
            0x1105 => MessageType::PlaylistReq,
            0x1106 => MessageType::BpmRangeReq,
            0x1107 => MessageType::TrackMenuForRatingReq,
            0x1108 => MessageType::YearMenuForDecadeReq,
            0x110a => MessageType::ArtistMenuForLabelReq,
            0x110d => MessageType::TrackMenuForColorReq,
            0x1110 => MessageType::TrackMenuForTimeReq,
            0x1111 => MessageType::TrackMenuForBitRateReq,
            0x1112 => MessageType::TrackMenuForHistoryReq,
            0x1114 => MessageType::NeighborMenuForKeyReq,

            // Filtered menu requests (two filters)
            0x1201 => MessageType::AlbumMenuForGenreAndArtistReq,
            0x1202 => MessageType::TrackMenuForArtistAndAlbumReq,
            0x1206 => MessageType::TrackMenuForBpmAndDistanceReq,
            0x1208 => MessageType::TrackMenuForDecadeAndYearReq,
            0x120a => MessageType::AlbumMenuForLabelAndArtistReq,
            0x1214 => MessageType::TrackMenuForKeyAndDistanceReq,

            // Search and multi-filter
            0x1300 => MessageType::SearchMenuReq,
            0x1301 => MessageType::TrackMenuForGenreArtistAndAlbumReq,
            0x1302 => MessageType::OriginalArtistMenuReq,
            0x130a => MessageType::TrackMenuForLabelArtistAndAlbumReq,

            // Original artist / remixer chain
            0x1402 => MessageType::AlbumMenuForOriginalArtistReq,
            0x1502 => MessageType::TrackMenuForOriginalArtistAndAlbumReq,
            0x1602 => MessageType::RemixerMenuReq,
            0x1702 => MessageType::AlbumMenuForRemixerReq,
            0x1802 => MessageType::TrackMenuForRemixerAndAlbumReq,

            // Data requests
            0x2002 => MessageType::MetadataReq,
            0x2003 => MessageType::AlbumArtReq,
            0x2004 => MessageType::WaveformPreviewReq,
            0x2006 => MessageType::FolderMenuReq,
            0x2104 => MessageType::CueListReq,
            0x2202 => MessageType::UnanalyzedMetadataReq,
            0x2204 => MessageType::BeatGridReq,
            0x2904 => MessageType::WaveformDetailReq,
            0x2b04 => MessageType::CueListExtReq,
            0x2c04 => MessageType::AnlzTagReq,
            0x3000 => MessageType::RenderMenuReq,

            // Responses
            0x4000 => MessageType::MenuAvailable,
            0x4001 => MessageType::MenuHeader,
            0x4002 => MessageType::AlbumArtResponse,
            0x4003 => MessageType::Unavailable,
            0x4101 => MessageType::MenuItem,
            0x4201 => MessageType::MenuFooter,
            0x4402 => MessageType::WaveformPreviewResponse,
            0x4602 => MessageType::BeatGridResponse,
            0x4702 => MessageType::CueListResponse,
            0x4a02 => MessageType::WaveformDetailResponse,
            0x4e02 => MessageType::CueListExtResponse,
            0x4f02 => MessageType::AnlzTagResponse,

            other => MessageType::Unknown(other),
        }
    }
}

impl From<MessageType> for u16 {
    fn from(mt: MessageType) -> u16 {
        match mt {
            // Lifecycle
            MessageType::SetupReq => 0x0000,
            MessageType::InvalidData => 0x0001,
            MessageType::TeardownReq => 0x0100,

            // Root menu requests
            MessageType::RootMenuReq => 0x1000,
            MessageType::GenreMenuReq => 0x1001,
            MessageType::ArtistMenuReq => 0x1002,
            MessageType::AlbumMenuReq => 0x1003,
            MessageType::TrackMenuReq => 0x1004,
            MessageType::BpmMenuReq => 0x1006,
            MessageType::RatingMenuReq => 0x1007,
            MessageType::YearMenuReq => 0x1008,
            MessageType::LabelMenuReq => 0x100a,
            MessageType::ColorMenuReq => 0x100d,
            MessageType::TimeMenuReq => 0x1010,
            MessageType::BitRateMenuReq => 0x1011,
            MessageType::HistoryMenuReq => 0x1012,
            MessageType::FilenameMenuReq => 0x1013,
            MessageType::KeyMenuReq => 0x1014,

            // Filtered menu requests (single filter)
            MessageType::ArtistMenuForGenreReq => 0x1101,
            MessageType::AlbumMenuForArtistReq => 0x1102,
            MessageType::TrackMenuForAlbumReq => 0x1103,
            MessageType::PlaylistReq => 0x1105,
            MessageType::BpmRangeReq => 0x1106,
            MessageType::TrackMenuForRatingReq => 0x1107,
            MessageType::YearMenuForDecadeReq => 0x1108,
            MessageType::ArtistMenuForLabelReq => 0x110a,
            MessageType::TrackMenuForColorReq => 0x110d,
            MessageType::TrackMenuForTimeReq => 0x1110,
            MessageType::TrackMenuForBitRateReq => 0x1111,
            MessageType::TrackMenuForHistoryReq => 0x1112,
            MessageType::NeighborMenuForKeyReq => 0x1114,

            // Filtered menu requests (two filters)
            MessageType::AlbumMenuForGenreAndArtistReq => 0x1201,
            MessageType::TrackMenuForArtistAndAlbumReq => 0x1202,
            MessageType::TrackMenuForBpmAndDistanceReq => 0x1206,
            MessageType::TrackMenuForDecadeAndYearReq => 0x1208,
            MessageType::AlbumMenuForLabelAndArtistReq => 0x120a,
            MessageType::TrackMenuForKeyAndDistanceReq => 0x1214,

            // Search and multi-filter
            MessageType::SearchMenuReq => 0x1300,
            MessageType::TrackMenuForGenreArtistAndAlbumReq => 0x1301,
            MessageType::OriginalArtistMenuReq => 0x1302,
            MessageType::TrackMenuForLabelArtistAndAlbumReq => 0x130a,

            // Original artist / remixer chain
            MessageType::AlbumMenuForOriginalArtistReq => 0x1402,
            MessageType::TrackMenuForOriginalArtistAndAlbumReq => 0x1502,
            MessageType::RemixerMenuReq => 0x1602,
            MessageType::AlbumMenuForRemixerReq => 0x1702,
            MessageType::TrackMenuForRemixerAndAlbumReq => 0x1802,

            // Data requests
            MessageType::MetadataReq => 0x2002,
            MessageType::AlbumArtReq => 0x2003,
            MessageType::WaveformPreviewReq => 0x2004,
            MessageType::FolderMenuReq => 0x2006,
            MessageType::CueListReq => 0x2104,
            MessageType::UnanalyzedMetadataReq => 0x2202,
            MessageType::BeatGridReq => 0x2204,
            MessageType::WaveformDetailReq => 0x2904,
            MessageType::CueListExtReq => 0x2b04,
            MessageType::AnlzTagReq => 0x2c04,
            MessageType::RenderMenuReq => 0x3000,

            // Responses
            MessageType::MenuAvailable => 0x4000,
            MessageType::MenuHeader => 0x4001,
            MessageType::AlbumArtResponse => 0x4002,
            MessageType::Unavailable => 0x4003,
            MessageType::MenuItem => 0x4101,
            MessageType::MenuFooter => 0x4201,
            MessageType::WaveformPreviewResponse => 0x4402,
            MessageType::BeatGridResponse => 0x4602,
            MessageType::CueListResponse => 0x4702,
            MessageType::WaveformDetailResponse => 0x4a02,
            MessageType::CueListExtResponse => 0x4e02,
            MessageType::AnlzTagResponse => 0x4f02,

            MessageType::Unknown(v) => v,
        }
    }
}

/// Menu identifiers used in menu requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MenuIdentifier {
    MainMenu = 1,
    SubMenu = 2,
    TrackInfo = 3,
    SortMenu = 5,
    Data = 8,
}

/// Menu item types that identify what kind of data a menu item represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItemType {
    // Basic metadata types
    Folder,
    AlbumTitle,
    Disc,
    TrackTitle,
    Genre,
    Artist,
    Playlist,
    Rating,
    Duration,
    Tempo,
    Label,
    Key,
    BitRate,
    Year,

    // Color variants
    ColorNone,
    ColorPink,
    ColorRed,
    ColorOrange,
    ColorYellow,
    ColorGreen,
    ColorAqua,
    ColorBlue,
    ColorPurple,

    // Additional metadata
    Comment,
    HistoryPlaylist,
    OriginalArtist,
    Remixer,
    DateAdded,

    // Root menu items
    GenreMenu,
    ArtistMenu,
    AlbumMenu,
    TrackMenu,
    PlaylistMenu,
    BpmMenu,
    RatingMenu,
    YearMenu,
    RemixerMenu,
    LabelMenu,
    OriginalArtistMenu,
    KeyMenu,
    DateAddedMenu,
    ColorMenu,
    FolderMenu,
    SearchMenu,
    TimeMenu,
    BitRateMenu,
    FilenameMenu,
    HistoryMenu,
    HotCueBankMenu,

    // Special
    All,

    // Composite TrackTitle variants
    TrackTitleAndAlbum,
    TrackTitleAndGenre,
    TrackTitleAndArtist,
    TrackTitleAndRating,
    TrackTitleAndDuration,
    TrackTitleAndBpm,
    TrackTitleAndLabel,
    TrackTitleAndKey,
    TrackTitleAndBitRate,
    TrackTitleAndColor,
    TrackTitleAndComment,
    TrackTitleAndOriginalArtist,
    TrackTitleAndRemixer,
    TrackTitleAndDjPlayCount,
    TrackTitleAndDateAdded,

    Unknown(u16),
}

impl From<u16> for MenuItemType {
    fn from(value: u16) -> Self {
        match value {
            // Basic metadata types
            0x0001 => MenuItemType::Folder,
            0x0002 => MenuItemType::AlbumTitle,
            0x0003 => MenuItemType::Disc,
            0x0004 => MenuItemType::TrackTitle,
            0x0006 => MenuItemType::Genre,
            0x0007 => MenuItemType::Artist,
            0x0008 => MenuItemType::Playlist,
            0x000a => MenuItemType::Rating,
            0x000b => MenuItemType::Duration,
            0x000d => MenuItemType::Tempo,
            0x000e => MenuItemType::Label,
            0x000f => MenuItemType::Key,
            0x0010 => MenuItemType::BitRate,
            0x0011 => MenuItemType::Year,

            // Color variants
            0x0013 => MenuItemType::ColorNone,
            0x0014 => MenuItemType::ColorPink,
            0x0015 => MenuItemType::ColorRed,
            0x0016 => MenuItemType::ColorOrange,
            0x0017 => MenuItemType::ColorYellow,
            0x0018 => MenuItemType::ColorGreen,
            0x0019 => MenuItemType::ColorAqua,
            0x001a => MenuItemType::ColorBlue,
            0x001b => MenuItemType::ColorPurple,

            // Additional metadata
            0x0023 => MenuItemType::Comment,
            0x0024 => MenuItemType::HistoryPlaylist,
            0x0028 => MenuItemType::OriginalArtist,
            0x0029 => MenuItemType::Remixer,
            0x002e => MenuItemType::DateAdded,

            // Root menu items
            0x0080 => MenuItemType::GenreMenu,
            0x0081 => MenuItemType::ArtistMenu,
            0x0082 => MenuItemType::AlbumMenu,
            0x0083 => MenuItemType::TrackMenu,
            0x0084 => MenuItemType::PlaylistMenu,
            0x0085 => MenuItemType::BpmMenu,
            0x0086 => MenuItemType::RatingMenu,
            0x0087 => MenuItemType::YearMenu,
            0x0088 => MenuItemType::RemixerMenu,
            0x0089 => MenuItemType::LabelMenu,
            0x008a => MenuItemType::OriginalArtistMenu,
            0x008b => MenuItemType::KeyMenu,
            0x008c => MenuItemType::DateAddedMenu,
            0x008e => MenuItemType::ColorMenu,
            0x0090 => MenuItemType::FolderMenu,
            0x0091 => MenuItemType::SearchMenu,
            0x0092 => MenuItemType::TimeMenu,
            0x0093 => MenuItemType::BitRateMenu,
            0x0094 => MenuItemType::FilenameMenu,
            0x0095 => MenuItemType::HistoryMenu,
            0x0098 => MenuItemType::HotCueBankMenu,

            // Special
            0x00a0 => MenuItemType::All,

            // Composite TrackTitle variants
            0x0204 => MenuItemType::TrackTitleAndAlbum,
            0x0604 => MenuItemType::TrackTitleAndGenre,
            0x0704 => MenuItemType::TrackTitleAndArtist,
            0x0a04 => MenuItemType::TrackTitleAndRating,
            0x0b04 => MenuItemType::TrackTitleAndDuration,
            0x0d04 => MenuItemType::TrackTitleAndBpm,
            0x0e04 => MenuItemType::TrackTitleAndLabel,
            0x0f04 => MenuItemType::TrackTitleAndKey,
            0x1004 => MenuItemType::TrackTitleAndBitRate,
            0x1a04 => MenuItemType::TrackTitleAndColor,
            0x2304 => MenuItemType::TrackTitleAndComment,
            0x2804 => MenuItemType::TrackTitleAndOriginalArtist,
            0x2904 => MenuItemType::TrackTitleAndRemixer,
            0x2a04 => MenuItemType::TrackTitleAndDjPlayCount,
            0x2e04 => MenuItemType::TrackTitleAndDateAdded,

            other => MenuItemType::Unknown(other),
        }
    }
}

impl From<MenuItemType> for u16 {
    fn from(mt: MenuItemType) -> u16 {
        match mt {
            // Basic metadata types
            MenuItemType::Folder => 0x0001,
            MenuItemType::AlbumTitle => 0x0002,
            MenuItemType::Disc => 0x0003,
            MenuItemType::TrackTitle => 0x0004,
            MenuItemType::Genre => 0x0006,
            MenuItemType::Artist => 0x0007,
            MenuItemType::Playlist => 0x0008,
            MenuItemType::Rating => 0x000a,
            MenuItemType::Duration => 0x000b,
            MenuItemType::Tempo => 0x000d,
            MenuItemType::Label => 0x000e,
            MenuItemType::Key => 0x000f,
            MenuItemType::BitRate => 0x0010,
            MenuItemType::Year => 0x0011,

            // Color variants
            MenuItemType::ColorNone => 0x0013,
            MenuItemType::ColorPink => 0x0014,
            MenuItemType::ColorRed => 0x0015,
            MenuItemType::ColorOrange => 0x0016,
            MenuItemType::ColorYellow => 0x0017,
            MenuItemType::ColorGreen => 0x0018,
            MenuItemType::ColorAqua => 0x0019,
            MenuItemType::ColorBlue => 0x001a,
            MenuItemType::ColorPurple => 0x001b,

            // Additional metadata
            MenuItemType::Comment => 0x0023,
            MenuItemType::HistoryPlaylist => 0x0024,
            MenuItemType::OriginalArtist => 0x0028,
            MenuItemType::Remixer => 0x0029,
            MenuItemType::DateAdded => 0x002e,

            // Root menu items
            MenuItemType::GenreMenu => 0x0080,
            MenuItemType::ArtistMenu => 0x0081,
            MenuItemType::AlbumMenu => 0x0082,
            MenuItemType::TrackMenu => 0x0083,
            MenuItemType::PlaylistMenu => 0x0084,
            MenuItemType::BpmMenu => 0x0085,
            MenuItemType::RatingMenu => 0x0086,
            MenuItemType::YearMenu => 0x0087,
            MenuItemType::RemixerMenu => 0x0088,
            MenuItemType::LabelMenu => 0x0089,
            MenuItemType::OriginalArtistMenu => 0x008a,
            MenuItemType::KeyMenu => 0x008b,
            MenuItemType::DateAddedMenu => 0x008c,
            MenuItemType::ColorMenu => 0x008e,
            MenuItemType::FolderMenu => 0x0090,
            MenuItemType::SearchMenu => 0x0091,
            MenuItemType::TimeMenu => 0x0092,
            MenuItemType::BitRateMenu => 0x0093,
            MenuItemType::FilenameMenu => 0x0094,
            MenuItemType::HistoryMenu => 0x0095,
            MenuItemType::HotCueBankMenu => 0x0098,

            // Special
            MenuItemType::All => 0x00a0,

            // Composite TrackTitle variants
            MenuItemType::TrackTitleAndAlbum => 0x0204,
            MenuItemType::TrackTitleAndGenre => 0x0604,
            MenuItemType::TrackTitleAndArtist => 0x0704,
            MenuItemType::TrackTitleAndRating => 0x0a04,
            MenuItemType::TrackTitleAndDuration => 0x0b04,
            MenuItemType::TrackTitleAndBpm => 0x0d04,
            MenuItemType::TrackTitleAndLabel => 0x0e04,
            MenuItemType::TrackTitleAndKey => 0x0f04,
            MenuItemType::TrackTitleAndBitRate => 0x1004,
            MenuItemType::TrackTitleAndColor => 0x1a04,
            MenuItemType::TrackTitleAndComment => 0x2304,
            MenuItemType::TrackTitleAndOriginalArtist => 0x2804,
            MenuItemType::TrackTitleAndRemixer => 0x2904,
            MenuItemType::TrackTitleAndDjPlayCount => 0x2a04,
            MenuItemType::TrackTitleAndDateAdded => 0x2e04,

            MenuItemType::Unknown(v) => v,
        }
    }
}

/// A dbserver protocol message.
#[derive(Debug, Clone)]
pub struct Message {
    /// Transaction ID for request/response matching.
    pub transaction: u32,
    /// The message type.
    pub kind: MessageType,
    /// The arguments (0–12 typed fields).
    pub args: Vec<Field>,
}

impl Message {
    /// Create a new message.
    pub fn new(transaction: u32, kind: MessageType, args: Vec<Field>) -> Self {
        Self {
            transaction,
            kind,
            args,
        }
    }

    /// Parse a message from a byte buffer, advancing the cursor.
    pub fn parse(buf: &mut impl Buf) -> Result<Self> {
        if buf.remaining() < 4 {
            return Err(ProDjLinkError::DbServer(
                "not enough data for message magic".into(),
            ));
        }
        let magic = buf.get_u32();
        if magic != MESSAGE_START {
            return Err(ProDjLinkError::DbServer(format!(
                "invalid message magic: expected 0x{MESSAGE_START:08x}, got 0x{magic:08x}"
            )));
        }

        if buf.remaining() < 7 {
            return Err(ProDjLinkError::DbServer(
                "not enough data for message header".into(),
            ));
        }
        let transaction = buf.get_u32();
        let kind = MessageType::from(buf.get_u16());
        let arg_count = buf.get_u8() as usize;

        if arg_count > MAX_ARGS {
            return Err(ProDjLinkError::DbServer(format!(
                "too many arguments: {arg_count} exceeds maximum of {MAX_ARGS}"
            )));
        }

        if buf.remaining() < arg_count {
            return Err(ProDjLinkError::DbServer(
                "not enough data for argument type list".into(),
            ));
        }
        // Read (and discard) the type-tag bytes; Field::parse reads its own tag.
        for _ in 0..arg_count {
            let _ = buf.get_u8();
        }

        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            let field = Field::parse(buf).map_err(|e| {
                ProDjLinkError::DbServer(format!("failed to parse argument {i}: {e}"))
            })?;
            args.push(field);
        }

        Ok(Self {
            transaction,
            kind,
            args,
        })
    }

    /// Serialize this message to bytes.
    pub fn serialize(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(self.transaction);
        buf.put_u16(u16::from(self.kind));
        buf.put_u8(self.args.len() as u8);

        // Argument type tags
        for arg in &self.args {
            buf.put_u8(arg.arg_type());
        }

        // Serialized field values
        for arg in &self.args {
            arg.serialize(&mut buf);
        }

        buf
    }

    /// Get argument at index, or error if out of bounds.
    pub fn arg(&self, index: usize) -> Result<&Field> {
        self.args.get(index).ok_or_else(|| {
            ProDjLinkError::DbServer(format!(
                "argument index {index} out of bounds (message has {} args)",
                self.args.len()
            ))
        })
    }

    /// Convenience: get a number argument at index.
    pub fn arg_number(&self, index: usize) -> Result<u32> {
        self.arg(index)?.as_number()
    }

    /// Convenience: get a string argument at index.
    pub fn arg_string(&self, index: usize) -> Result<&str> {
        self.arg(index)?.as_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    /// Build the raw wire bytes for a message by hand.
    fn build_wire_bytes(transaction: u32, kind: u16, fields: &[Field]) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(transaction);
        buf.put_u16(kind);
        buf.put_u8(fields.len() as u8);
        for f in fields {
            buf.put_u8(f.arg_type());
        }
        for f in fields {
            f.serialize(&mut buf);
        }
        buf
    }

    #[test]
    fn parse_hand_crafted_message() {
        let fields = vec![Field::number_with_size(1, 4), Field::string("hello")];
        let wire = build_wire_bytes(0x01, 0x2002, &fields);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.transaction, 0x01);
        assert_eq!(msg.kind, MessageType::MetadataReq);
        assert_eq!(msg.args.len(), 2);
        assert_eq!(msg.arg_number(0).unwrap(), 1);
        assert_eq!(msg.arg_string(1).unwrap(), "hello");
    }

    #[test]
    fn serialize_and_verify_wire_bytes() {
        let msg = Message::new(
            0x07,
            MessageType::SetupReq,
            vec![Field::number_with_size(0x11, 4)],
        );
        let wire = msg.serialize();
        let expected = build_wire_bytes(0x07, 0x0000, &[Field::number_with_size(0x11, 4)]);
        assert_eq!(wire, expected);
    }

    #[test]
    fn round_trip() {
        let original = Message::new(
            42,
            MessageType::AlbumArtReq,
            vec![
                Field::number_with_size(100, 4),
                Field::binary(vec![0xDE, 0xAD]),
                Field::string("track.mp3"),
            ],
        );
        let wire = original.serialize();
        let parsed = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(parsed.transaction, original.transaction);
        assert_eq!(parsed.kind, original.kind);
        assert_eq!(parsed.args, original.args);
    }

    #[test]
    fn parse_zero_args() {
        let wire = build_wire_bytes(0, 0x4000, &[]);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.kind, MessageType::MenuAvailable);
        assert!(msg.args.is_empty());
    }

    #[test]
    fn parse_all_three_field_types() {
        let fields = vec![
            Field::number_with_size(0xFF, 1),
            Field::binary(Bytes::from_static(&[1, 2, 3])),
            Field::string("mixed"),
        ];
        let wire = build_wire_bytes(99, 0x3000, &fields);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.args.len(), 3);
        assert_eq!(msg.args[0].as_number().unwrap(), 0xFF);
        assert_eq!(msg.args[1].as_binary().unwrap().as_ref(), &[1, 2, 3]);
        assert_eq!(msg.args[2].as_string().unwrap(), "mixed");
    }

    #[test]
    fn error_on_invalid_magic() {
        let mut buf = BytesMut::new();
        buf.put_u32(0xDEADBEEF); // wrong magic
        buf.put_u32(0);
        buf.put_u16(0);
        buf.put_u8(0);

        let err = Message::parse(&mut &buf[..]).unwrap_err();
        assert!(err.to_string().contains("invalid message magic"));
    }

    #[test]
    fn error_on_too_many_args() {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(0);
        buf.put_u16(0);
        buf.put_u8(13); // exceeds MAX_ARGS

        let err = Message::parse(&mut &buf[..]).unwrap_err();
        assert!(err.to_string().contains("too many arguments"));
    }

    #[test]
    fn message_type_round_trip() {
        let known_types: &[(u16, MessageType)] = &[
            // Lifecycle
            (0x0000, MessageType::SetupReq),
            (0x0001, MessageType::InvalidData),
            (0x0100, MessageType::TeardownReq),
            // Root menu requests
            (0x1000, MessageType::RootMenuReq),
            (0x1001, MessageType::GenreMenuReq),
            (0x1002, MessageType::ArtistMenuReq),
            (0x1003, MessageType::AlbumMenuReq),
            (0x1004, MessageType::TrackMenuReq),
            (0x1006, MessageType::BpmMenuReq),
            (0x1007, MessageType::RatingMenuReq),
            (0x1008, MessageType::YearMenuReq),
            (0x100a, MessageType::LabelMenuReq),
            (0x100d, MessageType::ColorMenuReq),
            (0x1010, MessageType::TimeMenuReq),
            (0x1011, MessageType::BitRateMenuReq),
            (0x1012, MessageType::HistoryMenuReq),
            (0x1013, MessageType::FilenameMenuReq),
            (0x1014, MessageType::KeyMenuReq),
            // Filtered menu requests (single filter)
            (0x1101, MessageType::ArtistMenuForGenreReq),
            (0x1102, MessageType::AlbumMenuForArtistReq),
            (0x1103, MessageType::TrackMenuForAlbumReq),
            (0x1105, MessageType::PlaylistReq),
            (0x1106, MessageType::BpmRangeReq),
            (0x1107, MessageType::TrackMenuForRatingReq),
            (0x1108, MessageType::YearMenuForDecadeReq),
            (0x110a, MessageType::ArtistMenuForLabelReq),
            (0x110d, MessageType::TrackMenuForColorReq),
            (0x1110, MessageType::TrackMenuForTimeReq),
            (0x1111, MessageType::TrackMenuForBitRateReq),
            (0x1112, MessageType::TrackMenuForHistoryReq),
            (0x1114, MessageType::NeighborMenuForKeyReq),
            // Filtered menu requests (two filters)
            (0x1201, MessageType::AlbumMenuForGenreAndArtistReq),
            (0x1202, MessageType::TrackMenuForArtistAndAlbumReq),
            (0x1206, MessageType::TrackMenuForBpmAndDistanceReq),
            (0x1208, MessageType::TrackMenuForDecadeAndYearReq),
            (0x120a, MessageType::AlbumMenuForLabelAndArtistReq),
            (0x1214, MessageType::TrackMenuForKeyAndDistanceReq),
            // Search and multi-filter
            (0x1300, MessageType::SearchMenuReq),
            (0x1301, MessageType::TrackMenuForGenreArtistAndAlbumReq),
            (0x1302, MessageType::OriginalArtistMenuReq),
            (0x130a, MessageType::TrackMenuForLabelArtistAndAlbumReq),
            // Original artist / remixer chain
            (0x1402, MessageType::AlbumMenuForOriginalArtistReq),
            (0x1502, MessageType::TrackMenuForOriginalArtistAndAlbumReq),
            (0x1602, MessageType::RemixerMenuReq),
            (0x1702, MessageType::AlbumMenuForRemixerReq),
            (0x1802, MessageType::TrackMenuForRemixerAndAlbumReq),
            // Data requests
            (0x2002, MessageType::MetadataReq),
            (0x2003, MessageType::AlbumArtReq),
            (0x2004, MessageType::WaveformPreviewReq),
            (0x2006, MessageType::FolderMenuReq),
            (0x2104, MessageType::CueListReq),
            (0x2202, MessageType::UnanalyzedMetadataReq),
            (0x2204, MessageType::BeatGridReq),
            (0x2904, MessageType::WaveformDetailReq),
            (0x2b04, MessageType::CueListExtReq),
            (0x2c04, MessageType::AnlzTagReq),
            (0x3000, MessageType::RenderMenuReq),
            // Responses
            (0x4000, MessageType::MenuAvailable),
            (0x4001, MessageType::MenuHeader),
            (0x4002, MessageType::AlbumArtResponse),
            (0x4003, MessageType::Unavailable),
            (0x4101, MessageType::MenuItem),
            (0x4201, MessageType::MenuFooter),
            (0x4402, MessageType::WaveformPreviewResponse),
            (0x4602, MessageType::BeatGridResponse),
            (0x4702, MessageType::CueListResponse),
            (0x4a02, MessageType::WaveformDetailResponse),
            (0x4e02, MessageType::CueListExtResponse),
            (0x4f02, MessageType::AnlzTagResponse),
        ];
        for &(raw, expected) in known_types {
            let mt = MessageType::from(raw);
            assert_eq!(mt, expected, "MessageType::from(0x{raw:04x})");
            assert_eq!(u16::from(mt), raw, "u16::from({expected:?})");
        }

        // Unknown variant round-trips
        let unknown = MessageType::from(0xBEEF);
        assert_eq!(unknown, MessageType::Unknown(0xBEEF));
        assert_eq!(u16::from(unknown), 0xBEEF);
    }

    #[test]
    fn menu_item_type_round_trip() {
        let known_types: &[(u16, MenuItemType)] = &[
            // Basic metadata types
            (0x0001, MenuItemType::Folder),
            (0x0002, MenuItemType::AlbumTitle),
            (0x0003, MenuItemType::Disc),
            (0x0004, MenuItemType::TrackTitle),
            (0x0006, MenuItemType::Genre),
            (0x0007, MenuItemType::Artist),
            (0x0008, MenuItemType::Playlist),
            (0x000a, MenuItemType::Rating),
            (0x000b, MenuItemType::Duration),
            (0x000d, MenuItemType::Tempo),
            (0x000e, MenuItemType::Label),
            (0x000f, MenuItemType::Key),
            (0x0010, MenuItemType::BitRate),
            (0x0011, MenuItemType::Year),
            // Color variants
            (0x0013, MenuItemType::ColorNone),
            (0x0014, MenuItemType::ColorPink),
            (0x0015, MenuItemType::ColorRed),
            (0x0016, MenuItemType::ColorOrange),
            (0x0017, MenuItemType::ColorYellow),
            (0x0018, MenuItemType::ColorGreen),
            (0x0019, MenuItemType::ColorAqua),
            (0x001a, MenuItemType::ColorBlue),
            (0x001b, MenuItemType::ColorPurple),
            // Additional metadata
            (0x0023, MenuItemType::Comment),
            (0x0024, MenuItemType::HistoryPlaylist),
            (0x0028, MenuItemType::OriginalArtist),
            (0x0029, MenuItemType::Remixer),
            (0x002e, MenuItemType::DateAdded),
            // Root menu items
            (0x0080, MenuItemType::GenreMenu),
            (0x0081, MenuItemType::ArtistMenu),
            (0x0082, MenuItemType::AlbumMenu),
            (0x0083, MenuItemType::TrackMenu),
            (0x0084, MenuItemType::PlaylistMenu),
            (0x0085, MenuItemType::BpmMenu),
            (0x0086, MenuItemType::RatingMenu),
            (0x0087, MenuItemType::YearMenu),
            (0x0088, MenuItemType::RemixerMenu),
            (0x0089, MenuItemType::LabelMenu),
            (0x008a, MenuItemType::OriginalArtistMenu),
            (0x008b, MenuItemType::KeyMenu),
            (0x008c, MenuItemType::DateAddedMenu),
            (0x008e, MenuItemType::ColorMenu),
            (0x0090, MenuItemType::FolderMenu),
            (0x0091, MenuItemType::SearchMenu),
            (0x0092, MenuItemType::TimeMenu),
            (0x0093, MenuItemType::BitRateMenu),
            (0x0094, MenuItemType::FilenameMenu),
            (0x0095, MenuItemType::HistoryMenu),
            (0x0098, MenuItemType::HotCueBankMenu),
            // Special
            (0x00a0, MenuItemType::All),
            // Composite TrackTitle variants
            (0x0204, MenuItemType::TrackTitleAndAlbum),
            (0x0604, MenuItemType::TrackTitleAndGenre),
            (0x0704, MenuItemType::TrackTitleAndArtist),
            (0x0a04, MenuItemType::TrackTitleAndRating),
            (0x0b04, MenuItemType::TrackTitleAndDuration),
            (0x0d04, MenuItemType::TrackTitleAndBpm),
            (0x0e04, MenuItemType::TrackTitleAndLabel),
            (0x0f04, MenuItemType::TrackTitleAndKey),
            (0x1004, MenuItemType::TrackTitleAndBitRate),
            (0x1a04, MenuItemType::TrackTitleAndColor),
            (0x2304, MenuItemType::TrackTitleAndComment),
            (0x2804, MenuItemType::TrackTitleAndOriginalArtist),
            (0x2904, MenuItemType::TrackTitleAndRemixer),
            (0x2a04, MenuItemType::TrackTitleAndDjPlayCount),
            (0x2e04, MenuItemType::TrackTitleAndDateAdded),
        ];
        for &(raw, expected) in known_types {
            let mt = MenuItemType::from(raw);
            assert_eq!(mt, expected, "MenuItemType::from(0x{raw:04x})");
            assert_eq!(u16::from(mt), raw, "u16::from({expected:?})");
        }

        // Unknown variant round-trips
        let unknown = MenuItemType::from(0xFFFF);
        assert_eq!(unknown, MenuItemType::Unknown(0xFFFF));
        assert_eq!(u16::from(unknown), 0xFFFF);
    }

    #[test]
    fn arg_accessor_bounds_checking() {
        let msg = Message::new(1, MessageType::SetupReq, vec![Field::number(5)]);

        assert!(msg.arg(0).is_ok());
        assert!(msg.arg(1).is_err());
        assert!(msg.arg_number(0).is_ok());
        assert!(msg.arg_number(1).is_err());
        assert!(msg.arg_string(0).is_err()); // wrong type
    }

    #[test]
    fn anlz_file_type_constants() {
        assert_eq!(ANLZ_FILE_TYPE_DAT, "DAT");
        assert_eq!(ANLZ_FILE_TYPE_EXT, "EXT");
        assert_eq!(ANLZ_FILE_TYPE_2EX, "2EX");
    }

    #[test]
    fn anlz_tag_constants() {
        assert_eq!(ANLZ_TAG_COLOR_WAVEFORM_PREVIEW, "PWV4");
        assert_eq!(ANLZ_TAG_COLOR_WAVEFORM_DETAIL, "PWV5");
        assert_eq!(ANLZ_TAG_3BAND_WAVEFORM_PREVIEW, "PWV6");
        assert_eq!(ANLZ_TAG_3BAND_WAVEFORM_DETAIL, "PWV7");
        assert_eq!(ANLZ_TAG_SONG_STRUCTURE, "PSSI");
        assert_eq!(ANLZ_TAG_CUE_COMMENT, "PCO2");
    }
}
