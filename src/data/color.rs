use std::fmt;

/// A rekordbox track color label.
///
/// Maps color IDs (used in the DJ Link protocol) to human-readable color names
/// and RGB values. Based on the Java beat-link `ColorItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorItem {
    pub id: u16,
    pub label: String,
    /// RGB color value; `None` for the "No Color" entry.
    pub color: Option<(u8, u8, u8)>,
}

impl ColorItem {
    /// Look up a color by its protocol ID.
    ///
    /// Returns `None` only for IDs that are completely unknown (> 8).
    /// ID 0 returns the "No Color" entry with `color: None`.
    pub fn for_id(id: u16) -> Option<ColorItem> {
        let (label, color) = match id {
            0 => ("No Color", None),
            // Java Color.PINK = (255, 175, 175)
            1 => ("Pink", Some((255, 175, 175))),
            // Java Color.RED = (255, 0, 0)
            2 => ("Red", Some((255, 0, 0))),
            // Java Color.ORANGE = (255, 200, 0)
            3 => ("Orange", Some((255, 200, 0))),
            // Java Color.YELLOW = (255, 255, 0)
            4 => ("Yellow", Some((255, 255, 0))),
            // Java Color.GREEN = (0, 255, 0)
            5 => ("Green", Some((0, 255, 0))),
            // Java Color.CYAN = (0, 255, 255)
            6 => ("Aqua", Some((0, 255, 255))),
            // Java Color.BLUE = (0, 0, 255)
            7 => ("Blue", Some((0, 0, 255))),
            // Purple (128, 0, 128)
            8 => ("Purple", Some((128, 0, 128))),
            _ => return None,
        };

        Some(ColorItem {
            id,
            label: label.to_string(),
            color,
        })
    }

    /// Whether the given ID represents "no color" (i.e. no color label assigned).
    pub fn is_no_color(id: u16) -> bool {
        id == 0
    }

    /// Returns the name of this color (e.g. "Pink", "Red", "No Color").
    pub fn color_name(&self) -> &str {
        &self.label
    }

    /// Returns the color name for a given protocol ID without constructing a
    /// full `ColorItem`. Returns `None` for unknown IDs (> 8).
    pub fn color_name_for_id(id: u16) -> Option<&'static str> {
        match id {
            0 => Some("No Color"),
            1 => Some("Pink"),
            2 => Some("Red"),
            3 => Some("Orange"),
            4 => Some("Yellow"),
            5 => Some("Green"),
            6 => Some("Aqua"),
            7 => Some("Blue"),
            8 => Some("Purple"),
            _ => None,
        }
    }
}

impl fmt::Display for ColorItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.color {
            Some((r, g, b)) => write!(f, "{} (#{:02X}{:02X}{:02X})", self.label, r, g, b),
            None => write!(f, "{}", self.label),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_id_no_color() {
        let c = ColorItem::for_id(0).unwrap();
        assert_eq!(c.id, 0);
        assert_eq!(c.label, "No Color");
        assert_eq!(c.color, None);
    }

    #[test]
    fn for_id_known_colors() {
        let cases: &[(u16, &str, (u8, u8, u8))] = &[
            (1, "Pink", (255, 175, 175)),
            (2, "Red", (255, 0, 0)),
            (3, "Orange", (255, 200, 0)),
            (4, "Yellow", (255, 255, 0)),
            (5, "Green", (0, 255, 0)),
            (6, "Aqua", (0, 255, 255)),
            (7, "Blue", (0, 0, 255)),
            (8, "Purple", (128, 0, 128)),
        ];
        for &(id, label, rgb) in cases {
            let c = ColorItem::for_id(id).unwrap();
            assert_eq!(c.id, id, "id mismatch for {label}");
            assert_eq!(c.label, label);
            assert_eq!(c.color, Some(rgb), "rgb mismatch for {label}");
        }
    }

    #[test]
    fn for_id_unknown_returns_none() {
        assert!(ColorItem::for_id(9).is_none());
        assert!(ColorItem::for_id(100).is_none());
        assert!(ColorItem::for_id(u16::MAX).is_none());
    }

    #[test]
    fn is_no_color_check() {
        assert!(ColorItem::is_no_color(0));
        assert!(!ColorItem::is_no_color(1));
        assert!(!ColorItem::is_no_color(8));
        assert!(!ColorItem::is_no_color(99));
    }

    #[test]
    fn equality() {
        let a = ColorItem::for_id(2).unwrap();
        let b = ColorItem::for_id(2).unwrap();
        let c = ColorItem::for_id(3).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn display_with_color() {
        let c = ColorItem::for_id(2).unwrap();
        assert_eq!(format!("{c}"), "Red (#FF0000)");
    }

    #[test]
    fn display_no_color() {
        let c = ColorItem::for_id(0).unwrap();
        assert_eq!(format!("{c}"), "No Color");
    }

    #[test]
    fn display_all_colors() {
        assert_eq!(
            format!("{}", ColorItem::for_id(1).unwrap()),
            "Pink (#FFAFAF)"
        );
        assert_eq!(
            format!("{}", ColorItem::for_id(5).unwrap()),
            "Green (#00FF00)"
        );
        assert_eq!(
            format!("{}", ColorItem::for_id(6).unwrap()),
            "Aqua (#00FFFF)"
        );
        assert_eq!(
            format!("{}", ColorItem::for_id(7).unwrap()),
            "Blue (#0000FF)"
        );
        assert_eq!(
            format!("{}", ColorItem::for_id(8).unwrap()),
            "Purple (#800080)"
        );
    }

    #[test]
    fn clone_and_debug() {
        let a = ColorItem::for_id(4).unwrap();
        let b = a.clone();
        assert_eq!(a, b);
        let _ = format!("{a:?}");
    }

    #[test]
    fn color_name_returns_label() {
        let item = ColorItem::for_id(1).unwrap();
        assert_eq!(item.color_name(), "Pink");
    }

    #[test]
    fn color_name_for_id_known() {
        assert_eq!(ColorItem::color_name_for_id(3), Some("Orange"));
        assert_eq!(ColorItem::color_name_for_id(0), Some("No Color"));
        assert_eq!(ColorItem::color_name_for_id(8), Some("Purple"));
    }

    #[test]
    fn color_name_for_id_unknown() {
        assert_eq!(ColorItem::color_name_for_id(9), None);
        assert_eq!(ColorItem::color_name_for_id(100), None);
    }
}
