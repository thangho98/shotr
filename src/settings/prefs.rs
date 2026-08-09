//! Application behaviour — the same for every shot, and never stored in a
//! [`crate::settings::Preset`].
//!
//! `export.rs` reads nothing else, which is the check that this boundary is the
//! right one. See [`super::style`] for the other half.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Every field carries a serde default so that adding new knobs later never
/// invalidates an existing prefs.json.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Prefs {
    /// Interface language.
    pub lang: crate::i18n::Lang,
    /// Where exports land. `None` means the platform's Pictures directory —
    /// stored rather than resolved so that a directory going missing later
    /// falls back instead of failing.
    pub save_dir: Option<PathBuf>,

    pub format: ExportFormat,
    pub jpeg_quality: u8,
    pub png_max_compression: bool,
    /// Tokens: `{date}`, `{time}`, `{unix}`.
    pub filename_template: String,

    /// Master switch for automatic redaction of OCR hits.
    pub redact: bool,
    pub redact_email: bool,
    pub redact_card: bool,
    pub redact_ip: bool,
    pub redact_key: bool,
    /// Off by default: the phone pattern is the loosest of the set and the
    /// most likely to cover something that is not a phone number.
    pub redact_phone: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            lang: crate::i18n::Lang::default(),
            save_dir: None,
            format: ExportFormat::Png,
            jpeg_quality: 90,
            png_max_compression: false,
            filename_template: "shotr-{date}-{time}".to_owned(),
            redact: false,
            redact_email: true,
            redact_card: true,
            redact_ip: true,
            redact_key: true,
            redact_phone: false,
        }
    }
}

impl Prefs {
    pub fn load() -> Self {
        super::read_json("prefs.json").unwrap_or_default()
    }

    pub fn save(&self) {
        super::write_json("prefs.json", self);
    }

    /// Where exports land, with the configured directory winning over the
    /// platform default only while it still exists.
    pub fn save_dir(&self) -> PathBuf {
        match &self.save_dir {
            Some(p) if p.is_dir() => p.clone(),
            _ => super::pictures_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let p = Prefs {
            format: ExportFormat::Jpeg,
            jpeg_quality: 55,
            redact: true,
            filename_template: "shot-{unix}".to_owned(),
            ..Default::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Prefs>(&json).unwrap(), p);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let p: Prefs = serde_json::from_str(r#"{"jpeg_quality": 10}"#).unwrap();
        assert_eq!(p.jpeg_quality, 10);
        assert_eq!(p.format, Prefs::default().format);
        assert_eq!(p.filename_template, Prefs::default().filename_template);
    }

    /// A directory that has gone away must not take exporting down with it.
    #[test]
    fn a_missing_save_directory_falls_back_to_pictures() {
        let p = Prefs {
            save_dir: Some(PathBuf::from("/definitely/not/here")),
            ..Default::default()
        };
        assert_eq!(p.save_dir(), pictures_dir_for_test());
    }

    fn pictures_dir_for_test() -> PathBuf {
        super::super::pictures_dir()
    }
}
