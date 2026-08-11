//! The controls the tool pill's options row is built from.
//!
//! egui's own widgets do not fit this row. Its `Slider` owns a label, a drag
//! value and a track whose proportions come from the style, and the design
//! specifies all three separately — a 13px name, an 84×4 track with a 14px
//! knob, and a monospace readout in a column of its own so the numbers do not
//! jitter as they change width. Rather than fight the style into shape for one
//! bar, each control here is allocated and painted directly.
//!
//! Every control is 26 points tall and sits on one baseline, which is what lets
//! the row be a plain left-to-right layout with no per-control alignment.
//!
//! Widths are `pub` and const because the capsule's backdrop is painted before
//! its contents: the width that places the glass and the width the widgets take
//! have to come from the same arithmetic, or the bar sits off to one side of
//! its own buttons.

use std::ops::RangeInclusive;

use eframe::egui;

use super::theme::{self, ACCENT};

/// The annotation palette.
///
/// Fixed, like [`ACCENT`], and for the same reason: these are drawn *into the
/// picture*, not into the chrome. A red that shifted with the interface theme
/// would mean the same screenshot exported two different colours depending on
/// what the desktop was doing at the time.
pub(super) const INK: [egui::Color32; 5] = [
    egui::Color32::from_rgb(0xff, 0x5a, 0x5a),
    egui::Color32::from_rgb(0xff, 0xd1, 0x66),
    egui::Color32::from_rgb(0x5e, 0xe0, 0xa0),
    egui::Color32::from_rgb(0x4a, 0x9e, 0xff),
    egui::Color32::from_rgb(0xe7, 0xe9, 0xee),
];

/// Height every control shares.
pub(super) const H: f32 = 26.0;
/// Gap between controls, and between groups.
pub(super) const GAP: f32 = 8.0;
pub(super) const RADIUS: u8 = 7;

pub(super) const SWATCH: f32 = 20.0;
pub(super) const TRACK_W: f32 = 84.0;
const TRACK_H: f32 = 4.0;
const KNOB: f32 = 14.0;
/// The readout sits in a column of its own so a value going from 9 to 10 does
/// not shove the controls to its right.
pub(super) const READOUT_W: f32 = 30.0;
pub(super) const STEPPER_W: f32 = 72.0;
pub(super) const SQUARE: f32 = 26.0;
pub(super) const CHIP_W: f32 = 28.0;
pub(super) const DIVIDER_W: f32 = 1.0;
const DIVIDER_H: f32 = 24.0;
/// Padding inside a toggle pill, per side.
pub(super) const PILL_PAD_X: f32 = 10.0;

const LABEL_SIZE: f32 = 13.0;
const READOUT_SIZE: f32 = 12.0;

fn label_font() -> egui::FontId {
    egui::FontId::new(LABEL_SIZE, egui::FontFamily::Proportional)
}

fn readout_font() -> egui::FontId {
    egui::FontId::new(READOUT_SIZE, egui::FontFamily::Monospace)
}

/// How wide a caption will be, so a row can be measured before it is drawn.
pub(super) fn text_w(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), label_font(), egui::Color32::WHITE)
        .size()
        .x
}

/// A caption beside a control.
pub(super) fn caption(ui: &mut egui::Ui, text: &str) {
    let w = text_w(ui, text);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::hover());
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        label_font(),
        theme::pal().text_dim,
    );
}

/// A hairline between groups.
pub(super) fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(DIVIDER_W, DIVIDER_H), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::pal().line);
}

/// One colour of the annotation palette.
pub(super) fn swatch(ui: &mut egui::Ui, ink: egui::Color32, on: bool) -> egui::Response {
    let (outer, response) = ui.allocate_exact_size(egui::vec2(SWATCH, H), egui::Sense::click());
    let rect = egui::Rect::from_center_size(outer.center(), egui::vec2(SWATCH, SWATCH));
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, ink);
    if on {
        // A ring rather than a tick: the swatch is 20px and any mark drawn on
        // top of it would have to be legible against all five inks at once.
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0_f32, theme::pal().text),
            egui::StrokeKind::Outside,
        );
    } else if response.hovered() {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0_f32, theme::pal().text_dim),
            egui::StrokeKind::Outside,
        );
    }
    response
}

/// Name, track and readout: the row's workhorse.
///
/// `suffix` is appended to the readout, so an opacity can say `40%` where a
/// stroke says `6` without a second control.
pub(super) fn slider(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    suffix: &str,
) -> egui::Response {
    caption(ui, name);

    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(TRACK_W, H), egui::Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let span = (hi - lo).max(f32::EPSILON);

    if let Some(p) = response.interact_pointer_pos() {
        // The knob is half its own width in from each end, so the travel is
        // shorter than the track and a drag to the very edge still reaches the
        // limit rather than stopping just short of it.
        let travel = (rect.width() - KNOB).max(f32::EPSILON);
        let t = ((p.x - rect.left() - KNOB / 2.0) / travel).clamp(0.0, 1.0);
        let now = lo + t * span;
        if (now - *value).abs() > f32::EPSILON {
            *value = now;
            response.mark_changed();
        }
    }

    let t = ((*value - lo) / span).clamp(0.0, 1.0);
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(TRACK_W, TRACK_H));
    let knob_x = track.left() + KNOB / 2.0 + t * (track.width() - KNOB);
    let painter = ui.painter();
    painter.rect_filled(track, TRACK_H / 2.0, theme::pal().surface);
    if knob_x > track.left() {
        let filled = egui::Rect::from_min_max(track.min, egui::pos2(knob_x, track.max.y));
        painter.rect_filled(filled, TRACK_H / 2.0, ACCENT);
    }
    painter.circle(
        egui::pos2(knob_x, track.center().y),
        KNOB / 2.0,
        theme::pal().text,
        egui::Stroke::new(1.0_f32, theme::strong_line()),
    );

    readout(ui, &format!("{}{suffix}", value.round() as i32));
    response
}

/// The value column beside a slider.
fn readout(ui: &mut egui::Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(READOUT_W, H), egui::Sense::hover());
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        readout_font(),
        theme::pal().text,
    );
}

/// − value + , for a quantity read more often than it is swept.
pub(super) fn stepper(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    step: f32,
) -> egui::Response {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(STEPPER_W, H), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS, theme::pal().surface);
    painter.rect_stroke(
        rect,
        RADIUS,
        egui::Stroke::new(1.0_f32, theme::pal().line),
        egui::StrokeKind::Inside,
    );

    let cell = 22.0;
    let minus = egui::Rect::from_min_size(rect.min, egui::vec2(cell, rect.height()));
    let plus = egui::Rect::from_min_size(
        egui::pos2(rect.right() - cell, rect.top()),
        egui::vec2(cell, rect.height()),
    );
    for (cell_rect, sign, glyph) in [(minus, -1.0_f32, "−"), (plus, 1.0, "+")] {
        let hot = ui.rect_contains_pointer(cell_rect);
        painter.text(
            cell_rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            label_font(),
            if hot {
                theme::pal().text
            } else {
                theme::pal().text_dim
            },
        );
        if response.clicked() && hot {
            *value = (*value + sign * step).clamp(*range.start(), *range.end());
            response.mark_changed();
        }
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{}", value.round() as i32),
        readout_font(),
        theme::pal().text,
    );
    response
}

/// A named on/off choice, wide enough for its own label.
pub(super) fn pill(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let w = text_w(ui, text) + PILL_PAD_X * 2.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::click());
    chrome(ui, rect, on, response.hovered());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        label_font(),
        ink_for(on),
    );
    response
}

/// The same choice where one letter says it all.
pub(super) fn square(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(SQUARE, H), egui::Sense::click());
    chrome(ui, rect, on, response.hovered());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        label_font(),
        // Unlike a named pill, a lone letter has no word to carry meaning, so
        // it keeps full contrast whether it is on or off.
        theme::pal().text,
    );
    response
}

/// Three stacked bars, ragged on the side the text will be ragged on.
pub(super) fn align_toggle(
    ui: &mut egui::Ui,
    align: crate::annotate::TextAlign,
    on: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(SQUARE, H), egui::Sense::click());
    chrome(ui, rect, on, response.hovered());

    let inner = rect.shrink2(egui::vec2(6.0, 0.0));
    let ink = if on {
        theme::pal().text
    } else {
        theme::pal().text_dim
    };
    let painter = ui.painter();
    // 2px bars, 3px apart, centred as a block.
    let top = inner.center().y - (2.0 * 3.0 + 3.0 * 2.0) / 2.0;
    for (i, frac) in [1.0_f32, 0.64, 1.0].into_iter().enumerate() {
        let w = inner.width() * frac;
        let x = match align {
            crate::annotate::TextAlign::Left => inner.left(),
            crate::annotate::TextAlign::Centre => inner.center().x - w / 2.0,
            crate::annotate::TextAlign::Right => inner.right() - w,
        };
        let y = top + i as f32 * 5.0;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, 2.0)),
            0.0,
            ink,
        );
    }
    response
}

/// Whether a shape is filled, shown as the fill itself.
///
/// "No fill" is the printing convention — 45° stripes — rather than an empty
/// box, which at 28×26 reads as a disabled control.
pub(super) fn fill_chip(ui: &mut egui::Ui, filled: bool, ink: egui::Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(CHIP_W, H), egui::Sense::click());
    let painter = ui.painter();
    if filled {
        painter.rect_filled(rect, RADIUS, ink);
    } else {
        painter.rect_filled(rect, RADIUS, theme::pal().surface);
        let clipped = painter.with_clip_rect(rect);
        let step = 10.0;
        let mut x = rect.left() - rect.height();
        while x < rect.right() + rect.height() {
            clipped.line_segment(
                [
                    egui::pos2(x, rect.bottom()),
                    egui::pos2(x + rect.height(), rect.top()),
                ],
                egui::Stroke::new(5.0_f32, theme::pal().surface_hi),
            );
            x += step;
        }
    }
    painter.rect_stroke(
        rect,
        RADIUS,
        egui::Stroke::new(1.0_f32, theme::strong_line()),
        egui::StrokeKind::Inside,
    );
    response
}

/// The background and outline shared by every toggle.
fn chrome(ui: &egui::Ui, rect: egui::Rect, on: bool, hovered: bool) {
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, RADIUS, ACCENT.gamma_multiply(0.40));
        return;
    }
    painter.rect_filled(rect, RADIUS, theme::pal().surface);
    let edge = if hovered {
        theme::pal().text_dim
    } else {
        theme::pal().line
    };
    painter.rect_stroke(
        rect,
        RADIUS,
        egui::Stroke::new(1.0_f32, edge),
        egui::StrokeKind::Inside,
    );
}

fn ink_for(on: bool) -> egui::Color32 {
    if on {
        theme::pal().text
    } else {
        theme::pal().text_dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The palette is drawn into the exported picture, so it must not be one of
    /// the theme's colours — those follow the desktop, and the same shot would
    /// export two different reds depending on the time of day.
    #[test]
    fn the_annotation_palette_is_independent_of_the_theme() {
        for pal in [theme::DARK, theme::LIGHT] {
            for ink in INK {
                for surface in [pal.panel, pal.canvas, pal.surface, pal.glass] {
                    assert_ne!(
                        ink, surface,
                        "an annotation ink is one of the chrome's own colours, so it would \
                         follow the desktop theme into the exported picture"
                    );
                }
            }
        }
    }

    /// Every control shares one height, which is what lets the options row be a
    /// plain horizontal layout with no per-control alignment.
    #[test]
    fn every_control_is_the_same_height() {
        const _: () = assert!(
            SWATCH < H,
            "the swatch is centred in the row rather than filling it"
        );
        const _: () = assert!(SQUARE == H, "a square toggle is as tall as it is wide");
        // The one the row's geometry is written against.
        assert_eq!(H, 26.0);
    }
}
