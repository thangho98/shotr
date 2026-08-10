//! The central canvas: region/window picking in Select mode, and the beautified
//! preview in Edit mode.

use crate::i18n::t;

use eframe::egui;

use super::{OcrMode, PickMode, ShotrApp, Zoom};
use crate::annotate::{Layer, Tool};
use crate::settings::Background;

impl ShotrApp {
    pub(super) fn select_central(&mut self, ui: &mut egui::Ui) {
        // The hub has no capture behind it — the placeholder texture would only
        // read as a broken screenshot.
        if self.hub {
            return;
        }
        let Some(raw_tex) = self.raw_texture.clone() else {
            return;
        };
        let tsize = raw_tex.size_vec2();
        if tsize.x < 1.0 || tsize.y < 1.0 {
            return;
        }

        let avail = ui.available_size();
        let sense = match self.pick_mode {
            PickMode::Region => egui::Sense::drag(),
            PickMode::Window => egui::Sense::click(),
        };
        let (canvas, resp) = ui.allocate_exact_size(avail, sense);
        let scale = (canvas.width() / tsize.x)
            .min(canvas.height() / tsize.y)
            .min(1.0);
        let img_rect = egui::Rect::from_center_size(canvas.center(), tsize * scale);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

        ui.painter()
            .image(raw_tex.id(), img_rect, uv, egui::Color32::WHITE);

        match self.pick_mode {
            PickMode::Region => self.region_pick(ui, &resp, img_rect, &raw_tex),
            PickMode::Window => self.window_pick(ui, &resp, img_rect),
        }

        if self.picking_fullscreen {
            self.paint_picker_hint(ui.painter(), canvas);
        }
    }

    /// One line of instructions, floated over the shot. The picker has no
    /// chrome of its own — that is the point of it.
    fn paint_picker_hint(&self, painter: &egui::Painter, canvas: egui::Rect) {
        // Only offer window mode when there are windows to offer. xcap returns
        // an empty list on this compositor, and advertising a key that then
        // does nothing is worse than not mentioning it.
        let text = match (self.pick_mode, self.windows.is_empty()) {
            (PickMode::Region, true) => t("Drag to select   ·   Enter: whole screen   ·   Esc: cancel"),
            (PickMode::Region, false) => {
                t("Drag to select   ·   Space: pick a window   ·   Enter: whole screen   ·   Esc: cancel")
            }
            (PickMode::Window, _) => {
                t("Click a window in the list   ·   Space: back to region   ·   Esc: cancel")
            }
        };
        let font = egui::FontId::proportional(15.0);
        let galley = painter.layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE);
        let pad = egui::vec2(16.0, 10.0);
        let size = galley.size() + pad * 2.0;
        let pos = egui::pos2(
            canvas.center().x - size.x / 2.0,
            canvas.max.y - size.y - 48.0,
        );
        let rect = egui::Rect::from_min_size(pos, size);
        painter.rect_filled(rect, 8.0, egui::Color32::from_black_alpha(190));
        painter.galley(pos + pad, galley, egui::Color32::WHITE);
    }

    fn region_pick(
        &mut self,
        ui: &mut egui::Ui,
        resp: &egui::Response,
        img_rect: egui::Rect,
        raw_tex: &egui::TextureHandle,
    ) {
        if resp.drag_started() {
            self.sel_start = resp.interact_pointer_pos().map(|p| clamp_pos(p, img_rect));
            self.sel_rect = None;
            self.crop_px = None;
        }
        if resp.dragged()
            && let (Some(a), Some(p)) = (self.sel_start, resp.interact_pointer_pos())
        {
            let r = egui::Rect::from_two_pos(a, clamp_pos(p, img_rect));
            self.sel_rect = Some(r);
            self.crop_px = rect_to_full_px(
                r,
                img_rect,
                self.capture_full.width(),
                self.capture_full.height(),
            );
        }
        // Releasing a selection drops straight into Edit; that is the fast path.
        if resp.drag_stopped() && self.crop_px.is_some() {
            self.finish_selection(true);
            return;
        }

        if let Some(sel) = self.sel_rect {
            let p = ui.painter();
            p.rect_filled(img_rect, 0.0, egui::Color32::from_black_alpha(110));
            let su = egui::Rect::from_min_max(
                egui::pos2(
                    (sel.min.x - img_rect.min.x) / img_rect.width(),
                    (sel.min.y - img_rect.min.y) / img_rect.height(),
                ),
                egui::pos2(
                    (sel.max.x - img_rect.min.x) / img_rect.width(),
                    (sel.max.y - img_rect.min.y) / img_rect.height(),
                ),
            );
            p.image(raw_tex.id(), sel, su, egui::Color32::WHITE);
            draw_border(p, sel, accent_stroke());
        }
    }

    /// Window picking is a list, not a hover-over-the-desktop overlay.
    ///
    /// The protocol that makes per-window capture possible here,
    /// `ext_foreign_toplevel_list_v1`, deliberately publishes no geometry: a
    /// client may *name* a window and *copy* it, never locate it. So there is
    /// no rectangle to hover. What you get in exchange is better than a crop —
    /// the pixels come from the window's own buffer, so a window behind another
    /// still captures whole.
    fn window_pick(&mut self, ui: &mut egui::Ui, resp: &egui::Response, img_rect: egui::Rect) {
        let painter = ui.painter().clone();
        painter.rect_filled(img_rect, 0.0, egui::Color32::from_black_alpha(170));

        if self.windows.is_empty() {
            painter.text(
                img_rect.center(),
                egui::Align2::CENTER_CENTER,
                t("No window to capture"),
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(200),
            );
            return;
        }

        const ROW: f32 = 34.0;
        let width = (img_rect.width() * 0.46).clamp(320.0, 760.0);
        let height = ROW * self.windows.len() as f32 + 16.0;
        let panel = egui::Rect::from_center_size(img_rect.center(), egui::vec2(width, height));
        painter.rect_filled(panel, 10.0, egui::Color32::from_rgb(0x1b, 0x1d, 0x22));
        draw_border(&painter, panel, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60)));

        let hover = resp.hover_pos();
        let mut chosen = None;
        self.hover_window = None;
        for (i, window) in self.windows.iter().enumerate() {
            let row = egui::Rect::from_min_size(
                panel.min + egui::vec2(8.0, 8.0 + ROW * i as f32),
                egui::vec2(width - 16.0, ROW),
            );
            let hot = hover.is_some_and(|h| row.contains(h));
            if hot {
                self.hover_window = Some(i);
                painter.rect_filled(row, 6.0, accent_stroke().color.gamma_multiply(0.35));
                if resp.clicked() {
                    chosen = Some(i);
                }
            }
            painter.text(
                row.left_center() + egui::vec2(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                window.label(),
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(if hot { 245 } else { 205 }),
            );
        }

        if let Some(i) = chosen {
            self.capture_window(i);
        }
    }

    pub(super) fn edit_central(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(tex) = self.texture.clone() else {
            return;
        };
        let tsize = tex.size_vec2();
        if tsize.x < 1.0 || tsize.y < 1.0 {
            return;
        }
        let avail = ui.available_size();
        let (canvas, resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());

        // While a label is being typed the canvas owns the keyboard. Otherwise
        // a stray earlier click in the watermark or preset field leaves that
        // box focused, and it eats the keystrokes meant for the image.
        if self.typing_text() {
            resp.request_focus();
        }

        let fit = (avail.x / tsize.x).min(avail.y / tsize.y).min(1.0);
        let scale = match self.zoom {
            Zoom::Fit => fit,
            // The preview texture is already downscaled; undo that so 100% means
            // 100% of the exported image, not of the preview bitmap.
            Zoom::Percent(p) => (p as f32 / 100.0) * self.preview_scale,
        };
        // What "Fit" currently works out to, so a wheel notch from Fit carries
        // on from what is on screen instead of jumping to 100%.
        self.shown_zoom = ((fit / self.preview_scale.max(f32::EPSILON)) * 100.0).round() as u32;
        self.shown_zoom = self.shown_zoom.clamp(Zoom::MIN, Zoom::MAX);

        self.zoom_input(&resp, canvas, tsize * scale);
        let img_rect = egui::Rect::from_center_size(canvas.center() + self.pan, tsize * scale);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

        // A checkerboard makes a transparent background read as transparent.
        if self.style.background == Background::None {
            paint_checker(ui.painter(), img_rect);
        }
        ui.painter()
            .image(tex.id(), img_rect, uv, egui::Color32::WHITE);

        // Screen → texture pixels → original screenshot pixels.
        let display = (img_rect.width() / tsize.x).max(f32::EPSILON);
        let geom = self.preview_geom;
        let preview_scale = self.preview_scale;
        let to_shot = move |p: egui::Pos2| -> [f32; 2] {
            geom.canvas_to_shot(
                (p.x - img_rect.min.x) / display,
                (p.y - img_rect.min.y) / display,
                preview_scale,
            )
        };
        let to_screen = move |s: [f32; 2]| -> egui::Pos2 {
            let c = geom.shot_to_canvas(s[0], s[1], preview_scale);
            egui::pos2(
                img_rect.min.x + c[0] * display,
                img_rect.min.y + c[1] * display,
            )
        };

        if self.ocr_mode == OcrMode::Off {
            self.annotation_input(&resp, &to_shot);
            self.paint_annotation_overlay(ui.painter(), &to_screen);
        } else {
            self.ocr_input(&resp, &to_shot);
            self.paint_ocr_overlay(ui.painter(), &to_screen);
        }

        // Only the Select tool gets the copy-and-close gesture; with a drawing
        // tool active a double click is two shapes, not a shortcut.
        if self.tool == Tool::Select && resp.double_clicked() {
            self.copy_and_close(ctx);
        }
    }


    /// Wheel zoom and drag-to-pan.
    ///
    /// Ctrl+wheel zooms about the pointer — the pixel under the cursor stays put
    /// — which is the difference between zoom that feels aimed and zoom that
    /// makes you hunt for what you were looking at. A bare wheel scrolls, and
    /// the middle button drags, because zooming with no way to pan can only ever
    /// show you the middle of an enlarged image.
    fn zoom_input(&mut self, resp: &egui::Response, canvas: egui::Rect, shown: egui::Vec2) {
        let ctx = resp.ctx.clone();
        let hovering = resp.hovered();
        // egui routes ctrl+wheel into `zoom_delta` and leaves `smooth_scroll_delta`
        // at zero, so reading the scroll delta under a ctrl branch finds nothing.
        // Going through `zoom_delta` also picks up trackpad pinch for free.
        let (zoom_delta, wheel, shift) = ctx.input(|i| {
            (i.zoom_delta(), i.smooth_scroll_delta, i.modifiers.shift)
        });

        if hovering && (zoom_delta - 1.0).abs() > 1e-4 {
            let before = self.effective_scale();
            self.zoom = self.zoom.scaled(self.shown_zoom, zoom_delta);
            let after = self.effective_scale();

            // Keep whatever sat under the pointer under the pointer.
            if let Some(p) = resp.hover_pos()
                && before > 0.0
            {
                let centre = canvas.center() + self.pan;
                let offset = p - centre;
                self.pan -= offset * (after / before - 1.0);
            }
        } else if hovering && wheel != egui::Vec2::ZERO {
            // Plain wheel scrolls the image; shift swaps the axis, as everywhere.
            self.pan += if shift {
                egui::vec2(wheel.y + wheel.x, 0.0)
            } else {
                egui::vec2(wheel.x, wheel.y)
            };
        }

        if resp.dragged_by(egui::PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }

        // Never let the image be dragged entirely out of sight.
        let slack = egui::vec2(
            (shown.x / 2.0 + canvas.width() / 2.0 - 24.0).max(0.0),
            (shown.y / 2.0 + canvas.height() / 2.0 - 24.0).max(0.0),
        );
        self.pan = egui::vec2(
            self.pan.x.clamp(-slack.x, slack.x),
            self.pan.y.clamp(-slack.y, slack.y),
        );
    }

    /// The on-screen scale factor the current zoom setting produces.
    fn effective_scale(&self) -> f32 {
        match self.zoom {
            Zoom::Fit => self.shown_zoom as f32 / 100.0 * self.preview_scale,
            Zoom::Percent(p) => p as f32 / 100.0 * self.preview_scale,
        }
    }

    fn ocr_input(&mut self, resp: &egui::Response, to_shot: &dyn Fn(egui::Pos2) -> [f32; 2]) {
        match self.ocr_mode {
            OcrMode::ManualRedact => {
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let [x, y] = to_shot(p);
                    if let Some(i) = self.word_at(x, y) {
                        match self.manual_redact.iter().position(|w| *w == i) {
                            Some(pos) => {
                                self.manual_redact.remove(pos);
                            }
                            None => self.manual_redact.push(i),
                        }
                        self.dirty = true;
                    }
                }
            }
            OcrMode::SelectText => {
                if resp.drag_started()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let at = to_shot(p);
                    self.ocr_drag = Some((at, at));
                }
                if resp.dragged()
                    && let Some(p) = resp.interact_pointer_pos()
                    && let Some((_, end)) = self.ocr_drag.as_mut()
                {
                    *end = to_shot(p);
                }
                if let Some((a, b)) = self.ocr_drag {
                    let rect = [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[0].max(b[0]),
                        a[1].max(b[1]),
                    ];
                    self.selected_words = self
                        .ocr_words
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| overlaps(w.rect, rect))
                        .map(|(i, _)| i)
                        .collect();
                }
                if resp.drag_stopped() {
                    self.ocr_drag = None;
                }
                if resp.clicked() {
                    self.selected_words.clear();
                }
            }
            OcrMode::Off => {}
        }
    }

    fn word_at(&self, x: f32, y: f32) -> Option<usize> {
        self.ocr_words
            .iter()
            .position(|w| x >= w.rect[0] && x <= w.rect[2] && y >= w.rect[1] && y <= w.rect[3])
    }

    fn paint_ocr_overlay(
        &self,
        painter: &egui::Painter,
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    ) {
        let rect_of = |r: [f32; 4]| {
            egui::Rect::from_two_pos(to_screen([r[0], r[1]]), to_screen([r[2], r[3]]))
        };

        // Every recognised word, faintly, so it is obvious what OCR picked up.
        for word in &self.ocr_words {
            painter.rect_filled(
                rect_of(word.rect),
                2.0,
                egui::Color32::from_rgba_unmultiplied(0x4a, 0x9e, 0xff, 28),
            );
        }
        for &i in &self.selected_words {
            if let Some(word) = self.ocr_words.get(i) {
                painter.rect_filled(
                    rect_of(word.rect),
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(0x4a, 0x9e, 0xff, 110),
                );
            }
        }
        for &i in &self.manual_redact {
            if let Some(word) = self.ocr_words.get(i) {
                draw_border(
                    painter,
                    rect_of(word.rect),
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0xff, 0x6b, 0x6b)),
                );
            }
        }
        if let Some((a, b)) = self.ocr_drag {
            draw_border(
                painter,
                egui::Rect::from_two_pos(to_screen(a), to_screen(b)),
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0x4a, 0x9e, 0xff)),
            );
        }
    }

    fn annotation_input(
        &mut self,
        resp: &egui::Response,
        to_shot: &dyn Fn(egui::Pos2) -> [f32; 2],
    ) {
        match self.tool {
            Tool::Select => self.select_layer_input(resp, to_shot),
            Tool::Text => {
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    // Clicking away commits whatever was being typed; clicking
                    // an existing label picks it back up instead of stacking a
                    // second one on top of it.
                    self.finish_text_edit();
                    let at = to_shot(p);
                    let existing = self
                        .layers
                        .iter()
                        .rposition(|l| l.kind == Tool::Text && l.hit(at[0], at[1]));
                    match existing {
                        Some(i) => {
                            self.undo.push(&self.layers);
                            let layer = self.layers.remove(i);
                            self.selected_layer = None;
                            self.text_caret = layer.text.len();
                            self.text_before = Some(layer.text.clone());
                            self.draft = Some(layer);
                            self.dirty = true;
                        }
                        None => {
                            self.draft = Some(Layer::new(
                                Tool::Text,
                                at,
                                self.ink(Tool::Text),
                                self.annot_stroke,
                                self.annot_font_size,
                                self.annot_blur,
                            ));
                            self.text_caret = 0;
                            self.text_before = None;
                        }
                    }
                    self.status = t("Type — Enter to finish, Esc to cancel").into();
                }
            }
            tool => {
                if resp.drag_started()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    self.draft = Some(Layer::new(
                        tool,
                        to_shot(p),
                        self.ink(tool),
                        self.annot_stroke,
                        self.annot_font_size,
                        self.annot_blur,
                    ));
                }
                if resp.dragged()
                    && let Some(p) = resp.interact_pointer_pos()
                    && let Some(draft) = self.draft.as_mut()
                {
                    draft.b = to_shot(p);
                }
                if resp.drag_stopped() {
                    self.commit_draft();
                }
            }
        }
    }

    /// Run the keyboard for the label being typed on the canvas.
    ///
    /// egui's own `TextEdit` cannot do this job: the label has to sit *on the
    /// image*, at the image's scale, in the annotation colour and the same
    /// typeface the exporter will bake. So we take the event stream and drive a
    /// caret ourselves.
    pub(super) fn text_edit_input(&mut self, ctx: &egui::Context) {
        if !self.typing_text() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut caret = self.text_caret;
        let mut preedit = std::mem::take(&mut self.text_preedit);
        let action = {
            let Some(draft) = self.draft.as_mut() else { return };
            apply_text_events(&mut draft.text, &mut caret, &mut preedit, &events)
        };
        self.text_caret = caret;
        self.text_preedit = preedit;
        match action {
            TextAction::Finish => self.finish_text_edit(),
            TextAction::Cancel => self.cancel_text_edit(),
            TextAction::Continue => {}
        }
    }


    /// The label being typed, drawn live with a blinking caret. Nothing is
    /// baked until the edit finishes, so typing never re-runs the pipeline.
    fn paint_text_editor(
        &self,
        painter: &egui::Painter,
        draft: &Layer,
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    ) {
        let origin = to_screen(draft.a);
        let unit = (to_screen([1.0, 0.0]).x - to_screen([0.0, 0.0]).x).abs();
        let px = (draft.font_size * unit).max(6.0);
        let font = egui::FontId::new(px, egui::FontFamily::Proportional);
        let c = draft.color;
        let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);

        // The same system font backs egui and the exporter, so what is on
        // screen here is what gets baked.
        let galley = painter.layout_no_wrap(draft.text.clone(), font.clone(), color);
        let size = galley.size();
        painter.galley(origin, galley, color);

        let box_rect = egui::Rect::from_min_size(
            origin,
            egui::vec2(size.x.max(px * 0.35), size.y.max(px * 1.2)),
        )
        .expand(px * 0.18);
        draw_border(
            painter,
            box_rect,
            egui::Stroke::new(1.0_f32, crate::app::theme::ACCENT.gamma_multiply(0.7)),
        );

        let head = draft.text.get(..self.text_caret).unwrap_or(&draft.text);
        let ahead = painter
            .layout_no_wrap(head.to_owned(), font.clone(), color)
            .size()
            .x;
        let caret_x = origin.x + ahead;

        // The composition an input method is still building, drawn underlined
        // at the caret the way every other text field shows it.
        if !self.text_preedit.is_empty() {
            let g = painter.layout_no_wrap(self.text_preedit.clone(), font, color);
            let w = g.size().x;
            let base = origin.y + size.y.max(px * 1.2);
            painter.galley(egui::pos2(caret_x, origin.y), g, color);
            painter.line_segment(
                [
                    egui::pos2(caret_x, base),
                    egui::pos2(caret_x + w, base),
                ],
                egui::Stroke::new(1.5_f32, color),
            );
        }

        let time = painter.ctx().input(|i| i.time);
        if (time * 1.6).fract() < 0.62 {
            painter.line_segment(
                [
                    egui::pos2(caret_x, box_rect.min.y + 2.0),
                    egui::pos2(caret_x, box_rect.max.y - 2.0),
                ],
                egui::Stroke::new(1.5_f32, color),
            );
        }

        // Turn the input method on and park its candidate window at the caret.
        // Without this eframe tells winit `set_ime_allowed(false)`, the
        // compositor never opens a text-input channel, and a Vietnamese IME
        // swallows every keystroke instead of delivering it.
        let caret_rect = egui::Rect::from_min_size(
            egui::pos2(caret_x, box_rect.min.y),
            egui::vec2(1.0, box_rect.height()),
        );
        painter.ctx().output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                rect: box_rect,
                cursor_rect: caret_rect,
            })
        });

        // Keep the blink going even when nothing else asks for a frame.
        painter.ctx().request_repaint_after(REPAINT_BLINK);
    }

    fn select_layer_input(
        &mut self,
        resp: &egui::Response,
        to_shot: &dyn Fn(egui::Pos2) -> [f32; 2],
    ) {
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let [sx, sy] = to_shot(p);
            // Topmost first: later layers are drawn on top.
            self.selected_layer = self.layers.iter().rposition(|l| l.hit(sx, sy));
        }

        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let [sx, sy] = to_shot(p);
            self.selected_layer = self.layers.iter().rposition(|l| l.hit(sx, sy));
            self.move_delta = self.selected_layer.map(|_| [0.0, 0.0]);
            self.drag_anchor = Some([sx, sy]);
        }
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(anchor) = self.drag_anchor
        {
            let [sx, sy] = to_shot(p);
            self.move_delta = Some([sx - anchor[0], sy - anchor[1]]);
        }
        if resp.drag_stopped() {
            // Re-render once, at the end — moving is previewed with a ghost
            // outline rather than re-running the pipeline every frame.
            if let (Some(i), Some(d)) = (self.selected_layer, self.move_delta)
                && (d[0].abs() > 0.5 || d[1].abs() > 0.5)
                && i < self.layers.len()
            {
                self.undo.push(&self.layers);
                self.layers[i].translate(d[0], d[1]);
                self.dirty = true;
            }
            self.move_delta = None;
            self.drag_anchor = None;
        }
    }

    fn paint_annotation_overlay(
        &self,
        painter: &egui::Painter,
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    ) {
        if let Some(draft) = &self.draft {
            if draft.kind == Tool::Text {
                self.paint_text_editor(painter, draft, to_screen);
            } else {
                paint_layer_preview(painter, draft, to_screen);
            }
        }

        // Only in Select mode. The selection survives a draw so the sidebar can
        // edit the new layer, but showing its box while a drawing tool is in
        // hand just litters the preview with outlines you cannot act on.
        if self.tool == Tool::Select
            && let Some(i) = self.selected_layer
            && let Some(layer) = self.layers.get(i)
        {
            let d = self.move_delta.unwrap_or([0.0, 0.0]);
            let [x0, y0, x1, y1] = layer.bounds();
            let rect = egui::Rect::from_two_pos(
                to_screen([x0 + d[0], y0 + d[1]]),
                to_screen([x1 + d[0], y1 + d[1]]),
            );
            draw_border(
                painter,
                rect,
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0x4a, 0x9e, 0xff)),
            );
        }
    }
}

/// Vector stand-in for a shape mid-drag. Baking the real thing into the bitmap
/// on every mouse move would mean re-running the whole pipeline at ~7 fps.
fn paint_layer_preview(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
) {
    let c = layer.color;
    let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let a = to_screen(layer.a);
    let b = to_screen(layer.b);
    let width = {
        // Approximate the on-screen stroke from how far one shot pixel travels.
        let p0 = to_screen([0.0, 0.0]);
        let p1 = to_screen([1.0, 0.0]);
        (layer.stroke * (p1.x - p0.x).abs()).max(1.0)
    };
    let stroke = egui::Stroke::new(width, color);
    let rect = egui::Rect::from_two_pos(a, b);

    match layer.kind {
        Tool::Arrow => {
            painter.line_segment([a, b], stroke);
            let angle = (b.y - a.y).atan2(b.x - a.x);
            let head = (width * 4.0).max(10.0);
            for sign in [-1.0_f32, 1.0] {
                let t = angle + std::f32::consts::PI + sign * 0.49;
                painter.line_segment(
                    [b, egui::pos2(b.x + head * t.cos(), b.y + head * t.sin())],
                    stroke,
                );
            }
        }
        Tool::Rect => draw_border(painter, rect, stroke),
        Tool::Ellipse => {
            let centre = rect.center();
            let (rx, ry) = (rect.width() / 2.0, rect.height() / 2.0);
            let points: Vec<egui::Pos2> = (0..=48)
                .map(|i| {
                    let t = i as f32 / 48.0 * std::f32::consts::TAU;
                    egui::pos2(centre.x + rx * t.cos(), centre.y + ry * t.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        Tool::Highlight => {
            painter.rect_filled(rect, 0.0, color.gamma_multiply(0.35));
        }
        Tool::Blur => {
            draw_border(
                painter,
                rect,
                egui::Stroke::new(1.5_f32, egui::Color32::from_gray(230)),
            );
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(90));
        }
        Tool::Text | Tool::Select | Tool::Fill => {}
    }
}

const REPAINT_BLINK: std::time::Duration = std::time::Duration::from_millis(120);

/// What a batch of key events asked the label editor to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TextAction {
    Continue,
    Finish,
    Cancel,
}

/// Apply one frame of input events to a label.
///
/// Kept pure — no `Context`, no app state — because the two halves of "typing
/// works" fail in completely different ways: whether the events *arrive* is
/// egui's and the compositor's problem, whether they *edit correctly* is ours.
/// Only the second half is testable, so it is separated out and tested hard.
///
/// `preedit` is the in-flight IME composition: with a Vietnamese input method
/// the characters arrive as `Ime(Preedit)` updates and land in the string only
/// on `Ime(Commit)`. Typing `dd`→`đ` never produces a plain `Text` event, so an
/// editor that only listens for `Text` looks completely dead to a Vietnamese
/// typist.
pub(super) fn apply_text_events(
    text: &mut String,
    caret: &mut usize,
    preedit: &mut String,
    events: &[egui::Event],
) -> TextAction {
    let insert = |text: &mut String, caret: &mut usize, s: &str| {
        let at = (*caret).min(text.len());
        // A stale caret can land mid-character after an IME commit.
        let at = if text.is_char_boundary(at) {
            at
        } else {
            prev_boundary(text, at)
        };
        text.insert_str(at, s);
        *caret = at + s.len();
    };

    for event in events {
        match event {
            egui::Event::Text(t) => insert(text, caret, t),
            egui::Event::Paste(t) => insert(text, caret, &t.replace(['\n', '\r'], " ")),
            egui::Event::Ime(ime) => match ime {
                egui::ImeEvent::Preedit(p) => p.clone_into(preedit),
                egui::ImeEvent::Commit(c) => {
                    preedit.clear();
                    insert(text, caret, c);
                }
                egui::ImeEvent::Enabled | egui::ImeEvent::Disabled => preedit.clear(),
            },
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                // While an IME composition is open the input method owns the
                // keyboard; acting on the raw keys as well would double up.
                if !preedit.is_empty() {
                    continue;
                }
                match key {
                    egui::Key::Enter => return TextAction::Finish,
                    egui::Key::Escape => return TextAction::Cancel,
                    egui::Key::Backspace if *caret > 0 => {
                        let from = prev_boundary(text, *caret);
                        text.replace_range(from..*caret, "");
                        *caret = from;
                    }
                    egui::Key::Delete if *caret < text.len() => {
                        let to = next_boundary(text, *caret);
                        text.replace_range(*caret..to, "");
                    }
                    egui::Key::ArrowLeft => {
                        *caret = if modifiers.command {
                            0
                        } else {
                            prev_boundary(text, *caret)
                        }
                    }
                    egui::Key::ArrowRight => {
                        *caret = if modifiers.command {
                            text.len()
                        } else {
                            next_boundary(text, *caret)
                        }
                    }
                    egui::Key::Home => *caret = 0,
                    egui::Key::End => *caret = text.len(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    TextAction::Continue
}

/// Previous UTF-8 character boundary before `i`. Slicing a `String` at a byte
/// that lands inside a multi-byte character panics, and Vietnamese is full of
/// them, so every caret move has to snap to a boundary.
fn prev_boundary(s: &str, i: usize) -> usize {
    let mut j = i.min(s.len()).saturating_sub(1);
    while j > 0 && !s.is_char_boundary(j) {
        j -= 1;
    }
    j
}

/// Next UTF-8 character boundary after `i`.
fn next_boundary(s: &str, i: usize) -> usize {
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Do two `[x0, y0, x1, y1]` boxes overlap at all?
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn accent_stroke() -> egui::Stroke {
    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0x4a, 0x9e, 0xff))
}

fn clamp_pos(p: egui::Pos2, r: egui::Rect) -> egui::Pos2 {
    egui::pos2(p.x.clamp(r.min.x, r.max.x), p.y.clamp(r.min.y, r.max.y))
}

/// Map a screen-space selection rect to pixel coordinates in the full capture.
fn rect_to_full_px(
    sel: egui::Rect,
    img_rect: egui::Rect,
    cap_w: u32,
    cap_h: u32,
) -> Option<[u32; 4]> {
    let sx = cap_w as f32 / img_rect.width();
    let sy = cap_h as f32 / img_rect.height();
    let x0 = ((sel.min.x - img_rect.min.x) * sx).max(0.0);
    let y0 = ((sel.min.y - img_rect.min.y) * sy).max(0.0);
    let x1 = ((sel.max.x - img_rect.min.x) * sx).min(cap_w as f32);
    let y1 = ((sel.max.y - img_rect.min.y) * sy).min(cap_h as f32);
    let w = (x1 - x0) as u32;
    let h = (y1 - y0) as u32;
    (w >= 4 && h >= 4).then_some([x0 as u32, y0 as u32, w, h])
}

fn draw_border(p: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    p.line_segment([r.left_top(), r.right_top()], stroke);
    p.line_segment([r.right_top(), r.right_bottom()], stroke);
    p.line_segment([r.right_bottom(), r.left_bottom()], stroke);
    p.line_segment([r.left_bottom(), r.left_top()], stroke);
}

fn paint_checker(p: &egui::Painter, rect: egui::Rect) {
    let cell = 10.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for row in 0..rows {
        for col in 0..cols {
            let on = (row + col) % 2 == 0;
            let c = if on {
                egui::Color32::from_gray(210)
            } else {
                egui::Color32::from_gray(160)
            };
            let min = rect.min + egui::vec2(col as f32 * cell, row as f32 * cell);
            let r = egui::Rect::from_min_size(min, egui::vec2(cell, cell)).intersect(rect);
            p.rect_filled(r, 0.0, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
    }


    #[test]
    fn selection_maps_back_to_capture_pixels() {
        // The preview is drawn at half size, so a 100px drag is 200 real pixels.
        let img_rect = r(0.0, 0.0, 960.0, 540.0);
        let sel = r(96.0, 54.0, 480.0, 270.0);
        let got = rect_to_full_px(sel, img_rect, 1920, 1080).unwrap();
        assert_eq!(got, [192, 108, 768, 432]);
    }

    #[test]
    fn a_tiny_selection_is_rejected() {
        let img_rect = r(0.0, 0.0, 1920.0, 1080.0);
        assert!(rect_to_full_px(r(10.0, 10.0, 12.0, 12.0), img_rect, 1920, 1080).is_none());
    }

    #[test]
    fn selection_is_clamped_to_the_capture() {
        let img_rect = r(0.0, 0.0, 100.0, 100.0);
        let got = rect_to_full_px(r(-50.0, -50.0, 200.0, 200.0), img_rect, 100, 100).unwrap();
        assert_eq!(got, [0, 0, 100, 100]);
    }

    /// Vietnamese is multi-byte all the way through, so a caret that steps by
    /// one byte would land mid-character and panic the next time the string is
    /// sliced. These walk the whole string in both directions to prove it
    /// cannot happen.
    #[test]
    fn the_caret_only_ever_lands_on_character_boundaries() {
        for text in ["Chào bạn", "Việt Nam vô địch", "a", "", "đđđ", "x 界 y"] {
            let mut i = 0;
            while i < text.len() {
                let next = next_boundary(text, i);
                assert!(next > i, "{text:?} stalled at {i}");
                assert!(text.is_char_boundary(next), "{text:?} split at {next}");
                // Slicing is what actually panics, so do it.
                let _ = &text[..next];
                i = next;
            }
            while i > 0 {
                let prev = prev_boundary(text, i);
                assert!(prev < i, "{text:?} stalled going back at {i}");
                assert!(text.is_char_boundary(prev), "{text:?} split at {prev}");
                let _ = &text[..prev];
                i = prev;
            }
        }
    }

    #[test]
    fn boundaries_clamp_at_both_ends() {
        assert_eq!(prev_boundary("Chào", 0), 0, "must not run off the front");
        let end = "Chào".len();
        assert_eq!(next_boundary("Chào", end), end, "must not run off the back");
        // A caret past the end (stale after a shorter text replaced a longer
        // one) must still resolve to something sliceable.
        assert!(prev_boundary("ab", 99) <= 2);
        assert_eq!(next_boundary("ab", 99), 2);
    }

    #[test]
    fn one_step_back_removes_exactly_one_character() {
        let text = "Việt";
        let i = prev_boundary(text, text.len());
        assert_eq!(&text[..i], "Việ", "backspace should drop just the 't'");
        let j = prev_boundary(text, i);
        assert_eq!(&text[..j], "Vi", "and then just the 'ệ'");
    }

    fn text(s: &str) -> egui::Event {
        egui::Event::Text(s.to_owned())
    }
    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn ime(e: egui::ImeEvent) -> egui::Event {
        egui::Event::Ime(e)
    }
    /// Run a batch against an empty label and return what it ended up saying.
    fn run(events: &[egui::Event]) -> (String, usize, String, TextAction) {
        let (mut t, mut c, mut p) = (String::new(), 0usize, String::new());
        let a = apply_text_events(&mut t, &mut c, &mut p, events);
        (t, c, p, a)
    }

    #[test]
    fn plain_typing_lands_in_the_label() {
        let (t, c, _, a) = run(&[text("Hi"), text(" there")]);
        assert_eq!(t, "Hi there");
        assert_eq!(c, t.len(), "caret should trail the text");
        assert_eq!(a, TextAction::Continue);
    }

    /// The Vietnamese path. Typing `dd` in Telex never emits a plain Text
    /// event — the input method builds the character in a preedit and only
    /// commits `đ` at the end. This is exactly the case that was dead before.
    #[test]
    fn an_ime_composition_only_lands_on_commit() {
        let (mut t, mut c, mut p) = (String::new(), 0usize, String::new());

        apply_text_events(&mut t, &mut c, &mut p, &[ime(egui::ImeEvent::Enabled)]);
        apply_text_events(&mut t, &mut c, &mut p, &[ime(egui::ImeEvent::Preedit("d".into()))]);
        assert_eq!(t, "", "a composition in flight must not be in the label yet");
        assert_eq!(p, "d", "but it must be visible as preedit");

        apply_text_events(&mut t, &mut c, &mut p, &[ime(egui::ImeEvent::Preedit("dd".into()))]);
        assert_eq!(p, "dd");

        apply_text_events(&mut t, &mut c, &mut p, &[ime(egui::ImeEvent::Commit("đ".into()))]);
        assert_eq!(t, "đ", "commit is what actually writes the character");
        assert_eq!(p, "", "and it clears the composition");
        assert_eq!(c, "đ".len(), "caret moves past the multi-byte character");
    }

    /// Enter is how you finish a label, but an input method uses Enter to
    /// accept a candidate. Acting on both would end the edit mid-word.
    #[test]
    fn keys_are_ignored_while_a_composition_is_open() {
        let (mut t, mut c, mut p) = ("xin".to_owned(), 3usize, String::new());
        apply_text_events(&mut t, &mut c, &mut p, &[ime(egui::ImeEvent::Preedit("ch".into()))]);

        let a = apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Enter)]);
        assert_eq!(a, TextAction::Continue, "Enter belongs to the IME here");

        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Backspace)]);
        assert_eq!(t, "xin", "backspace must not eat the committed text either");
    }

    #[test]
    fn enter_finishes_and_escape_cancels() {
        assert_eq!(run(&[text("a"), key(egui::Key::Enter)]).3, TextAction::Finish);
        assert_eq!(run(&[text("a"), key(egui::Key::Escape)]).3, TextAction::Cancel);
        // Whatever came before the key still applies.
        assert_eq!(run(&[text("a"), key(egui::Key::Enter)]).0, "a");
    }

    #[test]
    fn backspace_deletes_one_whole_vietnamese_character() {
        let (mut t, mut c, mut p) = ("Việt".to_owned(), "Việt".len(), String::new());
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Backspace)]);
        assert_eq!(t, "Việ");
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Backspace)]);
        assert_eq!(t, "Vi", "the two-byte ệ goes in one press, not two");
        assert_eq!(c, 2);
    }

    #[test]
    fn the_caret_can_walk_into_the_middle_and_insert_there() {
        let (mut t, mut c, mut p) = ("Chào".to_owned(), "Chào".len(), String::new());
        for _ in 0..2 {
            apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::ArrowLeft)]);
        }
        apply_text_events(&mut t, &mut c, &mut p, &[text("X")]);
        assert_eq!(t, "ChXào", "insert must happen at the caret, not the end");
    }

    #[test]
    fn home_and_end_jump_without_splitting_characters() {
        let (mut t, mut c, mut p) = ("Việt".to_owned(), 0usize, String::new());
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::End)]);
        assert_eq!(c, t.len());
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Home)]);
        assert_eq!(c, 0);
        apply_text_events(&mut t, &mut c, &mut p, &[text("A")]);
        assert_eq!(t, "AViệt");
    }

    #[test]
    fn delete_removes_forward_and_stops_at_the_end() {
        let (mut t, mut c, mut p) = ("đá".to_owned(), 0usize, String::new());
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Delete)]);
        assert_eq!(t, "á");
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Delete)]);
        assert_eq!(t, "");
        // Past the end it must be a no-op, not a panic.
        apply_text_events(&mut t, &mut c, &mut p, &[key(egui::Key::Delete)]);
        assert_eq!(t, "");
    }

    #[test]
    fn a_pasted_block_becomes_one_line() {
        let (t, _, _, _) = run(&[egui::Event::Paste("hai\ndòng".into())]);
        assert_eq!(t, "hai dòng", "the renderer draws a single line");
    }

    #[test]
    fn a_stale_caret_inside_a_character_does_not_panic() {
        // Byte 1 is inside 'đ'. Insertion must snap to a boundary.
        let (mut t, mut c, mut p) = ("đ".to_owned(), 1usize, String::new());
        apply_text_events(&mut t, &mut c, &mut p, &[text("x")]);
        assert!(t == "xđ" || t == "đx", "got {t:?}");
    }

    /// The bug this was written for: a ctrl+wheel handler that reads
    /// `smooth_scroll_delta` finds nothing, because egui diverts the wheel into
    /// `zoom_delta` the moment the zoom modifier is held and leaves the scroll
    /// delta at zero. Driving a real `egui::Context` pins that down, so an egui
    /// upgrade that changes it fails here instead of silently killing zoom.
    #[test]
    fn ctrl_wheel_arrives_as_zoom_delta_and_not_as_scroll() {
        let ctx = egui::Context::default();
        let wheel = |modifiers: egui::Modifiers| egui::RawInput {
            modifiers,
            events: vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 3.0),
                phase: egui::TouchPhase::Move,
                modifiers,
            }],
            ..Default::default()
        };

        let (mut zoom, mut scroll) = (1.0, egui::Vec2::ZERO);
        let _ = ctx.run_ui(wheel(egui::Modifiers::COMMAND), |ui| {
            ui.ctx().input(|i| {
                zoom = i.zoom_delta();
                scroll = i.smooth_scroll_delta;
            });
        });
        assert!(zoom > 1.0, "ctrl+wheel should zoom in, got factor {zoom}");
        assert_eq!(
            scroll,
            egui::Vec2::ZERO,
            "egui consumes the scroll when zooming — do not read it here"
        );
    }

    /// ...and the plain wheel has to keep working as a scroll, or panning dies.
    #[test]
    fn a_bare_wheel_still_arrives_as_scroll() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            events: vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 3.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let (mut zoom, mut scroll) = (1.0, egui::Vec2::ZERO);
        let _ = ctx.run_ui(raw, |ui| {
            ui.ctx().input(|i| {
                zoom = i.zoom_delta();
                scroll = i.smooth_scroll_delta;
            });
        });
        assert_eq!(zoom, 1.0, "no modifier means no zoom");
        assert_ne!(scroll, egui::Vec2::ZERO, "the wheel must reach the pan path");
    }
}
