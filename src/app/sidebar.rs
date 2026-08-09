//! The right-hand control panel and the bottom action bar.

use crate::i18n::{t, tf};

use eframe::egui;
use image::{Rgba, RgbaImage};

use super::ocr_job::OcrState;
use super::{
    Mode, OcrMode, PickMode, SWATCH_PX, ShotrApp, Swatch, Zoom, swatch_order, theme,
    to_color_image,
};
use crate::annotate::Tool;
use crate::export;
use crate::ocr::detect::Secret;
use crate::render::background::{BG_PRESETS, auto_preset, image_cover, linear, mesh};
use crate::settings::{
    Background, CustomKind, ExportFormat, RATIO_PRESETS, Ratio, RedactStyle, Rgba8, Style,
};
use crate::wallpaper;

impl ShotrApp {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.heading("shotr");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("Capture again")).clicked() {
                    self.start_capture(self.source);
                }
            });
        });
        theme::rule(ui);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.mode {
                Mode::Select => self.select_sidebar(ui),
                Mode::Edit => self.edit_sidebar(ui),
            });
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
        // Back to the windowed Select screen. On Linux and Windows that is the
        // picker over the shot just taken; the tray carries History, "Open
        // file…" and "From clipboard" on every platform now.
        if ui.button(t("Back to selection")).clicked() {
            self.mode = Mode::Select;
            self.sel_start = None;
            self.sel_rect = None;
            self.crop_px = None;
            self.status.clear();
        }
        theme::rule(ui);

        self.tool_section(ui);
        theme::rule(ui);

        self.preset_row(ui);
        theme::rule(ui);

        theme::card(ui, t("Layout"), |ui| {
        let s = &mut self.style;
        theme::slider_label(ui, "Padding", s.padding);
        ui.add(egui::Slider::new(&mut s.padding, 0..=400).show_value(false));

        ui.horizontal(|ui| {
            ui.label("Inset");
            if s.inset_auto_color {
                match self.detected_inset {
                    Some(_) => ui.label(egui::RichText::new(t("(background colour detected)")).weak().small()),
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
        ui.checkbox(&mut s.balance, "Balance")
            .on_hover_text("Trim uniform edges so the subject sits centred");

        theme::slider_label(ui, t("Border radius"), s.radius);
        ui.add(egui::Slider::new(&mut s.radius, 0..=120).show_value(false));
        theme::slider_label(ui, t("Add a shadow"), s.shadow);
        ui.add(egui::Slider::new(&mut s.shadow, 0..=100).show_value(false));
        });

        theme::card(ui, t("Background"), |ui| self.background_grid(ui));

        theme::card(ui, t("Ratio / Size"), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_custom = self.style.ratio
                == Ratio::Size(self.style.custom_size.0, self.style.custom_size.1);
            if ui.selectable_label(is_custom, "Custom…").clicked() {
                self.show_custom_size = !self.show_custom_size;
                if self.show_custom_size {
                    let (w, h) = self.style.custom_size;
                    self.style.ratio = Ratio::Size(w, h);
                }
            }
        });
        self.ratio_chips(ui);
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
        });

        self.ocr_section(ui);

        theme::card(ui, t("Watermark"), |ui| self.watermark_section(ui));

        theme::card(ui, t("Export"), |ui| self.export_section(ui));
        ui.add_space(10.0);

        if ui.button(t("Reset to defaults")).clicked() {
            self.style = Style::default();
        }
        ui.add_space(10.0);
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

    fn tool_section(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, t("Tools"));
        ui.horizontal_wrapped(|ui| {
            for tool in std::iter::once(Tool::Select).chain(Tool::DRAWABLE) {
                if super::icons::tool_button(ui, tool, self.tool == tool).clicked() {
                    self.tool = tool;
                }
            }
        });

        ui.add_space(4.0);

        // Remember what the controls said before drawing them, so a change can
        // be pushed onto the selected layer below.
        let before = (
            self.annot_color,
            self.annot_stroke,
            self.annot_font_size,
            self.annot_blur,
            self.annot_paint_alpha,
        );

        ui.horizontal(|ui| {
            ui.label(t("Colour"));
            color_button(ui, &mut self.annot_color);
        });

        if self.tool.uses_stroke() {
            theme::slider_label(ui, t("Stroke"), self.annot_stroke.round() as u32);
            ui.add(egui::Slider::new(&mut self.annot_stroke, 1.0..=40.0).show_value(false));
        }
        match self.tool {
            Tool::Text => {
                theme::slider_label(ui, t("Font size"), self.annot_font_size.round() as u32);
                ui.add(egui::Slider::new(&mut self.annot_font_size, 10.0..=160.0).show_value(false));
            }
            Tool::Blur => {
                theme::slider_label(ui, t("Blur amount"), self.annot_blur.round() as u32);
                ui.add(egui::Slider::new(&mut self.annot_blur, 2.0..=60.0).show_value(false));
            }
            Tool::Highlight => {
                // One dial from highlighter to solid cover, shown as a percent
                // because "180 out of 255" means nothing to anyone.
                let mut pct = (self.annot_paint_alpha as f32 / 255.0 * 100.0).round() as u8;
                theme::slider_label(ui, t("Paint opacity"), format!("{pct}%"));
                if ui
                    .add(egui::Slider::new(&mut pct, 5..=100).show_value(false))
                    .changed()
                {
                    self.annot_paint_alpha = (pct as f32 / 100.0 * 255.0).round() as u8;
                }
                ui.label(
                    egui::RichText::new(t("100% covers completely; lower is translucent, like a highlighter."))
                        .weak()
                        .small(),
                );
            }
            _ => {}
        }

        // Editing a shape you have already drawn is the whole reason these
        // sliders feel broken otherwise: they used to set the defaults for the
        // *next* shape only, so dragging them after drawing did nothing visible.
        let after = (
            self.annot_color,
            self.annot_stroke,
            self.annot_font_size,
            self.annot_blur,
            self.annot_paint_alpha,
        );
        if after != before
            && let Some(i) = self.selected_layer
            && let Some(kind) = self.layers.get(i).map(|l| l.kind)
        {
            // Work out the colour before taking the mutable borrow.
            let ink = self.ink(kind);
            let Some(layer) = self.layers.get_mut(i) else {
                return;
            };
            layer.color = ink;
            layer.stroke = self.annot_stroke;
            layer.font_size = self.annot_font_size;
            layer.blur = self.annot_blur;
            self.dirty = true;
        }

        if self.tool == Tool::Text {
            ui.label(
                egui::RichText::new(t("Click the image and type. Enter to finish, Esc to cancel. Click existing text to edit it."))
                    .weak()
                    .small(),
            );
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.undo.can_undo(), egui::Button::new("↶ Undo"))
                .clicked()
            {
                self.undo_annotation();
            }
            if ui
                .add_enabled(self.undo.can_redo(), egui::Button::new("↷ Redo"))
                .clicked()
            {
                self.redo_annotation();
            }
            let has_sel = self.selected_layer.is_some();
            if ui
                .add_enabled(has_sel, egui::Button::new(t("Delete layer")))
                .clicked()
            {
                self.delete_selected_layer();
            }
        });
        if !self.layers.is_empty() && ui.button(t("Clear all annotations")).clicked() {
            self.undo.push(&self.layers);
            self.layers.clear();
            self.selected_layer = None;
            self.dirty = true;
        }
    }

    fn preset_row(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, t("Your presets"));
        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;

        ui.horizontal(|ui| {
            let selected = self
                .presets
                .iter()
                .position(|p| p.style == self.style)
                .map(|i| self.presets[i].name.clone())
                .unwrap_or_else(|| "—".to_string());

            egui::ComboBox::from_id_salt("preset")
                .selected_text(selected)
                .width(180.0)
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

            let has_match = self.presets.iter().any(|p| p.style == self.style);
            if ui
                .add_enabled(has_match, egui::Button::new("🗑"))
                .on_hover_text("Xoá preset đang khớp")
                .clicked()
                && let Some(i) = self
                    .presets
                    .iter()
                    .position(|p| p.style == self.style)
            {
                delete = Some(i);
            }
        });

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text("Preset name")
                    .desired_width(180.0),
            );
            if ui.button(t("Save")).clicked() {
                self.save_preset();
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

    fn ocr_section(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, t("Text recognition (OCR)"), |ui| self.ocr_body(ui));
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
                ui.colored_label(egui::Color32::from_rgb(0xff, 0x6b, 0x6b), e);
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

    // ------------------------------------------------------------- bottom bar

    pub(super) fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if self.mode == Mode::Edit {
                if ui.button("Copy  Ctrl+C").clicked() {
                    self.do_copy();
                }
                if ui.button("Save  Ctrl+S").clicked() {
                    self.do_save(None);
                }
                if ui.button("Save As…").clicked()
                    && let Some(p) = export::save_as_dialog(&self.prefs)
                {
                    self.do_save(Some(p));
                }

                let label = match self.zoom {
                    Zoom::Fit => format!("Fit · {}%", self.shown_zoom),
                    Zoom::Percent(p) => format!("{p}%"),
                };
                egui::ComboBox::from_id_salt("zoom")
                    .selected_text(label)
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.zoom == Zoom::Fit, "Fit").clicked() {
                            self.set_zoom(Zoom::Fit);
                        }
                        for p in Zoom::STEPS {
                            let on = self.zoom == Zoom::Percent(p);
                            if ui.selectable_label(on, format!("{p}%")).clicked() {
                                self.set_zoom(Zoom::Percent(p));
                            }
                        }
                    });

                egui::ComboBox::from_id_salt("more")
                    .selected_text("More…")
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        if ui.button(t("Open image folder")).clicked() {
                            self.open_output_dir();
                            ui.close();
                        }
                        if ui.button(t("Copy the text in the image")).clicked() {
                            self.copy_text(false);
                            ui.close();
                        }
                        if ui.button(t("Take a new shot")).clicked() {
                            self.start_capture(self.source);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(t("Reset to defaults")).clicked() {
                            self.style = Style::default();
                            ui.close();
                        }
                        ui.separator();
                        ui.label(
                            egui::RichText::new(concat!("shotr ", env!("CARGO_PKG_VERSION")))
                                .weak()
                                .small(),
                        );
                    });
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.mode == Mode::Edit {
                    ui.label(
                        egui::RichText::new(t("Double-click the image to copy and close"))
                            .weak()
                            .small(),
                    );
                }
            });
        });
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).small());
        }
        ui.add_space(2.0);
    }
}

pub(super) fn color_button(ui: &mut egui::Ui, color: &mut Rgba8) {
    let mut c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    if ui.color_edit_button_srgba(&mut c).changed() {
        *color = c.to_srgba_unmultiplied();
    }
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
        // The shipped panel, minus the frame margins and everything the
        // scrollbar reserves: bar width + inner gap + outer gap.
        let avail = 336.0 - 16.0 - (8.0 + 10.0 + 4.0);
        let side = grid_cell_side(avail, 5, 6.0, 7.0 * 2.0);
        assert!(side >= 30.0, "swatches would be too small to read: {side}");
        let used = (side + 14.0) * 5.0 + 6.0 * 4.0;
        assert!(used <= avail + 0.5, "still overflows: {used} > {avail}");
    }
}
