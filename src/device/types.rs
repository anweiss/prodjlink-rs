use std::fmt;

// ---------------------------------------------------------------------------
// DeviceNumber
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceNumber(pub u8);

impl DeviceNumber {
    pub fn new(n: u8) -> Option<Self> {
        if n == 0 { None } else { Some(Self(n)) }
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
    Empty,
    Loading,
    Loaded,
    Playing,
    Looping,
    Paused,
    CuedUp,
    Cueing,
    Searching,
    SpunDown,
    Ended,
    Unknown(u8),
}

impl From<u8> for PlayState {
    fn from(n: u8) -> Self {
        match n {
            0x00 => Self::Empty,
            0x02 => Self::Loading,
            0x03 => Self::Loaded,
            0x04 => Self::Playing,
            0x05 => Self::Looping,
            0x06 => Self::Paused,
            0x07 => Self::CuedUp,
            0x08 => Self::Cueing,
            0x09 => Self::Searching,
            0x0e => Self::SpunDown,
            0x11 => Self::Ended,
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
