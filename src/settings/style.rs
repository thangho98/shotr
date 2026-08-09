//! The look of one shot — everything a [`crate::settings::Preset`] stores.
//!
//! The split against [`super::prefs`] follows a line the codebase already drew:
//! `render/` reads nothing but a `Style`, and `export.rs` reads nothing but a
//! `Prefs`. Redaction is the one thing cut in half — which patterns to catch is
//! a privacy policy and belongs with the preferences, while how the box is drawn
//! is pixels and belongs here.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ratio::Ratio;
use super::watermark::{WatermarkPos, WatermarkStyle};

pub type Rgba8 = [u8; 4];

/// Which background the canvas is painted with. Mirrors the swatch row in the
/// sidebar: 7 named presets, the desktop wallpaper, transparent, and custom.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum Background {
    /// Index into [`crate::render::background::BG_PRESETS`].
    Preset(usize),
    /// A mesh gradient derived from the screenshot's own dominant colours.
    Auto,
    /// The current desktop wallpaper. Best-effort — see `wallpaper::current`.
    Desktop,
    /// Fully transparent, so exported PNGs keep an alpha channel.
    None,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum CustomKind {
    Solid,
    Linear,
    Image,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct CustomBg {
    pub kind: CustomKind,
    pub color_a: Rgba8,
    pub color_b: Rgba8,
    /// Gradient direction in degrees, 0 = left→right, growing clockwise.
    pub angle: f32,
    pub image: Option<PathBuf>,
}

impl Default for CustomBg {
    fn default() -> Self {
        Self {
            kind: CustomKind::Linear,
            color_a: [0x6a, 0x11, 0xcb, 0xff],
            color_b: [0x25, 0x75, 0xfc, 0xff],
            angle: 45.0,
            image: None,
        }
    }
}

/// How a redaction box is drawn over sensitive text.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum RedactStyle {
    Solid,
    Blur,
}

/// Every field carries a serde default so that adding new knobs later never
/// invalidates an existing style.json.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Style {
    pub padding: u32,
    pub inset: u32,
    pub inset_color: Rgba8,
    /// Tint the inset frame with the screenshot's own background colour when one
    /// can be detected. Falls back to `inset_color` when it cannot.
    pub inset_auto_color: bool,
    /// Trim uniform margins off the screenshot so the content sits centred.
    pub balance: bool,
    pub radius: u32,
    /// 0 = no shadow, 100 = heaviest. Drives blur sigma, alpha and offset together.
    pub shadow: u32,
    pub background: Background,
    pub custom_bg: CustomBg,
    pub ratio: Ratio,
    /// Remembered width×height behind the Ratio “Custom…” button.
    pub custom_size: (u32, u32),

    pub watermark: bool,
    pub watermark_text: String,
    pub watermark_pos: WatermarkPos,
    pub watermark_style: WatermarkStyle,
    /// Multiplier on the size derived from the canvas width.
    pub watermark_size: f32,
    pub watermark_opacity: u8,
    pub watermark_color: Rgba8,
    /// Repeat the mark across the whole image instead of placing it once.
    pub watermark_tiled: bool,
    /// Rotation in degrees, applied whether tiled or placed.
    pub watermark_angle: f32,
    /// A logo to stamp instead of the text.
    pub watermark_image: Option<PathBuf>,

    pub redact_style: RedactStyle,
    pub redact_color: Rgba8,
    pub redact_blur: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            padding: 64,
            inset: 0,
            inset_color: [0xff, 0xff, 0xff, 0xff],
            inset_auto_color: true,
            balance: false,
            radius: 16,
            shadow: 45,
            background: Background::Preset(0),
            custom_bg: CustomBg::default(),
            ratio: Ratio::Auto,
            custom_size: (1600, 1000),
            watermark: false,
            watermark_text: "Screenshot by shotr".to_owned(),
            watermark_pos: WatermarkPos::BottomRight,
            watermark_style: WatermarkStyle::Shadow,
            watermark_size: 1.0,
            // Watermarking tools land around 20–40% for protecting an image;
            // a corner credit line wants a little more presence than that.
            watermark_opacity: 150,
            watermark_color: [255, 255, 255, 255],
            watermark_tiled: false,
            watermark_angle: 0.0,
            watermark_image: None,
            redact_style: RedactStyle::Solid,
            redact_color: [0x1c, 0x1c, 0x1e, 0xff],
            redact_blur: 14.0,
        }
    }
}

impl Style {
    pub fn load() -> Self {
        super::read_json("style.json").unwrap_or_default()
    }

    pub fn save(&self) {
        super::write_json("style.json", self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let s = Style {
            inset: 12,
            background: Background::Preset(4),
            ratio: Ratio::Size(1080, 1920),
            custom_bg: CustomBg {
                angle: 137.5,
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Style>(&json).unwrap(), s);
    }

    /// The `#[serde(default)]` is what lets a style.json written by an older
    /// build survive new fields being added.
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Style = serde_json::from_str(r#"{"padding": 100}"#).unwrap();
        assert_eq!(s.padding, 100);
        assert_eq!(s.radius, Style::default().radius);
        assert_eq!(s.background, Style::default().background);
    }

    #[test]
    fn an_empty_object_is_the_default_style() {
        assert_eq!(serde_json::from_str::<Style>("{}").unwrap(), Style::default());
    }
}
