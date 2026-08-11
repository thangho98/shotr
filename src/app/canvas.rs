//! The central canvas: region/window picking in Select mode, and the beautified
//! preview in Edit mode.

use crate::i18n::t;

use eframe::egui;

use super::{OcrMode, PickMode, ShotrApp, Zoom};
use crate::annotate::{Head, Layer, TextAlign, Tool};
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
        // Cloned up front: hit-testing a label needs to lay its glyphs out, and
        // that has to happen while `self` is borrowed mutably below.
        let ui_painter = ui.painter().clone();

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
            self.annotation_input(&resp, &to_shot, &to_screen, &ui_painter);
            self.sync_detached();
            if let Some(p) = resp.hover_pos() {
                self.paint_ghost(ui.painter(), to_shot(p), &to_screen);
            }
            self.paint_annotation_overlay(ui.painter(), &to_screen);
        } else {
            self.ocr_input(&resp, &to_shot);
            self.paint_ocr_overlay(ui.painter(), &to_screen);
        }

        // Re-render here rather than leaving it to the next frame.
        //
        // The bitmap is rebuilt near the top of `ui`, *before* any of this
        // runs, so a shape finished by the mouse-up just handled is in neither
        // picture: the draft that was drawing it has been taken, and the
        // texture does not carry it yet. That frame therefore shows nothing,
        // and the annotation blinks out for exactly one frame as the button
        // comes up.
        //
        // Doing it now still counts, because the painter above recorded only
        // the texture *id*: egui uploads the delta for that id before it draws
        // this frame's shapes, so the pixels arrive in time.
        if self.dirty {
            self.rebuild_texture(ctx);
            self.dirty = false;
        }

        // Only the Select tool gets the copy-and-close gesture; with a drawing
        // tool active a double click is two shapes, not a shortcut.
        //
        // And never when the double click landed *on* an annotation. There it
        // means "I am trying to do something with this shape" — poking at one
        // twice is the most natural thing to try — and closing the whole editor
        // is the last thing that should happen. It also made selection look
        // broken: the window vanished before the outline could be noticed.
        if self.tool == Tool::Select && resp.double_clicked() {
            let on_a_shape = resp
                .interact_pointer_pos()
                .and_then(|p| layer_at(&ui_painter, &self.layers, to_shot(p)))
                .is_some();
            if !on_a_shape {
                self.copy_and_close(ctx);
            }
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
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
        painter: &egui::Painter,
    ) {
        match self.tool {
            Tool::Select => self.select_layer_input(resp, to_shot, to_screen, painter),
            Tool::Text => {
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    // Clicking away commits whatever was being typed; clicking
                    // an existing label picks it back up instead of stacking a
                    // second one on top of it.
                    self.finish_text_edit();
                    let at = to_shot(p);
                    let existing = self.layers.iter().rposition(|l| {
                        l.kind == Tool::Text && contains(painter, l, at)
                    });
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
                            self.draft = Some(self.new_layer(Tool::Text, at));
                            self.text_caret = 0;
                            self.text_before = None;
                        }
                    }
                    self.status = t("Type — Enter to finish, Esc to cancel").into();
                }
            }
            Tool::Badge => {
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let mut badge = self.new_layer(Tool::Badge, to_shot(p));
                    badge.text = self.next_badge().to_string();
                    self.undo.push(&self.layers);
                    self.layers.push(badge);
                    self.dirty = true;
                }
            }
            tool => {
                if resp.drag_started()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let mut draft = self.new_layer(tool, to_shot(p));
                    if self.freehand() {
                        draft.path.push(to_shot(p));
                    }
                    self.draft = Some(draft);
                }
                if resp.dragged()
                    && let Some(p) = resp.interact_pointer_pos()
                    && let Some(draft) = self.draft.as_mut()
                {
                    let at = to_shot(p);
                    draft.b = at;
                    if !draft.path.is_empty() {
                        // Only when the pointer has actually moved: a stationary
                        // press otherwise piles hundreds of identical points
                        // into the layer, and every one of them is a segment
                        // the distance field has to walk.
                        let last = draft.path[draft.path.len() - 1];
                        let moved = (at[0] - last[0]).abs() + (at[1] - last[1]).abs();
                        if moved > 1.0 {
                            draft.path.push(at);
                        }
                    }
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

        // The same system font backs egui and the exporter, and `text::px_scale`
        // is what makes the same number mean the same size to both, so what is
        // on screen here is what gets baked.
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
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
        painter: &egui::Painter,
    ) {
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.selected_layer = layer_at(painter, &self.layers, to_shot(p));
            if let Some(i) = self.selected_layer {
                self.load_dials_from(i);
            }
        }

        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            // The knob wins over the shape under it: it is deliberately outside
            // the frame, but a small shape leaves the two close together.
            self.tail_drag = self.grabbed_tail(p, to_screen);
            if self.tail_drag {
                self.undo.push(&self.layers);
            }
            self.turn_from = (!self.tail_drag)
                .then(|| self.grabbed_handle(painter, p, to_screen))
                .flatten();
            if self.turn_from.is_some() {
                // A turn writes itself onto the layer frame by frame, so the
                // only moment there is still an old angle to keep is this one.
                self.undo.push(&self.layers);
            } else {
                let at = to_shot(p);
                self.selected_layer = layer_at(painter, &self.layers, at);
                if let Some(i) = self.selected_layer {
                    self.load_dials_from(i);
                }
                self.move_delta = self.selected_layer.map(|_| [0.0, 0.0]);
                self.drag_anchor = Some(at);
            }
        }
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
        {
            if self.tail_drag {
                if let Some(layer) = self.selected_layer.and_then(|i| self.layers.get_mut(i)) {
                    // The tail follows the pointer; the tip stays where it was.
                    // Nothing else to set — the arrow's size and direction are
                    // both read off these two points.
                    layer.a = to_shot(p);
                }
            } else if let (Some(grab), Some(i)) = (self.turn_from, self.selected_layer) {
                let at = to_shot(p);
                if let Some(layer) = self.layers.get_mut(i) {
                    let c = layer.centre();
                    let now = (at[1] - c[1]).atan2(at[0] - c[0]);
                    layer.angle = now - grab;
                    // No `dirty` here: the shape is out of the bitmap for the
                    // length of the turn and the overlay is drawing it.
                }
            } else if let Some(anchor) = self.drag_anchor {
                let [sx, sy] = to_shot(p);
                self.move_delta = Some([sx - anchor[0], sy - anchor[1]]);
            }
        }
        if resp.drag_stopped() {
            if self.tail_drag {
                self.tail_drag = false;
                self.move_delta = None;
                self.drag_anchor = None;
                return;
            }
            // A turn is written straight onto the layer as it happens, so
            // there is nothing to commit here beyond ending the gesture.
            if self.turn_from.take().is_some() {
                self.move_delta = None;
                self.drag_anchor = None;
                return;
            }
            if let (Some(i), Some(d)) = (self.selected_layer, self.move_delta)
                && (d[0].abs() > 0.5 || d[1].abs() > 0.5)
                && i < self.layers.len()
            {
                self.undo.push(&self.layers);
                self.layers[i].translate(d[0], d[1]);
            }
            self.move_delta = None;
            self.drag_anchor = None;
        }
    }

    /// Decide which annotation the overlay owns, and re-render if that changed.
    ///
    /// Kept in one place rather than set at each drag edge: selecting,
    /// deselecting, deleting, undoing and dropping a drag all have to agree
    /// with the bitmap, and every one of them used to be its own chance to
    /// leave a shape invisible or doubled.
    fn sync_detached(&mut self) {
        let wanted = self.detached_wanted();
        if self.detached_layer != wanted {
            self.detached_layer = wanted;
            self.dirty = true;
        }
    }

    fn detached_wanted(&self) -> Option<usize> {
        if self.tool != Tool::Select {
            return None;
        }
        let i = self.selected_layer?;
        let kind = self.layers.get(i)?.kind;
        let _ = kind;
        // Only while it is being dragged or turned. Selection alone leaves the
        // shape in the bitmap, because the vector stand-in is not faithful for
        // every tool — a detached Blur would show as a flat box rather than as
        // blurred pixels — and the frame is drawn around it either way.
        //
        // Turning has to be in here as much as moving does: the render pipeline
        // costs 200–600ms for one preview of this size, so re-rendering per
        // frame turns a drag into a slideshow. Measured with
        // `examples/render_demo`.
        (self.drag_anchor.is_some() || self.turn_from.is_some() || self.tail_drag).then_some(i)
    }

    /// Whether a press landed on the arrow's tail handle.
    ///
    /// Checked before the rotate knob, and before the shape itself, because the
    /// handle sits *on* the arrow: whichever is tested first wins, and a press
    /// on a 12px dot is far more likely to mean the dot.
    fn grabbed_tail(&self, p: egui::Pos2, to_screen: &dyn Fn([f32; 2]) -> egui::Pos2) -> bool {
        if self.tool != Tool::Select {
            return false;
        }
        let Some(layer) = self.selected_layer.and_then(|i| self.layers.get(i)) else {
            return false;
        };
        if layer.kind != Tool::Arrow {
            return false;
        }
        // Generous, because nobody aims at a 6px dot.
        (p - to_screen(layer.a)).length() <= TURN_KNOB + 9.0
    }

    /// The angle the pointer sits at, if a drag started on the rotate knob.
    ///
    /// Returned relative to the shape's current angle, so the knob does not
    /// jump to the pointer the moment it is grabbed.
    fn grabbed_handle(
        &self,
        painter: &egui::Painter,
        p: egui::Pos2,
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    ) -> Option<f32> {
        if self.tool != Tool::Select {
            return None;
        }
        let i = self.selected_layer?;
        let layer = self.layers.get(i)?;
        if !Layer::turnable(layer.kind) {
            return None;
        }
        let (_, knob) = turn_handle(painter, layer, to_screen);
        // Generous: the knob is 5px and nobody aims at 5px.
        if (p - knob).length() > TURN_KNOB + 8.0 {
            return None;
        }
        let c = to_screen(layer.centre());
        Some((p.y - c.y).atan2(p.x - c.x) - layer.angle)
    }

    /// A pale copy of what the next drag would leave, following the pointer.
    ///
    /// It answers the question the tool row cannot: not *which tool* is in hand
    /// but which of its forms, at what colour and what size. It goes through
    /// `paint_layer_preview` like everything else — a third drawing path would
    /// be a fifth place to drift from what the exporter bakes.
    ///
    /// Never sets `dirty`: it is overlay only, so it costs a repaint and not a
    /// render.
    fn paint_ghost(
        &self,
        painter: &egui::Painter,
        at: [f32; 2],
        to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    ) {
        // Nothing to preview while the real thing is being drawn, and nothing
        // to preview for a tool that draws nothing.
        if self.draft.is_some() || self.tool == Tool::Select || self.tool == Tool::Fill {
            return;
        }
        let Some(ghost) = self.ghost_layer(at) else {
            return;
        };
        // Pale, so it reads as a promise rather than as ink already down. No
        // rim: a translucent white ring under translucent ink turns the whole
        // ghost muddy, and the ghost's job is to say which shape and what
        // colour, not to be a faithful copy.
        let mut ghost = ghost;
        ghost.color[3] = (ghost.color[3] as f32 * GHOST_ALPHA) as u8;
        ghost.border = 0.0;
        if ghost.kind == Tool::Text {
            paint_text_layer(painter, &ghost, to_screen);
        } else {
            paint_layer_preview(painter, &ghost, to_screen);
        }
    }

    /// The shape the ghost stands for, at a size that reads without pretending
    /// to be the size the drag will actually give it.
    fn ghost_layer(&self, at: [f32; 2]) -> Option<Layer> {
        let reach = GHOST_REACH / self.preview_scale.max(f32::EPSILON);
        let mut ghost = self.new_layer(self.tool, at);
        // The pointer is the ghost's top-left corner, not its middle: a drag
        // starts where the cursor is, so a ghost centred on it promises a shape
        // half of which is behind where the drag will begin.
        match self.tool {
            // The tip goes *at* the pointer and the tail falls away behind it
            // on the diagonal: that is the gesture the tool is for, and it puts
            // the ghost's own top-left corner under the cursor like every other
            // one.
            Tool::Arrow => {
                let run = reach * std::f32::consts::FRAC_1_SQRT_2;
                ghost.a = [at[0] + run, at[1] + run];
                ghost.b = at;
            }
            Tool::Line => {
                ghost.a = at;
                ghost.b = [at[0] + reach, at[1] + reach * 0.55];
            }
            Tool::Text => ghost.text = GHOST_TEXT.to_owned(),
            // A badge is placed by a click rather than dragged, so its ghost is
            // the disc itself — and it shows the number the click would leave,
            // which is the only thing about it that is not obvious.
            Tool::Badge => {
                ghost.a = [at[0] + ghost.font_size, at[1] + ghost.font_size];
                ghost.text = self.next_badge().to_string();
            }
            _ if self.freehand() => {
                // A stroke, not a block: the block ghost promised the wrong
                // tool entirely.
                let step = reach / 4.0;
                ghost.path = (0..=4)
                    .map(|i| {
                        let t = i as f32 / 4.0;
                        [
                            at[0] + t * reach,
                            at[1] + (t * std::f32::consts::PI).sin() * step,
                        ]
                    })
                    .collect();
                ghost.a = ghost.path[0];
                ghost.b = ghost.path[0];
            }
            _ => {
                ghost.a = at;
                ghost.b = [at[0] + reach, at[1] + reach * 0.6];
            }
        }
        Some(ghost)
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
        // edit the new layer, but showing its outline while a drawing tool is
        // in hand just litters the preview with marks you cannot act on.
        if self.tool == Tool::Select
            && let Some(i) = self.selected_layer
            && let Some(layer) = self.layers.get(i)
        {
            let d = self.move_delta.unwrap_or([0.0, 0.0]);
            let moved = shifted(layer, d);
            // Ramp the halo in, so a selection announces itself instead of
            // snapping into place.
            let fade = painter
                .ctx()
                .animate_bool_with_time(egui::Id::new(("selected", i)), true, 0.10);

            // While it is out of the bitmap the overlay *is* the annotation.
            if self.detached_layer == Some(i) {
                if moved.kind == Tool::Text {
                    paint_text_layer(painter, &moved, to_screen);
                } else {
                    paint_layer_preview(painter, &moved, to_screen);
                }
            }
            paint_selection(painter, &moved, to_screen, fade);
        }
    }
}

/// A copy of `layer` shifted by `d`, for previewing a move without touching
/// the real one until the button comes up.
fn shifted(layer: &Layer, d: [f32; 2]) -> Layer {
    let mut moved = layer.clone();
    moved.translate(d[0], d[1]);
    moved
}

/// The text of a label, drawn where it really sits.
///
/// `paint_layer_preview` cannot help here: a label has no geometry to trace,
/// only glyphs, and while it is being moved the overlay is the only copy of it
/// on screen.
fn paint_text_layer(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
) {
    let c = layer.color;
    let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let galley = text_galley(painter, layer, to_screen, color);
    // `TextShape` turns about `pos`, and `Layer::centre` for a label is its
    // anchor too, so the two agree without a correction — but alignment moves
    // the *line* off the anchor, so the shape has to start where the ink does.
    let pos = to_screen(layer.a) - egui::vec2(galley.size().x * align_shift(layer.align), 0.0);
    painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(layer.angle));
}

/// How far left of its anchor a label starts, as a fraction of its width.
/// Mirrors `annotate::Label::shift`.
fn align_shift(align: TextAlign) -> f32 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Centre => 0.5,
        TextAlign::Right => 1.0,
    }
}

/// A label laid out the way it will be baked, underline included.
///
/// egui carries an underline in `TextFormat`, which `layout_no_wrap` does not
/// take — hence the job. Worth the extra lines: the alternative is drawing the
/// rule by hand here and again in `render::text`, which is exactly how the
/// stand-in and the bake have drifted apart every other time.
fn text_galley(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &layer.text,
        0.0,
        egui::TextFormat {
            font_id: text_font(layer, to_screen),
            color,
            underline: if layer.underline {
                egui::Stroke::new((text_font(layer, to_screen).size * 0.06).max(1.0), color)
            } else {
                egui::Stroke::NONE
            },
            ..Default::default()
        },
    );
    painter.layout_job(job)
}

/// The on-screen font for a label, scaled the way the picture is.
fn text_font(layer: &Layer, to_screen: &dyn Fn([f32; 2]) -> egui::Pos2) -> egui::FontId {
    let unit = (to_screen([1.0, 0.0]).x - to_screen([0.0, 0.0]).x).abs();
    egui::FontId::new((layer.font_size * unit).max(6.0), egui::FontFamily::Proportional)
}

/// The selection frame: a dashed rectangle round the annotation.
///
/// Deliberately the same mark for every tool. Two cleverer versions came first
/// and both were worse — a halo drawn under the ink meant redrawing the ink
/// here, which the vector stand-in cannot do faithfully for a blur, and tracing
/// each shape's own silhouette worked for a rectangle but fell apart on an
/// arrow, whose head left loose strokes joined to nothing. A dashed box says
/// "this one is selected" without pretending to be part of the picture.
fn paint_selection(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    fade: f32,
) {
    if fade <= 0.01 {
        return;
    }
    let accent = crate::app::theme::ACCENT.gamma_multiply(fade);
    let stroke = egui::Stroke::new(1.5_f32, accent);

    // A shape with a silhouette of its own gets traced; everything else gets a
    // dashed box. Two marks on purpose: an outline is only worth drawing when
    // there is an edge to follow, and a blur has none — tracing its box would
    // just be the dashed frame with the dashes taken out.
    if layer.kind == Tool::Arrow {
        let pts: Vec<egui::Pos2> = crate::annotate::arrow_points(layer)
            .into_iter()
            .map(&to_screen)
            .collect();
        if pts.len() >= 3 {
            painter.add(egui::Shape::closed_line(pts, stroke));
            // One handle, at the tail, because one drag does both jobs: how far
            // it is from the tip is the size, and which way it lies is the
            // direction. A second knob would be a second control for the same
            // two numbers.
            let tail = to_screen(layer.a);
            // White, not the palette's ink: the handle sits on the picture,
            // where a colour that follows the desktop theme would vanish into
            // half the screenshots people take.
            painter.circle(
                tail,
                TURN_KNOB + 1.0,
                egui::Color32::WHITE,
                egui::Stroke::new(2.0_f32, accent),
            );
        }
        return;
    }

    let rect = selection_rect(painter, layer, to_screen);

    // Dashes are laid along the path from its start, so going round the corners
    // in one call keeps them evenly spaced all the way round.
    let turn = |p: egui::Pos2| turn_on_screen(layer, to_screen, p);
    let corners = [
        turn(rect.left_top()),
        turn(rect.right_top()),
        turn(rect.right_bottom()),
        turn(rect.left_bottom()),
        turn(rect.left_top()),
    ];
    painter.extend(egui::Shape::dashed_line(&corners, stroke, 6.0, 4.0));

    if Layer::turnable(layer.kind) {
        let (stem, knob) = turn_handle(painter, layer, to_screen);
        painter.line_segment([stem, knob], stroke);
        painter.circle_filled(knob, TURN_KNOB, accent);
    }
}

/// The rotate handle: where its stem leaves the frame, and where the knob sits.
///
/// Above the frame's top edge, turned with it, so it always reads as "this end
/// is the top of the shape".
fn turn_handle(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
) -> (egui::Pos2, egui::Pos2) {
    let rect = selection_rect(painter, layer, to_screen);
    let stem = egui::pos2(rect.center().x, rect.min.y);
    let knob = egui::pos2(rect.center().x, rect.min.y - TURN_ARM);
    (
        turn_on_screen(layer, to_screen, stem),
        turn_on_screen(layer, to_screen, knob),
    )
}

/// Turn a point that was worked out in the shape's upright frame into where it
/// really lands on screen.
fn turn_on_screen(
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    p: egui::Pos2,
) -> egui::Pos2 {
    if layer.angle.abs() < 1e-4 {
        return p;
    }
    let c = to_screen(layer.centre());
    let (sin, cos) = layer.angle.sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    egui::pos2(c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos)
}

/// Where the frame sits: the shape's own extent, plus room to clear its stroke.
fn selection_rect(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
) -> egui::Rect {
    let a = to_screen(layer.a);
    let unit = screen_unit(to_screen);
    // A freehand mark is a path, so its two corners say nothing: `b` never
    // moved off `a` and a frame taken from them is a dot at the stroke's start.
    if !layer.path.is_empty() {
        let mut r = egui::Rect::NOTHING;
        for p in &layer.path {
            r = r.union(egui::Rect::from_center_size(
                to_screen(*p),
                egui::Vec2::splat(1.0),
            ));
        }
        return r.expand((layer.stroke * unit).max(1.0) / 2.0 + 5.0);
    }
    // A badge is a disc placed by a click, so it has no second corner either.
    if layer.kind == Tool::Badge {
        let r = (layer.font_size * unit).max(4.0);
        return egui::Rect::from_center_size(a, egui::Vec2::splat(r * 2.0)).expand(5.0);
    }
    if layer.kind == Tool::Text {
        let galley = text_galley(painter, layer, to_screen, egui::Color32::WHITE);
        let start = a - egui::vec2(galley.size().x * align_shift(layer.align), 0.0);
        return egui::Rect::from_min_size(start, galley.size()).expand(5.0);
    }
    let half = (layer.stroke * unit).max(1.0) / 2.0;
    egui::Rect::from_two_pos(a, to_screen(layer.b)).expand(half + 5.0)
}

/// The outline of a stroked shape, drawn with whatever stroke is handed in.
///
/// One path, used twice: once in the annotation's own colour and once —
/// underneath and much fatter — as the selection halo. Sharing it is the whole
/// point, because it means the halo traces the arrowhead and the ellipse for
/// free instead of each needing its silhouette worked out by hand.
fn stroke_shape(
    painter: &egui::Painter,
    layer: &Layer,
    to_screen: &dyn Fn([f32; 2]) -> egui::Pos2,
    stroke: egui::Stroke,
) {
    // Built in shot space and turned there, so a rotated shape needs no special
    // case: `place` is the only thing that knows about the angle.
    let place = |p: [f32; 2]| to_screen(turn_in_shot(layer, p));
    let (a, b) = (layer.a, layer.b);

    match layer.kind {
        Tool::Line => {
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if len < 1.0 {
                return;
            }
            // The same lengths `draw_arrow` uses, in the same units.
            let reach = (layer.stroke * 4.0).max(10.0).min(len);
            let along = (b[1] - a[1]).atan2(b[0] - a[0]);
            let corner = |sign: f32| {
                let t = along + std::f32::consts::PI + sign * 0.49;
                [b[0] + reach * t.cos(), b[1] + reach * t.sin()]
            };
            let (l, r) = (corner(-1.0), corner(1.0));
            let neck = [(l[0] + r[0]) / 2.0, (l[1] + r[1]) / 2.0];

            let shaft = match layer.head {
                Head::None | Head::Open => vec![(a, b)],
                Head::Solid => vec![(a, neck)],
                Head::Dashed => dashes(a, neck, layer.stroke * 2.0),
            };
            for (from, to) in &shaft {
                painter.line_segment([place(*from), place(*to)], stroke);
                // The exporter unions capsules, so every end is round.
                // `line_segment` has butt caps, which is what made the arrow
                // snap from soft to hard on mouse-up.
                for p in [from, to] {
                    painter.circle_filled(place(*p), stroke.width / 2.0, stroke.color);
                }
            }
            match layer.head {
                Head::None => {}
                Head::Open => {
                    for barb in [l, r] {
                        painter.line_segment([place(b), place(barb)], stroke);
                        painter.circle_filled(place(barb), stroke.width / 2.0, stroke.color);
                    }
                    painter.circle_filled(place(b), stroke.width / 2.0, stroke.color);
                }
                Head::Solid | Head::Dashed => {
                    painter.add(egui::Shape::convex_polygon(
                        vec![place(b), place(l), place(r)],
                        stroke.color,
                        stroke,
                    ));
                }
            }
        }
        Tool::Rect => {
            let pts: Vec<egui::Pos2> = rounded_corners(a, b, layer.corner)
                .into_iter()
                .map(place)
                .collect();
            fill_or_outline(painter, layer, pts, stroke);
        }
        Tool::Ellipse => {
            let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
            let (rx, ry) = ((b[0] - a[0]) / 2.0, (b[1] - a[1]) / 2.0);
            let pts: Vec<egui::Pos2> = (0..48)
                .map(|i| {
                    let t = i as f32 / 48.0 * std::f32::consts::TAU;
                    place([cx + rx * t.cos(), cy + ry * t.sin()])
                })
                .collect();
            fill_or_outline(painter, layer, pts, stroke);
        }
        _ => {}
    }
}

/// The outline of a rectangle whose corners are rounded by `r`, as a polygon.
///
/// The exporter's rounded box comes out of the distance field for free; here it
/// has to be walked, and the arc has to be sampled finely enough that the
/// stand-in and the bake do not visibly disagree at a large radius.
fn rounded_corners(a: [f32; 2], b: [f32; 2], r: f32) -> Vec<[f32; 2]> {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    let r = r.clamp(0.0, ((x1 - x0) / 2.0).min((y1 - y0) / 2.0));
    if r < 0.5 {
        return corners(a, b).to_vec();
    }
    const STEPS: usize = 8;
    let quarter = std::f32::consts::FRAC_PI_2;
    // Centre of each corner's arc, and the angle its sweep starts at, going
    // clockwise from the top left.
    let arcs = [
        ([x0 + r, y0 + r], std::f32::consts::PI),
        ([x1 - r, y0 + r], -quarter * 2.0 + quarter),
        ([x1 - r, y1 - r], 0.0),
        ([x0 + r, y1 - r], quarter),
    ];
    let mut out = Vec::with_capacity(arcs.len() * (STEPS + 1));
    for (c, from) in arcs {
        for i in 0..=STEPS {
            let t = from + quarter * i as f32 / STEPS as f32;
            out.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
        }
    }
    out
}

/// Draw a closed shape the way the exporter will: as an area when the layer is
/// filled, as a line when it is not.
///
/// A filled shape keeps its stroke — the renderer rasterises the interior and
/// the same band outside it — so the polygon is drawn with both.
fn fill_or_outline(
    painter: &egui::Painter,
    layer: &Layer,
    pts: Vec<egui::Pos2>,
    stroke: egui::Stroke,
) {
    if layer.filled {
        painter.add(egui::Shape::convex_polygon(pts, stroke.color, stroke));
    } else {
        painter.add(egui::Shape::closed_line(pts, stroke));
    }
}

/// A dashed line as a list of segments, mirroring `annotate::dashes`.
fn dashes(a: [f32; 2], b: [f32; 2], gap: f32) -> Vec<([f32; 2], [f32; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let gap = gap.max(1.0);
    if len < gap * 2.0 {
        return vec![(a, b)];
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < len {
        let to = (at + gap).min(len);
        out.push((
            [a[0] + ux * at, a[1] + uy * at],
            [a[0] + ux * to, a[1] + uy * to],
        ));
        at = to + gap;
    }
    out
}

/// How far one shot pixel travels on screen./// How far one shot pixel travels on screen.
fn screen_unit(to_screen: &dyn Fn([f32; 2]) -> egui::Pos2) -> f32 {
    (to_screen([1.0, 0.0]).x - to_screen([0.0, 0.0]).x)
        .abs()
        .max(f32::EPSILON)
}

/// The four corners of a rectangle, in order.
fn corners(a: [f32; 2], b: [f32; 2]) -> [[f32; 2]; 4] {
    [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]]
}

/// A point in the shape's upright frame, moved to where the angle puts it.
/// The inverse of [`Layer::unturn`], which hit-testing uses.
fn turn_in_shot(layer: &Layer, p: [f32; 2]) -> [f32; 2] {
    if layer.angle.abs() < 1e-4 {
        return p;
    }
    let c = layer.centre();
    let (sin, cos) = layer.angle.sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + dx * cos - dy * sin, c[1] + dx * sin + dy * cos]
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
    let rim = layer.border * screen_unit(to_screen);

    match layer.kind {
        Tool::Arrow => {
            // The bake is one filled silhouette, so the stand-in has to be one
            // too. epaint cannot fill a concave path, and the arrow is concave
            // where the head meets the shaft — so it goes down as a strip of
            // convex quads plus the head, which share edges and read as one
            // shape. The rim is the same strip drawn underneath with a stroke:
            // the seams between quads are covered by the fill on top, leaving
            // only the outer boundary showing.
            let pts: Vec<egui::Pos2> = crate::annotate::arrow_points(layer)
                .into_iter()
                .map(&to_screen)
                .collect();
            if pts.len() >= 3 {
                let c = layer.border_color;
                let rim_ink = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                if rim > 0.5 {
                    painter.add(egui::Shape::closed_line(
                        pts.clone(),
                        egui::Stroke::new(rim * 2.0, rim_ink),
                    ));
                }
                super::icons::fill_outline(painter, &pts, color);
            }
        }
        Tool::Line | Tool::Rect | Tool::Ellipse => {
            // The rim is the same shape a stroke wider, drawn underneath — the
            // exporter gets it from one distance field, so this is the closest
            // an outline-based painter can come. There is no shadow here at
            // all: egui cannot blur, and an unblurred approximation would be a
            // fifth place for the stand-in and the bake to disagree.
            if rim > 0.5 {
                let c = layer.border_color;
                let under = egui::Stroke::new(
                    width + rim * 2.0,
                    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                );
                stroke_shape(painter, layer, to_screen, under);
            }
            stroke_shape(painter, layer, to_screen, stroke)
        }
        Tool::Highlight if !layer.path.is_empty() => {
            // A pen leaves round ends and round joints, which is what the union
            // of capsules in the exporter gives. `line` has neither, so the
            // joints get a dot each.
            let pts: Vec<egui::Pos2> = layer.path.iter().map(|p| to_screen(*p)).collect();
            painter.add(egui::Shape::line(pts.clone(), stroke));
            for p in pts {
                painter.circle_filled(p, stroke.width / 2.0, color);
            }
        }
        Tool::Highlight => {
            let pts: Vec<egui::Pos2> = if layer.oval {
                let (cx, cy) = ((layer.a[0] + layer.b[0]) / 2.0, (layer.a[1] + layer.b[1]) / 2.0);
                let (rx, ry) = ((layer.b[0] - layer.a[0]) / 2.0, (layer.b[1] - layer.a[1]) / 2.0);
                (0..48)
                    .map(|i| {
                        let t = i as f32 / 48.0 * std::f32::consts::TAU;
                        to_screen(turn_in_shot(layer, [cx + rx * t.cos(), cy + ry * t.sin()]))
                    })
                    .collect()
            } else {
                corners(layer.a, layer.b)
                    .map(|p| to_screen(turn_in_shot(layer, p)))
                    .to_vec()
            };
            if rim > 0.5 {
                let c = layer.border_color;
                painter.add(egui::Shape::convex_polygon(
                    pts.clone(),
                    egui::Color32::TRANSPARENT,
                    egui::Stroke::new(
                        rim * 2.0,
                        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                    ),
                ));
            }
            // The paint tool's opacity is the layer's own alpha — a fixed
            // fraction here made every stroke pale while it was being dragged
            // and then jump solid the moment the button came up.
            painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
        }
        Tool::Blur => {
            draw_border(
                painter,
                rect,
                egui::Stroke::new(1.5_f32, egui::Color32::from_gray(230)),
            );
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(90));
        }
        Tool::Badge => {
            let r = layer.font_size * screen_unit(to_screen);
            painter.circle_filled(a, r, color);
            painter.text(
                a,
                egui::Align2::CENTER_CENTER,
                &layer.text,
                egui::FontId::new((r * 1.1).max(6.0), egui::FontFamily::Proportional),
                egui::Color32::WHITE,
            );
        }
        Tool::Text | Tool::Select | Tool::Fill => {}
    }
}

/// How solid the ghost is against the real thing.
const GHOST_ALPHA: f32 = 0.45;
/// How big the ghost is drawn, in preview points.
const GHOST_REACH: f32 = 76.0;
/// What a text ghost says. Two letters, so it shows both cases at the size the
/// label will actually be.
const GHOST_TEXT: &str = "Aa";

const TURN_ARM: f32 = 26.0;
const TURN_KNOB: f32 = 5.0;

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

/// The topmost annotation under a point, if any.
///
/// Later layers are drawn over earlier ones, so the search runs from the back:
/// clicking where two shapes overlap picks the one you can actually see.
pub(super) fn layer_at(
    painter: &egui::Painter,
    layers: &[Layer],
    at: [f32; 2],
) -> Option<usize> {
    layers
        .iter()
        .rposition(|l| contains(painter, l, at))
}

/// Is `at` inside the area this annotation can be grabbed by?
fn contains(painter: &egui::Painter, layer: &Layer, at: [f32; 2]) -> bool {
    let [x0, y0, x1, y1] = hit_bounds(painter, layer);
    at[0] >= x0 && at[0] <= x1 && at[1] >= y0 && at[1] <= y1
}

/// The rectangle an annotation can be grabbed by, in shot pixels.
///
/// Everything but a label can answer this from its own two corners. A label
/// cannot: `Layer::b` is unused for text, so `Layer::bounds` returns a square
/// of `2 × font_size` around the point that was first clicked — the *start* of
/// the string. A ten-character label at size 34 is some 200px wide and only its
/// first 34 could be clicked, which is why text was almost impossible to
/// select. So it gets measured instead.
///
/// `font_size` is already in shot pixels, so laying the text out at that size
/// gives an extent in the same units the layer is stored in — no conversion,
/// and the same font the exporter will bake.
fn hit_bounds(painter: &egui::Painter, layer: &Layer) -> [f32; 4] {
    if layer.kind != Tool::Text {
        return layer.bounds();
    }
    let size = painter
        .layout_no_wrap(
            layer.text.clone(),
            egui::FontId::new(layer.font_size.max(1.0), egui::FontFamily::Proportional),
            egui::Color32::WHITE,
        )
        .size();
    // A little slack so the very edge of a glyph is still grabbable.
    let pad = (layer.font_size * 0.3).max(4.0);
    let x0 = layer.a[0] - size.x * align_shift(layer.align);
    [x0 - pad, layer.a[1] - pad, x0 + size.x + pad, layer.a[1] + size.y + pad]
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

    /// The overlay turns points one way and hit-testing turns them back. If the
    /// two ever disagree the shape is drawn somewhere you cannot click it.
    #[test]
    fn turning_a_point_and_turning_it_back_lands_where_it_started() {
        let mut layer = Layer::new(Tool::Rect, [20.0, 40.0], [255, 0, 0, 255], 2.0, 20.0, 8.0);
        layer.b = [120.0, 90.0];
        layer.angle = 0.7;
        for p in [[20.0, 40.0], [120.0, 90.0], [0.0, 0.0], [70.0, 65.0]] {
            let back = layer.unturn(turn_in_shot(&layer, p));
            assert!(
                (back[0] - p[0]).abs() < 0.01 && (back[1] - p[1]).abs() < 0.01,
                "{p:?} came back as {back:?} — the overlay and hit-testing turn by \
                 different amounts"
            );
        }
    }

    /// The highlight used to be painted with `rect_filled`, which cannot tilt:
    /// the dashed frame turned and the yellow block underneath stayed upright.
    #[test]
    fn a_turned_highlight_is_no_longer_an_upright_rectangle() {
        let mut layer = Layer::new(Tool::Highlight, [0.0, 0.0], [255, 255, 0, 255], 2.0, 20.0, 8.0);
        layer.b = [100.0, 40.0];
        layer.angle = 0.5;
        let pts = corners(layer.a, layer.b).map(|p| turn_in_shot(&layer, p));
        assert!(
            (pts[0][1] - pts[1][1]).abs() > 1.0,
            "the top edge is still level at {pts:?}, so the fill ignores the angle"
        );
    }


    #[test]
    fn selection_maps_back_to_capture_pixels() {
        // The preview is drawn at half size, so a 100px drag is 200 real pixels.
        let img_rect = r(0.0, 0.0, 960.0, 540.0);
        let sel = r(96.0, 54.0, 480.0, 270.0);
        let got = rect_to_full_px(sel, img_rect, 1920, 1080).unwrap();
        assert_eq!(got, [192, 108, 768, 432]);
    }

    /// Clicking where two shapes overlap has to pick the one you can see —
    /// the later layer, which is drawn on top.
    #[test]
    fn the_topmost_shape_under_the_pointer_wins() {
        let mut under = Layer::new(Tool::Rect, [0.0, 0.0], [255, 0, 0, 255], 2.0, 20.0, 8.0);
        under.b = [100.0, 100.0];
        let mut over = Layer::new(Tool::Rect, [50.0, 50.0], [0, 255, 0, 255], 2.0, 20.0, 8.0);
        over.b = [150.0, 150.0];
        let layers = vec![under, over];

        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            assert_eq!(
                layer_at(&p, &layers, [75.0, 75.0]),
                Some(1),
                "picked the shape underneath, so clicking selects something invisible"
            );
            assert_eq!(layer_at(&p, &layers, [10.0, 10.0]), Some(0));
            assert_eq!(
                layer_at(&p, &layers, [400.0, 400.0]),
                None,
                "empty canvas reported a shape, so double-clicking it would not copy and close"
            );
        });
    }

    /// A label has to be grabbable along its whole length.
    ///
    /// `Layer::b` is unused for text, so `bounds` gives a square of
    /// `2 × font_size` around the point first clicked — the *start* of the
    /// string. Everything past that was unselectable, which is what "text
    /// select khó quá" meant.
    #[test]
    fn the_whole_label_can_be_grabbed_not_just_its_first_letter() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let mut label = Layer::new(Tool::Text, [100.0, 100.0], [255, 0, 0, 255], 2.0, 34.0, 8.0);
            label.text = "a fairly long caption".to_owned();

            let [x0, _, x1, y1] = hit_bounds(&painter, &label);
            let width = x1 - x0;
            assert!(
                width > 34.0 * 4.0,
                "the grab area is {width}px for a 21-character label at size 34 — \
                 back to a square round the first letter"
            );

            // The far end of the string, which used to miss entirely.
            assert!(contains(&painter, &label, [x1 - 6.0, 110.0]));
            // ...and the line's own height, not a fixed square.
            assert!(y1 > 100.0 + 34.0);
            // Well past the end is still empty canvas.
            assert!(!contains(&painter, &label, [x1 + 60.0, 110.0]));
        });
    }

    /// Measuring must not make everything else grabbable from a distance.
    #[test]
    fn a_shape_is_still_grabbed_by_its_own_area() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let mut rect = Layer::new(Tool::Rect, [10.0, 10.0], [255, 0, 0, 255], 4.0, 20.0, 8.0);
            rect.b = [90.0, 60.0];
            assert!(contains(&painter, &rect, [50.0, 35.0]), "inside the box");
            assert!(!contains(&painter, &rect, [300.0, 35.0]), "far outside it");
        });
    }

    /// The double click that copies and closes must not fire on a shape.
    ///
    /// Poking at an annotation twice is the most natural thing to try, and it
    /// used to shut the editor — which is what "I cannot select annotations"
    /// looked like from the outside.
    #[test]
    fn a_double_click_on_a_shape_is_not_the_copy_and_close_gesture() {
        let mut shape = Layer::new(Tool::Rect, [10.0, 10.0], [255, 0, 0, 255], 2.0, 20.0, 8.0);
        shape.b = [90.0, 90.0];
        let layers = vec![shape];

        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            assert!(
                layer_at(&p, &layers, [50.0, 50.0]).is_some(),
                "a double click here would close the editor instead of acting on the shape"
            );
            assert!(
                layer_at(&p, &layers, [500.0, 50.0]).is_none(),
                "double-clicking bare canvas must still copy and close"
            );
        });
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

    /// Every tool has to be outlineable, at any scale, including a shape
    /// dragged out to nothing. A zero-length arrow normalises to NaN, and the
    /// tessellator answers that with geometry sprayed across the canvas rather
    /// than with an error.
    #[test]
    fn selecting_any_shape_outlines_it_without_producing_nonsense() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for scale in [0.25_f32, 1.0, 4.0] {
                let to_screen = move |p: [f32; 2]| egui::pos2(p[0] * scale, p[1] * scale);
                for tool in std::iter::once(Tool::Select).chain(Tool::DRAWABLE) {
                    for (a, b) in [([10.0, 10.0], [90.0, 70.0]), ([40.0, 40.0], [40.0, 40.0])] {
                        let mut layer =
                            Layer::new(tool, a, [255, 0, 0, 255], 6.0, 24.0, 8.0);
                        layer.b = b;
                        layer.text = "hi".to_owned();
                        // Both routes a selection can take.
                        paint_selection(&painter, &layer, &to_screen, 1.0);
                        paint_layer_preview(&painter, &layer, &to_screen);
                    }
                }
            }
        });
    }

    /// A move is previewed on a copy; the stored annotation must not shift
    /// until the button comes up, or cancelling would be impossible and undo
    /// would have nothing coherent to restore.
    #[test]
    fn previewing_a_move_leaves_the_stored_shape_alone() {
        let mut layer = Layer::new(Tool::Rect, [10.0, 10.0], [255, 0, 0, 255], 2.0, 20.0, 8.0);
        layer.b = [50.0, 40.0];
        let preview = shifted(&layer, [100.0, 5.0]);

        assert_eq!(layer.a, [10.0, 10.0], "the real annotation moved early");
        assert_eq!(preview.a, [110.0, 15.0]);
        assert_eq!(preview.b, [150.0, 45.0], "the whole shape must travel, not just its origin");
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

    /// The one-frame blink fix rests entirely on this: a texture set *after*
    /// the shape that references it has been recorded still reaches the GPU
    /// for that same frame, because the shape carries only the id.
    ///
    /// `edit_central` re-renders after handling input for exactly that reason.
    /// If an egui upgrade ever deferred the upload by a frame, annotations
    /// would start blinking out again as the mouse is released — with no error
    /// anywhere — so the behaviour is pinned here.
    #[test]
    fn a_texture_set_after_it_is_painted_still_lands_this_frame() {
        let ctx = egui::Context::default();
        let mut tex = ctx.load_texture(
            "probe",
            egui::ColorImage::filled([2, 2], egui::Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        // Let the frame that allocates it go by, so what is measured below is
        // the update and not the creation.
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});

        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(8.0, 8.0));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
            // Only now, after the shape above has been recorded.
            tex.set(
                egui::ColorImage::filled([2, 2], egui::Color32::BLUE),
                egui::TextureOptions::LINEAR,
            );
        });

        assert!(
            out.textures_delta.set.iter().any(|(id, _)| *id == tex.id()),
            "a texture updated after being painted no longer reaches the same frame; \
             annotations will blink for one frame when the mouse comes up"
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
