//! What goes inside the sidebar card.
//!
//! The card itself — its shape, its overhang, the strip with the window
//! controls — is [`super::shell`]'s job. This file starts underneath that
//! strip and fills the rest of the column.

use crate::i18n::{t, tf};

use eframe::egui;
use image::{Rgba, RgbaImage};

use super::ocr_job::OcrState;
use super::{
    Mode, OcrMode, PickMode, SWATCH_PX, Section, ShotrApp, Swatch, swatch_order, theme,
    to_color_image,
};
use crate::export;
use crate::ocr::detect::Secret;
use crate::render::background::{BG_PRESETS, auto_preset, image_cover, linear, mesh};
use crate::settings::{
    Background, CustomKind, ExportFormat, RATIO_PRESETS, Ratio, RedactStyle, Rgba8, Style,
};
use crate::wallpaper;

impl ShotrApp {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        if ui
            .add(egui::Button::new(t("Capture again")).min_size(egui::vec2(width, 27.0)))
            .clicked()
        {
            self.start_capture(self.source);
        }

        if self.mode == Mode::Edit {
            ui.add_space(10.0);
            self.preset_row(ui);
        }
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.mode {
                Mode::Select => self.select_sidebar(ui),
                Mode::Edit => self.edit_sidebar(ui),
            });
    }

    /// One accordion group. The fold draws the heading and the rule; this holds
    /// the rule that only one of them is open at a time.
    fn section_fold(
        &mut self,
        ui: &mut egui::Ui,
        which: Section,
        title: &str,
        body: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        let open = self.open_section == Some(which);
        if theme::fold(ui, title, open, |ui| body(self, ui)) {
            self.open_section = if open { None } else { Some(which) };
        }
    }

    // ----------------------------------------------------------------- select

    /// The two ways into an image that is not a fresh capture.
    fn open_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(t("Open file…")).clicked()
                && let Some(p) = export::open_image_dialog()
            {
                self.open_image(&p);
            }
            if ui.button(t("From clipboard")).clicked() {
                self.open_from_clipboard();
            }
        });
    }

    fn select_sidebar(&mut self, ui: &mut egui::Ui) {
        // Opened from the tray as a hub, there is no shot to pick a region out
        // of — offering the crop controls over an empty placeholder invites a
        // click that cannot do anything.
        if self.hub {
            ui.strong(t("Open a shot"));
            ui.add_space(6.0);
            self.open_buttons(ui);
            self.history_strip(ui);
            return;
        }

        ui.strong(t("Step 1 — pick a region"));

        ui.add_space(4.0);
        if !self.windows.is_empty() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.pick_mode, PickMode::Region, t("Region"));
                ui.selectable_value(&mut self.pick_mode, PickMode::Window, t("Window"));
                ui.label(egui::RichText::new("Space").weak().small());
            });
        }

        ui.add_space(4.0);
        match self.pick_mode {
            PickMode::Region => {
                ui.label(t("Drag on the image to pick a region."));
                match self.crop_px {
                    Some([_, _, w, h]) => ui.label(tf(
                        "Selection: {w} × {h} px",
                        &[("w", &w.to_string()), ("h", &h.to_string())],
                    )),
                    None => ui.label(t("(no region selected)")),
                };
            }
            PickMode::Window => {
                if self.windows.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            t("This compositor will not list windows. Use Region instead."),
                        )
                        .weak(),
                    );
                } else {
                    ui.label(tf(
                        "{n} windows. Click one in the list.",
                        &[("n", &self.windows.len().to_string())],
                    ));
                    ui.label(
                        egui::RichText::new(
                            t("Taken from the window's own buffer, so one behind another still comes out whole."),
                        )
                        .weak()
                        .small(),
                    );
                }
            }
        }

        ui.add_space(8.0);
        let can_crop = self.crop_px.is_some();
        if ui
            .add_enabled(can_crop, egui::Button::new(t("Crop to selection")))
            .clicked()
        {
            self.finish_selection(true);
        }
        if ui.button(t("Use the whole screen")).clicked() {
            self.finish_selection(false);
        }

        ui.add_space(6.0);
        self.open_buttons(ui);

        self.history_strip(ui);
    }

    fn history_strip(&mut self, ui: &mut egui::Ui) {
        if self.history.is_empty() {
            return;
        }
        theme::rule(ui);
        theme::section(ui, t("History"));
        ui.add_space(4.0);

        // Thumbnails load lazily; the vector is cleared whenever history changes.
        if self.history_thumbs.len() != self.history.len() {
            self.history_thumbs = vec![None; self.history.len()];
        }

        let mut open: Option<std::path::PathBuf> = None;
        egui::ScrollArea::horizontal()
            .id_salt("history")
            .max_height(90.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for i in 0..self.history.len() {
                        if self.history_thumbs[i].is_none()
                            && let Ok(img) = image::open(&self.history[i].thumb)
                        {
                            let rgba = img.to_rgba8();
                            self.history_thumbs[i] = Some(ui.ctx().load_texture(
                                format!("hist-{}", self.history[i].ts),
                                to_color_image(&rgba),
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                        let Some(tex) = &self.history_thumbs[i] else {
                            continue;
                        };
                        let size = tex.size_vec2();
                        let scale = 72.0 / size.y.max(1.0);
                        let image =
                            egui::Image::new(egui::load::SizedTexture::new(tex.id(), size * scale));
                        if ui
                            .add(egui::Button::image(image).corner_radius(4))
                            .clicked()
                        {
                            open = Some(self.history[i].image.clone());
                        }
                    }
                });
            });

        if let Some(path) = open {
            self.open_image(&path);
        }
    }

    // ------------------------------------------------------------------- edit

    fn edit_sidebar(&mut self, ui: &mut egui::Ui) {
        self.section_fold(ui, Section::Background, t("Background"), |s, ui| {
            s.background_grid(ui)
        });
        self.section_fold(ui, Section::Layout, t("Layout"), |s, ui| s.layout_section(ui));
        self.section_fold(ui, Section::Ratio, t("Ratio / Size"), |s, ui| {
            s.ratio_section(ui)
        });
        self.section_fold(ui, Section::Ocr, t("Text recognition (OCR)"), |s, ui| {
            s.ocr_body(ui)
        });
        self.section_fold(ui, Section::Watermark, t("Watermark"), |s, ui| {
            s.watermark_section(ui)
        });
        self.section_fold(ui, Section::Export, t("Export"), |s, ui| s.export_section(ui));

        ui.add_space(12.0);
        if ui.button(t("Reset to defaults")).clicked() {
            self.style = Style::default();
        }
        ui.add_space(16.0);
    }

    fn layout_section(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.style;
        theme::slider_label(ui, t("Padding"), s.padding);
        ui.add(egui::Slider::new(&mut s.padding, 0..=400).show_value(false));
        theme::slider_label(ui, t("Border radius"), s.radius);
        ui.add(egui::Slider::new(&mut s.radius, 0..=120).show_value(false));
        theme::slider_label(ui, t("Add a shadow"), s.shadow);
        ui.add(egui::Slider::new(&mut s.shadow, 0..=100).show_value(false));

        ui.horizontal(|ui| {
            ui.label(t("Inset"));
            if s.inset_auto_color {
                match self.detected_inset {
                    Some(_) => ui.label(
                        egui::RichText::new(t("(background colour detected)"))
                            .weak()
                            .small(),
                    ),
                    None => ui.label(
                        egui::RichText::new(t("(no background colour found)"))
                            .weak()
                            .small(),
                    ),
                };
            }
        });
        let s = &mut self.style;
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut s.inset, 0..=80).show_value(true));
            // The swatch shows what is actually being drawn, and touching it is
            // how you take the colour back off auto.
            let mut shown = match (s.inset_auto_color, self.detected_inset) {
                (true, Some(c)) => c,
                _ => s.inset_color,
            };
            let before = shown;
            color_button(ui, &mut shown);
            if shown != before {
                s.inset_color = shown;
                s.inset_auto_color = false;
            }
            if !s.inset_auto_color && ui.small_button("auto").clicked() {
                s.inset_auto_color = true;
            }
        });
        // Only means anything while the colour is being detected, so it is only
        // offered then — and only once there is an inset to drop.
        if s.inset_auto_color && s.inset > 0 {
            ui.checkbox(
                &mut s.inset_only_if_detected,
                t("Only when a colour is found"),
            )
            .on_hover_text(t(
                "Leave the inset off rather than falling back to a plain colour.",
            ));
        }
        ui.checkbox(&mut s.balance, t("Balance"))
            .on_hover_text(t("Trim uniform edges so the subject sits centred"));
    }

    fn ratio_section(&mut self, ui: &mut egui::Ui) {
        self.ratio_chips(ui);
        ui.horizontal(|ui| {
            let is_custom =
                self.style.ratio == Ratio::Size(self.style.custom_size.0, self.style.custom_size.1);
            if ui.selectable_label(is_custom, t("Custom…")).clicked() {
                self.show_custom_size = !self.show_custom_size;
                if self.show_custom_size {
                    let (w, h) = self.style.custom_size;
                    self.style.ratio = Ratio::Size(w, h);
                }
            }
        });
        if self.show_custom_size {
            ui.horizontal(|ui| {
                let (mut w, mut h) = self.style.custom_size;
                ui.label("W");
                let a = ui.add(egui::DragValue::new(&mut w).range(64..=8000).speed(4));
                ui.label("H");
                let b = ui.add(egui::DragValue::new(&mut h).range(64..=8000).speed(4));
                if a.changed() || b.changed() {
                    self.style.custom_size = (w, h);
                    self.style.ratio = Ratio::Size(w, h);
                }
            });
        }
    }


    /// Watermark controls.
    ///
    /// The set of options follows what watermarking tools converge on: a
    /// wordmark or a logo, anchored on a nine-square grid or tiled across the
    /// whole picture, with size, angle and opacity on their own dials.
    fn watermark_section(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.style.watermark, t("Enable watermark"));
        if !self.style.watermark {
            return;
        }

        // --- what to stamp -------------------------------------------------
        let has_logo = self.style.watermark_image.is_some();
        ui.horizontal(|ui| {
            if ui.selectable_label(!has_logo, t("Text")).clicked() {
                self.style.watermark_image = None;
            }
            if ui.selectable_label(has_logo, t("Logo image")).clicked() && !has_logo
                && let Some(p) = export::open_image_dialog() {
                    self.style.watermark_image = Some(p);
                }
        });

        match self.style.watermark_image.clone() {
            Some(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).weak().small());
                    if ui.small_button(t("Change…")).clicked()
                        && let Some(p) = export::open_image_dialog()
                    {
                        self.style.watermark_image = Some(p);
                    }
                });
            }
            None => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.style.watermark_text)
                            .hint_text("Enter text")
                            .desired_width(ui.available_width() - 34.0),
                    );
                    if ui.small_button("©").on_hover_text("Chèn ký hiệu bản quyền").clicked() {
                        self.style.watermark_text.insert(0, '©');
                        self.style.watermark_text.insert(1, ' ');
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    for style in crate::settings::WatermarkStyle::ALL {
                        ui.selectable_value(&mut self.style.watermark_style, style, style.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(t("Text colour"));
                    color_button(ui, &mut self.style.watermark_color);
                });
            }
        }

        // --- where to put it -----------------------------------------------
        ui.add_space(4.0);
        ui.checkbox(&mut self.style.watermark_tiled, t("Tile across the image"))
            .on_hover_text("Repeat diagonally across the image — the anti-reuse look");

        if !self.style.watermark_tiled {
            ui.label(egui::RichText::new(t("Position")).weak().small());
            // The nine-square grid, laid out as it reads.
            egui::Grid::new("wm-pos").spacing([4.0, 4.0]).show(ui, |ui| {
                for (i, pos) in crate::settings::WatermarkPos::ALL.into_iter().enumerate() {
                    let on = self.style.watermark_pos == pos;
                    if ui.add(egui::Button::new(if on { "●" } else { "○" }).min_size(egui::vec2(28.0, 22.0)).selected(on)).clicked() {
                        self.style.watermark_pos = pos;
                    }
                    if (i + 1) % 3 == 0 {
                        ui.end_row();
                    }
                }
            });
        }

        // --- how it looks ---------------------------------------------------
        theme::slider_label(ui, t("Size"), format!("{:.0}%", self.style.watermark_size * 100.0));
        ui.add(egui::Slider::new(&mut self.style.watermark_size, 0.4..=4.0).show_value(false));

        let pct = (self.style.watermark_opacity as f32 / 255.0 * 100.0).round() as u8;
        theme::slider_label(ui, t("Opacity"), format!("{pct}%"));
        let mut p = pct;
        if ui.add(egui::Slider::new(&mut p, 5..=100).show_value(false)).changed() {
            self.style.watermark_opacity = (p as f32 / 100.0 * 255.0).round() as u8;
        }

        theme::slider_label(ui, t("Angle"), format!("{:.0}°", self.style.watermark_angle));
        ui.add(egui::Slider::new(&mut self.style.watermark_angle, -90.0..=90.0).show_value(false));
        if self.style.watermark_tiled {
            ui.label(
                egui::RichText::new(t("Protective tiling is usually 20–40% opacity at -30°."))
                    .weak()
                    .small(),
            );
        }
    }

    /// Presets on one row: pick, name, save — the design puts them there
    /// because they are one thought, not three sections.
    fn preset_row(&mut self, ui: &mut egui::Ui) {
        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;
        let matching = self.presets.iter().position(|p| p.style == self.style);

        ui.horizontal(|ui| {
            let selected = matching
                .map(|i| self.presets[i].name.clone())
                .unwrap_or_else(|| "—".to_string());
            egui::ComboBox::from_id_salt("preset")
                .selected_text(selected)
                .width(126.0)
                .show_ui(ui, |ui| {
                    if self.presets.is_empty() {
                        ui.label(egui::RichText::new(t("(no presets yet)")).weak());
                    }
                    for (i, p) in self.presets.iter().enumerate() {
                        if ui.selectable_label(false, &p.name).clicked() {
                            apply = Some(i);
                        }
                    }
                });

            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text(t("Preset name"))
                    .desired_width(84.0),
            );
            if ui.button(t("Save")).clicked() {
                self.save_preset();
            }
            if matching.is_some()
                && ui
                    .small_button("🗑")
                    .on_hover_text(t("Delete the preset that matches"))
                    .clicked()
            {
                delete = matching;
            }
        });

        if let Some(i) = apply {
            self.style = self.presets[i].style.clone();
            self.preset_name = self.presets[i].name.clone();
        }
        if let Some(i) = delete {
            self.delete_preset(i);
        }
    }

    fn background_grid(&mut self, ui: &mut egui::Ui) {
        let swatches = std::mem::take(&mut self.swatches);
        let mut picked: Option<Swatch> = None;

        // Size the cells from the width actually on offer. Hard-coding it means
        // any change to button padding, font size or panel width silently
        // pushes the last column off the edge — and because this grid is the
        // widest thing in the sidebar, it drags the whole scroll area out with
        // it and clips every row below.
        const COLS: usize = 5;
        const GAP: f32 = 6.0;
        let pad = ui.spacing().button_padding.x * 2.0;
        let side = grid_cell_side(ui.available_width(), COLS, GAP, pad);

        egui::Grid::new("bg-grid")
            .num_columns(COLS)
            .spacing([GAP, GAP])
            .show(ui, |ui| {
                for (i, (sw, tex)) in swatches.iter().enumerate() {
                    let selected = self.style.background == sw.background();
                    let img = egui::Image::new(egui::load::SizedTexture::new(
                        tex.id(),
                        egui::vec2(side, side),
                    ));
                    let resp = ui
                        .add(egui::Button::image(img).selected(selected).corner_radius(6))
                        .on_hover_text(sw.label());
                    if resp.clicked() {
                        picked = Some(*sw);
                    }
                    if (i + 1) % COLS == 0 {
                        ui.end_row();
                    }
                }
            });

        self.swatches = swatches;

        if let Some(sw) = picked {
            self.style.background = sw.background();
            // Custom-with-image but nothing chosen yet would render as a flat
            // colour; the gradient is a friendlier landing spot.
            if sw == Swatch::Custom
                && self.style.custom_bg.kind == CustomKind::Image
                && self.style.custom_bg.image.is_none()
            {
                self.style.custom_bg.kind = CustomKind::Linear;
            }
        }

        if self.style.background == Background::Custom {
            ui.add_space(4.0);
            let c = &mut self.style.custom_bg;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut c.kind, CustomKind::Solid, t("Solid colour"));
                ui.selectable_value(&mut c.kind, CustomKind::Linear, "Gradient");
                ui.selectable_value(&mut c.kind, CustomKind::Image, t("Image"));
            });
            match c.kind {
                CustomKind::Solid => {
                    ui.horizontal(|ui| {
                        ui.label(t("Colour"));
                        color_button(ui, &mut c.color_a);
                    });
                }
                CustomKind::Linear => {
                    ui.horizontal(|ui| {
                        ui.label("A");
                        color_button(ui, &mut c.color_a);
                        ui.label("B");
                        color_button(ui, &mut c.color_b);
                    });
                    ui.add(egui::Slider::new(&mut c.angle, 0.0..=360.0).text("Corner"));
                }
                CustomKind::Image => {
                    let label = c
                        .image
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Chọn ảnh nền…".to_string());
                    if ui.button(label).clicked()
                        && let Some(p) = export::open_image_dialog()
                    {
                        c.image = Some(p);
                    }
                }
            }
        }
    }

    fn ratio_chips(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for preset in RATIO_PRESETS {
                let selected = self.style.ratio == preset.ratio;
                let hint = match preset.ratio {
                    Ratio::Size(w, h) => format!("{w} × {h}"),
                    _ => preset.name.to_string(),
                };
                if ui
                    .selectable_label(selected, preset.name)
                    .on_hover_text(hint)
                    .clicked()
                {
                    self.style.ratio = preset.ratio;
                }
            }
        });
    }

    fn export_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Xuất file");
        let s = &mut self.prefs;
        ui.horizontal(|ui| {
            for f in ExportFormat::ALL {
                ui.selectable_value(&mut s.format, f, f.label());
            }
        });
        match s.format {
            ExportFormat::Jpeg => {
                ui.add(egui::Slider::new(&mut s.jpeg_quality, 40..=100).text("Quality"));
                ui.label(
                    egui::RichText::new(
                        t("JPEG has no alpha channel — a transparent background becomes white."),
                    )
                    .weak()
                    .small(),
                );
            }
            ExportFormat::Png => {
                ui.checkbox(&mut s.png_max_compression, t("Maximum compression (slower)"));
            }
            ExportFormat::Webp => {
                ui.label(
                    egui::RichText::new(t("WebP is lossless here, so there is no quality slider."))
                        .weak()
                        .small(),
                );
            }
        }
        ui.horizontal(|ui| {
            ui.label(t("Filename"));
            ui.add(
                egui::TextEdit::singleline(&mut s.filename_template)
                    .hint_text("shotr-{date}-{time}")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.label(
            egui::RichText::new(format!(
                "→ {}.{}",
                crate::export::expand_template(&s.filename_template, 1_700_000_000),
                s.format.extension()
            ))
            .weak()
            .small(),
        );
    }

    fn ocr_body(&mut self, ui: &mut egui::Ui) {

        match self.ocr_state.clone() {
            OcrState::Absent => {
                ui.label(
                    egui::RichText::new(t("Needs a 12 MB model; everything runs on this machine."))
                        .weak()
                        .small(),
                );
                if ui.button(t("Download the OCR model")).clicked() {
                    let ctx = ui.ctx().clone();
                    self.start_model_download(&ctx);
                }
                ui.label(
                    egui::RichText::new(
                        "Model này không đọc được dấu tiếng Việt. \
                         Cài gói tesseract-data-vie để đọc đúng.",
                    )
                    .weak()
                    .small(),
                );
                return;
            }
            OcrState::Downloading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Downloading the model…"));
                });
                return;
            }
            OcrState::Reading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Reading text…"));
                });
                return;
            }
            OcrState::Failed(e) => {
                ui.colored_label(theme::pal().danger, e);
                if ui.button(t("Try again")).clicked() {
                    let ctx = ui.ctx().clone();
                    self.start_ocr(&ctx);
                }
                return;
            }
            OcrState::Ready => {}
        }

        if self.ocr_words.is_empty() {
            ui.label(egui::RichText::new(t("No text found.")).weak().small());
        }
        // Say which engine read the image: the two differ on Vietnamese badly
        // enough that a user seeing mangled diacritics needs to know why.
        match crate::ocr::tesseract::best_langs() {
            Some(langs) => {
                let msg = tf("Read with tesseract ({langs})", &[("langs", &langs)]);
                ui.label(egui::RichText::new(msg).weak().small());
            }
            None => {
                ui.label(
                    egui::RichText::new(t("Read with ocrs — no Vietnamese diacritics."))
                        .weak()
                        .small(),
                );
            }
        }

        let found = self.active_finding_count();
        ui.checkbox(
            &mut self.prefs.redact,
            tf("Redact sensitive data ({found} found)", &[("found", &found.to_string())]),
        );

        if self.prefs.redact {
            let counts = [
                self.count_of(Secret::Email),
                self.count_of(Secret::CreditCard),
                self.count_of(Secret::IpAddress),
                self.count_of(Secret::ApiKey),
                self.count_of(Secret::Phone),
            ];
            ui.indent("redact-kinds", |ui| {
                let s = &mut self.prefs;
                for (flag, kind, n) in [
                    (&mut s.redact_email, Secret::Email, counts[0]),
                    (&mut s.redact_card, Secret::CreditCard, counts[1]),
                    (&mut s.redact_ip, Secret::IpAddress, counts[2]),
                    (&mut s.redact_key, Secret::ApiKey, counts[3]),
                    (&mut s.redact_phone, Secret::Phone, counts[4]),
                ] {
                    ui.checkbox(flag, format!("{} ({n})", kind.label()));
                }
            });

            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.style.redact_style,
                    RedactStyle::Solid,
                    t("Solid"),
                );
                ui.selectable_value(&mut self.style.redact_style, RedactStyle::Blur, t("Blur"));
            });
            match self.style.redact_style {
                RedactStyle::Solid => {
                    ui.horizontal(|ui| {
                        ui.label(t("Redaction colour"));
                        color_button(ui, &mut self.style.redact_color);
                    });
                }
                RedactStyle::Blur => {
                    ui.add(
                        egui::Slider::new(&mut self.style.redact_blur, 2.0..=60.0).text("Blur amount"),
                    );
                }
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.ocr_mode, OcrMode::Off, t("Off"));
            ui.selectable_value(&mut self.ocr_mode, OcrMode::SelectText, t("Select text"));
            ui.selectable_value(&mut self.ocr_mode, OcrMode::ManualRedact, "Che tay");
        });

        match self.ocr_mode {
            OcrMode::SelectText => {
                ui.label(
                    egui::RichText::new(t("Drag on the image to select text."))
                        .weak()
                        .small(),
                );
                ui.horizontal(|ui| {
                    let n = self.selected_words.len();
                    if ui
                        .add_enabled(n > 0, egui::Button::new(tf("{n} words copied", &[("n", &n.to_string())])))
                        .clicked()
                    {
                        self.copy_text(true);
                    }
                    if ui.button(t("Copy all")).clicked() {
                        self.copy_text(false);
                    }
                });
            }
            OcrMode::ManualRedact => {
                ui.label(
                    egui::RichText::new(t("Click a word to hide or reveal it."))
                        .weak()
                        .small(),
                );
                if !self.manual_redact.is_empty()
                    && ui
                        .button(tf("Hide {n} words", &[("n", &self.manual_redact.len().to_string())]))
                        .clicked()
                {
                    self.manual_redact.clear();
                    self.dirty = true;
                }
            }
            OcrMode::Off => {}
        }
    }

    pub(super) fn rebuild_swatches(&mut self, ctx: &egui::Context) {
        self.swatches.clear();
        for sw in swatch_order() {
            let img = match sw {
                Swatch::Preset(i) => mesh(SWATCH_PX, SWATCH_PX, &BG_PRESETS[i]),
                // The Auto swatch previews the palette derived from this very
                // screenshot, so it changes as you capture different things.
                Swatch::Auto => mesh(SWATCH_PX, SWATCH_PX, &auto_preset(&self.shot_preview)),
                Swatch::None => checkerboard(SWATCH_PX, SWATCH_PX),
                Swatch::Desktop => match self.bg_image.as_ref() {
                    Some(img) => image_cover(SWATCH_PX, SWATCH_PX, img),
                    None => match wallpaper::current().and_then(|p| image::open(p).ok()) {
                        Some(img) => image_cover(SWATCH_PX, SWATCH_PX, &img.to_rgba8()),
                        None => {
                            RgbaImage::from_pixel(SWATCH_PX, SWATCH_PX, Rgba([90, 92, 100, 255]))
                        }
                    },
                },
                Swatch::Custom => {
                    let c = &self.style.custom_bg;
                    match c.kind {
                        CustomKind::Solid => {
                            RgbaImage::from_pixel(SWATCH_PX, SWATCH_PX, Rgba(c.color_a))
                        }
                        _ => linear(SWATCH_PX, SWATCH_PX, c.color_a, c.color_b, c.angle),
                    }
                }
            };
            let tex = ctx.load_texture(
                format!("swatch-{}", sw.label()),
                to_color_image(&img),
                egui::TextureOptions::LINEAR,
            );
            self.swatches.push((sw, tex));
        }
    }

}

pub(super) fn color_button(ui: &mut egui::Ui, color: &mut Rgba8) -> egui::Response {
    let mut c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let resp = ui.color_edit_button_srgba(&mut c);
    if resp.changed() {
        *color = c.to_srgba_unmultiplied();
    }
    resp
}

fn checkerboard(w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let cell = 8;
    for y in 0..h {
        for x in 0..w {
            let on = ((x / cell) + (y / cell)) % 2 == 0;
            let v = if on { 220 } else { 160 };
            img.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    img
}

/// Side length for one square swatch so that `cols` of them, plus their button
/// padding and the gaps between, fit inside `avail`.
fn grid_cell_side(avail: f32, cols: usize, gap: f32, pad: f32) -> f32 {
    let cols = cols.max(1);
    let cell = (avail - gap * (cols - 1) as f32) / cols as f32;
    (cell - pad).floor().clamp(20.0, 56.0)
}

#[cfg(test)]
mod tests {
    use super::grid_cell_side;

    /// The swatch grid is the widest thing in the sidebar. If it overflows it
    /// does not just clip itself — it widens the scroll area, so every row
    /// below wraps to the wrong width and gets cut off too. So: no overflow,
    /// at any padding or panel width we might plausibly end up with.
    #[test]
    fn a_row_of_swatches_never_overflows_the_panel() {
        for avail in [200.0_f32, 240.0, 272.0, 284.0, 300.0, 320.0, 400.0] {
            for pad in [4.0_f32, 8.0, 14.0, 18.0] {
                for cols in [4_usize, 5, 6] {
                    let gap = 6.0;
                    let side = grid_cell_side(avail, cols, gap, pad);
                    let used = (side + pad) * cols as f32 + gap * (cols - 1) as f32;
                    // The clamp floor can win on a very narrow panel; that is a
                    // deliberate legibility floor, not a layout bug.
                    if side > 20.0 {
                        assert!(
                            used <= avail + 0.5,
                            "avail={avail} pad={pad} cols={cols}: used {used}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn swatches_stay_legible_and_never_grow_absurd() {
        assert_eq!(grid_cell_side(120.0, 5, 6.0, 14.0), 20.0, "narrow: floor");
        assert_eq!(grid_cell_side(2000.0, 5, 6.0, 14.0), 56.0, "wide: ceiling");
    }

    /// The regression this was written for: 5 columns of 44px images at the
    /// button padding the theme actually uses had to fit the real sidebar.
    #[test]
    fn the_shipped_sidebar_width_fits_five_columns() {
        // The shipped card, minus its 12 px of padding on each side and
        // everything the scrollbar reserves: bar width + inner gap + outer gap.
        let avail = 336.0 - 24.0 - (8.0 + 10.0 + 4.0);
        let side = grid_cell_side(avail, 5, 6.0, 7.0 * 2.0);
        assert!(side >= 30.0, "swatches would be too small to read: {side}");
        let used = (side + 14.0) * 5.0 + 6.0 * 4.0;
        assert!(used <= avail + 0.5, "still overflows: {used} > {avail}");
    }
}
