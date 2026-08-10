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
    ("1 – 6", "Pick a drawing tool"),
    ("`", "Back to the Select tool"),
    ("Esc", "Back to the Select tool, or cancel"),
    ("{mod} + C", "Copy the finished image and close"),
    ("{mod} + S", "Save the finished image"),
    ("{mod} + Z", "Undo, and Shift to redo"),
    ("{mod} + wheel", "Zoom in and out"),
    ("Middle drag", "Pan the image"),
    ("{mod} + 0", "Fit to the window"),
    ("{mod} + 1", "Back to 100%"),
    ("Space", "Switch between region and window picking"),
    ("Enter", "Take the whole screen"),
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
            // `{mod}` is filled in per platform: both Ctrl and Cmd work
            // everywhere, but the label has to name the one that is actually
            // under the reader's thumb.
            let keys = keys.replace("{mod}", crate::app::MOD_LABEL);
            ui.add_sized(
                egui::vec2(120.0, 18.0),
                egui::Label::new(egui::RichText::new(keys).monospace()),
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
    /// The placeholder has to be substituted, or the About list reads
    /// "{mod} + C" at the user.
    #[test]
    fn no_row_reaches_the_screen_still_holding_the_placeholder() {
        let mut substituted = 0;
        for (keys, _) in super::KEYS {
            let shown = keys.replace("{mod}", crate::app::MOD_LABEL);
            assert!(!shown.contains('{'), "{keys} still has a placeholder in it");
            if keys.contains("{mod}") {
                substituted += 1;
            }
        }
        assert!(
            substituted >= 5,
            "only {substituted} rows name the modifier; the list has stopped using it"
        );
    }

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
