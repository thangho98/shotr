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

/// The glyphs the window chrome needs that are not tools.
///
/// Drawn rather than typed for the same reason the tool icons are. The obvious
/// characters — `↶ ↷ ⋯ ▢` — are not in Latin-1, and this project has already
/// been bitten once by assuming a font covers a range it does not: the
/// Vietnamese tone marks in U+1EA0–U+1EF9 rendered as empty boxes for a while.
/// A missing glyph in a *window control* would be worse, because the button
/// would still work and simply look broken.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Undo,
    Redo,
    /// The "more" affordance: three dots.
    More,
    /// Delete the selected annotation.
    Trash,
    /// The window controls. `cfg`-ed away on macOS, which draws Apple's
    /// coloured lights instead and must not be able to reach for these.
    #[cfg(not(target_os = "macos"))]
    Close,
    #[cfg(not(target_os = "macos"))]
    Minimise,
    #[cfg(not(target_os = "macos"))]
    Maximise,
}

/// Paint one chrome glyph inside `rect`.
pub fn draw_glyph(painter: &egui::Painter, rect: egui::Rect, glyph: Glyph, color: egui::Color32) {
    let box_ = rect.shrink(rect.width() * 0.24);
    let at =
        |x: f32, y: f32| egui::pos2(box_.min.x + x * box_.width(), box_.min.y + y * box_.height());
    let w = (rect.width() * 0.075).max(1.2);
    let stroke = egui::Stroke::new(w, color);

    match glyph {
        Glyph::Undo | Glyph::Redo => {
            // An arrow that goes over the top and comes back down on the side
            // it points to. The head has to sit exactly on the arc's end — the
            // first version put it near one and the pair read as two plain
            // semicircles, which is indistinguishable from a reload icon.
            let mirror = |x: f32| if glyph == Glyph::Undo { x } else { 1.0 - x };
            let (cx, cy, r) = (0.5_f32, 0.70_f32, 0.34_f32);
            let arc: Vec<egui::Pos2> = (0..=24)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 - i as f32 / 24.0);
                    at(mirror(cx + r * a.cos()), cy - r * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(arc, stroke));
            let tip = at(mirror(cx - r), cy);
            for dx in [-0.15_f32, 0.15] {
                painter.line_segment([tip, at(mirror(cx - r + dx), cy - 0.17)], stroke);
            }
        }
        Glyph::More => {
            for x in [0.12_f32, 0.5, 0.88] {
                painter.circle_filled(at(x, 0.5), w * 0.9, color);
            }
        }
        Glyph::Trash => {
            // Lid, then a tapering body: enough to read as a bin at 12pt.
            painter.line_segment([at(0.0, 0.22), at(1.0, 0.22)], stroke);
            painter.line_segment([at(0.36, 0.22), at(0.42, 0.06)], stroke);
            painter.line_segment([at(0.42, 0.06), at(0.58, 0.06)], stroke);
            painter.line_segment([at(0.58, 0.06), at(0.64, 0.22)], stroke);
            painter.line_segment([at(0.14, 0.22), at(0.24, 1.0)], stroke);
            painter.line_segment([at(0.86, 0.22), at(0.76, 1.0)], stroke);
            painter.line_segment([at(0.24, 1.0), at(0.76, 1.0)], stroke);
        }
        #[cfg(not(target_os = "macos"))]
        Glyph::Close => {
            painter.line_segment([at(0.05, 0.05), at(0.95, 0.95)], stroke);
            painter.line_segment([at(0.95, 0.05), at(0.05, 0.95)], stroke);
        }
        #[cfg(not(target_os = "macos"))]
        Glyph::Minimise => {
            painter.line_segment([at(0.02, 0.5), at(0.98, 0.5)], stroke);
        }
        #[cfg(not(target_os = "macos"))]
        Glyph::Maximise => {
            painter.rect_stroke(
                egui::Rect::from_two_pos(at(0.06, 0.06), at(0.94, 0.94)),
                1.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
    }
}

/// A square button carrying one chrome glyph.
pub fn glyph_button(
    ui: &mut egui::Ui,
    glyph: Glyph,
    enabled: bool,
    size: egui::Vec2,
) -> egui::Response {
    // A disabled button keeps its space in the row — the bar must not reflow as
    // undo becomes available — but stops reporting clicks entirely.
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hot = enabled && response.hovered();
    if hot {
        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let color = if enabled {
        ui.visuals().widgets.inactive.fg_stroke.color
    } else {
        super::theme::pal().text_dim.gamma_multiply(0.45)
    };
    let side = size.min_elem();
    draw_glyph(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(side)),
        glyph,
        color,
    );
    response
}

/// A tool button: icon, selected state, and the tool's name as a tooltip.
///
/// `key` is the digit that selects the tool from the keyboard. It is printed in
/// the corner of the button and repeated in the tooltip, because a shortcut
/// nobody can see is a shortcut nobody uses.
pub fn tool_button(
    ui: &mut egui::Ui,
    tool: Tool,
    selected: bool,
    key: Option<char>,
) -> egui::Response {
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

    match key {
        Some(digit) => {
            painter.text(
                rect.right_bottom() - egui::vec2(3.0, 1.0),
                egui::Align2::RIGHT_BOTTOM,
                digit,
                // `\`` is a small mark that sits high in its em box; at the
                // digits' size it reads as a speck rather than a key name.
                egui::FontId::monospace(if digit.is_ascii_digit() { 9.0 } else { 12.0 }),
                if selected {
                    super::theme::ACCENT
                } else {
                    super::theme::pal().text_dim
                },
            );
            response.on_hover_text(format!("{}  {digit}", tool.label()))
        }
        None => response.on_hover_text(tool.label()),
    }
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

    /// The chrome glyphs stand in for characters that would otherwise be at
    /// the mercy of whatever font the system supplies, so every one of them
    /// has to actually draw — at the sizes the top bar and the strip use.
    #[test]
    fn every_chrome_glyph_draws_without_panicking() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for glyph in [
                Glyph::Undo,
                Glyph::Redo,
                Glyph::More,
                Glyph::Trash,
                #[cfg(not(target_os = "macos"))]
                Glyph::Close,
                #[cfg(not(target_os = "macos"))]
                Glyph::Minimise,
                #[cfg(not(target_os = "macos"))]
                Glyph::Maximise,
            ] {
                for side in [10.0_f32, 22.0, 24.0, 48.0] {
                    draw_glyph(
                        &painter,
                        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(side, side)),
                        glyph,
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
