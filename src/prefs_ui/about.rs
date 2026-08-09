//! Shortcuts and version — read-only, on purpose.
//!
//! Editable shortcuts need a global hotkey library, and the obvious one is X11
//! only, so it cannot work on Wayland at all; that is its own project rather
//! than a checkbox. Auto-update is likewise. Listing what exists is honest and
//! useful now, and neither section is an empty tab promising something absent.

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

pub fn shortcuts(ui: &mut egui::Ui) {
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
    ui.add_space(12.0);
    theme::section(ui, t("A shortcut for capturing"));
    ui.label(
        egui::RichText::new(t(
            "Bind a system shortcut to: shotr --capture",
        ))
        .weak()
        .small(),
    );
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
