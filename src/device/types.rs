use std::fmt;

// ---------------------------------------------------------------------------
// DeviceNumber
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceNumber(pub u8);

impl DeviceNumber {
    pub fn new(n: u8) -> Option<Self> {
        Some(Self(n))
    }
}

impl fmt::Display for DeviceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u8> for DeviceNumber {
    fn from(n: u8) -> Self {
        Self(n)
    }
}

// ---------------------------------------------------------------------------
// BeatNumber
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeatNumber(pub u32);

// ---------------------------------------------------------------------------
// Bpm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bpm(pub f64);

impl fmt::Display for Bpm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Pitch
// ---------------------------------------------------------------------------

const PITCH_NORMAL: f64 = 0x100000 as f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pitch(pub i32);

impl Pitch {
    pub fn to_percentage(&self) -> f64 {
        (self.0 as f64 - PITCH_NORMAL) / PITCH_NORMAL * 100.0
    }

    pub fn to_multiplier(&self) -> f64 {
        self.0 as f64 / PITCH_NORMAL
    }

    pub fn from_percentage(pct: f64) -> Self {
        Self(((pct / 100.0 * PITCH_NORMAL) + PITCH_NORMAL) as i32)
    }

    pub fn effective_bpm(&self, base_bpm: Bpm) -> Bpm {
        Bpm(base_bpm.0 * self.to_multiplier())
    }
}

// ---------------------------------------------------------------------------
// DeviceType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Cdj,
    Mixer,
    Rekordbox,
    Unknown(u8),
}

impl From<u8> for DeviceType {
    fn from(n: u8) -> Self {
        match n {
            1 => Self::Cdj,
            2 => Self::Mixer,
            3 => Self::Rekordbox,
            _ => Self::Unknown(n),
        }
    }
}

impl From<DeviceType> for u8 {
    fn from(dt: DeviceType) -> u8 {
        match dt {
            DeviceType::Cdj => 1,
            DeviceType::Mixer => 2,
            DeviceType::Rekordbox => 3,
            DeviceType::Unknown(n) => n,
        }
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cdj => write!(f, "CDJ"),
            Self::Mixer => write!(f, "Mixer"),
            Self::Rekordbox => write!(f, "rekordbox"),
            Self::Unknown(n) => write!(f, "Unknown({n})"),
        }
    }
}

// ---------------------------------------------------------------------------
// TrackSourceSlot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackSourceSlot {
    NoTrack,
    CdSlot,
    SdSlot,
    UsbSlot,
    Collection,
    /// Second USB port, used by XDJ-XZ in four-deck mode (wire value 7).
    Usb2Slot,
    Unknown(u8),
}

impl From<u8> for TrackSourceSlot {
    fn from(n: u8) -> Self {
        match n {
            0 => Self::NoTrack,
            1 => Self::CdSlot,
            2 => Self::SdSlot,
            3 => Self::UsbSlot,
            4 => Self::Collection,
            7 => Self::Usb2Slot,
            _ => Self::Unknown(n),
        }
    }
}

impl From<TrackSourceSlot> for u8 {
    fn from(slot: TrackSourceSlot) -> u8 {
        match slot {
            TrackSourceSlot::NoTrack => 0,
            TrackSourceSlot::CdSlot => 1,
            TrackSourceSlot::SdSlot => 2,
            TrackSourceSlot::UsbSlot => 3,
            TrackSourceSlot::Collection => 4,
            TrackSourceSlot::Usb2Slot => 7,
            TrackSourceSlot::Unknown(n) => n,
        }
    }
}

// ---------------------------------------------------------------------------
// TrackType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackType {
    NoTrack,
    Rekordbox,
    Unanalyzed,
    CdDigitalAudio,
    Unknown(u8),
}

impl From<u8> for TrackType {
    fn from(n: u8) -> Self {
        match n {
            0 => Self::NoTrack,
            1 => Self::Rekordbox,
            2 => Self::Unanalyzed,
            5 => Self::CdDigitalAudio,
            _ => Self::Unknown(n),
        }
    }
}

// ---------------------------------------------------------------------------
// PlayState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayState {
    /// No track has been loaded (0x00).
    NoTrack,
    /// A track is being loaded (0x02).
    Loading,
    /// Playing normally (0x03).
    Playing,
    /// Playing a loop (0x04).
    Looping,
    /// Paused anywhere other than the cue point (0x05).
    Paused,
    /// Paused at the cue point (0x06).
    Cued,
    /// Cue play in progress — playback while cue button held (0x07).
    CuePlaying,
    /// Cue scratch — returns to cue point when jog wheel released (0x08).
    CueScratch,
    /// Searching forwards or backwards (0x09).
    Searching,
    /// Reached end of track and stopped (0x11).
    Ended,
    Unknown(u8),
}

impl From<u8> for PlayState {
    fn from(n: u8) -> Self {
        match n {
            0x00 => Self::NoTrack,
            0x02 => Self::Loading,
            0x03 => Self::Playing,
            0x04 => Self::Looping,
            0x05 => Self::Paused,
            0x06 => Self::Cued,
            0x07 => Self::CuePlaying,
            0x08 => Self::CueScratch,
            0x09 => Self::Searching,
            0x11 => Self::Ended,
            _ => Self::Unknown(n),
        }
    }
}

// ---------------------------------------------------------------------------
// PlayState2
// ---------------------------------------------------------------------------

/// Secondary play state indicating motion (byte at offset 0x8b in CDJ status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayState2 {
    /// Player is moving (playing/searching).
    Moving,
    /// Player is stopped.
    Stopped,
    /// Opus Quad specific: deck is moving (value `0xfa`).
    OpusMoving,
    /// Unknown value.
    Unknown(u8),
}

impl PlayState2 {
    /// Whether this state indicates the deck is in motion.
    pub fn is_moving(&self) -> bool {
        matches!(self, Self::Moving | Self::OpusMoving)
    }
}

impl From<u8> for PlayState2 {
    fn from(n: u8) -> Self {
        match n {
            0x6a | 0x7a => Self::Moving,
            0xfa => Self::OpusMoving,
            0x6e | 0x7e | 0xfe => Self::Stopped,
            _ => Self::Unknown(n),
        }
    }
}

// ---------------------------------------------------------------------------
// PlayState3
// ---------------------------------------------------------------------------

/// Tertiary play state indicating jog mode and direction (byte at offset 0x9d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayState3 {
    /// No track is loaded.
    NoTrack,
    /// Player is paused or playing in reverse.
    PausedOrReverse,
    /// Playing forward in vinyl jog mode.
    ForwardVinyl,
    /// Playing forward in CDJ jog mode.
    ForwardCdj,
    /// Unknown value.
    Unknown(u8),
}

impl From<u8> for PlayState3 {
    fn from(n: u8) -> Self {
        match n {
            0x00 => Self::NoTrack,
            0x01 => Self::PausedOrReverse,
            0x09 => Self::ForwardVinyl,
            0x0d => Self::ForwardCdj,
            _ => Self::Unknown(n),
        }
    }
}

// ---------------------------------------------------------------------------
// OnAirStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnAirStatus {
    On,
    Off,
}

// ---------------------------------------------------------------------------
// OpusQuadMode
// ---------------------------------------------------------------------------

/// Operating mode when interacting with a Pioneer Opus Quad all-in-one unit.
///
/// The Opus Quad appears as a single device on the network but exposes 4
/// internal players.  Depending on the chosen mode the virtual CDJ will
/// participate differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpusQuadMode {
    /// Normal mode — we claim a device number and participate in the network.
    Normal,
    /// Lighting mode — we act as a rekordbox-lighting proxy (device number 0x11 / 17).
    Lighting,
    /// Direct database access via SQLite — no network participation needed for metadata.
    DirectDatabase,
}

impl Default for OpusQuadMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl OpusQuadMode {
    /// Device number to claim when in lighting mode.
    pub const LIGHTING_DEVICE_NUMBER: u8 = 0x11;
}

// ---------------------------------------------------------------------------
// Opus Quad device-number remapping
// ---------------------------------------------------------------------------

/// Range base for normal Opus Quad internal device numbers (9–12 → decks 1–4).
const OPUS_QUAD_NORMAL_BASE: u8 = 9;
/// Range base for Opus Quad lighting-mode device numbers (17–20 → decks 1–4).
const OPUS_QUAD_LIGHTING_BASE: u8 = 17;

/// Remap an Opus Quad raw device number to a standard player number (1–4).
///
/// The Opus Quad uses internal numbers 9–12 (normal) or 17–20 (lighting mode).
/// Returns the original number unchanged if it does not fall in either range.
pub fn remap_opus_quad_device(raw_num: u8) -> u8 {
    if (OPUS_QUAD_NORMAL_BASE..OPUS_QUAD_NORMAL_BASE + 4).contains(&raw_num) {
        raw_num - OPUS_QUAD_NORMAL_BASE + 1
    } else if (OPUS_QUAD_LIGHTING_BASE..OPUS_QUAD_LIGHTING_BASE + 4).contains(&raw_num) {
        raw_num - OPUS_QUAD_LIGHTING_BASE + 1
    } else {
        raw_num
    }
}

/// Map a standard player number (1–4) back to the Opus Quad internal number.
///
/// Uses the normal range (9–12) by default.  Pass `lighting = true` to map
/// into the lighting range (17–20).  Returns the original number unchanged
/// if it is not in 1–4.
pub fn unmap_opus_quad_device(player_num: u8, lighting: bool) -> u8 {
    if (1..=4).contains(&player_num) {
        let base = if lighting {
            OPUS_QUAD_LIGHTING_BASE
        } else {
            OPUS_QUAD_NORMAL_BASE
        };
        player_num - 1 + base
    } else {
        player_num
    }
}

// ---------------------------------------------------------------------------
// SlotReference
// ---------------------------------------------------------------------------

/// A reference to a specific media slot on a specific player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotReference {
    pub player: DeviceNumber,
    pub slot: TrackSourceSlot,
}

impl SlotReference {
    pub fn new(player: DeviceNumber, slot: TrackSourceSlot) -> Self {
        Self { player, slot }
    }
}

impl fmt::Display for SlotReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Player {} {:?}", self.player, self.slot)
    }
}

// ---------------------------------------------------------------------------
// DeckReference
// ---------------------------------------------------------------------------

/// A reference to a specific deck (player + hot cue number).
/// hot_cue = 0 means the main deck, 1-8 are hot cue slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeckReference {
    pub player: DeviceNumber,
    pub hot_cue: u8,
}

impl DeckReference {
    pub fn new(player: DeviceNumber, hot_cue: u8) -> Self {
        Self { player, hot_cue }
    }

    /// Reference to the main deck of a player (not a hot cue).
    pub fn main_deck(player: DeviceNumber) -> Self {
        Self { player, hot_cue: 0 }
    }
}

impl fmt::Display for DeckReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hot_cue == 0 {
            write!(f, "Player {}", self.player)
        } else {
            write!(f, "Player {} Hot Cue {}", self.player, self.hot_cue)
        }
    }
}

// ---------------------------------------------------------------------------
// PlaybackState
// ---------------------------------------------------------------------------

/// An immutable snapshot of a player's playback position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackState {
    pub player: DeviceNumber,
    /// Current playback position in milliseconds.
    pub position: u64,
    /// Whether the player is currently playing.
    pub playing: bool,
}

impl PlaybackState {
    pub fn new(player: DeviceNumber, position: u64, playing: bool) -> Self {
        Self {
            player,
            position,
            playing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- OpusQuadMode --

    #[test]
    fn opus_quad_mode_default_is_normal() {
        assert_eq!(OpusQuadMode::default(), OpusQuadMode::Normal);
    }

    #[test]
    fn opus_quad_mode_lighting_device_number() {
        assert_eq!(OpusQuadMode::LIGHTING_DEVICE_NUMBER, 0x11);
        assert_eq!(OpusQuadMode::LIGHTING_DEVICE_NUMBER, 17);
    }

    #[test]
    fn opus_quad_mode_variants_are_distinct() {
        assert_ne!(OpusQuadMode::Normal, OpusQuadMode::Lighting);
        assert_ne!(OpusQuadMode::Normal, OpusQuadMode::DirectDatabase);
        assert_ne!(OpusQuadMode::Lighting, OpusQuadMode::DirectDatabase);
    }

    #[test]
    fn opus_quad_mode_debug_display() {
        let _ = format!("{:?}", OpusQuadMode::Normal);
        let _ = format!("{:?}", OpusQuadMode::Lighting);
        let _ = format!("{:?}", OpusQuadMode::DirectDatabase);
    }

    // -- remap_opus_quad_device --

    #[test]
    fn remap_normal_range_9_to_12() {
        assert_eq!(remap_opus_quad_device(9), 1);
        assert_eq!(remap_opus_quad_device(10), 2);
        assert_eq!(remap_opus_quad_device(11), 3);
        assert_eq!(remap_opus_quad_device(12), 4);
    }

    #[test]
    fn remap_lighting_range_17_to_20() {
        assert_eq!(remap_opus_quad_device(17), 1);
        assert_eq!(remap_opus_quad_device(18), 2);
        assert_eq!(remap_opus_quad_device(19), 3);
        assert_eq!(remap_opus_quad_device(20), 4);
    }

    #[test]
    fn remap_passthrough_for_other_numbers() {
        assert_eq!(remap_opus_quad_device(1), 1);
        assert_eq!(remap_opus_quad_device(5), 5);
        assert_eq!(remap_opus_quad_device(8), 8);
        assert_eq!(remap_opus_quad_device(13), 13);
        assert_eq!(remap_opus_quad_device(16), 16);
        assert_eq!(remap_opus_quad_device(21), 21);
        assert_eq!(remap_opus_quad_device(33), 33);
    }

    // -- unmap_opus_quad_device --

    #[test]
    fn unmap_normal_range() {
        assert_eq!(unmap_opus_quad_device(1, false), 9);
        assert_eq!(unmap_opus_quad_device(2, false), 10);
        assert_eq!(unmap_opus_quad_device(3, false), 11);
        assert_eq!(unmap_opus_quad_device(4, false), 12);
    }

    #[test]
    fn unmap_lighting_range() {
        assert_eq!(unmap_opus_quad_device(1, true), 17);
        assert_eq!(unmap_opus_quad_device(2, true), 18);
        assert_eq!(unmap_opus_quad_device(3, true), 19);
        assert_eq!(unmap_opus_quad_device(4, true), 20);
    }

    #[test]
    fn unmap_passthrough_for_other_numbers() {
        assert_eq!(unmap_opus_quad_device(5, false), 5);
        assert_eq!(unmap_opus_quad_device(0, false), 0);
        assert_eq!(unmap_opus_quad_device(33, true), 33);
    }

    #[test]
    fn remap_unmap_round_trip_normal() {
        for player in 1..=4u8 {
            let raw = unmap_opus_quad_device(player, false);
            assert_eq!(remap_opus_quad_device(raw), player);
        }
    }

    #[test]
    fn remap_unmap_round_trip_lighting() {
        for player in 1..=4u8 {
            let raw = unmap_opus_quad_device(player, true);
            assert_eq!(remap_opus_quad_device(raw), player);
        }
    }

    // -- PlayState2 --

    #[test]
    fn play_state_2_is_moving() {
        assert!(PlayState2::Moving.is_moving());
        assert!(PlayState2::OpusMoving.is_moving());
        assert!(!PlayState2::Stopped.is_moving());
        assert!(!PlayState2::Unknown(0x01).is_moving());
    }

    #[test]
    fn play_state_2_opus_moving_value() {
        assert_eq!(PlayState2::from(0xfa), PlayState2::OpusMoving);
    }

    #[test]
    fn play_state_2_other_moving_values() {
        assert_eq!(PlayState2::from(0x6a), PlayState2::Moving);
        assert_eq!(PlayState2::from(0x7a), PlayState2::Moving);
    }

    #[test]
    fn play_state_2_stopped_values() {
        assert_eq!(PlayState2::from(0x6e), PlayState2::Stopped);
        assert_eq!(PlayState2::from(0x7e), PlayState2::Stopped);
        assert_eq!(PlayState2::from(0xfe), PlayState2::Stopped);
    }
}
