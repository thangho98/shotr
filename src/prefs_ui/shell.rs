//! The Preferences window's own chrome.
//!
//! The same language as the editor — no system titlebar, a rounded frame, and a
//! card standing proud of it — but not the same shape. Here the card is *shorter*
//! than the content beside it and hangs off the frame's left edge, which is what
//! keeps a 780×540 window from reading as a scaled-down editor.
//!
//! As in [`crate::app::shell`], none of this can be `SidePanel` + `CentralPanel`:
//! egui has no z-order between panels and the card has to overlap the frame.

use eframe::egui;

use super::{PrefsApp, Section, icons};
use crate::app::theme;
use crate::i18n::t;

/// The nav card, from the design.
const CARD_W: f32 = 186.0;
/// How far it hangs off the frame's left edge.
const CARD_OVERHANG: f32 = 20.0;
/// The transparent strip that overhang needs, and the only margin this window
/// keeps: every pixel of it is window that swallows clicks meant for whatever
/// is behind, so there is none on the other three edges.
pub(super) const MARGIN_LEFT: f32 = CARD_OVERHANG;
/// How far it is inset from the body top and bottom. This is the whole trick:
/// a card shorter than what it sits beside reads as an object, not a column.
const CARD_INSET_V: f32 = 14.0;
const CARD_GAP: f32 = 12.0;
const CARD_RADIUS: u8 = 16;

const TITLEBAR_H: f32 = 44.0;
const STATUSBAR_H: f32 = 38.0;
const CONTENT_PAD: f32 = 20.0;

const NAV_PAD: f32 = 10.0;
const NAV_H: f32 = 30.0;
const NAV_GAP: f32 = 2.0;
const NAV_RADIUS: u8 = 9;
const NAV_ICON: f32 = 15.0;
const FOOTER_H: f32 = 30.0;

/// Where each piece landed this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shell {
    pub window: egui::Rect,
    pub titlebar: egui::Rect,
    /// The nav card. Overhangs the frame's left edge, inset top and bottom.
    pub card: egui::Rect,
    /// The scrolling pane, already padded.
    pub content: egui::Rect,
    pub status: egui::Rect,
}

/// Divide the window up. Pure, so the geometry can be checked without a display.
pub fn layout(available: egui::Rect) -> Shell {
    // Only the left edge needs a transparent margin, and only as much as the
    // card hangs off it: every pixel of margin is window that swallows clicks
    // meant for whatever is behind.
    let window = egui::Rect::from_min_max(
        available.min + egui::vec2(CARD_OVERHANG, 0.0),
        available.max,
    );
    let titlebar = egui::Rect::from_min_size(
        window.left_top(),
        egui::vec2(window.width(), TITLEBAR_H.min(window.height())),
    );
    let status = egui::Rect::from_min_size(
        egui::pos2(window.min.x, (window.max.y - STATUSBAR_H).max(titlebar.max.y)),
        egui::vec2(window.width(), STATUSBAR_H.min(window.height())),
    );
    let body = egui::Rect::from_min_max(
        egui::pos2(window.min.x, titlebar.max.y),
        egui::pos2(window.max.x, status.min.y.max(titlebar.max.y)),
    );
    let card = egui::Rect::from_min_size(
        egui::pos2(window.min.x - CARD_OVERHANG, body.min.y + CARD_INSET_V),
        egui::vec2(
            CARD_W.min(body.width() + CARD_OVERHANG),
            (body.height() - CARD_INSET_V * 2.0).max(0.0),
        ),
    );
    let left = (card.max.x + CARD_GAP + CONTENT_PAD).min(window.max.x);
    let content = egui::Rect::from_min_max(
        egui::pos2(left, body.min.y),
        egui::pos2((window.max.x - CONTENT_PAD).max(left), body.max.y),
    );

    Shell {
        window,
        titlebar,
        card,
        content,
        status,
    }
}

impl PrefsApp {
    /// Frame, title bar, content, then the card on top of all three.
    pub(super) fn shell_ui(&mut self, ui: &mut egui::Ui) {
        let shell = layout(ui.max_rect());
        if shell.window.width() < 40.0 || shell.window.height() < 40.0 {
            return;
        }
        theme::window_frame(ui.painter(), shell.window);

        self.title_bar(ui, shell);
        self.content_pane(ui, shell);
        self.status_bar(ui, shell);

        if self.card_grad.is_none() {
            self.card_grad = Some(theme::sidebar_gradient(ui.ctx()));
        }
        self.nav_card(ui, shell);

        // No band along the west edge: the card covers it, and a 6px strip that
        // resizes the window instead of selecting a section would be a trap.
        crate::app::shell::resize_bands(ui, shell.window, shell.window.min.x, false);
    }

    // -------------------------------------------------------- the title bar

    fn title_bar(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let bar = shell.titlebar;
        // Before the buttons that sit on it: within one layer egui gives the
        // click to the last widget that claimed the spot.
        self.drag_band(ui, bar);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(bar.min.x, bar.max.y - 1.0),
                egui::vec2(bar.width(), 1.0),
            ),
            0.0,
            theme::pal().line,
        );
        let text_x = self.window_controls(ui, bar);
        ui.painter().text(
            egui::pos2(text_x, bar.center().y),
            egui::Align2::LEFT_CENTER,
            t("shotr — Preferences"),
            egui::FontId::proportional(12.0),
            theme::pal().text_dim,
        );
    }

    /// A band that moves the window, and maximises it on a double click.
    fn drag_band(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let resp = ui.interact(rect, ui.id().with("titlebar"), egui::Sense::click_and_drag());
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            self.toggle_maximised(ui.ctx());
        }
    }

    /// The flag is mirrored rather than read fresh: not every compositor
    /// reports `maximized`, and a `None` read as `false` is a one-way door.
    fn toggle_maximised(&mut self, ctx: &egui::Context) {
        if let Some(actual) = ctx.input(|i| i.viewport().maximized) {
            self.maximised = actual;
        }
        self.maximised = !self.maximised;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximised));
    }

    /// Draw the window buttons and return where the title text may start.
    ///
    /// macOS gets Apple's lights on the left; everywhere else gets that
    /// platform's own controls on the right, which is the split the editor
    /// already makes.
    #[cfg(target_os = "macos")]
    fn window_controls(&mut self, ui: &mut egui::Ui, bar: egui::Rect) -> f32 {
        let mut x = bar.min.x + 16.0 + theme::LIGHT_D / 2.0;
        for (i, colour) in theme::LIGHTS.into_iter().enumerate() {
            let centre = egui::pos2(x, bar.center().y);
            let rect =
                egui::Rect::from_center_size(centre, egui::Vec2::splat(theme::LIGHT_D + 4.0));
            let resp = ui.interact(rect, ui.id().with(("light", i)), egui::Sense::click());
            let painter = ui.painter();
            painter.circle_filled(centre, theme::LIGHT_D / 2.0, colour);
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
        x - theme::LIGHT_GAP - theme::LIGHT_D / 2.0 + 8.0
    }

    #[cfg(not(target_os = "macos"))]
    fn window_controls(&mut self, ui: &mut egui::Ui, bar: egui::Rect) -> f32 {
        use crate::app::icons::Glyph;
        let mut x = bar.max.x - 12.0;
        for (which, glyph) in [
            (0_usize, Glyph::Close),
            (2, Glyph::Maximise),
            (1, Glyph::Minimise),
        ] {
            let rect = egui::Rect::from_center_size(
                egui::pos2(x - 11.0, bar.center().y),
                egui::Vec2::splat(22.0),
            );
            let resp = ui.interact(rect, ui.id().with(("ctl", which)), egui::Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 5.0, egui::Color32::from_white_alpha(20));
            }
            crate::app::icons::draw_glyph(ui.painter(), rect, glyph, theme::pal().text);
            if resp.clicked() {
                self.window_button(ui.ctx(), which);
            }
            x -= 26.0;
        }
        bar.min.x + 16.0
    }

    /// Close, minimise, maximise — the order the lights are drawn in.
    fn window_button(&mut self, ctx: &egui::Context, which: usize) {
        match which {
            0 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            1 => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            _ => self.toggle_maximised(ctx),
        }
    }

    // ----------------------------------------------------------- the nav card

    fn nav_card(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let card = shell.card;
        if card.height() < FOOTER_H + NAV_H {
            return;
        }
        theme::card_surface(
            ui.painter(),
            card,
            CARD_RADIUS,
            self.card_grad.as_ref(),
        );

        let footer = egui::Rect::from_min_size(
            egui::pos2(card.min.x, card.max.y - FOOTER_H),
            egui::vec2(card.width(), FOOTER_H),
        );
        let mut y = card.min.y + NAV_PAD;
        for section in Section::ALL {
            let rect = egui::Rect::from_min_size(
                egui::pos2(card.min.x + NAV_PAD, y),
                egui::vec2(card.width() - NAV_PAD * 2.0, NAV_H),
            );
            if rect.max.y > footer.min.y {
                break;
            }
            if self.nav_item(ui, rect, *section) {
                self.section = *section;
                self.status.clear();
            }
            y += NAV_H + NAV_GAP;
        }

        // Version and licence, pinned to the bottom of the card. They are
        // reference material, not a section, and putting them here is what lets
        // About be about anything else.
        let painter = ui.painter();
        painter.rect_filled(
            egui::Rect::from_min_size(footer.left_top(), egui::vec2(footer.width(), 1.0)),
            0.0,
            theme::pal().line,
        );
        let font = egui::FontId::proportional(11.0);
        painter.text(
            footer.left_center() + egui::vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            env!("CARGO_PKG_VERSION"),
            font.clone(),
            theme::pal().text_dim,
        );
        painter.text(
            footer.right_center() - egui::vec2(12.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            "GPL-3.0",
            font,
            theme::pal().text_dim,
        );
    }

    /// One icon-and-label row. Returns whether it was clicked.
    fn nav_item(&self, ui: &mut egui::Ui, rect: egui::Rect, section: Section) -> bool {
        let on = self.section == section;
        let resp = ui.interact(rect, ui.id().with(("nav", section)), egui::Sense::click());
        let painter = ui.painter();
        let radius = egui::CornerRadius::same(NAV_RADIUS);
        if on || resp.hovered() {
            painter.rect_filled(rect, radius, theme::wash(if on { 26 } else { 13 }));
        }
        if on {
            painter.rect_stroke(
                rect,
                radius,
                egui::Stroke::new(1.0_f32, theme::wash(23)),
                egui::StrokeKind::Inside,
            );
        }
        let ink = if on {
            theme::pal().text
        } else {
            theme::pal().text_dim
        };
        let icon = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 9.0 + NAV_ICON / 2.0, rect.center().y),
            egui::Vec2::splat(NAV_ICON),
        );
        icons::draw(
            painter,
            icon,
            section,
            if on { theme::ACCENT } else { ink },
        );
        painter.text(
            egui::pos2(icon.max.x + 9.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            section.label(),
            egui::FontId::proportional(13.0),
            ink,
        );
        resp.clicked()
    }

    // -------------------------------------------------------- the status bar

    /// One line, and the only thing this window says about saving: there is no
    /// Save button, so without it a change looks provisional.
    fn status_bar(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let bar = shell.status;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(bar.left_top(), egui::vec2(bar.width(), 1.0)),
            0.0,
            theme::pal().line,
        );
        let message = if self.status.is_empty() {
            t("Changes are saved as you make them").to_owned()
        } else {
            self.status.clone()
        };
        // Clear of the card's overhang, which the bar itself runs under.
        ui.painter().text(
            egui::pos2(shell.content.min.x, bar.center().y),
            egui::Align2::LEFT_CENTER,
            message,
            egui::FontId::proportional(11.0),
            theme::pal().text_dim,
        );
    }

    // ------------------------------------------------------- the content pane

    fn content_pane(&mut self, ui: &mut egui::Ui, shell: Shell) {
        let pane = shell.content;
        if pane.width() < 80.0 || pane.height() < 40.0 {
            return;
        }
        let mut ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("content")
                .max_rect(pane)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(pane);
        ui.add_space(6.0);
        theme::section(&mut ui, self.section.label());
        ui.add_space(10.0);
        egui::ScrollArea::vertical()
            .id_salt("content-scroll")
            // Or the bar tracks the widest *row* instead of the pane: Shortcuts
            // is a column of short lines, so it floated 46px in from the edge,
            // nowhere near anything it belongs to.
            .auto_shrink([false, false])
            .show(&mut ui, |ui| self.section_ui(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(w: f32, h: f32) -> Shell {
        layout(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(w, h),
        ))
    }

    /// The whole point of this window's card: it is *shorter* than the body it
    /// sits beside, and it hangs off the frame's left edge. Lose either and the
    /// design collapses into an ordinary sidebar.
    #[test]
    fn the_card_is_shorter_than_the_body_and_hangs_off_the_left_edge() {
        let s = window(780.0, 540.0);
        assert_eq!(
            s.window.min.x - s.card.min.x,
            CARD_OVERHANG,
            "the card no longer overhangs the frame"
        );
        assert!(
            s.card.min.y > s.titlebar.max.y && s.card.max.y < s.status.min.y,
            "the card must be inset from the body, not fill it: {:?}",
            s.card
        );
    }

    /// The card must stay inside the window the OS gave us, or the compositor
    /// clips the overhang away and the design silently disappears.
    #[test]
    fn the_card_stays_inside_the_window_the_os_gave_us() {
        for (w, h) in [(780.0, 540.0), (640.0, 460.0), (1400.0, 900.0)] {
            let available = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h));
            let s = layout(available);
            assert!(
                available.contains_rect(s.card),
                "{w}x{h}: the card is clipped by the real window: {:?}",
                s.card
            );
        }
    }

    /// Content that started under the card would be unreadable, and the gap is
    /// what makes the card read as sitting on top of the frame.
    #[test]
    fn the_content_starts_clear_of_the_card() {
        let s = window(780.0, 540.0);
        assert!(
            s.content.min.x >= s.card.max.x + CARD_GAP,
            "content at {} runs under a card ending at {}",
            s.content.min.x,
            s.card.max.x
        );
    }

    /// A window dragged down to nothing must not produce inverted rectangles —
    /// egui paints those happily and the result is unreadable, not merely small.
    #[test]
    fn a_tiny_window_produces_no_inverted_rectangles() {
        for (w, h) in [(120.0_f32, 90.0_f32), (60.0, 60.0), (300.0, 200.0)] {
            let s = window(w, h);
            for (name, r) in [
                ("window", s.window),
                ("titlebar", s.titlebar),
                ("card", s.card),
                ("content", s.content),
                ("status", s.status),
            ] {
                assert!(
                    r.min.x <= r.max.x && r.min.y <= r.max.y,
                    "{w}x{h}: {name} came out inside out: {r:?}"
                );
            }
        }
    }
}
