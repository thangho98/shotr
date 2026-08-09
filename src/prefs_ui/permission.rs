//! The macOS screen recording grant.
//!
//! Named `prefs_ui` rather than `prefs` so it is never confused with
//! [`crate::settings::Prefs`], which is the data.

use eframe::egui;

use crate::i18n::t;

/// True when this process may capture the screen.
///
/// The preflight call answers without prompting, which is what makes it safe to
/// ask on every frame.
pub fn granted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Ask the system to show its prompt. Returns whether the grant is already held.
///
/// macOS shows this at most once per process, so a second press does nothing
/// visible — hence the button below it, which always works.
pub fn request() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

/// Open the exact pane, rather than telling the user to go and find it.
pub fn open_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub fn ui(ui: &mut egui::Ui, status: &mut String) {
    let ok = granted();

    ui.horizontal(|ui| {
        let (dot, label) = if ok {
            (egui::Color32::from_rgb(0x3d, 0xd6, 0x8c), t("Screen recording is allowed"))
        } else {
            (
                egui::Color32::from_rgb(0xff, 0x6b, 0x6b),
                t("shotr cannot record the screen yet"),
            )
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 5.0, dot);
        ui.label(label);
    });

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(t(
            "Screenshots are taken by the system tool, which can only see the screen if shotr is allowed to.",
        ))
        .weak()
        .small(),
    );

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if !ok && ui.button(t("Ask for permission")).clicked() {
            request();
            *status = t("If no dialog appeared, use the button beside this one.").into();
        }
        if ui.button(t("Open System Settings")).clicked() {
            open_settings();
        }
    });

    ui.add_space(10.0);
    // This costs real debugging time for everyone who hits it: the checkbox goes
    // on, shotr still refuses, and nothing explains why.
    ui.label(
        egui::RichText::new(t(
            "macOS reads this permission once when an app starts. After allowing it, quit shotr from the menu bar and start it again.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(t(
            "The permission is remembered for shotr.app in /Applications. A binary run straight from a terminal borrows the terminal's permission instead.",
        ))
        .weak()
        .small(),
    );
}

#[cfg(test)]
mod tests {
    /// Preflight must never prompt — it runs every frame while the window is
    /// open, and a prompt per frame would be unusable.
    #[test]
    fn preflight_answers_without_prompting() {
        let a = super::granted();
        let b = super::granted();
        assert_eq!(a, b, "preflight must be a pure query");
    }
}
