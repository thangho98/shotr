//! How the watermark is set.

use crate::i18n::t;

use serde::{Deserialize, Serialize};

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
