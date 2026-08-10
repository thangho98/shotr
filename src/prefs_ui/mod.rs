//! The Preferences window: `shotr --settings`.
//!
//! A separate process, the same shape everything the tray offers already uses.
//! It reads and writes [`crate::settings::Prefs`] only — the look of a shot is
//! [`crate::settings::Style`], and that stays in the editor's sidebar where the
//! image it applies to is visible.
//!
//! It draws its own window, for the same reason the editor does: a card that
//! overhangs the frame cannot be built out of a system-decorated rectangle. See
//! [`shell`].

mod about;
mod icons;
#[cfg(target_os = "macos")]
mod permission;
mod sections;
mod shell;
mod shortcuts;

use eframe::egui;

use crate::app::theme;
use crate::i18n::t;
use crate::settings::Prefs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Section {
    #[cfg(target_os = "macos")]
    Permission,
    General,
    Export,
    Redaction,
    Shortcuts,
    About,
}

impl Section {
    /// In the order a person meets them: the thing that stops shotr working
    /// first, then what they came to change, then reference material.
    const ALL: &'static [Section] = &[
        #[cfg(target_os = "macos")]
        Section::Permission,
        Section::General,
        Section::Export,
        Section::Redaction,
        Section::Shortcuts,
        Section::About,
    ];

    /// What the window opens on.
    ///
    /// Permission leads the list, but almost nobody comes here for it — so it
    /// only takes the opening slot while the grant it exists to fix is missing,
    /// which is the one visit that has nothing to do with settings.
    fn opening() -> Section {
        #[cfg(target_os = "macos")]
        if !permission::granted() {
            return Section::Permission;
        }
        Section::General
    }

    fn label(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Section::Permission => t("Permission"),
            Section::General => t("General"),
            Section::Export => t("Export"),
            Section::Redaction => t("Redaction"),
            Section::Shortcuts => t("Shortcuts"),
            Section::About => t("About"),
        }
    }
}

pub struct PrefsApp {
    prefs: Prefs,
    /// Last persisted copy. Saving on change rather than on close means a crash
    /// cannot lose a setting the user watched themselves make.
    saved: Prefs,
    section: Section,
    status: String,
    shortcuts: shortcuts::State,
    /// The nav card's gradient. Dropped when the palette changes, because the
    /// texture holds the old one.
    card_grad: Option<egui::TextureHandle>,
    /// Mirrored rather than read from the viewport each time — see
    /// [`shell`], and the same trap the editor documents.
    maximised: bool,
}

impl PrefsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The language is already set: `run` needs it before this, for the title.
        let prefs = Prefs::load();
        theme::apply(&cc.egui_ctx, prefs.theme);
        Self {
            saved: prefs.clone(),
            prefs,
            section: Section::opening(),
            status: String::new(),
            shortcuts: shortcuts::State::default(),
            card_grad: None,
            maximised: false,
        }
    }

    /// Whatever the chosen section puts in the content pane.
    fn section_ui(&mut self, ui: &mut egui::Ui) {
        match self.section {
            #[cfg(target_os = "macos")]
            Section::Permission => permission::ui(ui, &mut self.status),
            Section::General => sections::general(ui, &mut self.prefs),
            Section::Export => sections::export(ui, &mut self.prefs),
            Section::Redaction => sections::redaction(ui, &mut self.prefs),
            Section::Shortcuts => shortcuts::ui(ui, &mut self.prefs, &mut self.shortcuts),
            Section::About => about::version(ui),
        }
    }
}

impl eframe::App for PrefsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Following the desktop means following it while the window is open.
        theme::sync(ui.ctx());

        self.shell_ui(ui);

        if self.prefs != self.saved {
            if self.prefs.theme != self.saved.theme {
                theme::set_mode(ui.ctx(), self.prefs.theme);
                self.card_grad = None;
            }
            self.prefs.save();
            self.saved = self.prefs.clone();
        }
    }

    /// Transparent, so the frame this window draws for itself can have rounded
    /// corners and the card can hang off its left edge.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

pub fn run() -> eframe::Result {
    // Before the title is built, not inside `PrefsApp::new`: the viewport is
    // described first, so a language set later leaves an English title bar over
    // a Vietnamese window.
    crate::i18n::set(Prefs::load().lang);

    // The design's 780×540 frame, plus the transparent strip the card hangs
    // into. Decorations off and transparency on for the same reason as the
    // editor — and transparency can only be asked for at creation.
    let viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size([780.0 + shell::MARGIN_LEFT, 540.0])
        .with_min_inner_size([620.0 + shell::MARGIN_LEFT, 440.0])
        .with_title(t("shotr — Preferences"))
        .with_icon(crate::app::window_icon());
    eframe::run_native(
        "shotr-settings",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(PrefsApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::Section;

    /// Every section has to be reachable from the nav, or a whole pane of
    /// settings exists with no way to open it.
    #[test]
    fn every_section_is_in_the_nav_and_named() {
        for section in Section::ALL {
            assert!(
                !section.label().trim().is_empty(),
                "a nav entry would draw as a blank row"
            );
        }
        assert!(
            Section::ALL.contains(&Section::opening()),
            "the window opens on a section the nav does not list"
        );
    }
}
