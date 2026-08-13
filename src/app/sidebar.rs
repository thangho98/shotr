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
        if theme::primary_button(ui, t("Capture again")).clicked() {
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
            self.sync_ocr_mode();
        }
    }

    /// Text recognition may only be armed while its own fold is open.
    ///
    /// A live `OcrMode` takes every click on the canvas — `edit_central` hands
    /// input to `ocr_input` instead of `annotation_input` — so collapsing the
    /// section used to leave the blue word overlay up, every drawing tool dead,
    /// and the only "Off" button hidden inside the section that had just been
    /// closed. The tool pill and the status line went on describing annotation
    /// the whole time, which is what made it read as the editor having frozen
    /// rather than as a mode being stuck on.
    ///
    /// Called again when recognition finishes, so a section opened while the
    /// worker was still reading arms itself once there is something to arm on.
    pub(crate) fn sync_ocr_mode(&mut self) {
        // Armed only with words to act on: an empty word list would deaden every
        // click on the canvas and give nothing back, which is the same trap in
        // the other direction.
        self.ocr_mode = if self.open_section == Some(Section::Ocr) && !self.ocr_words.is_empty() {
            // Opening the section is the request to see the words; landing on
            // `Off` would show nothing and offer no clue that a mode is needed.
            OcrMode::SelectText
        } else {
            OcrMode::Off
        };
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
            theme::segmented(
                ui,
                "pick-mode",
                &mut self.pick_mode,
                &[
                    (PickMode::Region, t("Region")),
                    (PickMode::Window, t("Window")),
                ],
            );
            theme::hint(ui, "Space");
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
        let mut pin: Option<std::path::PathBuf> = None;
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
                        let thumb = ui
                            .add(egui::Button::image(image).corner_radius(4))
                            .on_hover_text(t("Click to open. Right-click to pin."));
                        if thumb.clicked() {
                            open = Some(self.history[i].image.clone());
                        }
                        // A secondary action on a thumbnail, rather than a second
                        // button per row: the strip is 72px tall and a row of
                        // paired buttons would halve the number of shots visible.
                        thumb.context_menu(|ui| {
                            if ui.button(t("Pin to screen")).clicked() {
                                pin = Some(self.history[i].image.clone());
                                ui.close();
                            }
                        });
                    }
                });
            });

        if let Some(path) = open {
            self.open_image(&path);
        }
        if let Some(path) = pin {
            self.pin_shot(Some(&path));
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
        if theme::ghost_button(ui, t("Reset to defaults")).clicked() {
            self.style = Style::default();
        }
        ui.add_space(16.0);
    }

    fn layout_section(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            let s = &mut self.style;
            theme::slider_row(ui, t("Padding"), &mut s.padding, 0..=400, "");
            theme::slider_row(ui, t("Radius"), &mut s.radius, 0..=120, "");
            theme::slider_row(ui, t("Shadow"), &mut s.shadow, 0..=100, "");
            theme::rule(ui);

            // The inset carries a colour, so its row spends the number box's
            // width on the swatch and on the way back to automatic instead.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::ROW_GAP;
                theme::row_label(ui, t("Inset"));
                // Keep width back for what comes after the rail, and only for
                // what is actually going to be drawn: the `auto` button appears
                // exactly when the colour is *not* automatic, and reserving for
                // it the other way round overflows the card by its own width the
                // moment the swatch is touched.
                //
                // Its width is measured, not guessed. "auto" is 34pt and
                // "tự động" is half again as wide, so a constant fits in English
                // and overflows the card in Vietnamese — where the overflow does
                // not merely clip, it widens the scroll area and cuts off every
                // row below.
                const W_SWATCH: f32 = 22.0;
                let mut keep = W_SWATCH + theme::ROW_GAP;
                if !s.inset_auto_color {
                    keep += theme::text_button_width(ui, t("auto"), 11.0) + theme::ROW_GAP;
                }
                let rail_w = (ui.available_width() - keep).max(48.0);
                theme::rail(ui, rail_w, &mut s.inset, &(0..=80));
                // The swatch shows what is actually being drawn, and touching it
                // is how you take the colour back off auto.
                let mut shown = match (s.inset_auto_color, self.detected_inset) {
                    (true, Some(c)) => c,
                    _ => s.inset_color,
                };
                let before = shown;
                color_button(ui, &mut shown, 22.0);
                if shown != before {
                    s.inset_color = shown;
                    s.inset_auto_color = false;
                }
                if !s.inset_auto_color
                    && ui
                        .add(egui::Button::new(
                            egui::RichText::new(t("auto")).size(11.0).color(theme::ACCENT),
                        ))
                        .clicked()
                {
                    s.inset_auto_color = true;
                }
            });
            if s.inset_auto_color {
                theme::hint(
                    ui,
                    match self.detected_inset {
                        Some(_) => t("(background colour detected)"),
                        None => t("(no background colour found)"),
                    },
                );
            }

            let s = &mut self.style;
            // Only means anything while the colour is being detected, so it is
            // only offered then — and only once there is an inset to drop.
            if s.inset_auto_color && s.inset > 0 {
                theme::checkbox(
                    ui,
                    &mut s.inset_only_if_detected,
                    t("Only when a colour is found"),
                )
                .on_hover_text(t(
                    "Leave the inset off rather than falling back to a plain colour.",
                ));
            }
            theme::checkbox(ui, &mut s.balance, t("Balance"))
                .on_hover_text(t("Trim uniform edges so the subject sits centred"));
        });
    }

    /// Aspect ratios on a track, social sizes on a grid.
    ///
    /// A wrapped row of thirteen identical chips wraps unevenly at this width
    /// and, worse, never says what "Reddit" means in pixels — which is the only
    /// thing anyone picking it wants to know.
    fn ratio_section(&mut self, ui: &mut egui::Ui) {
        // `RATIO_PRESETS` is the source of truth for both lists: an entry added
        // there has to appear here without this function being touched.
        let aspects: Vec<(Ratio, &str)> = RATIO_PRESETS
            .iter()
            .filter(|p| !matches!(p.ratio, Ratio::Size(..)))
            .map(|p| (p.ratio, p.name))
            .collect();
        theme::segmented(ui, "ratio-aspect", &mut self.style.ratio, &aspects);

        ui.add_space(6.0);
        theme::section(ui, t("Social sizes"));
        ui.add_space(3.0);

        const COLS: usize = 2;
        const GAP: f32 = 6.0;
        let chip_w = ((ui.available_width() - GAP * (COLS - 1) as f32) / COLS as f32).floor();
        let socials: Vec<&crate::settings::RatioPreset> = RATIO_PRESETS
            .iter()
            .filter(|p| matches!(p.ratio, Ratio::Size(..)))
            .collect();
        egui::Grid::new("ratio-socials")
            .num_columns(COLS)
            .spacing([GAP, GAP])
            .show(ui, |ui| {
                for (i, preset) in socials.iter().enumerate() {
                    let Ratio::Size(w, h) = preset.ratio else {
                        continue;
                    };
                    let on = self.style.ratio == preset.ratio;
                    if theme::chip(ui, chip_w, on, preset.name, &format!("{w}×{h}")).clicked() {
                        self.style.ratio = preset.ratio;
                    }
                    if (i + 1) % COLS == 0 {
                        ui.end_row();
                    }
                }
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            theme::row_label(ui, t("Custom"));
            let (mut w, mut h) = self.style.custom_size;
            let changed = theme::field(ui, theme::RADIUS_SMALL, |ui| {
                let a = ui.add_sized(
                    egui::vec2(64.0, 24.0),
                    egui::DragValue::new(&mut w).range(64..=8000).speed(4),
                );
                ui.label(egui::RichText::new("×").size(12.0).color(theme::pal().text_dim));
                let b = ui.add_sized(
                    egui::vec2(64.0, 24.0),
                    egui::DragValue::new(&mut h).range(64..=8000).speed(4),
                );
                ui.label(egui::RichText::new("px").size(12.0).color(theme::pal().text_dim));
                a.changed() || b.changed()
            });
            // Typing a size is how the custom ratio gets chosen; there is no
            // separate switch, because a size nobody selected does nothing.
            if changed {
                self.style.custom_size = (w, h);
                self.style.ratio = Ratio::Size(w, h);
            }
        });

        ui.add_space(3.0);
        theme::hint(
            ui,
            match self.style.ratio {
                Ratio::Auto => t("Auto grows the canvas to fit the shot plus its padding."),
                _ => t("The shot is fitted inside the pinned canvas."),
            },
        );
    }


    /// Watermark controls.
    ///
    /// The set of options follows what watermarking tools converge on: a
    /// wordmark or a logo, anchored on a nine-square grid or tiled across the
    /// whole picture, with size, angle and opacity on their own dials.
    fn watermark_section(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            theme::checkbox(ui, &mut self.style.watermark, t("Enable watermark"));
            if !self.style.watermark {
                return;
            }

            // --- what to stamp ---------------------------------------------
            let has_logo = self.style.watermark_image.is_some();
            let mut logo = has_logo;
            if theme::segmented(
                ui,
                "wm-kind",
                &mut logo,
                &[(false, t("Text")), (true, t("Logo image"))],
            ) {
                if logo {
                    // Asked for a logo with none chosen: the dialog is the whole
                    // point of the choice, so it opens here rather than later.
                    match export::open_image_dialog() {
                        Some(p) => self.style.watermark_image = Some(p),
                        None => self.style.watermark_image = None,
                    }
                } else {
                    self.style.watermark_image = None;
                }
            }

            match self.style.watermark_image.clone() {
                Some(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        theme::hint(ui, name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t("Change…")).clicked()
                                && let Some(p) = export::open_image_dialog()
                            {
                                self.style.watermark_image = Some(p);
                            }
                        });
                    });
                }
                None => {
                    const W_COPY: f32 = 30.0;
                    let mut bar = theme::Bar::new(ui, "wm-text", theme::H_BAR_CARD);
                    {
                        let mut cell = bar.rest("wm-text", W_COPY);
                        cell.add(
                            egui::TextEdit::singleline(&mut self.style.watermark_text)
                                .hint_text("Enter text")
                                .frame(theme::welded_field())
                                .vertical_align(egui::Align::Center)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    {
                        let mut cell = bar.cell("wm-copy", W_COPY);
                        let size = cell.available_size();
                        if cell
                            .add(egui::Button::new("©").min_size(size))
                            .on_hover_text("Chèn ký hiệu bản quyền")
                            .clicked()
                        {
                            self.style.watermark_text.insert(0, '©');
                            self.style.watermark_text.insert(1, ' ');
                        }
                    }

                    let styles: Vec<(crate::settings::WatermarkStyle, &str)> =
                        crate::settings::WatermarkStyle::ALL
                            .into_iter()
                            .map(|s| (s, s.label()))
                            .collect();
                    theme::segmented(ui, "wm-style", &mut self.style.watermark_style, &styles);
                    ui.horizontal(|ui| {
                        theme::row_label(ui, t("Text colour"));
                        color_button(ui, &mut self.style.watermark_color, 22.0);
                    });
                }
            }

            // Nothing chooses *where* any more: the mark sits under the shot,
            // sharing its right edge. There is no anchor to pick and nothing to
            // tile, so the nine-square grid and the tiling switch are gone.

            // --- how it looks -----------------------------------------------
            // Stored as a multiplier, a byte and a float; read as whole
            // percentages and whole degrees, which is what the box has to show —
            // a rail handing 137.4193% to a number box is unreadable.
            let mut size_pct = (self.style.watermark_size * 100.0).round() as u32;
            if theme::slider_row(ui, t("Size"), &mut size_pct, 40..=400, "%") {
                self.style.watermark_size = size_pct as f32 / 100.0;
            }
            let mut opacity_pct =
                (f32::from(self.style.watermark_opacity) / 255.0 * 100.0).round() as u32;
            if theme::slider_row(ui, t("Opacity"), &mut opacity_pct, 5..=100, "%") {
                self.style.watermark_opacity = (opacity_pct as f32 / 100.0 * 255.0).round() as u8;
            }
            let mut angle = self.style.watermark_angle.round() as i32;
            if theme::slider_row(ui, t("Angle"), &mut angle, -90..=90, "°") {
                self.style.watermark_angle = angle as f32;
            }
            theme::hint(ui, t("Sits under the shot, sharing its right edge."));
        });
    }

    /// Presets on one row: pick, name, save — the design welds them into one
    /// bar because they are one thought, not three sections.
    fn preset_row(&mut self, ui: &mut egui::Ui) {
        const W_SAVE: f32 = 46.0;
        const W_DELETE: f32 = 30.0;

        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;
        let matching = self.presets.iter().position(|p| p.style == self.style);

        // Picking a preset and naming one are the same size of decision, so the
        // combo and the field get the same width rather than a flexible one and
        // a fixed one.
        let fixed = W_SAVE + if matching.is_some() { W_DELETE } else { 0.0 };
        let half = ((ui.available_width() - fixed) / 2.0).floor().max(48.0);

        let mut bar = theme::Bar::new(ui, "presets", theme::H_BAR);
        {
            let mut cell = bar.rest("preset-pick", fixed + half);
            let selected = matching
                .map(|i| self.presets[i].name.clone())
                .unwrap_or_else(|| "—".to_string());
            let width = cell.available_width();
            egui::ComboBox::from_id_salt("preset")
                .selected_text(selected)
                .width(width)
                .show_ui(&mut cell, |ui| {
                    if self.presets.is_empty() {
                        ui.label(egui::RichText::new(t("(no presets yet)")).weak());
                    }
                    for (i, p) in self.presets.iter().enumerate() {
                        if ui.selectable_label(false, &p.name).clicked() {
                            apply = Some(i);
                        }
                    }
                });
        }
        {
            let mut cell = bar.cell("preset-name", half);
            let height = cell.available_height();
            cell.add_sized(
                egui::vec2(half, height),
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text(t("Preset name"))
                    .frame(theme::welded_field())
                    .vertical_align(egui::Align::Center),
            );
        }
        {
            let mut cell = bar.cell("preset-save", W_SAVE);
            let size = cell.available_size();
            if cell.add(egui::Button::new(t("Save")).min_size(size)).clicked() {
                self.save_preset();
            }
        }
        if matching.is_some() {
            let mut cell = bar.cell("preset-delete", W_DELETE);
            let size = cell.available_size();
            if cell
                .add(egui::Button::new("🗑").min_size(size))
                .on_hover_text(t("Delete the preset that matches"))
                .clicked()
            {
                delete = matching;
            }
        }

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
        // No button padding to subtract any more: a swatch is the gradient and
        // nothing else — no frame, no inset — so the whole cell is image. The
        // gradients are what the grid is for, and a border round each one turned
        // twenty-three of them into twenty-three boxes.
        let side = grid_cell_side(ui.available_width(), COLS, GAP, 0.0);

        egui::Grid::new("bg-grid")
            .num_columns(COLS)
            .spacing([GAP, GAP])
            .show(ui, |ui| {
                for (i, (sw, tex)) in swatches.iter().enumerate() {
                    let selected = self.style.background == sw.background();
                    let resp = theme::swatch(ui, tex, side, selected).on_hover_text(sw.label());
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

        // What was chosen, in words: nineteen gradients look alike at 40px, and
        // Auto and Desktop are not fixed colours at all.
        let current = swatch_order()
            .into_iter()
            .find(|sw| sw.background() == self.style.background);
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            theme::hint(ui, current.map(Swatch::label).unwrap_or_default());
            let note = match current {
                Some(Swatch::Auto) => t("built from the shot"),
                Some(Swatch::Desktop) => t("current wallpaper"),
                _ => "",
            };
            if !note.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::hint(ui, note);
                });
            }
        });

        if self.style.background == Background::Custom {
            ui.add_space(6.0);
            theme::card(ui, |ui| {
                let c = &mut self.style.custom_bg;
                theme::segmented(
                    ui,
                    "custom-bg",
                    &mut c.kind,
                    &[
                        (CustomKind::Solid, t("Solid colour")),
                        (CustomKind::Linear, "Gradient"),
                        (CustomKind::Image, t("Image")),
                    ],
                );
                match c.kind {
                    CustomKind::Solid => {
                        ui.horizontal(|ui| {
                            theme::row_label(ui, t("Colour"));
                            color_button(ui, &mut c.color_a, 22.0);
                        });
                    }
                    CustomKind::Linear => {
                        ui.horizontal(|ui| {
                            theme::row_label(ui, t("Colours"));
                            color_button(ui, &mut c.color_a, 22.0);
                            color_button(ui, &mut c.color_b, 22.0);
                        });
                        let mut angle = c.angle.round() as i32;
                        if theme::slider_row(ui, t("Corner"), &mut angle, 0..=360, "°") {
                            c.angle = angle as f32;
                        }
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
            });
        }
    }

    fn export_section(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            let s = &mut self.prefs;
            let formats: Vec<(ExportFormat, &str)> =
                ExportFormat::ALL.into_iter().map(|f| (f, f.label())).collect();
            theme::segmented(ui, "export-format", &mut s.format, &formats);

            match s.format {
                ExportFormat::Jpeg => {
                    theme::slider_row(ui, t("Quality"), &mut s.jpeg_quality, 40..=100, "");
                    theme::hint(
                        ui,
                        t("JPEG has no alpha channel — a transparent background becomes white."),
                    );
                }
                ExportFormat::Png => {
                    theme::checkbox(
                        ui,
                        &mut s.png_max_compression,
                        t("Maximum compression (slower)"),
                    );
                }
                ExportFormat::Webp => {
                    theme::hint(ui, t("WebP is lossless here, so there is no quality slider."));
                }
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::ROW_GAP;
                theme::row_label(ui, t("Filename"));
                theme::field(ui, theme::RADIUS_SMALL, |ui| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), theme::H_BAR_CARD),
                        egui::TextEdit::singleline(&mut s.filename_template)
                            .hint_text("shotr-{date}-{time}")
                            .vertical_align(egui::Align::Center)
                            .margin(egui::Margin::symmetric(theme::FIELD_PAD, 0)),
                    );
                });
            });
            theme::hint(
                ui,
                format!(
                    "→ {}.{}",
                    crate::export::expand_template(&s.filename_template, 1_700_000_000),
                    s.format.extension()
                ),
            );
        });
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

        theme::card(ui, |ui| {
            if self.ocr_words.is_empty() {
                theme::hint(ui, t("No text found."));
            }
            // Say which engine read the image: the two differ on Vietnamese badly
            // enough that a user seeing mangled diacritics needs to know why.
            match crate::ocr::tesseract::best_langs() {
                Some(langs) => theme::hint(
                    ui,
                    tf("Read with tesseract ({langs})", &[("langs", &langs)]),
                ),
                None => theme::hint(ui, t("Read with ocrs — no Vietnamese diacritics.")),
            }

            let found = self.active_finding_count();
            theme::checkbox(
                ui,
                &mut self.prefs.redact,
                &tf(
                    "Redact sensitive data ({found} found)",
                    &[("found", &found.to_string())],
                ),
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
                    ui.spacing_mut().item_spacing.y = theme::ROW_GAP;
                    let s = &mut self.prefs;
                    for (flag, kind, n) in [
                        (&mut s.redact_email, Secret::Email, counts[0]),
                        (&mut s.redact_card, Secret::CreditCard, counts[1]),
                        (&mut s.redact_ip, Secret::IpAddress, counts[2]),
                        (&mut s.redact_key, Secret::ApiKey, counts[3]),
                        (&mut s.redact_phone, Secret::Phone, counts[4]),
                    ] {
                        theme::checkbox(ui, flag, &format!("{} ({n})", kind.label()));
                    }
                });

                theme::segmented(
                    ui,
                    "redact-style",
                    &mut self.style.redact_style,
                    &[
                        (RedactStyle::Solid, t("Solid")),
                        (RedactStyle::Blur, t("Blur")),
                    ],
                );
                match self.style.redact_style {
                    RedactStyle::Solid => {
                        ui.horizontal(|ui| {
                            theme::row_label(ui, t("Colour"));
                            color_button(ui, &mut self.style.redact_color, 22.0);
                        });
                    }
                    RedactStyle::Blur => {
                        let mut blur = self.style.redact_blur.round() as i32;
                        if theme::slider_row(ui, t("Blur amount"), &mut blur, 2..=60, "") {
                            self.style.redact_blur = blur as f32;
                        }
                    }
                }
            }

            theme::segmented(
                ui,
                "ocr-mode",
                &mut self.ocr_mode,
                &[
                    (OcrMode::Off, t("Off")),
                    (OcrMode::SelectText, t("Select text")),
                    (OcrMode::ManualRedact, t("Cover by hand")),
                ],
            );
        });

        ui.add_space(6.0);
        match self.ocr_mode {
            OcrMode::SelectText => {
                theme::hint(ui, t("Drag on the image to select text."));
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
                theme::hint(ui, t("Click a word to hide or reveal it."));
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

/// `side` is 22 for a button sharing a row with other controls and 24 for one
/// standing on its own — egui takes the size from `interact_size`, which is
/// shared, so it has to be passed rather than set.
pub(super) fn color_button(ui: &mut egui::Ui, color: &mut Rgba8, side: f32) -> egui::Response {
    let mut c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let resp = theme::color_swatch(ui, &mut c, side);
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
    use super::{grid_cell_side, theme};

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
        // everything the scrollbar reserves. Taken from the theme rather than
        // spelled out, or retuning the bar silently invalidates this.
        let bar = theme::SCROLL_BAR + theme::SCROLL_INNER + theme::SCROLL_OUTER;
        let avail = 336.0 - 24.0 - bar;
        let side = grid_cell_side(avail, 5, 6.0, 7.0 * 2.0);
        assert!(side >= 30.0, "swatches would be too small to read: {side}");
        let used = (side + 14.0) * 5.0 + 6.0 * 4.0;
        assert!(used <= avail + 0.5, "still overflows: {used} > {avail}");
    }
}
