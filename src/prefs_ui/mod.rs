//! The Preferences window: `shotr --settings`.
//!
//! A separate process, the same shape everything the tray offers already uses.
//! It reads and writes [`crate::settings::Prefs`] only — the look of a shot is
//! [`crate::settings::Style`], and that stays in the editor's sidebar where the
//! image it applies to is visible.

mod about;
#[cfg(target_os = "macos")]
mod permission;
mod sections;

use eframe::egui;

use crate::app::theme;
use crate::i18n::t;
use crate::settings::Prefs;

#[derive(Clone, Copy, PartialEq, Eq)]
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
}

impl PrefsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // The language is already set: `run` needs it before this, for the title.
        let prefs = Prefs::load();
        Self {
            saved: prefs.clone(),
            prefs,
            section: Section::ALL[0],
            status: String::new(),
        }
    }
}

impl eframe::App for PrefsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sections")
            .exact_size(180.0)
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.add_space(12.0);
                for section in Section::ALL {
                    if ui
                        .selectable_label(self.section == *section, section.label())
                        .clicked()
                    {
                        self.section = *section;
                        self.status.clear();
                    }
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.add_space(8.0);
            theme::section(ui, self.section.label());
            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| match self.section {
                #[cfg(target_os = "macos")]
                Section::Permission => permission::ui(ui, &mut self.status),
                Section::General => sections::general(ui, &mut self.prefs),
                Section::Export => sections::export(ui, &mut self.prefs),
                Section::Redaction => sections::redaction(ui, &mut self.prefs),
                Section::Shortcuts => about::shortcuts(ui),
                Section::About => about::version(ui),
            });

            if !self.status.is_empty() {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(&self.status).weak().small());
            }
        });

        if self.prefs != self.saved {
            self.prefs.save();
            self.saved = self.prefs.clone();
        }
    }
}

pub fn run() -> eframe::Result {
    // Before the title is built, not inside `PrefsApp::new`: the viewport is
    // described first, so a language set later leaves an English title bar over
    // a Vietnamese window.
    crate::i18n::set(Prefs::load().lang);

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([720.0, 560.0])
        .with_min_inner_size([620.0, 460.0])
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
