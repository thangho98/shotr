//! The settings that are the same for every shot.

use eframe::egui;

use crate::app::theme;
use crate::i18n::{self, t};
use crate::settings::{ExportFormat, Prefs, ThemeMode};

pub fn general(ui: &mut egui::Ui, prefs: &mut Prefs) {
    theme::section(ui, t("Language"));
    ui.horizontal(|ui| {
        for lang in i18n::Lang::ALL {
            if ui
                .selectable_label(prefs.lang == lang, lang.label())
                .clicked()
            {
                prefs.lang = lang;
                i18n::set(lang);
            }
        }
    });

    ui.add_space(12.0);
    theme::section(ui, t("Theme"));
    // Only the choice is made here. Applying it is the window's job — this one
    // repaints itself on the next frame, and the editor reads the same setting.
    ui.horizontal(|ui| {
        for mode in ThemeMode::ALL {
            if ui
                .selectable_label(prefs.theme == mode, mode.label())
                .clicked()
            {
                prefs.theme = mode;
            }
        }
    });
    if prefs.theme == ThemeMode::System {
        ui.label(
            egui::RichText::new(t("Changes with the desktop, while shotr is running."))
                .weak()
                .small(),
        );
    }

    ui.add_space(12.0);
    theme::section(ui, t("Where shots are saved"));
    let shown = prefs.save_dir().display().to_string();
    ui.label(egui::RichText::new(shown).weak().small());
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(t("Choose a folder…")).clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            prefs.save_dir = Some(dir);
        }
        // A directory that has gone missing already falls back on its own; this
        // is for going back to the default on purpose.
        if prefs.save_dir.is_some() && ui.button(t("Use the default")).clicked() {
            prefs.save_dir = None;
        }
    });
}

pub fn export(ui: &mut egui::Ui, prefs: &mut Prefs) {
    theme::section(ui, t("Default format"));
    ui.horizontal(|ui| {
        for f in ExportFormat::ALL {
            ui.selectable_value(&mut prefs.format, f, f.label());
        }
    });

    match prefs.format {
        ExportFormat::Jpeg => {
            ui.add_space(8.0);
            ui.add(egui::Slider::new(&mut prefs.jpeg_quality, 40..=100).text(t("Quality")));
        }
        ExportFormat::Png => {
            ui.add_space(8.0);
            ui.checkbox(
                &mut prefs.png_max_compression,
                t("Maximum compression (slower)"),
            );
        }
        ExportFormat::Webp => {}
    }

    ui.add_space(12.0);
    theme::section(ui, t("File name"));
    ui.add(egui::TextEdit::singleline(&mut prefs.filename_template).desired_width(f32::INFINITY));
    ui.add_space(4.0);
    let example = crate::export::expand_template(&prefs.filename_template, 1_700_000_000);
    ui.label(
        egui::RichText::new(format!("{example}.{}", prefs.format.extension()))
            .weak()
            .small(),
    );
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(t("Tokens: {date}, {time}, {unix}"))
            .weak()
            .small(),
    );
}

pub fn redaction(ui: &mut egui::Ui, prefs: &mut Prefs) {
    ui.label(
        egui::RichText::new(t(
            "Which kinds of text are covered when redaction is switched on for a shot.",
        ))
        .weak()
        .small(),
    );
    ui.add_space(10.0);
    ui.checkbox(&mut prefs.redact, t("Redact by default"));
    ui.add_space(8.0);
    for (on, label) in [
        (&mut prefs.redact_email, t("Email addresses")),
        (&mut prefs.redact_card, t("Card numbers")),
        (&mut prefs.redact_ip, t("IP addresses")),
        (&mut prefs.redact_key, t("API keys")),
        (&mut prefs.redact_phone, t("Phone numbers")),
    ] {
        ui.checkbox(on, label);
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(t(
            "Phone numbers are off by default: that pattern is the loosest of the set and the most likely to cover something that is not a phone number.",
        ))
        .weak()
        .small(),
    );
}
