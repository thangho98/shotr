//! Tool icons, drawn with the painter rather than shipped as images.
//!
//! Six small glyphs is not worth an icon font or a pile of PNGs: as strokes
//! they scale to any DPI for free, take their colour from the theme so a
//! selected button lights up on its own, and add nothing to the binary.
//!
//! Every glyph is drawn inside a unit square and mapped onto the button, so
//! they all sit on the same optical grid instead of each being nudged by hand.

use eframe::egui;

use crate::annotate::Tool;

/// Side of a tool button, in points.
pub const BUTTON: f32 = 34.0;

/// A tool button: icon, selected state, and the tool's name as a tooltip.
pub fn tool_button(ui: &mut egui::Ui, tool: Tool, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(BUTTON, BUTTON), egui::Sense::click());

    let visuals = ui.style().interact_selectable(&response, selected);
    // A touch of easing on hover; instant state changes read as cheap.
    let warmth = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered() || selected, 0.12);

    let painter = ui.painter();
    if selected || warmth > 0.01 {
        let fill = if selected {
            super::theme::ACCENT.gamma_multiply(0.30)
        } else {
            visuals.bg_fill.gamma_multiply(warmth)
        };
        painter.rect_filled(rect, 8.0, fill);
    }
    if selected {
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(1.0_f32, super::theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }

    let ink = if selected {
        super::theme::ACCENT
    } else {
        visuals.fg_stroke.color
    };
    draw(painter, rect, tool, ink);
    response.on_hover_text(tool.label())
}

/// The square a glyph is drawn into: inset from the button so neighbouring
/// icons never touch, and never collapsed to nothing on a small button.
fn glyph_box(rect: egui::Rect) -> egui::Rect {
    let inset = (rect.width() * 0.28).min(rect.width() / 2.0 - 3.0).max(0.0);
    rect.shrink(inset)
}

/// Paint `tool`'s glyph inside `rect`.
pub fn draw(painter: &egui::Painter, rect: egui::Rect, tool: Tool, color: egui::Color32) {
    // Glyphs are authored in a 0..1 square with a margin, then mapped out.
    let box_ = glyph_box(rect);
    let at = |x: f32, y: f32| egui::pos2(box_.min.x + x * box_.width(), box_.min.y + y * box_.height());
    let w = (rect.width() * 0.055).max(1.2);
    let stroke = egui::Stroke::new(w, color);

    match tool {
        Tool::Select => {
            // A pointer arrow: outline plus a filled tail.
            let tip = at(0.08, 0.0);
            let left = at(0.08, 1.0);
            let right = at(0.92, 0.62);
            let notch = at(0.38, 0.66);
            painter.add(egui::Shape::convex_polygon(
                vec![tip, right, notch, left],
                color,
                egui::Stroke::NONE,
            ));
        }
        Tool::Arrow => {
            let a = at(0.0, 1.0);
            let b = at(1.0, 0.0);
            painter.line_segment([a, b], stroke);
            for p in [at(0.45, 0.0), at(1.0, 0.55)] {
                painter.line_segment([b, p], stroke);
            }
        }
        Tool::Rect => {
            painter.rect_stroke(
                egui::Rect::from_two_pos(at(0.0, 0.12), at(1.0, 0.88)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Tool::Ellipse => {
            let c = box_.center();
            let (rx, ry) = (box_.width() / 2.0, box_.height() * 0.38);
            let pts: Vec<egui::Pos2> = (0..=40)
                .map(|i| {
                    let t = i as f32 / 40.0 * std::f32::consts::TAU;
                    egui::pos2(c.x + rx * t.cos(), c.y + ry * t.sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, stroke));
        }
        Tool::Text => {
            // A serif "T": crossbar, stem, and a foot so it reads as type.
            painter.line_segment([at(0.05, 0.06), at(0.95, 0.06)], stroke);
            painter.line_segment([at(0.5, 0.06), at(0.5, 0.94)], stroke);
            painter.line_segment([at(0.28, 0.94), at(0.72, 0.94)], stroke);
        }
        Tool::Blur => {
            // Dots thinning out left to right: the idea of detail being lost.
            for (row, y) in [0.16_f32, 0.5, 0.84].into_iter().enumerate() {
                for col in 0..4 {
                    let x = 0.1 + col as f32 * 0.27;
                    let fade = 1.0 - (col as f32 + (row % 2) as f32 * 0.5) / 4.6;
                    painter.circle_filled(
                        at(x, y),
                        w * 0.9,
                        color.gamma_multiply(fade.clamp(0.15, 1.0)),
                    );
                }
            }
        }
        Tool::Highlight => {
            // A marker nib over the swipe it just laid down.
            painter.add(egui::Shape::convex_polygon(
                vec![at(0.0, 0.30), at(0.62, 0.0), at(0.92, 0.34), at(0.30, 0.64)],
                color,
                egui::Stroke::NONE,
            ));
            painter.line_segment(
                [at(0.05, 0.92), at(0.95, 0.92)],
                egui::Stroke::new(w * 2.2, color.gamma_multiply(0.45)),
            );
        }
        Tool::Fill => {
            painter.rect_filled(
                egui::Rect::from_two_pos(at(0.0, 0.12), at(1.0, 0.88)),
                2.0,
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing must work for every tool the sidebar can offer. The match in
    /// [`draw`] has no catch-all arm, so a new variant breaks the build rather
    /// than shipping an invisible button — this covers the runtime half.
    #[test]
    fn every_offered_tool_draws_without_panicking() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for tool in std::iter::once(Tool::Select).chain(Tool::DRAWABLE) {
                for side in [16.0_f32, BUTTON, 96.0] {
                    draw(
                        &painter,
                        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(side, side)),
                        tool,
                        egui::Color32::WHITE,
                    );
                }
            }
        });
    }

    /// The glyph has to stay inside its button, or icons in a row overlap.
    #[test]
    fn the_glyph_box_stays_inside_the_button() {
        for side in [12.0_f32, 20.0, BUTTON, 64.0] {
            let rect =
                egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(side, side));
            let inner = glyph_box(rect);
            assert!(rect.contains_rect(inner), "{side}px: glyph escaped the button");
            assert!(inner.width() > 0.0, "{side}px: glyph box collapsed");
        }
    }

    /// A button smaller than twice the inset would invert the rectangle, which
    /// makes every glyph draw backwards or vanish.
    #[test]
    fn a_tiny_button_still_leaves_room_to_draw() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(6.0, 6.0));
        let inner = glyph_box(rect);
        assert!(inner.width() > 0.0, "got {inner:?}");
        assert!(inner.min.x <= inner.max.x && inner.min.y <= inner.max.y);
    }
}
