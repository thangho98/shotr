//! The six nav glyphs, drawn rather than shipped.
//!
//! Same reasoning as [`crate::app::icons`]: at 15px each of these is a handful
//! of line segments, so drawing them keeps the binary free of an icon font and
//! an SVG rasteriser — and they follow the palette without a second asset per
//! theme.
//!
//! The paths come from the design's 24×24 viewBox, so the coordinates below can
//! be read against it directly.

use eframe::egui::{self, Color32, Pos2, Stroke};

use super::Section;

/// Paint one nav glyph inside `rect`.
pub fn draw(painter: &egui::Painter, rect: egui::Rect, section: Section, color: Color32) {
    let s = rect.width() / 24.0;
    let at = |x: f32, y: f32| egui::pos2(rect.min.x + x * s, rect.min.y + y * s);
    // 1.7 viewBox units, which at 15px is a hair over one physical pixel. Below
    // that a stroked glyph turns into a grey smudge, hence the floor.
    let stroke = Stroke::new((1.7 * s).max(1.0), color);
    let line = |pts: Vec<Pos2>| painter.add(egui::Shape::line(pts, stroke));
    let closed = |pts: Vec<Pos2>| painter.add(egui::Shape::closed_line(pts, stroke));
    let ring = |x: f32, y: f32, r: f32| painter.circle_stroke(at(x, y), r * s, stroke);

    match section {
        #[cfg(target_os = "macos")]
        Section::Permission => {
            closed(vec![
                at(12.0, 3.0),
                at(19.0, 6.0),
                at(19.0, 11.0),
                at(15.5, 17.5),
                at(12.0, 21.0),
                at(8.5, 17.5),
                at(5.0, 11.0),
                at(5.0, 6.0),
            ]);
        }
        Section::General => {
            line(vec![at(4.0, 7.0), at(14.0, 7.0)]);
            line(vec![at(18.0, 7.0), at(20.0, 7.0)]);
            ring(16.0, 7.0, 2.0);
            line(vec![at(4.0, 17.0), at(8.0, 17.0)]);
            line(vec![at(12.0, 17.0), at(20.0, 17.0)]);
            ring(10.0, 17.0, 2.0);
        }
        Section::Export => {
            line(vec![at(12.0, 3.0), at(12.0, 14.0)]);
            line(vec![at(8.0, 10.0), at(12.0, 14.0), at(16.0, 10.0)]);
            line(vec![
                at(4.0, 17.0),
                at(4.0, 19.5),
                at(6.0, 21.0),
                at(18.0, 21.0),
                at(20.0, 19.5),
                at(20.0, 17.0),
            ]);
        }
        Section::Redaction => {
            // The lens: two mirrored humps 6 units deep, sampled rather than
            // spelled out as béziers — egui has no curve primitive and at 15px
            // sixteen segments are already smooth.
            const N: usize = 16;
            let hump = |sign: f32| {
                (0..=N).map(move |i| {
                    let t = i as f32 / N as f32;
                    let x = 3.0 + 18.0 * t;
                    (x, 12.0 + sign * 6.0 * (std::f32::consts::PI * t).sin())
                })
            };
            let mut lens: Vec<Pos2> = hump(-1.0).map(|(x, y)| at(x, y)).collect();
            lens.extend(hump(1.0).rev().skip(1).map(|(x, y)| at(x, y)));
            closed(lens);
            line(vec![at(4.0, 4.0), at(20.0, 20.0)]);
        }
        Section::Shortcuts => {
            closed(vec![
                at(3.0, 6.0),
                at(21.0, 6.0),
                at(21.0, 18.0),
                at(3.0, 18.0),
            ]);
            for x in [7.0_f32, 11.0, 15.0] {
                painter.circle_filled(at(x, 10.0), stroke.width * 0.6, color);
            }
            line(vec![at(8.0, 14.0), at(16.0, 14.0)]);
        }
        Section::About => {
            ring(12.0, 12.0, 9.0);
            line(vec![at(12.0, 16.0), at(12.0, 11.0)]);
            painter.circle_filled(at(12.0, 8.0), stroke.width * 0.6, color);
        }
    }
}
