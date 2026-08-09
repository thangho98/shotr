//! Persisted user settings, split by what owns them.
//!
//! * [`Style`] — the look of one shot. This is what a [`Preset`] stores.
//! * [`Prefs`] — application behaviour, identical across every shot.
//!
//! Three files rather than one, each with a single concern, so a corrupt or
//! missing one costs only its own defaults:
//!
//! ```text
//! prefs.json     Prefs
//! style.json     the Style in use, restored next launch
//! presets.json   Vec<Preset>
//! ```

mod prefs;
mod ratio;
mod style;
mod watermark;

pub use prefs::{ExportFormat, Prefs};
pub use ratio::{RATIO_PRESETS, Ratio, RatioPreset};
pub use style::{Background, CustomBg, CustomKind, RedactStyle, Rgba8, Style};
pub use watermark::{WatermarkPos, WatermarkStyle};

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// A named bundle, saved by the user via the sidebar dropdown.
///
/// Holds a [`Style`] and nothing else: a preset that could carry a save
/// directory or an output format would change where files land and in what
/// format, which is not what picking a look is meant to do.
#[derive(Clone, PartialEq, Serialize, serde::Deserialize, Debug)]
pub struct Preset {
    pub name: String,
    pub style: Style,
}

pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "shotr").map(|d| d.config_dir().to_path_buf())
}

fn read_json<T: DeserializeOwned>(file: &str) -> Option<T> {
    let path = config_dir()?.join(file);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Best-effort persist; a read-only config dir must not break the app.
fn write_json<T: Serialize>(file: &str, value: &T) {
    let Some(path) = config_dir().map(|d| d.join(file)) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn load_presets() -> Vec<Preset> {
    read_json("presets.json").unwrap_or_default()
}

pub fn save_presets(presets: &[Preset]) {
    write_json("presets.json", &presets);
}

/// The platform's Pictures directory, or the closest thing to it.
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

    /// A preset carries a look and nothing else. If a preference ever leaks in
    /// here, loading that preset would silently change where files are written.
    #[test]
    fn a_preset_serialises_a_style_and_no_preferences() {
        let p = Preset {
            name: "Blog".into(),
            style: Style {
                padding: 96,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Preset>(&json).unwrap(), p);

        for leaked in [
            "filename_template",
            "save_dir",
            "format",
            "jpeg_quality",
            "lang",
            "redact_email",
        ] {
            assert!(
                !json.contains(leaked),
                "{leaked} is a preference and must not travel inside a preset"
            );
        }
    }

    /// Style and Prefs are separate files so that one being unreadable costs
    /// only its own defaults.
    #[test]
    fn style_and_prefs_deserialise_independently() {
        let style: Style = serde_json::from_str(r#"{"padding": 7}"#).unwrap();
        let prefs: Prefs = serde_json::from_str(r#"{"jpeg_quality": 7}"#).unwrap();
        assert_eq!(style.padding, 7);
        assert_eq!(prefs.jpeg_quality, 7);
        // Neither knows the other's fields, so a stray key is simply ignored.
        let style: Style = serde_json::from_str(r#"{"jpeg_quality": 7}"#).unwrap();
        assert_eq!(style, Style::default());
    }
}
