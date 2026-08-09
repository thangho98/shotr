//! Where the watermark sits and how it is set.

use crate::i18n::t;

use serde::{Deserialize, Serialize};

/// The classic nine-square grid — every watermarking tool offers this because a
/// mark that clashes with the subject just needs to move to a quieter corner.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum WatermarkPos {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl WatermarkPos {
    pub const ALL: [WatermarkPos; 9] = [
        WatermarkPos::TopLeft,
        WatermarkPos::Top,
        WatermarkPos::TopRight,
        WatermarkPos::Left,
        WatermarkPos::Center,
        WatermarkPos::Right,
        WatermarkPos::BottomLeft,
        WatermarkPos::Bottom,
        WatermarkPos::BottomRight,
    ];
}

/// How the wordmark is set. Each exists to stay readable over a different kind
/// of background — plain text disappears over busy pixels, which is the one
/// failure a watermark cannot afford.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum WatermarkStyle {
    /// Text alone.
    Plain,
    /// A soft dark offset behind it.
    Shadow,
    /// A dark ring all the way around each glyph.
    Outline,
    /// A rounded dark plate behind the whole line.
    Pill,
}

impl WatermarkStyle {
    pub const ALL: [WatermarkStyle; 4] = [
        WatermarkStyle::Plain,
        WatermarkStyle::Shadow,
        WatermarkStyle::Outline,
        WatermarkStyle::Pill,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WatermarkStyle::Plain => t("Plain text"),
            WatermarkStyle::Shadow => t("Add a shadow"),
            WatermarkStyle::Outline => t("Outlined"),
            WatermarkStyle::Pill => t("Rounded plate"),
        }
    }
}
