//! Version, and the keys the editor binds while an image is open.
//!
//! This file used to say editable shortcuts were out of reach, because a global
//! hotkey library is X11 only and so cannot serve Wayland. That part still
//! holds. What it missed is that **Linux needs no library**: COSMIC and GNOME
//! already bind a command to a key, and macOS is the only platform with no way
//! to do it at all. So the capture hotkeys moved to [`super::shortcuts`], which
//! is editable exactly where the system provides nothing — and the keys here
//! stayed, because they belong to a window rather than the whole desktop and
//! nothing outside shotr can collide with them.
//!
//! Auto-update is still its own project.

use eframe::egui;

use crate::app::theme;
use crate::i18n::t;

const REPO: &str = "https://github.com/thangho98/shotr";

/// What the editor binds today, taken from the same key handling it uses.
const KEYS: &[(&str, &str)] = &[
    ("Ctrl + wheel", "Zoom in and out"),
    ("Middle drag", "Pan the image"),
    ("Ctrl + 0", "Fit to the window"),
    ("Ctrl + 1", "Back to 100%"),
    ("Space", "Switch between region and window picking"),
    ("Enter", "Take the whole screen"),
    ("Esc", "Cancel"),
];

/// What the editor binds while an image is open. These stay fixed: they are
/// window shortcuts, not global ones, so nothing outside shotr can collide with
/// them and there is nothing for a picker to resolve.
pub fn editor_keys(ui: &mut egui::Ui) {
    theme::section(ui, t("In the editor"));
    ui.label(
        egui::RichText::new(t("These are fixed for now."))
            .weak()
            .small(),
    );
    ui.add_space(10.0);
    for (keys, what) in KEYS {
        ui.horizontal(|ui| {
            ui.add_sized(
                egui::vec2(120.0, 18.0),
                egui::Label::new(egui::RichText::new(*keys).monospace()),
            );
            ui.label(t(what));
        });
    }
}

pub fn version(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(t("Version"));
        ui.label(egui::RichText::new(env!("CARGO_PKG_VERSION")).monospace());
    });
    ui.add_space(10.0);
    if ui.button(t("Releases")).clicked() {
        let _ = open_url(&format!("{REPO}/releases"));
    }
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("shotr — GPL-3.0-only")
            .weak()
            .small(),
    );
}

/// Whatever the desktop uses to open a link.
fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(url).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    /// A shortcut with no description, or the other way round, would render as a
    /// blank row.
    #[test]
    fn every_listed_shortcut_says_what_it_does() {
        for (keys, what) in super::KEYS {
            assert!(!keys.trim().is_empty(), "a shortcut row has no keys");
            assert!(!what.trim().is_empty(), "{keys} has no description");
        }
    }
}
