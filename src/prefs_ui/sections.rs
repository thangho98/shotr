//! The settings that are the same for every shot.

use eframe::egui;

use crate::app::theme;
use crate::i18n::{self, t};
use crate::settings::{ExportFormat, Prefs, ThemeMode};

pub fn general(ui: &mut egui::Ui, prefs: &mut Prefs) {
    theme::section(ui, t("Language"));
    let langs: Vec<(i18n::Lang, &str)> = i18n::Lang::ALL
        .into_iter()
        .map(|l| (l, l.label()))
        .collect();
    if theme::segmented(ui, "lang", &mut prefs.lang, &langs) {
        i18n::set(prefs.lang);
    }

    ui.add_space(12.0);
    theme::section(ui, t("Theme"));
    // Only the choice is made here. Applying it is the window's job — this one
    // repaints itself on the next frame, and the editor reads the same setting.
    let modes: Vec<(ThemeMode, &str)> = ThemeMode::ALL
        .into_iter()
        .map(|m| (m, m.label()))
        .collect();
    theme::segmented(ui, "theme", &mut prefs.theme, &modes);
    if prefs.theme == ThemeMode::System {
        ui.add_space(4.0);
        theme::hint(ui, t("Changes with the desktop, while shotr is running."));
    }

    ui.add_space(12.0);
    theme::section(ui, t("Where shots are saved"));
    let shown = prefs.save_dir().display().to_string();
    theme::hint(ui, shown);
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
    let formats: Vec<(ExportFormat, &str)> = ExportFormat::ALL
        .into_iter()
        .map(|f| (f, f.label()))
        .collect();
    theme::segmented(ui, "default-format", &mut prefs.format, &formats);

    match prefs.format {
        ExportFormat::Jpeg => {
            ui.add_space(8.0);
            theme::slider_row(ui, t("Quality"), &mut prefs.jpeg_quality, 40..=100, "");
        }
        ExportFormat::Png => {
            ui.add_space(8.0);
            theme::checkbox(
                ui,
                &mut prefs.png_max_compression,
                t("Maximum compression (slower)"),
            );
        }
        ExportFormat::Webp => {}
    }

    ui.add_space(12.0);
    theme::section(ui, t("File name"));
    theme::field(ui, theme::RADIUS_CONTROL, |ui| {
        ui.add_sized(
            egui::vec2(ui.available_width(), theme::H_BAR),
            egui::TextEdit::singleline(&mut prefs.filename_template)
                .vertical_align(egui::Align::Center)
                .margin(egui::Margin::symmetric(theme::FIELD_PAD, 0)),
        );
    });
    ui.add_space(4.0);
    let example = crate::export::expand_template(&prefs.filename_template, 1_700_000_000);
    theme::hint(ui, format!("{example}.{}", prefs.format.extension()));
    ui.add_space(2.0);
    theme::hint(ui, t("Tokens: {date}, {time}, {unix}"));
}

pub fn redaction(ui: &mut egui::Ui, prefs: &mut Prefs) {
    theme::hint(
        ui,
        t("Which kinds of text are covered when redaction is switched on for a shot."),
    );
    ui.add_space(10.0);
    theme::checkbox(ui, &mut prefs.redact, t("Redact by default"));
    ui.add_space(8.0);
    ui.spacing_mut().item_spacing.y = theme::ROW_GAP;
    for (on, label) in [
        (&mut prefs.redact_email, t("Email addresses")),
        (&mut prefs.redact_card, t("Card numbers")),
        (&mut prefs.redact_ip, t("IP addresses")),
        (&mut prefs.redact_key, t("API keys")),
        (&mut prefs.redact_phone, t("Phone numbers")),
    ] {
        theme::checkbox(ui, on, label);
    }
    ui.add_space(6.0);
    theme::hint(
        ui,
        t("Phone numbers are off by default: that pattern is the loosest of the set and the most likely to cover something that is not a phone number."),
    );
}
