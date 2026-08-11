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
use super::{Mode, SIDEBAR_W, ShotrApp, Zoom, controls};
use crate::annotate::{Cover, Head, Layer, TextAlign, Tool};
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

const SEP_W: f32 = 1.0;
/// The options row, and the space around it inside the capsule.
const OPT_ROW_H: f32 = 38.0;
const OPT_PAD_TOP: f32 = 5.0;
const OPT_PAD_X: f32 = 6.0;
/// Between the tool row and the hairline under it.
const HAIRLINE_GAP: f32 = 5.0;

/// The arrow heads, in the order the row offers them.
const HEADS: [(Head, &str); 3] = [
    (Head::Solid, "Solid head"),
    (Head::Open, "Open head"),
    (Head::Dashed, "Dashed"),
];

/// The two ways of hiding what is under a redaction.
const COVERS: [(Cover, &str); 2] = [(Cover::Blur, "Blur"), (Cover::Pixelate, "Pixelate")];

/// Moving a shape through the stack. `1` is towards the viewer.
const ORDERING: [(isize, &str); 2] = [(1, "Bring forward"), (-1, "Send back")];

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
        // The row shows for every drawing tool. Select has one only when a
        // shape is chosen — with nothing selected there is nothing to reorder,
        // duplicate or delete, and a row of dead buttons is worse than none.
        let open = self.tool != Tool::Select || self.selected_layer.is_some();

        let width = capsule_width(ui, open, self.tool);
        if width > canvas.width() - 16.0 {
            return;
        }

        // Two curves, deliberately offset: the height leads and the contents
        // follow a beat behind, so the eye tracks the capsule's edge rather
        // than the controls sliding in.
        let grow = ease(anim(ui.ctx(), "pill_grow", open, 0.20));
        let fade = anim(ui.ctx(), "pill_fade", open, 0.16);

        let rect = pill_rect(canvas, width, grow);
        theme::glass(ui.painter(), rect);

        let row = egui::Rect::from_min_size(
            egui::pos2(
                rect.center().x - tool_row_width() / 2.0,
                rect.min.y + theme::PILL_PAD,
            ),
            egui::vec2(tool_row_width(), super::icons::BUTTON),
        );
        self.tool_row(ui, row);

        if grow > 0.001 {
            let hair = row.bottom() + HAIRLINE_GAP;
            ui.painter().hline(
                rect.min.x..=rect.max.x,
                hair,
                egui::Stroke::new(1.0_f32, theme::pal().line),
            );
            self.options_panel(ui, rect, hair, grow, fade);
        }
    }

    /// The seven tool buttons. Fixed width, and centred in the capsule, so
    /// nothing in the options row below can move them.
    fn tool_row(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("tool_row")
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.spacing_mut().item_spacing = egui::vec2(theme::PILL_GAP, 0.0);
        for (tool, key) in TOOLS {
            if tool == Tool::Select {
                separator(&mut ui);
            }
            if super::icons::tool_button(&mut ui, tool, self.tool == tool, Some(key)).clicked() {
                self.finish_text_edit();
                self.tool = tool;
            }
        }
    }

    /// The options row, inside a container that is clipped as it opens.
    ///
    /// The clip is what makes the row appear to slide out from under the
    /// hairline instead of fading in on top of the picture.
    fn options_panel(
        &mut self,
        ui: &mut egui::Ui,
        capsule: egui::Rect,
        hairline_y: f32,
        grow: f32,
        fade: f32,
    ) {
        let full = OPT_PAD_TOP + OPT_ROW_H;
        let container = egui::Rect::from_min_size(
            egui::pos2(capsule.min.x, hairline_y + 1.0),
            egui::vec2(capsule.width(), full * grow),
        );
        // Centred on the capsule's own centre. The padding is *around* the row,
        // not part of it — adding it to the width and then starting the widgets
        // at the left edge put the whole row half the padding off centre, which
        // read as the capsule having uneven margins.
        let width = self.row_width(ui);
        let inner = egui::Rect::from_min_size(
            egui::pos2(
                capsule.center().x - width / 2.0,
                // Travels the last 6px into place, which is the same offset the
                // design gives the row's transform.
                hairline_y + 1.0 + OPT_PAD_TOP - 6.0 * (1.0 - grow),
            ),
            egui::vec2(width, OPT_ROW_H),
        );

        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("tool_options")
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.set_clip_rect(container);
        ui.set_opacity(fade);
        ui.spacing_mut().item_spacing = egui::vec2(controls::GAP, 0.0);
        self.tool_options(&mut ui);

        // The capsule's backdrop is painted before its contents, so the two
        // widths have to agree. They come from different code — `row_items`
        // measures and `tool_options` draws — because egui cannot build a row
        // from data when every control needs a different `&mut`. This is what
        // makes the drift loud: a control added to one and not the other shows
        // up on the first run rather than as a row overflowing its own glass.
        debug_assert!(
            (ui.min_rect().width() - width).abs() < 1.0,
            "the {:?} options row lays out {}pt wide against {width}pt measured — \
             row_items and tool_options have drifted apart",
            self.tool,
            ui.min_rect().width(),
        );
    }

    /// The controls belonging to the tool in hand.
    fn tool_options(&mut self, ui: &mut egui::Ui) {
        let before = self.annotation_dials();

        match self.tool {
            Tool::Arrow => {
                self.swatches(ui);
                controls::divider(ui);
                controls::slider(ui, t("Stroke"), &mut self.annot_stroke, 1.0..=40.0, "");
                controls::divider(ui);
                for (head, name) in HEADS {
                    if controls::pill(ui, t(name), self.annot_head == head).clicked() {
                        self.annot_head = head;
                    }
                }
                self.rim_options(ui);
            }
            Tool::Text => {
                self.swatches(ui);
                controls::divider(ui);
                controls::caption(ui, t("Size"));
                controls::stepper(ui, &mut self.annot_font_size, 10.0..=160.0, 2.0);
                controls::divider(ui);
                if controls::square(ui, "U", self.annot_underline).clicked() {
                    self.annot_underline = !self.annot_underline;
                }
                controls::divider(ui);
                for align in [TextAlign::Left, TextAlign::Centre, TextAlign::Right] {
                    if controls::align_toggle(ui, align, self.annot_align == align).clicked() {
                        self.annot_align = align;
                    }
                }
                self.rim_options(ui);
            }
            Tool::Rect | Tool::Ellipse => {
                self.swatches(ui);
                controls::divider(ui);
                controls::slider(ui, t("Stroke"), &mut self.annot_stroke, 1.0..=40.0, "");
                controls::divider(ui);
                controls::caption(ui, t("Fill"));
                let ink = self.ink(self.tool);
                let filled = egui::Color32::from_rgba_unmultiplied(ink[0], ink[1], ink[2], ink[3]);
                if controls::fill_chip(ui, self.annot_filled, filled).clicked() {
                    self.annot_filled = !self.annot_filled;
                }
                if self.tool == Tool::Rect {
                    controls::caption(ui, t("Corner"));
                    controls::stepper(ui, &mut self.annot_corner, 0.0..=120.0, 4.0);
                }
                self.rim_options(ui);
            }
            Tool::Blur => {
                for (cover, name) in COVERS {
                    if controls::pill(ui, t(name), self.annot_cover == cover).clicked() {
                        self.annot_cover = cover;
                    }
                }
                controls::divider(ui);
                controls::slider(ui, t("Amount"), &mut self.annot_blur, 2.0..=60.0, "");
                controls::divider(ui);
                controls::caption(ui, t("Pixelate survives a re-encode."));
            }
            Tool::Highlight => {
                self.swatches(ui);
                controls::divider(ui);
                let mut pct = self.annot_paint_alpha as f32 / 255.0 * 100.0;
                if controls::slider(ui, t("Opacity"), &mut pct, 5.0..=100.0, "%").changed() {
                    self.annot_paint_alpha = (pct / 100.0 * 255.0).round() as u8;
                }
                self.rim_options(ui);
            }
            Tool::Select | Tool::Fill => {
                for (shift, name) in ORDERING {
                    if controls::pill(ui, t(name), false).clicked() {
                        self.reorder_selected_layer(shift);
                    }
                }
                if controls::pill(ui, t("Duplicate"), false).clicked() {
                    self.duplicate_selected_layer();
                }
                controls::divider(ui);
                if super::icons::glyph_button(
                    ui,
                    Glyph::Trash,
                    true,
                    egui::vec2(controls::SQUARE, controls::H),
                )
                .on_hover_text(t("Delete the selected shape  ⌫"))
                .clicked()
                {
                    self.delete_selected_layer();
                }
            }
        }

        // Changing a dial has to change the shape you just drew, not only the
        // next one — otherwise the controls look broken.
        if self.annotation_dials() != before {
            self.apply_dials_to_selection();
        }
    }

    /// The rim and its colour, at the end of every row that has ink.
    ///
    /// Shared rather than per-tool because the reason it exists — a red arrow
    /// over a red part of the picture disappears without it — is not a property
    /// of any one tool.
    fn rim_options(&mut self, ui: &mut egui::Ui) {
        controls::divider(ui);
        controls::slider(ui, t("Rim"), &mut self.annot_border, 0.0..=24.0, "");
        if controls::fill_chip(
            ui,
            self.annot_border > 0.5,
            egui::Color32::from_rgba_unmultiplied(
                self.annot_border_color[0],
                self.annot_border_color[1],
                self.annot_border_color[2],
                255,
            ),
        )
        .on_hover_text(t("Rim colour"))
        .clicked()
        {
            // Two rims are worth having and a picker is not: white for a dark
            // picture, black for a light one.
            self.annot_border_color = if self.annot_border_color[0] > 127 {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            };
        }
    }

    /// The five inks, as the palette drawn into the picture.
    fn swatches(&mut self, ui: &mut egui::Ui) {
        for ink in controls::INK {
            let chosen = self.annot_color == ink.to_array();
            if controls::swatch(ui, ink, chosen).clicked() {
                self.annot_color = ink.to_array();
            }
        }
    }

    /// Every dial the options row can move, as the layer a new shape would be.
    ///
    /// One list rather than two: `new_layer` already says what a dial *is*, so
    /// a control added to the row cannot be forgotten by the rule below. A
    /// tuple of them ran out of road at fourteen, which is where Rust stops
    /// deriving `PartialEq`.
    fn annotation_dials(&self) -> Layer {
        self.new_layer(self.tool, [0.0, 0.0])
    }

    /// Carry a changed dial onto the shape already selected.
    fn apply_dials_to_selection(&mut self) {
        let Some(i) = self.selected_layer else { return };
        let Some(kind) = self.layers.get(i).map(|l| l.kind) else {
            return;
        };
        let dials = self.new_layer(kind, [0.0, 0.0]);
        let Some(layer) = self.layers.get_mut(i) else {
            return;
        };
        // Everything but the geometry and the words: those belong to the shape,
        // not to the row.
        *layer = Layer {
            a: layer.a,
            b: layer.b,
            text: std::mem::take(&mut layer.text),
            kind,
            ..dials
        };
        self.dirty = true;
    }

    /// How wide the current tool's options row will be.
    ///
    /// The capsule's backdrop is painted before its contents, so this and the
    /// drawing above have to agree; a test lays out every row for real and
    /// fails if they drift.
    fn row_width(&self, ui: &egui::Ui) -> f32 {
        row_width(ui, self.tool)
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

/// A chrome glyph as a button, sized to sit in a 44 px bar.
fn glyph_button(ui: &mut egui::Ui, glyph: Glyph, enabled: bool) -> egui::Response {
    super::icons::glyph_button(ui, glyph, enabled, egui::vec2(26.0, 24.0))
}

/// Where the tool pill's backdrop goes: centred on the canvas, at the height
/// the options row has currently opened to.
///
/// Pure, and the same function the painter uses, so the test that says
/// "centred" is testing the placement rather than restating the arithmetic.
fn pill_rect(canvas: egui::Rect, width: f32, grow: f32) -> egui::Rect {
    let closed = super::icons::BUTTON + theme::PILL_PAD * 2.0;
    let extra = HAIRLINE_GAP + 1.0 + OPT_PAD_TOP + OPT_ROW_H;
    egui::Rect::from_min_size(
        egui::pos2(canvas.center().x - width / 2.0, canvas.min.y + 14.0),
        egui::vec2(width, closed + extra * grow),
    )
}

/// The seven tool buttons and the separator, without the capsule's padding.
///
/// Fixed, whatever the tool: this is the promise the whole layout rests on.
/// The bar used to be sized to its *current* contents, so switching between
/// Select and a drawing tool shifted every button sideways by half the options
/// group. Now the options live under the buttons and cannot reach them.
fn tool_row_width() -> f32 {
    super::icons::BUTTON * 7.0 + SEP_W + theme::PILL_GAP * 7.0
}

/// How wide the capsule is drawn: whatever the tool in hand needs.
///
/// Following the current row rather than the widest one costs nothing, which
/// took a wrong turn to see. Both the capsule and the tool row are centred on
/// the canvas, so they share a centre line — the capsule's *edges* move as the
/// row changes and the buttons do not. Sizing it to the widest row instead only
/// left the short rows swimming in empty glass.
///
/// Eased rather than jumped, so switching tools reads as one object changing
/// shape instead of two different bars.
fn capsule_width(ui: &egui::Ui, open: bool, tool: Tool) -> f32 {
    let content = if open {
        (row_width(ui, tool) + OPT_PAD_X * 2.0).max(tool_row_width())
    } else {
        tool_row_width()
    };
    let target = content + theme::PILL_PAD * 2.0;
    ui.ctx()
        .animate_value_with_time(egui::Id::new("pill_width"), target, 0.20)
}

/// One control's footprint in the options row.
enum Item {
    /// The five inks, which travel together.
    Swatches,
    Divider,
    Caption(&'static str),
    /// A caption, a track and a readout.
    Slider(&'static str),
    Stepper,
    Pill(&'static str),
    Square,
    /// The fill chip, which is wider than a square toggle.
    Chip,
}

impl Item {
    /// Width, and how many widgets it allocates — the second is what decides
    /// how many gaps fall between them.
    fn extent(&self, ui: &egui::Ui) -> (f32, f32) {
        match self {
            Item::Swatches => (controls::SWATCH * 5.0, 5.0),
            Item::Divider => (controls::DIVIDER_W, 1.0),
            Item::Caption(s) => (controls::text_w(ui, s), 1.0),
            Item::Slider(s) => (
                controls::text_w(ui, s) + controls::TRACK_W + controls::READOUT_W,
                3.0,
            ),
            Item::Stepper => (controls::STEPPER_W, 1.0),
            Item::Pill(s) => (controls::text_w(ui, s) + controls::PILL_PAD_X * 2.0, 1.0),
            Item::Square => (controls::SQUARE, 1.0),
            Item::Chip => (controls::CHIP_W, 1.0),
        }
    }
}

/// What a tool's options row is made of, in order.
///
/// The single description the width and the drawing both come from — or would,
/// if egui let a row be built from data. It cannot, because each control needs
/// a different `&mut`, so the drawing is written out by hand in `tool_options`
/// and a test lays out every row for real and fails if the two drift apart.
fn row_items(tool: Tool) -> Vec<Item> {
    let stroke = || Item::Slider(t("Stroke"));
    // The rim group, on every row that puts ink into the picture.
    let rim = |v: &mut Vec<Item>| {
        v.push(Item::Divider);
        v.push(Item::Slider(t("Rim")));
        v.push(Item::Chip);
    };
    match tool {
        Tool::Arrow => {
            let mut v = vec![Item::Swatches, Item::Divider, stroke(), Item::Divider];
            v.extend(HEADS.map(|(_, name)| Item::Pill(t(name))));
            rim(&mut v);
            v
        }
        Tool::Text => {
            let mut v = vec![
                Item::Swatches,
                Item::Divider,
                Item::Caption(t("Size")),
                Item::Stepper,
                Item::Divider,
                Item::Square,
                Item::Divider,
                Item::Square,
                Item::Square,
                Item::Square,
            ];
            rim(&mut v);
            v
        }
        Tool::Rect => {
            let mut v = vec![
                Item::Swatches,
                Item::Divider,
                stroke(),
                Item::Divider,
                Item::Caption(t("Fill")),
                Item::Chip,
                Item::Caption(t("Corner")),
                Item::Stepper,
            ];
            rim(&mut v);
            v
        }
        Tool::Ellipse => {
            let mut v = vec![
                Item::Swatches,
                Item::Divider,
                stroke(),
                Item::Divider,
                Item::Caption(t("Fill")),
                Item::Chip,
            ];
            rim(&mut v);
            v
        }
        Tool::Blur => {
            let mut v: Vec<Item> = COVERS.iter().map(|(_, n)| Item::Pill(t(n))).collect();
            v.push(Item::Divider);
            v.push(Item::Slider(t("Amount")));
            v.push(Item::Divider);
            v.push(Item::Caption(t("Pixelate survives a re-encode.")));
            v
        }
        Tool::Highlight => {
            let mut v = vec![Item::Swatches, Item::Divider, Item::Slider(t("Opacity"))];
            rim(&mut v);
            v
        }
        Tool::Select | Tool::Fill => {
            let mut v: Vec<Item> = ORDERING.iter().map(|(_, n)| Item::Pill(t(n))).collect();
            v.push(Item::Pill(t("Duplicate")));
            v.push(Item::Divider);
            v.push(Item::Square);
            v
        }
    }
}

/// How wide one tool's options row lays out — the controls only, without the
/// inline padding that keeps them off the capsule's edge.
fn row_width(ui: &egui::Ui, tool: Tool) -> f32 {
    let (width, widgets) = row_items(tool)
        .iter()
        .map(|item| item.extent(ui))
        .fold((0.0, 0.0), |(w, n), (iw, in_)| (w + iw, n + in_));
    width + controls::GAP * (widgets - 1.0).max(0.0)
}

/// The animation the capsule opens on.
///
/// egui's animator ramps linearly; the design asks for
/// `cubic-bezier(.22, .75, .3, 1)`, which is most of its travel in the first
/// third. Solved rather than approximated, because the whole point of the two
/// offset curves is that they are *different* shapes — an eased height against
/// a linear fade — and two eyeballed ease-outs would land on the same one.
fn ease(t: f32) -> f32 {
    const X1: f32 = 0.22;
    const Y1: f32 = 0.75;
    const X2: f32 = 0.3;
    const Y2: f32 = 1.0;
    let bez = |a: f32, b: f32, u: f32| {
        let v = 1.0 - u;
        3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
    };
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    // Newton on x(u) = t. The curve is monotonic in u, so a handful of steps
    // is plenty and no bracketing is needed.
    let mut u = t;
    for _ in 0..6 {
        let x = bez(X1, X2, u) - t;
        let v = 1.0 - u;
        let dx = 3.0 * v * v * X1 + 6.0 * v * u * (X2 - X1) + 3.0 * u * u * (1.0 - X2);
        if dx.abs() < 1e-6 {
            break;
        }
        u -= x / dx;
        u = u.clamp(0.0, 1.0);
    }
    bez(Y1, Y2, u)
}

/// One named animation, so the two curves cannot collide on an id.
fn anim(ctx: &egui::Context, name: &str, on: bool, seconds: f32) -> f32 {
    ctx.animate_bool_with_time(egui::Id::new(name), on, seconds)
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

    /// The pill is centred in the canvas however far the options row has
    /// opened.
    ///
    /// It was once anchored on the *widest* form it could take while being
    /// drawn at its current one, so in the resting state — Select, nothing
    /// chosen — the bar sat 72px left of centre. Reported as the toolbar not
    /// being centred in the frame.
    #[test]
    fn the_tool_pill_is_centred_however_far_it_has_opened() {
        // A canvas that starts well right of the window's left edge, as the
        // real one does: a pill centred on the *window* instead would pass a
        // symmetric test and fail this one.
        let canvas = egui::Rect::from_min_max(egui::pos2(336.0, 60.0), egui::pos2(1280.0, 800.0));
        for grow in [0.0_f32, 0.3, 1.0] {
            let rect = pill_rect(canvas, 420.0, grow);
            assert!(
                (rect.center().x - canvas.center().x).abs() < 0.01,
                "at grow={grow} the pill's centre is {}, the canvas's is {}",
                rect.center().x,
                canvas.center().x
            );
        }
    }

    /// The capsule grows *downward only*. If its top ever moved, the tool
    /// buttons would slide up and down as the options row opened, which is the
    /// one thing the docked row exists to avoid.
    #[test]
    fn opening_the_options_row_moves_only_the_bottom_edge() {
        let canvas = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 800.0));
        let shut = pill_rect(canvas, 420.0, 0.0);
        let open = pill_rect(canvas, 420.0, 1.0);
        assert_eq!(shut.min.y, open.min.y, "the capsule's top edge moved");
        assert!(
            open.height() > shut.height(),
            "the capsule did not grow at all"
        );
        assert_eq!(
            shut.height(),
            super::super::icons::BUTTON + theme::PILL_PAD * 2.0,
            "the closed capsule is no longer exactly the tool row"
        );
    }

    /// The capsule follows the row it is holding, and the tool row stays put
    /// anyway — both are centred on the canvas, so they share a centre line and
    /// only the capsule's edges move.
    ///
    /// Reported as uneven padding: the row was centred in a capsule sized for
    /// the *widest* row, so every short row sat in a pool of empty glass.
    #[test]
    fn the_capsule_follows_its_row_without_moving_the_buttons() {
        let canvas = egui::Rect::from_min_max(egui::pos2(336.0, 60.0), egui::pos2(1280.0, 800.0));
        for width in [tool_row_width(), 520.0, 700.0] {
            let rect = pill_rect(canvas, width, 1.0);
            let row_centre = rect.center().x;
            assert!(
                (row_centre - canvas.center().x).abs() < 0.01,
                "at capsule width {width} the tool row's centre moved to {row_centre}"
            );
        }
    }

    /// The whole promise of the layout: the tool row's width does not depend on
    /// which tool is in hand.
    #[test]
    fn the_tool_row_is_the_same_width_for_every_tool() {
        let w = tool_row_width();
        assert!(w > 0.0);
        assert_eq!(
            w,
            super::super::icons::BUTTON * 7.0 + SEP_W + theme::PILL_GAP * 7.0,
            "seven buttons, one separator and the gaps between them"
        );
    }

    /// The opening curve has to start and end where the animation does, and
    /// never go backwards on the way — an eased height that overshoots would
    /// show as the capsule bouncing.
    #[test]
    fn the_opening_curve_runs_from_shut_to_open_without_turning_back() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        let mut last = 0.0_f32;
        for i in 0..=50 {
            let v = ease(i as f32 / 50.0);
            assert!(
                (0.0..=1.0).contains(&v),
                "the curve left the unit range at t={}: {v}",
                i as f32 / 50.0
            );
            assert!(v >= last - 1e-4, "the curve went backwards at t={}", i as f32 / 50.0);
            last = v;
        }
        assert!(
            ease(0.33) > 0.6,
            "the curve is no longer front-loaded, so the height and the fade \
             have become the same shape"
        );
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
