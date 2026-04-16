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
            Self::White => 0x80,
            Self::One => 0x81,
            Self::Two => 0x82,
            Self::Three => 0x83,
            Self::Four => 0x84,
            Self::Five => 0x85,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::White,
            0x81 => Self::One,
            0x82 => Self::Two,
            0x83 => Self::Three,
            0x84 => Self::Four,
            0x85 => Self::Five,
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
            Self::Cdj => 0x80,
            Self::Vinyl => 0x81,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Cdj,
            0x81 => Self::Vinyl,
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
            Self::Six => 0x80,
            Self::Ten => 0x81,
            Self::Sixteen => 0x82,
            Self::Wide => 0x83,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Six,
            0x81 => Self::Ten,
            0x82 => Self::Sixteen,
            0x83 => Self::Wide,
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
            Self::Minus36 => 0x80,
            Self::Minus42 => 0x81,
            Self::Minus48 => 0x82,
            Self::Minus54 => 0x83,
            Self::Minus60 => 0x84,
            Self::Minus66 => 0x85,
            Self::Minus72 => 0x86,
            Self::Minus78 => 0x87,
            Self::MemoryCue => 0x88,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Minus36,
            0x81 => Self::Minus42,
            0x82 => Self::Minus48,
            0x83 => Self::Minus54,
            0x84 => Self::Minus60,
            0x85 => Self::Minus66,
            0x86 => Self::Minus72,
            0x87 => Self::Minus78,
            0x88 => Self::MemoryCue,
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
            Self::English => 0x81,
            Self::French => 0x82,
            Self::German => 0x83,
            Self::Italian => 0x84,
            Self::Dutch => 0x85,
            Self::Spanish => 0x86,
            Self::Russian => 0x87,
            Self::Korean => 0x88,
            Self::ChineseSimplified => 0x89,
            Self::ChineseTraditional => 0x8a,
            Self::Japanese => 0x8b,
            Self::Portuguese => 0x8c,
            Self::Swedish => 0x8d,
            Self::Czech => 0x8e,
            Self::Hungarian => 0x8f,
            Self::Danish => 0x90,
            Self::Greek => 0x91,
            Self::Turkish => 0x92,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x81 => Self::English,
            0x82 => Self::French,
            0x83 => Self::German,
            0x84 => Self::Italian,
            0x85 => Self::Dutch,
            0x86 => Self::Spanish,
            0x87 => Self::Russian,
            0x88 => Self::Korean,
            0x89 => Self::ChineseSimplified,
            0x8a => Self::ChineseTraditional,
            0x8b => Self::Japanese,
            0x8c => Self::Portuguese,
            0x8d => Self::Swedish,
            0x8e => Self::Czech,
            0x8f => Self::Hungarian,
            0x90 => Self::Danish,
            0x91 => Self::Greek,
            0x92 => Self::Turkish,
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
            Self::Elapsed => 0x80,
            Self::Remaining => 0x81,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Elapsed,
            0x81 => Self::Remaining,
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
            Self::Continue => 0x80,
            Self::Single => 0x81,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Continue,
            0x81 => Self::Single,
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
            Self::Off => 0x80,
            Self::On => 0x81,
        }
    }

    pub fn from_byte(b: u8) -> Self {
        match b {
            0x80 => Self::Off,
            0x81 => Self::On,
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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

impl PlayerSettings {
    /// Build a load-settings command packet (type 0x34) for the given target device.
    ///
    /// `source_device` is our virtual CDJ number; `target_device` is the player
    /// to configure.
    pub fn build_settings_packet(&self, source_device: u8, target_device: u8) -> Vec<u8> {
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
        assert_eq!(pkt[base + LCD_BRIGHTNESS_OFF], 0x81); // LcdBrightness::One
        assert_eq!(pkt[base + JOG_MODE_OFF], 0x80); // JogMode::Cdj
        assert_eq!(pkt[base + AUTO_CUE_LEVEL_OFF], 0x82); // AutoCueLevel::Minus48
        assert_eq!(pkt[base + TEMPO_RANGE_OFF], 0x81); // TempoRange::Ten
        assert_eq!(pkt[base + LANGUAGE_OFF], 0x81); // Language::English
        assert_eq!(pkt[base + TIME_DISPLAY_OFF], 0x80); // TimeDisplayMode::Elapsed
        assert_eq!(pkt[base + PLAY_MODE_OFF], 0x80); // PlayMode::Continue
        assert_eq!(pkt[base + QUANTIZE_OFF], 0x81); // QuantizeMode::On
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
        assert_eq!(pkt[base + LCD_BRIGHTNESS_OFF], 0x85);
        assert_eq!(pkt[base + JOG_MODE_OFF], 0x81);
        assert_eq!(pkt[base + AUTO_CUE_LEVEL_OFF], 0x86);
        assert_eq!(pkt[base + TEMPO_RANGE_OFF], 0x83);
        assert_eq!(pkt[base + LANGUAGE_OFF], 0x8b);
        assert_eq!(pkt[base + TIME_DISPLAY_OFF], 0x81);
        assert_eq!(pkt[base + PLAY_MODE_OFF], 0x81);
        assert_eq!(pkt[base + QUANTIZE_OFF], 0x80);
    }

    // -- Enum round-trip tests --

    #[test]
    fn lcd_brightness_round_trip() {
        for variant in [
            LcdBrightness::White,
            LcdBrightness::One,
            LcdBrightness::Two,
            LcdBrightness::Three,
            LcdBrightness::Four,
            LcdBrightness::Five,
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
        for variant in [
            TempoRange::Six,
            TempoRange::Ten,
            TempoRange::Sixteen,
            TempoRange::Wide,
        ] {
            assert_eq!(TempoRange::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn auto_cue_level_round_trip() {
        for variant in [
            AutoCueLevel::Minus36,
            AutoCueLevel::Minus42,
            AutoCueLevel::Minus48,
            AutoCueLevel::Minus54,
            AutoCueLevel::Minus60,
            AutoCueLevel::Minus66,
            AutoCueLevel::Minus72,
            AutoCueLevel::Minus78,
            AutoCueLevel::MemoryCue,
        ] {
            assert_eq!(AutoCueLevel::from_byte(variant.to_byte()), variant);
        }
    }

    #[test]
    fn language_round_trip() {
        for variant in [
            Language::English,
            Language::French,
            Language::German,
            Language::Italian,
            Language::Dutch,
            Language::Spanish,
            Language::Russian,
            Language::Korean,
            Language::ChineseSimplified,
            Language::ChineseTraditional,
            Language::Japanese,
            Language::Portuguese,
            Language::Swedish,
            Language::Czech,
            Language::Hungarian,
            Language::Danish,
            Language::Greek,
            Language::Turkish,
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

    // -- Protocol byte value verification (matches Java PlayerSettings) --

    #[test]
    fn lcd_brightness_protocol_values() {
        assert_eq!(LcdBrightness::White.to_byte(), 0x80);
        assert_eq!(LcdBrightness::One.to_byte(), 0x81);
        assert_eq!(LcdBrightness::Five.to_byte(), 0x85);
    }

    #[test]
    fn jog_mode_protocol_values() {
        assert_eq!(JogMode::Cdj.to_byte(), 0x80);
        assert_eq!(JogMode::Vinyl.to_byte(), 0x81);
    }

    #[test]
    fn auto_cue_level_protocol_values() {
        assert_eq!(AutoCueLevel::Minus36.to_byte(), 0x80);
        assert_eq!(AutoCueLevel::Minus48.to_byte(), 0x82);
        assert_eq!(AutoCueLevel::MemoryCue.to_byte(), 0x88);
    }

    #[test]
    fn language_protocol_values() {
        assert_eq!(Language::English.to_byte(), 0x81);
        assert_eq!(Language::Japanese.to_byte(), 0x8b);
        assert_eq!(Language::Turkish.to_byte(), 0x92);
    }

    #[test]
    fn tempo_range_protocol_values() {
        assert_eq!(TempoRange::Six.to_byte(), 0x80);
        assert_eq!(TempoRange::Ten.to_byte(), 0x81);
        assert_eq!(TempoRange::Wide.to_byte(), 0x83);
    }

    #[test]
    fn time_display_protocol_values() {
        assert_eq!(TimeDisplayMode::Elapsed.to_byte(), 0x80);
        assert_eq!(TimeDisplayMode::Remaining.to_byte(), 0x81);
    }

    #[test]
    fn play_mode_protocol_values() {
        assert_eq!(PlayMode::Continue.to_byte(), 0x80);
        assert_eq!(PlayMode::Single.to_byte(), 0x81);
    }

    #[test]
    fn quantize_mode_protocol_values() {
        assert_eq!(QuantizeMode::Off.to_byte(), 0x80);
        assert_eq!(QuantizeMode::On.to_byte(), 0x81);
    }
}
