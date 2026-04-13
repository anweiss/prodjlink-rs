//! Player settings — maps to the Java `PlayerSettings` concept.
//!
//! Models per-player settings that can be pushed to a CDJ via the
//! load-settings command (type 0x34 on port 50002).

use crate::protocol::header::MAGIC_HEADER;
use crate::util::number_to_bytes;

// ---------------------------------------------------------------------------
// Setting enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LcdBrightness {
    White,
    #[default]
    One,
    Two,
    Three,
    Four,
    Five,
}

impl LcdBrightness {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::White => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::White,
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            4 => Self::Four,
            5 => Self::Five,
            _ => Self::One,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum JogMode {
    #[default]
    Cdj,
    Vinyl,
}

impl JogMode {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Cdj => 0,
            Self::Vinyl => 1,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Cdj,
            1 => Self::Vinyl,
            _ => Self::Cdj,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TempoRange {
    Six,
    #[default]
    Ten,
    Sixteen,
    Wide,
}

impl TempoRange {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Six => 1,
            Self::Ten => 2,
            Self::Sixteen => 3,
            Self::Wide => 4,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Six,
            2 => Self::Ten,
            3 => Self::Sixteen,
            4 => Self::Wide,
            _ => Self::Ten,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AutoCueLevel {
    Minus36,
    Minus42,
    #[default]
    Minus48,
    Minus54,
    Minus60,
    Minus66,
    Minus72,
    Minus78,
    MemoryCue,
}

impl AutoCueLevel {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Minus36 => 0,
            Self::Minus42 => 1,
            Self::Minus48 => 2,
            Self::Minus54 => 3,
            Self::Minus60 => 4,
            Self::Minus66 => 5,
            Self::Minus72 => 6,
            Self::Minus78 => 7,
            Self::MemoryCue => 8,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Minus36,
            1 => Self::Minus42,
            2 => Self::Minus48,
            3 => Self::Minus54,
            4 => Self::Minus60,
            5 => Self::Minus66,
            6 => Self::Minus72,
            7 => Self::Minus78,
            8 => Self::MemoryCue,
            _ => Self::Minus48,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    English,
    French,
    German,
    Italian,
    Dutch,
    Spanish,
    Russian,
    Korean,
    ChineseSimplified,
    ChineseTraditional,
    Japanese,
    Portuguese,
    Swedish,
    Czech,
    Hungarian,
    Danish,
    Greek,
    Turkish,
}

impl Language {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::English => 1,
            Self::French => 2,
            Self::German => 3,
            Self::Italian => 4,
            Self::Dutch => 5,
            Self::Spanish => 6,
            Self::Russian => 7,
            Self::Korean => 8,
            Self::ChineseSimplified => 9,
            Self::ChineseTraditional => 10,
            Self::Japanese => 11,
            Self::Portuguese => 12,
            Self::Swedish => 13,
            Self::Czech => 14,
            Self::Hungarian => 15,
            Self::Danish => 16,
            Self::Greek => 17,
            Self::Turkish => 18,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::English,
            2 => Self::French,
            3 => Self::German,
            4 => Self::Italian,
            5 => Self::Dutch,
            6 => Self::Spanish,
            7 => Self::Russian,
            8 => Self::Korean,
            9 => Self::ChineseSimplified,
            10 => Self::ChineseTraditional,
            11 => Self::Japanese,
            12 => Self::Portuguese,
            13 => Self::Swedish,
            14 => Self::Czech,
            15 => Self::Hungarian,
            16 => Self::Danish,
            17 => Self::Greek,
            18 => Self::Turkish,
            _ => Self::English,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TimeDisplayMode {
    #[default]
    Elapsed,
    Remaining,
}

impl TimeDisplayMode {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Elapsed => 0,
            Self::Remaining => 1,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Elapsed,
            1 => Self::Remaining,
            _ => Self::Elapsed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayMode {
    #[default]
    Continue,
    Single,
}

impl PlayMode {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Continue => 0,
            Self::Single => 1,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Continue,
            1 => Self::Single,
            _ => Self::Continue,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QuantizeMode {
    Off,
    #[default]
    On,
}

impl QuantizeMode {
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::On => 1,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Off,
            1 => Self::On,
            _ => Self::On,
        }
    }
}

// ---------------------------------------------------------------------------
// PlayerSettings struct
// ---------------------------------------------------------------------------

/// Load-settings command packet type byte (port 50002).
const LOAD_SETTINGS_TYPE: u8 = 0x34;

/// Payload offsets within the settings payload (relative to 0x24, the start of
/// the variable-length payload region in a standard command packet).
const SETTINGS_PAYLOAD_LEN: usize = 0x14;

// Offsets within the payload (relative to 0x24)
const LCD_BRIGHTNESS_OFF: usize = 0x00;
const JOG_MODE_OFF: usize = 0x01;
const AUTO_CUE_LEVEL_OFF: usize = 0x02;
const TEMPO_RANGE_OFF: usize = 0x03;
const LANGUAGE_OFF: usize = 0x04;
const TIME_DISPLAY_OFF: usize = 0x08;
const PLAY_MODE_OFF: usize = 0x0c;
const QUANTIZE_OFF: usize = 0x0d;

const PREFIX_LEN: usize = 0x24;

/// Device name embedded in outgoing settings packets, null-padded to 20 bytes.
const DEVICE_NAME: &[u8; 20] = b"prodjlink-rs\0\0\0\0\0\0\0\0";

/// Player settings that can be pushed to a CDJ via the load-settings command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSettings {
    pub lcd_brightness: LcdBrightness,
    pub jog_mode: JogMode,
    pub auto_cue_level: AutoCueLevel,
    pub tempo_range: TempoRange,
    pub language: Language,
    pub time_display_mode: TimeDisplayMode,
    pub play_mode: PlayMode,
    pub quantize_mode: QuantizeMode,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            lcd_brightness: LcdBrightness::default(),
            jog_mode: JogMode::default(),
            auto_cue_level: AutoCueLevel::default(),
            tempo_range: TempoRange::default(),
            language: Language::default(),
            time_display_mode: TimeDisplayMode::default(),
            play_mode: PlayMode::default(),
            quantize_mode: QuantizeMode::default(),
        }
    }
}

impl PlayerSettings {
    /// Build a load-settings command packet (type 0x34) for the given target device.
    ///
    /// `source_device` is our virtual CDJ number; `target_device` is the player
    /// to configure.
    pub fn build_settings_packet(
        &self,
        source_device: u8,
        target_device: u8,
    ) -> Vec<u8> {
        let total_len = PREFIX_LEN + SETTINGS_PAYLOAD_LEN;
        let mut buf = vec![0u8; total_len];

        // Standard command prefix
        buf[0x00..0x0a].copy_from_slice(&MAGIC_HEADER);
        buf[0x0a] = LOAD_SETTINGS_TYPE;
        buf[0x0c..0x20].copy_from_slice(DEVICE_NAME);
        buf[0x20] = 0x01; // argument count marker
        buf[0x21] = source_device;
        number_to_bytes(SETTINGS_PAYLOAD_LEN as u32, &mut buf, 0x22, 2);

        // Settings payload
        let base = PREFIX_LEN;
        buf[base + LCD_BRIGHTNESS_OFF] = self.lcd_brightness.to_byte();
        buf[base + JOG_MODE_OFF] = self.jog_mode.to_byte();
        buf[base + AUTO_CUE_LEVEL_OFF] = self.auto_cue_level.to_byte();
        buf[base + TEMPO_RANGE_OFF] = self.tempo_range.to_byte();
        buf[base + LANGUAGE_OFF] = self.language.to_byte();
        buf[base + TIME_DISPLAY_OFF] = self.time_display_mode.to_byte();
        buf[base + PLAY_MODE_OFF] = self.play_mode.to_byte();
        buf[base + QUANTIZE_OFF] = self.quantize_mode.to_byte();

        // Target device at the end of payload (last byte)
        buf[base + SETTINGS_PAYLOAD_LEN - 1] = target_device;

        buf
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::read_device_name;

    #[test]
    fn default_settings() {
        let s = PlayerSettings::default();
        assert_eq!(s.lcd_brightness, LcdBrightness::One);
        assert_eq!(s.jog_mode, JogMode::Cdj);
        assert_eq!(s.auto_cue_level, AutoCueLevel::Minus48);
        assert_eq!(s.tempo_range, TempoRange::Ten);
        assert_eq!(s.language, Language::English);
        assert_eq!(s.time_display_mode, TimeDisplayMode::Elapsed);
        assert_eq!(s.play_mode, PlayMode::Continue);
        assert_eq!(s.quantize_mode, QuantizeMode::On);
    }

    #[test]
    fn build_settings_packet_has_magic_header() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 1);
        assert_eq!(&pkt[..10], &MAGIC_HEADER);
    }

    #[test]
    fn build_settings_packet_type_byte() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 1);
        assert_eq!(pkt[0x0a], LOAD_SETTINGS_TYPE);
    }

    #[test]
    fn build_settings_packet_device_name() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 1);
        let name = read_device_name(&pkt, 0x0c, 20);
        assert_eq!(name, "prodjlink-rs");
    }

    #[test]
    fn build_settings_packet_source_device() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(7, 2);
        assert_eq!(pkt[0x21], 7);
    }

    #[test]
    fn build_settings_packet_target_device() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 3);
        assert_eq!(pkt[PREFIX_LEN + SETTINGS_PAYLOAD_LEN - 1], 3);
    }

    #[test]
    fn build_settings_packet_length() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 1);
        assert_eq!(pkt.len(), PREFIX_LEN + SETTINGS_PAYLOAD_LEN);
    }

    #[test]
    fn build_settings_packet_default_values() {
        let s = PlayerSettings::default();
        let pkt = s.build_settings_packet(5, 1);
        let base = PREFIX_LEN;
        assert_eq!(pkt[base + LCD_BRIGHTNESS_OFF], 1); // LcdBrightness::One
        assert_eq!(pkt[base + JOG_MODE_OFF], 0); // JogMode::Cdj
        assert_eq!(pkt[base + AUTO_CUE_LEVEL_OFF], 2); // AutoCueLevel::Minus48
        assert_eq!(pkt[base + TEMPO_RANGE_OFF], 2); // TempoRange::Ten
        assert_eq!(pkt[base + LANGUAGE_OFF], 1); // Language::English
        assert_eq!(pkt[base + TIME_DISPLAY_OFF], 0); // TimeDisplayMode::Elapsed
        assert_eq!(pkt[base + PLAY_MODE_OFF], 0); // PlayMode::Continue
        assert_eq!(pkt[base + QUANTIZE_OFF], 1); // QuantizeMode::On
    }

    #[test]
    fn build_settings_packet_custom_values() {
        let s = PlayerSettings {
            lcd_brightness: LcdBrightness::Five,
            jog_mode: JogMode::Vinyl,
            auto_cue_level: AutoCueLevel::Minus72,
            tempo_range: TempoRange::Wide,
            language: Language::Japanese,
            time_display_mode: TimeDisplayMode::Remaining,
            play_mode: PlayMode::Single,
            quantize_mode: QuantizeMode::Off,
        };
        let pkt = s.build_settings_packet(5, 2);
        let base = PREFIX_LEN;
        assert_eq!(pkt[base + LCD_BRIGHTNESS_OFF], 5);
        assert_eq!(pkt[base + JOG_MODE_OFF], 1);
        assert_eq!(pkt[base + AUTO_CUE_LEVEL_OFF], 6);
        assert_eq!(pkt[base + TEMPO_RANGE_OFF], 4);
        assert_eq!(pkt[base + LANGUAGE_OFF], 11);
        assert_eq!(pkt[base + TIME_DISPLAY_OFF], 1);
        assert_eq!(pkt[base + PLAY_MODE_OFF], 1);
        assert_eq!(pkt[base + QUANTIZE_OFF], 0);
    }

    // -- Enum round-trip tests --

    #[test]
    fn lcd_brightness_round_trip() {
        for variant in [
            LcdBrightness::White, LcdBrightness::One, LcdBrightness::Two,
            LcdBrightness::Three, LcdBrightness::Four, LcdBrightness::Five,
        ] {
            assert_eq!(LcdBrightness::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn jog_mode_round_trip() {
        for variant in [JogMode::Cdj, JogMode::Vinyl] {
            assert_eq!(JogMode::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn tempo_range_round_trip() {
        for variant in [TempoRange::Six, TempoRange::Ten, TempoRange::Sixteen, TempoRange::Wide] {
            assert_eq!(TempoRange::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn auto_cue_level_round_trip() {
        for variant in [
            AutoCueLevel::Minus36, AutoCueLevel::Minus42, AutoCueLevel::Minus48,
            AutoCueLevel::Minus54, AutoCueLevel::Minus60, AutoCueLevel::Minus66,
            AutoCueLevel::Minus72, AutoCueLevel::Minus78, AutoCueLevel::MemoryCue,
        ] {
            assert_eq!(AutoCueLevel::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn language_round_trip() {
        for variant in [
            Language::English, Language::French, Language::German, Language::Italian,
            Language::Dutch, Language::Spanish, Language::Russian, Language::Korean,
            Language::ChineseSimplified, Language::ChineseTraditional, Language::Japanese,
            Language::Portuguese, Language::Swedish, Language::Czech, Language::Hungarian,
            Language::Danish, Language::Greek, Language::Turkish,
        ] {
            assert_eq!(Language::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn time_display_mode_round_trip() {
        for variant in [TimeDisplayMode::Elapsed, TimeDisplayMode::Remaining] {
            assert_eq!(TimeDisplayMode::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn play_mode_round_trip() {
        for variant in [PlayMode::Continue, PlayMode::Single] {
            assert_eq!(PlayMode::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn quantize_mode_round_trip() {
        for variant in [QuantizeMode::Off, QuantizeMode::On] {
            assert_eq!(QuantizeMode::from_byte(variant.to_byte()), variant);
        }
    }
}
