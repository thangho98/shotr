//! The editor's own window: a borderless frame with an overhanging sidebar.
//!
//! There is no system titlebar here. The window is transparent, we draw a
//! rounded frame inside it, and the sidebar is a separate card that stands
//! *proud* of that frame — taller than it, overlapping its left edge, carrying
//! its own shadow and the window controls. The picture tucks in behind.
//!
//! egui has no z-order between panels, so none of this can be built out of
//! `SidePanel` + `CentralPanel`: it is one painter and a handful of explicit
//! rectangles, painted back to front.
//!
//! Owning the chrome means owning the behaviour that came with it — dragging,
//! resizing, maximising and closing are all implemented here, because with
//! decorations off nothing else will do them.

use eframe::egui;

use super::icons::Glyph;
use super::theme;
use super::{Mode, SIDEBAR_W, ShotrApp, Zoom};
use crate::annotate::Tool;
use crate::export;
use crate::i18n::{t, tf};

/// The tools the floating pill offers, in order, each with the key that
/// selects it. Select is last and sits behind a separator: it is the tool you
/// return to, not one of the six you draw with.
///
/// It gets the key *left of* `1` rather than `7` for exactly that reason —
/// it is reached far more often than any single drawing tool, and `7` is a
/// stretch from the home position.
///
/// One list, used by the pill, by the keyboard handler and by the shortcut
/// list in Preferences — so a tool cannot gain a button without gaining a key.
pub(super) const TOOLS: [(Tool, char); 7] = [
    (Tool::Arrow, '1'),
    (Tool::Text, '2'),
    (Tool::Rect, '3'),
    (Tool::Ellipse, '4'),
    (Tool::Blur, '5'),
    (Tool::Highlight, '6'),
    (Tool::Select, '`'),
];

/// Width of the option controls in the pill, from the design.
const OPT_COLOR_W: f32 = 28.0;
const OPT_SLIDER_W: f32 = 92.0;
const SEP_W: f32 = 1.0;
/// Clearance the pill needs above the picture, so a shot fitted to the canvas
/// never slides under it.
const PILL_CLEARANCE: f32 = 86.0;
const PICTURE_PAD: f32 = 26.0;
/// How thick the invisible grab band along each window edge is.
const RESIZE_BAND: f32 = 6.0;
/// How far in from a corner the diagonal grab starts.
const RESIZE_CORNER: f32 = 16.0;

/// Where each piece of the shell landed this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Shell {
    /// The frame the user sees. Everything outside it is the transparent
    /// margin the shadows fall into.
    pub window: egui::Rect,
    /// The card. Taller than the window and overlapping its left edge.
    pub sidebar: egui::Rect,
    /// The three bars below run *under* the card's left overhang, so their
    /// fills and rules have no seam against it. Anything that has to be seen
    /// goes through [`Shell::clear_of_card`] first.
    pub topbar: egui::Rect,
    pub canvas: egui::Rect,
    pub status: egui::Rect,
}

impl Shell {
    /// The part of a bar the card is not sitting on top of.
    pub fn clear_of_card(&self, rect: egui::Rect) -> egui::Rect {
        let mut r = rect;
        r.min.x = self.sidebar.max.x.min(r.max.x);
        r
    }
}

/// Divide the window up. Pure, so the geometry that the whole shell rests on
/// can be checked without a display.
pub(super) fn layout(available: egui::Rect) -> Shell {
    let window = available.shrink(theme::SHELL_MARGIN);
    let sidebar = egui::Rect::from_min_size(
        window.left_top() - egui::vec2(0.0, theme::OVERHANG_V),
        egui::vec2(
            SIDEBAR_W.min(window.width()),
            window.height() + theme::OVERHANG_V * 2.0,
        ),
    );

    // The bars run under the card by exactly the horizontal overhang, so the
    // canvas fill meets the card with no seam even where its rounded corners
    // curve away from the edge.
    let mut content = window;
    content.min.x = (sidebar.max.x - theme::OVERHANG_H).min(window.max.x);

    let topbar = egui::Rect::from_min_size(
        content.left_top(),
        egui::vec2(content.width(), theme::TOPBAR_H),
    );
    let status = egui::Rect::from_min_size(
        egui::pos2(content.min.x, content.max.y - theme::STATUSBAR_H),
        egui::vec2(content.width(), theme::STATUSBAR_H),
    );
    let canvas = egui::Rect::from_min_max(
        egui::pos2(content.min.x, topbar.max.y),
        egui::pos2(content.max.x, status.min.y.max(topbar.max.y)),
    );

    Shell {
        window,
        sidebar,
        topbar,
        canvas,
        status,
    }
}

impl ShotrApp {
    /// Draw the whole window: frame, picture column, sidebar card, and the
    /// grab bands that stand in for the titlebar and the resize border.
    pub(super) fn shell_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let shell = layout(ui.max_rect());
        if shell.window.width() < 40.0 || shell.window.height() < 40.0 {
            return;
        }
        theme::window_frame(ui.painter(), shell.window);
        ui.painter().rect_filled(shell.canvas, 0.0, theme::pal().canvas);

        // Back to front: the frame's content first, then the card that overlaps
        // it, then the invisible grab bands on top of both.
        self.top_bar(ui, ctx, shell);
        self.canvas_column(ui, ctx, shell);
        self.status_bar(ui, shell);

        if self.sidebar_grad.is_none() {
            self.sidebar_grad = Some(theme::sidebar_gradient(ctx));
        }
        theme::card_surface(
            ui.painter(),
            shell.sidebar,
            theme::SIDEBAR_RADIUS,
            self.sidebar_grad.as_ref(),
        );
        self.sidebar_column(ui, shell);

        // The card overhangs the top and bottom left corners, so the horizontal
        // bands start past it — its own top edge is where the window controls
        // live, and a resize band there would swallow them.
        resize_bands(ui, shell.window, shell.sidebar.max.x, true);
    }

    // ------------------------------------------------------------- the frame

    /// A band that moves the window, and maximises it on a double click.
    ///
    /// Allocated *before* the buttons that sit on it: within one layer egui
    /// gives a click to the last widget that claimed the spot, so the handle
    /// has to go down first or it would swallow every button on the strip.
    fn drag_band(&mut self, ui: &mut egui::Ui, rect: egui::Rect, salt: &str) {
        let resp = ui.interact(
            rect,
            ui.id().with(("drag", salt)),
            egui::Sense::click_and_drag(),
        );
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            self.toggle_maximised(ui.ctx());
        }
    }

    /// Flip between maximised and restored.
    ///
    /// The flag has to be mirrored rather than read fresh each time: not every
    /// compositor reports `maximized` at all, and a `None` read as `false`
    /// sends `Maximized(true)` forever — the window would go up on the first
    /// double click and never come back down.
    fn toggle_maximised(&mut self, ctx: &egui::Context) {
        if let Some(actual) = ctx.input(|i| i.viewport().maximized) {
            self.maximised = actual;
        }
        self.maximised = !self.maximised;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximised));
    }
}

/// The grab bands along the window edges.
///
/// Placed last, so they are on top of everything and a drag near the edge
/// resizes rather than draws.
///
/// `inner_x` is where the top and bottom bands start, and `west` whether the
/// left edge gets one at all. Both windows that draw their own chrome have a
/// card in the way of those edges, and the card wants the clicks: in the editor
/// it overhangs the top and bottom left corners, and in Preferences it covers
/// the left edge outright.
pub(crate) fn resize_bands(ui: &mut egui::Ui, w: egui::Rect, inner_x: f32, west: bool) {
    use egui::ResizeDirection as D;
    let inner_x = inner_x.min(w.max.x - RESIZE_CORNER);

    let bands: [(egui::Rect, D, egui::CursorIcon); 6] = [
        (
            egui::Rect::from_min_max(
                egui::pos2(w.min.x, w.min.y + RESIZE_CORNER),
                egui::pos2(
                    if west { w.min.x + RESIZE_BAND } else { w.min.x },
                    w.max.y - RESIZE_CORNER,
                ),
            ),
            D::West,
            egui::CursorIcon::ResizeWest,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(w.max.x - RESIZE_BAND, w.min.y + RESIZE_CORNER),
                egui::pos2(w.max.x, w.max.y - RESIZE_CORNER),
            ),
            D::East,
            egui::CursorIcon::ResizeEast,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(inner_x, w.min.y),
                egui::pos2(w.max.x - RESIZE_CORNER, w.min.y + RESIZE_BAND),
            ),
            D::North,
            egui::CursorIcon::ResizeNorth,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(inner_x, w.max.y - RESIZE_BAND),
                egui::pos2(w.max.x - RESIZE_CORNER, w.max.y),
            ),
            D::South,
            egui::CursorIcon::ResizeSouth,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(w.max.x - RESIZE_CORNER, w.max.y - RESIZE_CORNER),
                w.max,
            ),
            D::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(w.max.x - RESIZE_CORNER, w.min.y),
                egui::pos2(w.max.x, w.min.y + RESIZE_CORNER),
            ),
            D::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        ),
    ];

    for (i, (rect, dir, cursor)) in bands.into_iter().enumerate() {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        let resp = ui.interact(rect, ui.id().with(("resize", i)), egui::Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if resp.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

impl ShotrApp {
    // ----------------------------------------------------------- the sidebar

    fn sidebar_column(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let card = shell.sidebar;
        let strip = egui::Rect::from_min_size(
            card.left_top(),
            egui::vec2(card.width(), theme::STRIP_H),
        );
        self.drag_band(ui, strip, "strip");
        self.window_controls(ui, strip);

        // 12 px of side padding, and enough at the bottom that the last control
        // never runs into the card's rounded corner.
        let body = egui::Rect::from_min_max(
            egui::pos2(card.min.x + 12.0, strip.max.y),
            card.max - egui::vec2(12.0, 14.0),
        );
        if body.width() < 40.0 {
            return;
        }
        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("sidebar")
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(body);
        self.sidebar(&mut ui);
    }

    /// The three window buttons and the wordmark.
    ///
    /// macOS gets Apple's lights on the left; everywhere else gets that
    /// platform's own controls on the right. Shipping the lights on Windows
    /// would be one desktop's chrome pasted onto another.
    #[cfg(target_os = "macos")]
    fn window_controls(&mut self, ui: &mut egui::Ui, strip: egui::Rect) {
        let mut x = strip.min.x + 14.0 + theme::LIGHT_D / 2.0;
        for (i, colour) in theme::LIGHTS.into_iter().enumerate() {
            let centre = egui::pos2(x, strip.center().y);
            let rect = egui::Rect::from_center_size(
                centre,
                egui::Vec2::splat(theme::LIGHT_D + 4.0),
            );
            let resp = ui.interact(rect, ui.id().with(("light", i)), egui::Sense::click());
            let painter = ui.painter();
            painter.circle_filled(centre, theme::LIGHT_D / 2.0, colour);
            // The inset dark ring in the design: without it the lights glow
            // against the card instead of sitting in it.
            painter.circle_stroke(
                centre,
                theme::LIGHT_D / 2.0 - 0.25,
                egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(64)),
            );
            if resp.clicked() {
                self.window_button(ui.ctx(), i);
            }
            x += theme::LIGHT_D + theme::LIGHT_GAP;
        }
        wordmark(ui, strip, egui::Align2::RIGHT_CENTER);
    }

    #[cfg(not(target_os = "macos"))]
    fn window_controls(&mut self, ui: &mut egui::Ui, strip: egui::Rect) {
        wordmark(ui, strip, egui::Align2::LEFT_CENTER);
        // Minimise, maximise, close — the order Windows puts them in, laid out
        // from the right so close ends up outermost.
        let mut x = strip.max.x - 12.0;
        for (which, glyph) in [
            (0_usize, super::icons::Glyph::Close),
            (2, super::icons::Glyph::Maximise),
            (1, super::icons::Glyph::Minimise),
        ] {
            let rect = egui::Rect::from_center_size(
                egui::pos2(x - 11.0, strip.center().y),
                egui::Vec2::splat(22.0),
            );
            let resp = ui.interact(rect, ui.id().with(("ctl", which)), egui::Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 5.0, egui::Color32::from_white_alpha(20));
            }
            super::icons::draw_glyph(ui.painter(), rect, glyph, theme::pal().text);
            if resp.clicked() {
                self.window_button(ui.ctx(), which);
            }
            x -= 26.0;
        }
    }

    /// Close, minimise, maximise — in that order, which is the order the lights
    /// are drawn in.
    fn window_button(&mut self, ctx: &egui::Context, which: usize) {
        match which {
            0 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            1 => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            _ => self.toggle_maximised(ctx),
        }
    }

    // ------------------------------------------------------------ the top bar

    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, shell: Shell) {
        let bar = shell.topbar;
        self.drag_band(ui, shell.clear_of_card(bar), "topbar");
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(bar.min.x, bar.max.y - 1.0),
                egui::vec2(bar.width(), 1.0),
            ),
            0.0,
            theme::pal().line,
        );

        let inner = shell.clear_of_card(bar).shrink2(egui::vec2(14.0, 9.0));
        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("topbar")
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.set_clip_rect(bar);

        if self.mode != Mode::Edit {
            // Opened from the tray with nothing captured, this screen is a hub
            // and wants a title. Reached by going back from the editor it is a
            // region picker, and the instructions are on the status bar.
            if self.hub {
                ui.label(
                    egui::RichText::new(t("Open a shot"))
                        .size(12.0)
                        .color(theme::pal().text_dim),
                );
            }
            return;
        }

        let name = export::default_path(&self.prefs)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        ui.label(egui::RichText::new(name).size(12.0).color(theme::pal().text_dim));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self.more_menu(ui);
            if ui
                .button(format!("{}  {}+S", t("Save"), super::MOD_LABEL))
                .clicked()
            {
                self.do_save(None);
            }
            // Labelled with the shortcut, so it has to do what the shortcut
            // does — copy and leave.
            if ui
                .button(format!("{}  {}+C", t("Copy"), super::MOD_LABEL))
                .clicked()
            {
                self.copy_and_close(ctx);
            }
            // No shortcut on the label, because the pin's own keys belong to the
            // pin window and the one that starts a pin is global, bound in
            // Preferences. Like Copy, this leaves: the pin *is* the shot now, and
            // an editor left open behind it is a second copy of the same picture.
            if ui
                .button(t("Pin"))
                .on_hover_text(t("Keep this shot floating above other windows"))
                .clicked()
            {
                self.pin_and_close(ctx);
            }
            separator(ui);
            if glyph_button(ui, Glyph::Redo, self.undo.can_redo())
                .on_hover_text(t("Redo"))
                .clicked()
            {
                self.redo_annotation();
            }
            if glyph_button(ui, Glyph::Undo, self.undo.can_undo())
                .on_hover_text(t("Undo"))
                .clicked()
            {
                self.undo_annotation();
            }
        });
    }

    /// Everything that does not earn a button of its own.
    fn more_menu(&mut self, ui: &mut egui::Ui) {
        let resp = glyph_button(ui, Glyph::More, true).on_hover_text(t("More…"));
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(210.0);
            if ui.button(t("Save As…")).clicked() {
                if let Some(p) = export::save_as_dialog(&self.prefs) {
                    self.do_save(Some(p));
                }
                ui.close();
            }
            if ui.button(t("Open image folder")).clicked() {
                self.open_output_dir();
                ui.close();
            }
            // Printed with its keys, the way the top bar prints Copy's and
            // Save's: a menu is where a shortcut is found, and this one has no
            // button of its own to carry it.
            let plain = format!(
                "{}  {}+Shift+C",
                t("Copy the shot as captured"),
                super::MOD_LABEL
            );
            if ui.button(plain).clicked() {
                self.copy_as_captured();
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
            // Back to the windowed Select screen — the picker over the shot
            // just taken, on the platforms that have one. macOS never passes
            // through it: Apple's overlay hands back a finished region.
            if ui.button(t("Back to selection")).clicked() {
                self.mode = Mode::Select;
                self.sel_start = None;
                self.sel_rect = None;
                self.crop_px = None;
                self.status.clear();
                ui.close();
            }
            ui.separator();
            let has_layer = self.selected_layer.is_some();
            if ui
                .add_enabled(has_layer, egui::Button::new(t("Delete layer")))
                .clicked()
            {
                self.delete_selected_layer();
                ui.close();
            }
            let has_any = !self.layers.is_empty();
            if ui
                .add_enabled(has_any, egui::Button::new(t("Clear all annotations")))
                .clicked()
            {
                self.undo.push(&self.layers);
                self.layers.clear();
                self.selected_layer = None;
                self.dirty = true;
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

    // ------------------------------------------------------------- the canvas

    fn canvas_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, shell: Shell) {
        // The fill runs under the card; the picture and the pill do not.
        let canvas = shell.clear_of_card(shell.canvas);
        if canvas.height() < 20.0 {
            return;
        }
        // The picture keeps clear of the pill above it, and of the frame's own
        // edges — the design pads it 92/28/28. The Select screen has no pill,
        // so reserving the clearance there would only push the shot down.
        let top = if self.mode == Mode::Edit {
            PILL_CLEARANCE
        } else {
            PICTURE_PAD
        };
        let picture = egui::Rect::from_min_max(
            egui::pos2(canvas.min.x + PICTURE_PAD, canvas.min.y + top),
            egui::pos2(canvas.max.x - PICTURE_PAD, canvas.max.y - PICTURE_PAD),
        );
        if picture.width() > 20.0 && picture.height() > 20.0 {
            let mut inner = ui.new_child(egui::UiBuilder::new().id_salt("picture").max_rect(picture));
            inner.set_clip_rect(canvas);
            match self.mode {
                Mode::Select => self.select_central(&mut inner),
                Mode::Edit => self.edit_central(&mut inner, ctx),
            }
        }

        if self.mode == Mode::Edit {
            self.tool_pill(ui, canvas);
        }
    }

    /// The floating tool bar: seven tools with their digits printed on them,
    /// then whatever the selected tool can be adjusted by.
    ///
    /// Its width is worked out up front rather than measured, because the
    /// backdrop has to be painted before the widgets that sit on it, and a
    /// width that disagreed with the contents would show as everything sitting
    /// slightly off to one side.
    fn tool_pill(&mut self, ui: &mut egui::Ui, canvas: egui::Rect) {
        let drawing = self.tool != Tool::Select;
        // Select's "option" is the one thing you can do to a chosen shape.
        // Nothing else in the window says a shape is selected, or that
        // deleting one is possible at all.
        let can_delete = !drawing && self.selected_layer.is_some();

        // Hidden on the *widest* form, not the current one: a bar that appeared
        // and vanished as the tool changed would be worse than a cramped one.
        if pill_width(true, false) > canvas.width() - 16.0 {
            return;
        }
        let rect = pill_rect(canvas, drawing, can_delete);
        theme::glass(ui.painter(), rect);

        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("pill")
                .max_rect(rect.shrink(theme::PILL_PAD))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.spacing_mut().item_spacing = egui::vec2(theme::PILL_GAP, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(OPT_COLOR_W, 26.0);
        ui.spacing_mut().slider_width = OPT_SLIDER_W;

        for (tool, key) in TOOLS {
            if tool == Tool::Select {
                separator(&mut ui);
            }
            if super::icons::tool_button(&mut ui, tool, self.tool == tool, Some(key)).clicked() {
                self.finish_text_edit();
                self.tool = tool;
            }
        }
        if drawing {
            separator(&mut ui);
            self.tool_options(&mut ui);
        } else if can_delete {
            separator(&mut ui);
            if super::icons::glyph_button(&mut ui, Glyph::Trash, true, egui::vec2(OPT_COLOR_W, 26.0))
                .on_hover_text(t("Delete the selected shape  ⌫"))
                .clicked()
            {
                self.delete_selected_layer();
            }
        }
    }

    /// The one colour and the one slider that belong to the selected tool.
    fn tool_options(&mut self, ui: &mut egui::Ui) {
        let before = (
            self.annot_color,
            self.annot_stroke,
            self.annot_font_size,
            self.annot_blur,
            self.annot_paint_alpha,
        );

        super::sidebar::color_button(ui, &mut self.annot_color, 24.0).on_hover_text(t("Colour"));
        // The slider shows no number — there is no room for one in the pill —
        // so the tooltip has to, or the only way to read the current value is
        // to change it and watch the picture.
        match self.tool {
            Tool::Text => {
                ui.add(egui::Slider::new(&mut self.annot_font_size, 10.0..=160.0).show_value(false))
                    .on_hover_text(dialled(t("Font size"), self.annot_font_size));
            }
            Tool::Blur => {
                ui.add(egui::Slider::new(&mut self.annot_blur, 2.0..=60.0).show_value(false))
                    .on_hover_text(dialled(t("Blur amount"), self.annot_blur));
            }
            Tool::Highlight => {
                let mut pct = (self.annot_paint_alpha as f32 / 255.0 * 100.0).round() as u8;
                let hover = format!(
                    "{}\n{}",
                    dialled(t("Paint opacity"), pct as f32),
                    t("100% covers completely; lower is translucent, like a highlighter.")
                );
                if ui
                    .add(egui::Slider::new(&mut pct, 5..=100).show_value(false))
                    .on_hover_text(hover)
                    .changed()
                {
                    self.annot_paint_alpha = (pct as f32 / 100.0 * 255.0).round() as u8;
                }
            }
            _ => {
                ui.add(egui::Slider::new(&mut self.annot_stroke, 1.0..=40.0).show_value(false))
                    .on_hover_text(dialled(t("Stroke"), self.annot_stroke));
            }
        }

        // Dragging a slider after drawing has to change the shape you just
        // drew, not only the next one — otherwise the controls look broken.
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
            let ink = self.ink(kind);
            if let Some(layer) = self.layers.get_mut(i) {
                layer.color = ink;
                layer.stroke = self.annot_stroke;
                layer.font_size = self.annot_font_size;
                layer.blur = self.annot_blur;
                self.dirty = true;
            }
        }
    }

    // --------------------------------------------------------- the status bar

    fn status_bar(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let bar = shell.status;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(bar.left_top(), egui::vec2(bar.width(), 1.0)),
            0.0,
            theme::pal().line,
        );

        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("status")
                .max_rect(shell.clear_of_card(bar).shrink2(egui::vec2(14.0, 6.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.set_clip_rect(bar);
        ui.spacing_mut().item_spacing.x = 12.0;

        if self.mode == Mode::Edit {
            self.zoom_text(&mut ui);
        }

        let now = ui.ctx().input(|i| i.time);
        let fresh = !self.status.is_empty() && now < self.status_until;
        if fresh {
            // Come back when it expires, or the message stays until something
            // else happens to ask for a frame.
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(
                (self.status_until - now).max(0.05),
            ));
        }
        let message = if fresh { self.status.clone() } else { self.hint() };
        ui.label(
            egui::RichText::new(message)
                .size(11.0)
                .color(theme::pal().text_dim),
        );
    }

    /// What the status bar says when it has nothing more urgent to report.
    ///
    /// The old sidebar carried a paragraph of help under every tool. There is
    /// no room for that here, and most of it was noise — but the two lines that
    /// documented gestures with no other affordance (click-to-type, and
    /// double-click-to-copy) had nowhere else to go, so they surface here for
    /// exactly the tool they belong to.
    fn hint(&self) -> String {
        if self.mode != Mode::Edit {
            return String::new();
        }
        match self.tool {
            Tool::Text => t(
                "Click the image and type. Enter to finish, Esc to cancel. Click existing text to edit it.",
            ),
            // Naming the copy-and-close gesture here was a mistake: it told
            // the one tool that selects, moves and deletes to advertise the
            // gesture that shuts the window instead. Reported as annotations
            // not being selectable at all.
            Tool::Select => t("Click a shape to select it · drag to move · Backspace deletes"),
            _ => t("` and 1–6 pick a tool · Esc returns to Select"),
        }
        .to_owned()
    }

    /// Zoom, as plain text rather than a button.
    ///
    /// Reviewed twice and turned down both times as a control with chrome: the
    /// status bar is not a place for a widget that competes with the picture.
    /// It brightens and shows its caret on hover, which is enough to read as
    /// clickable.
    fn zoom_text(&mut self, ui: &mut egui::Ui) {
        let label = match self.zoom {
            Zoom::Fit => tf("Fit · {p}%", &[("p", &self.shown_zoom.to_string())]),
            Zoom::Percent(p) => format!("{p}%"),
        };
        let font = egui::FontId::proportional(12.0);
        let text_w = ui
            .painter()
            .layout_no_wrap(label.clone(), font.clone(), theme::pal().text)
            .size()
            .x;

        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(text_w + 13.0, 20.0), egui::Sense::click());
        let hot = resp.hovered();
        let ink = if hot { theme::pal().text } else { theme::pal().text_dim };
        let painter = ui.painter();
        painter.text(rect.left_center(), egui::Align2::LEFT_CENTER, label, font, ink);
        let c = egui::pos2(rect.max.x - 4.0, rect.center().y);
        let caret = if hot {
            ink
        } else {
            ink.gamma_multiply(0.45)
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x - 3.5, c.y - 1.5),
                egui::pos2(c.x + 3.5, c.y - 1.5),
                egui::pos2(c.x, c.y + 2.5),
            ],
            caret,
            egui::Stroke::NONE,
        ));

        egui::Popup::menu(&resp).show(|ui| {
            if ui.selectable_label(self.zoom == Zoom::Fit, t("Fit")).clicked() {
                self.set_zoom(Zoom::Fit);
                ui.close();
            }
            for p in Zoom::STEPS {
                let on = self.zoom == Zoom::Percent(p);
                if ui.selectable_label(on, format!("{p}%")).clicked() {
                    self.set_zoom(Zoom::Percent(p));
                    ui.close();
                }
            }
        });
    }
}

/// The wordmark on the sidebar strip.
fn wordmark(ui: &mut egui::Ui, strip: egui::Rect, align: egui::Align2) {
    let at = match align {
        egui::Align2::RIGHT_CENTER => strip.right_center() - egui::vec2(14.0, 0.0),
        _ => strip.left_center() + egui::vec2(14.0, 0.0),
    };
    ui.painter().text(
        at,
        align,
        "shotr",
        egui::FontId::proportional(14.0),
        theme::pal().text,
    );
}

/// A control's name and where it is currently set. Concatenated rather than
/// run through `tf`: there is no sentence here for word order to scramble.
fn dialled(name: &str, value: f32) -> String {
    format!("{name}  {}", value.round() as i32)
}

/// A chrome glyph as a button, sized to sit in a 44 px bar.
fn glyph_button(ui: &mut egui::Ui, glyph: Glyph, enabled: bool) -> egui::Response {
    super::icons::glyph_button(ui, glyph, enabled, egui::vec2(26.0, 24.0))
}

/// Where the tool pill's backdrop goes: centred on the canvas, at the width it
/// is actually about to be drawn at.
///
/// It used to be anchored on the *widest* form the pill can take, so the seven
/// tool buttons never moved when Select dropped the options group. That kept the
/// buttons still and left the bar 72px left of centre in the **resting** state —
/// Select, nothing chosen — which is the state the editor sits in and therefore
/// the one you look at. Reported as the toolbar not being centred in the frame,
/// and it was. The old cost is back with it: switching between Select and a
/// drawing tool shifts the bar by half the options group.
///
/// Pure, and the same function the painter uses, so the test that says "centred"
/// is testing the placement rather than restating the arithmetic.
fn pill_rect(canvas: egui::Rect, drawing: bool, can_delete: bool) -> egui::Rect {
    let width = pill_width(drawing, can_delete);
    let height = super::icons::BUTTON + theme::PILL_PAD * 2.0;
    egui::Rect::from_min_size(
        egui::pos2(canvas.center().x - width / 2.0, canvas.min.y + 14.0),
        egui::vec2(width, height),
    )
}

/// How wide the tool pill needs to be for what it is about to hold: six drawing
/// tools, a separator and Select, then whatever options the current tool has.
///
/// One function, because the backdrop is painted before the widgets that sit on
/// it: the width that places the glass and the width its contents take have to
/// come from the same place, or the bar sits off to one side of its own buttons.
fn pill_width(drawing: bool, can_delete: bool) -> f32 {
    let tools = super::icons::BUTTON * 7.0 + SEP_W + theme::PILL_GAP * 7.0 + theme::PILL_PAD * 2.0;
    if drawing {
        tools + SEP_W + OPT_COLOR_W + OPT_SLIDER_W + theme::PILL_GAP * 3.0
    } else if can_delete {
        tools + SEP_W + OPT_COLOR_W + theme::PILL_GAP * 2.0
    } else {
        tools
    }
}

/// A hairline between groups in a horizontal bar.
fn separator(ui: &mut egui::Ui) {
    let h = (ui.available_height() - 6.0).max(8.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SEP_W, h), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::pal().line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pill is centred in the canvas at every width it can take.
    ///
    /// It was anchored on the *widest* form it could take while being drawn at its
    /// current one, so with the options group absent — Select, nothing chosen,
    /// which is the state the editor rests in — the bar sat 72 px left of centre.
    /// Reported as the toolbar not being centred in the frame.
    ///
    /// Asserted on `pill_rect` — the function the painter itself calls — so this
    /// checks the placement rather than restating the arithmetic. Anchoring on any
    /// width other than the one being drawn fails it.
    #[test]
    fn the_tool_pill_is_centred_whatever_it_is_holding() {
        // A canvas that starts well right of the window's left edge, as the real
        // one does: the card takes the left of the window, so a pill centred on
        // the *window* instead would pass a symmetric test and fail this one.
        let canvas = egui::Rect::from_min_max(egui::pos2(336.0, 60.0), egui::pos2(1280.0, 800.0));
        for (drawing, can_delete, what) in [
            (true, false, "a drawing tool, with its options"),
            (false, true, "Select with a shape chosen, so Delete shows"),
            (false, false, "Select with nothing chosen — the resting state"),
        ] {
            let rect = pill_rect(canvas, drawing, can_delete);
            assert!(
                (rect.center().x - canvas.center().x).abs() < 0.01,
                "{what}: the pill's centre is {}, the canvas's is {}",
                rect.center().x,
                canvas.center().x
            );
            assert!(
                rect.width() > 0.0 && rect.height() > 0.0,
                "{what}: the pill has no area"
            );
        }
    }

    /// Every form of the pill has to be narrower than the one the hide test uses,
    /// or the bar can vanish for a tool that would in fact have fitted.
    #[test]
    fn the_widest_pill_really_is_the_widest() {
        let widest = pill_width(true, false);
        for (drawing, can_delete) in [(false, true), (false, false)] {
            assert!(
                pill_width(drawing, can_delete) <= widest,
                "drawing={drawing} can_delete={can_delete} is wider than the form used to hide it"
            );
        }
    }

    fn window(w: f32, h: f32) -> Shell {
        layout(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(w, h),
        ))
    }

    /// The whole point of the shell: the card is taller than the frame and
    /// hangs off it top and bottom. If it ever matched the frame's height the
    /// design would collapse into an ordinary sidebar.
    #[test]
    fn the_sidebar_card_stands_proud_of_the_window() {
        let s = window(1280.0, 820.0);
        assert!(
            s.sidebar.min.y < s.window.min.y,
            "the card no longer overhangs the top"
        );
        assert!(
            s.sidebar.max.y > s.window.max.y,
            "the card no longer overhangs the bottom"
        );
        assert_eq!(
            s.sidebar.height() - s.window.height(),
            theme::OVERHANG_V * 2.0,
            "the overhang is what makes the card read as a separate object"
        );
    }

    /// The card overlaps the frame's left edge, but the picture must never be
    /// underneath it — that is what the 20 px of padding buys.
    #[test]
    fn the_picture_column_starts_clear_of_the_card() {
        let s = window(1280.0, 820.0);
        assert_eq!(
            s.sidebar.min.x, s.window.min.x,
            "the card must overlap the frame, not sit beside it"
        );
        assert_eq!(
            s.sidebar.max.x - s.canvas.min.x,
            theme::OVERHANG_H,
            "the canvas fill must tuck under the card, or its rounded corners show a seam"
        );
        assert_eq!(
            s.clear_of_card(s.canvas).min.x,
            s.sidebar.max.x,
            "a shot drawn under the card would be clipped by it"
        );
    }

    /// Top bar, canvas and status bar have to tile the frame exactly. A gap
    /// shows as a stripe of the wrong colour; an overlap hides a control.
    #[test]
    fn the_picture_column_tiles_without_a_gap() {
        for (w, h) in [(1280.0, 820.0), (900.0, 560.0), (2560.0, 1400.0)] {
            let s = window(w, h);
            assert_eq!(s.topbar.max.y, s.canvas.min.y, "{w}x{h}: gap under the top bar");
            assert_eq!(s.canvas.max.y, s.status.min.y, "{w}x{h}: gap above the status bar");
            assert_eq!(s.topbar.min.y, s.window.min.y, "{w}x{h}: top bar floats");
            assert_eq!(s.status.max.y, s.window.max.y, "{w}x{h}: status bar floats");
        }
    }

    /// The card hangs off the frame, but it must still fit inside the window
    /// the OS gave us — outside that, the compositor clips it away silently and
    /// the overhang simply disappears at the top and bottom.
    #[test]
    fn the_card_stays_inside_the_window_the_os_gave_us() {
        for (w, h) in [(1320.0, 860.0), (900.0, 560.0), (400.0, 300.0)] {
            let available =
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h));
            let s = layout(available);
            assert!(
                available.contains_rect(s.sidebar),
                "{w}x{h}: the card is clipped by the real window: {:?}",
                s.sidebar
            );
        }
    }

    /// A window dragged down to nothing must not produce inverted rectangles —
    /// egui will happily paint those, and the result is unreadable rather than
    /// merely small.
    #[test]
    fn a_tiny_window_produces_no_inverted_rectangles() {
        for (w, h) in [(120.0_f32, 90.0_f32), (60.0, 60.0), (300.0, 200.0)] {
            let s = window(w, h);
            for (name, r) in [
                ("window", s.window),
                ("sidebar", s.sidebar),
                ("topbar", s.topbar),
                ("canvas", s.canvas),
                ("status", s.status),
            ] {
                assert!(
                    r.min.x <= r.max.x && r.min.y <= r.max.y,
                    "{w}x{h}: {name} came out inside out: {r:?}"
                );
            }
        }
    }

    /// Every tool the pill offers needs a key, and no key may be used twice —
    /// a duplicate would silently make one tool unreachable.
    #[test]
    fn every_tool_has_its_own_key() {
        let mut keys: Vec<char> = TOOLS.iter().map(|(_, k)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), TOOLS.len(), "two tools share a key");
    }

    /// The six drawing tools sit on `1`–`6` in the order they are shown, and
    /// Select takes the key beside them rather than the far end of the row.
    #[test]
    fn the_drawing_tools_run_1_to_6_and_select_sits_next_to_them() {
        let drawing: Vec<char> = TOOLS[..6].iter().map(|(_, k)| *k).collect();
        assert_eq!(drawing, vec!['1', '2', '3', '4', '5', '6']);
        assert_eq!(
            TOOLS[6],
            (Tool::Select, '`'),
            "Select must stay on the key left of 1 — it is reached far more \
             often than any one drawing tool"
        );
    }

    /// The pill's order is part of the design: the six drawing tools, then
    /// Select on its own behind a separator.
    #[test]
    fn select_is_the_last_tool_in_the_pill() {
        assert_eq!(TOOLS[6].0, Tool::Select, "Select moved out of last place");
        assert!(
            TOOLS[..6].iter().all(|(t, _)| *t != Tool::Select),
            "Select appears twice"
        );
    }
}
