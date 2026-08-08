//! Persisted user settings. Every field carries a serde default so that adding
//! new knobs later never invalidates an existing settings.json.

use crate::i18n::t;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// Output file format.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ExportFormat {
    Png,
    Jpeg,
    /// Lossless only — the `image` crate has no lossy WebP encoder, so there is
    /// no quality knob for this one.
    Webp,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 3] = [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Webp];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Webp => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Webp => "WebP",
        }
    }

    pub fn from_extension(ext: Option<&str>) -> Option<Self> {
        match ext?.to_ascii_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "webp" => Some(ExportFormat::Webp),
            _ => None,
        }
    }

    /// Only JPEG carries an alpha channel loss and a quality dial.
    pub fn has_alpha(self) -> bool {
        self != ExportFormat::Jpeg
    }
}

/// How a redaction box is drawn over sensitive text.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum RedactStyle {
    Solid,
    Blur,
}

/// Output shape. `Auto` grows the canvas to fit the screenshot plus padding;
/// the others pin the canvas and fit the screenshot inside it.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum Ratio {
    Auto,
    /// width / height
    Aspect(f32),
    /// Exact pixel size, used by the social-media presets.
    Size(u32, u32),
}

pub struct RatioPreset {
    pub name: &'static str,
    pub ratio: Ratio,
}

pub const RATIO_PRESETS: &[RatioPreset] = &[
    RatioPreset {
        name: "Auto",
        ratio: Ratio::Auto,
    },
    RatioPreset {
        name: "4:3",
        ratio: Ratio::Aspect(4.0 / 3.0),
    },
    RatioPreset {
        name: "3:2",
        ratio: Ratio::Aspect(3.0 / 2.0),
    },
    RatioPreset {
        name: "16:9",
        ratio: Ratio::Aspect(16.0 / 9.0),
    },
    RatioPreset {
        name: "1:1",
        ratio: Ratio::Aspect(1.0),
    },
    RatioPreset {
        name: "Twitter",
        ratio: Ratio::Size(1600, 900),
    },
    RatioPreset {
        name: "Facebook",
        ratio: Ratio::Size(1200, 630),
    },
    RatioPreset {
        name: "Instagram",
        ratio: Ratio::Size(1080, 1080),
    },
    RatioPreset {
        name: "LinkedIn",
        ratio: Ratio::Size(1200, 627),
    },
    RatioPreset {
        name: "Youtube",
        ratio: Ratio::Size(1280, 720),
    },
    RatioPreset {
        name: "Pinterest",
        ratio: Ratio::Size(1000, 1500),
    },
    RatioPreset {
        name: "Reddit",
        ratio: Ratio::Size(1200, 628),
    },
    RatioPreset {
        name: "Snapchat",
        ratio: Ratio::Size(1080, 1920),
    },
];

/// Where a watermark sits. The classic nine-square grid — every watermarking
/// tool offers this because a mark that clashes with the subject just needs to
/// move to a quieter corner.
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

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Settings {
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
    /// Interface language.
    pub lang: crate::i18n::Lang,
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

    /// Master switch for automatic redaction of OCR hits.
    pub redact: bool,
    pub redact_email: bool,
    pub redact_card: bool,
    pub redact_ip: bool,
    pub redact_key: bool,
    /// Off by default: the phone pattern is the loosest of the set and the
    /// most likely to cover something that is not a phone number.
    pub redact_phone: bool,
    pub redact_style: RedactStyle,
    pub redact_color: Rgba8,
    pub redact_blur: f32,

    pub format: ExportFormat,
    pub jpeg_quality: u8,
    pub png_max_compression: bool,
    /// Tokens: `{date}`, `{time}`, `{unix}`.
    pub filename_template: String,
}

impl Default for Settings {
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
            lang: crate::i18n::Lang::default(),
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
            redact: false,
            redact_email: true,
            redact_card: true,
            redact_ip: true,
            redact_key: true,
            redact_phone: false,
            redact_style: RedactStyle::Solid,
            redact_color: [0x1c, 0x1c, 0x1e, 0xff],
            redact_blur: 14.0,
            format: ExportFormat::Png,
            jpeg_quality: 90,
            png_max_compression: false,
            filename_template: "shotr-{date}-{time}".to_owned(),
        }
    }
}

/// A named bundle of settings, saved by the user via the sidebar dropdown.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Preset {
    pub name: String,
    pub settings: Settings,
}

pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "shotr").map(|d| d.config_dir().to_path_buf())
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Best-effort persist; a read-only config dir must not break the app.
    pub fn save(&self) {
        let Some(path) = settings_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

fn presets_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("presets.json"))
}

pub fn load_presets() -> Vec<Preset> {
    presets_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_presets(presets: &[Preset]) {
    let Some(path) = presets_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = std::fs::write(&path, json);
    }
}

/// Where exported images land.
pub fn pictures_dir() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(p) = dirs.picture_dir() {
            return p.to_path_buf();
        }
        return dirs.home_dir().to_path_buf();
    }
    Path::new(".").to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
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
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    /// The `#[serde(default)]` on `Settings` is what lets a settings.json written
    /// by an older build survive new fields being added.
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"padding": 100}"#).unwrap();
        assert_eq!(s.padding, 100);
        assert_eq!(s.radius, Settings::default().radius);
        assert_eq!(s.background, Settings::default().background);
    }

    #[test]
    fn an_empty_object_is_the_default_settings() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn every_ratio_preset_is_reachable_and_sane() {
        for p in RATIO_PRESETS {
            match p.ratio {
                Ratio::Aspect(r) => assert!(r > 0.0, "{} has a non-positive ratio", p.name),
                Ratio::Size(w, h) => assert!(w > 0 && h > 0, "{} has a zero dimension", p.name),
                Ratio::Auto => {}
            }
        }
    }
}
