//! Watermarks: text or logo, placed or tiled.
//!
//! Everything is built as a small transparent RGBA *stamp* first, then composited
//! once. That indirection is what makes the variations cheap — opacity, rotation
//! and tiling all operate on the finished stamp, so they compose freely instead
//! of each needing its own path through the text renderer.

use ab_glyph::FontArc;
use image::{Rgba, RgbaImage};

use super::frame::{blend, rounded_coverage};
use super::text;
use crate::settings::{Style, WatermarkPos, WatermarkStyle};

/// Draw the watermark onto `canvas`.
///
/// `logo`, when present, replaces the text entirely — a picture mark and a
/// wordmark are alternatives, not layers.
pub fn draw(
    canvas: &mut RgbaImage,
    settings: &Style,
    font: Option<&FontArc>,
    logo: Option<&RgbaImage>,
) {
    let s = settings;
    if !s.watermark {
        return;
    }
    let Some(stamp) = build_stamp(canvas.width(), s, font, logo) else {
        return;
    };
    let alpha = s.watermark_opacity as f32 / 255.0;
    // Angle applies either way: a single mark set on a slant is as common as a
    // diagonal tile, and both come from the same rotated stamp.
    let stamp = rotate(&stamp, s.watermark_angle.to_radians());

    if s.watermark_tiled {
        tile_across(canvas, &stamp, alpha);
    } else {
        let (x, y) = place(canvas.width(), canvas.height(), &stamp, s);
        stamp_onto(canvas, &stamp, x, y, alpha);
    }
}

/// The mark itself, on transparent pixels. `None` when there is nothing to say.
fn build_stamp(
    canvas_w: u32,
    s: &Style,
    font: Option<&FontArc>,
    logo: Option<&RgbaImage>,
) -> Option<RgbaImage> {
    // Size follows the canvas so a watermark looks the same on a phone-sized
    // crop and on an ultrawide.
    let base = (14.0 + canvas_w as f32 * 0.006).clamp(11.0, 48.0) * s.watermark_size;

    if let Some(logo) = logo {
        let h = (base * 2.5).round().max(8.0);
        let w = (logo.width() as f32 / logo.height().max(1) as f32 * h).round().max(8.0);
        return Some(image::imageops::resize(
            logo,
            w as u32,
            h as u32,
            image::imageops::FilterType::Lanczos3,
        ));
    }

    let font = font?;
    let label = s.watermark_text.trim();
    if label.is_empty() {
        return None;
    }
    Some(text_stamp(font, base, label, s))
}

/// Render the wordmark, including whatever backing its style calls for.
fn text_stamp(font: &FontArc, px: f32, label: &str, s: &Style) -> RgbaImage {
    let w = text::measure(font, px, label);
    let pad = match s.watermark_style {
        WatermarkStyle::Pill => px * 0.6,
        _ => px * 0.25,
    };
    let tw = (w + pad * 2.0).ceil().max(1.0) as u32;
    let th = (px * 1.5 + pad * 2.0).ceil().max(1.0) as u32;
    let mut tile = RgbaImage::from_pixel(tw, th, Rgba([0, 0, 0, 0]));
    let (tx, ty) = (pad, pad);
    let ink = Rgba(s.watermark_color);

    match s.watermark_style {
        WatermarkStyle::Plain => {}
        WatermarkStyle::Shadow => {
            let off = (px * 0.06).max(1.0);
            text::draw(&mut tile, font, px, tx + off, ty + off, Rgba([0, 0, 0, 110]), label);
        }
        WatermarkStyle::Outline => {
            // Eight offsets rather than four: on diagonals a four-way outline
            // leaves the corners of each glyph bare.
            let r = (px * 0.05).max(1.0);
            for (dx, dy) in [
                (-r, 0.0), (r, 0.0), (0.0, -r), (0.0, r),
                (-r, -r), (r, -r), (-r, r), (r, r),
            ] {
                text::draw(&mut tile, font, px, tx + dx, ty + dy, Rgba([0, 0, 0, 160]), label);
            }
        }
        WatermarkStyle::Pill => {
            let radius = th / 2;
            for y in 0..th {
                for x in 0..tw {
                    let cov = rounded_coverage(x, y, tw, th, radius);
                    if cov > 0.0 {
                        blend(&mut tile, x, y, Rgba([0, 0, 0, 130]), cov);
                    }
                }
            }
        }
    }
    text::draw(&mut tile, font, px, tx, ty, ink, label);
    tile
}

/// Top-left corner for the stamp, given the chosen anchor.
fn place(cw: u32, ch: u32, stamp: &RgbaImage, s: &Style) -> (i64, i64) {
    let margin = (cw.min(ch) as f32 * 0.02).max(8.0) as i64;
    let (cw, ch) = (cw as i64, ch as i64);
    let (sw, sh) = (stamp.width() as i64, stamp.height() as i64);

    let left = margin;
    let centre_x = (cw - sw) / 2;
    let right = cw - sw - margin;
    let top = margin;
    let middle_y = (ch - sh) / 2;
    let bottom = ch - sh - margin;

    match s.watermark_pos {
        WatermarkPos::TopLeft => (left, top),
        WatermarkPos::Top => (centre_x, top),
        WatermarkPos::TopRight => (right, top),
        WatermarkPos::Left => (left, middle_y),
        WatermarkPos::Center => (centre_x, middle_y),
        WatermarkPos::Right => (right, middle_y),
        WatermarkPos::BottomLeft => (left, bottom),
        WatermarkPos::Bottom => (centre_x, bottom),
        WatermarkPos::BottomRight => (right, bottom),
    }
}

/// Composite `stamp` at `(ox, oy)`, scaling its alpha by `alpha`.
fn stamp_onto(canvas: &mut RgbaImage, stamp: &RgbaImage, ox: i64, oy: i64, alpha: f32) {
    for y in 0..stamp.height() {
        for x in 0..stamp.width() {
            let (cx, cy) = (ox + x as i64, oy + y as i64);
            if cx < 0 || cy < 0 {
                continue;
            }
            let px = stamp.get_pixel(x, y);
            if px.0[3] == 0 {
                continue;
            }
            blend(canvas, cx as u32, cy as u32, *px, alpha);
        }
    }
}

/// Repeat the stamp over the whole image, offsetting alternate rows so the
/// repeats do not line up into obvious columns.
fn tile_across(canvas: &mut RgbaImage, tile: &RgbaImage, alpha: f32) {
    let step_x = (tile.width() as i64).max(1) + (tile.width() as i64) / 2;
    let step_y = (tile.height() as i64).max(1) + (tile.height() as i64) / 2;
    let (cw, ch) = (canvas.width() as i64, canvas.height() as i64);

    let mut row = 0i64;
    let mut y = -(tile.height() as i64);
    while y < ch {
        let stagger = if row % 2 == 0 { 0 } else { step_x / 2 };
        let mut x = -(tile.width() as i64) + stagger;
        while x < cw {
            stamp_onto(canvas, tile, x, y, alpha);
            x += step_x;
        }
        y += step_y;
        row += 1;
    }
}

/// Rotate an image about its centre, growing the canvas to fit.
///
/// Sampled backwards — for each destination pixel, find where it came from —
/// which is what keeps the result free of the holes a forward mapping leaves.
pub fn rotate(img: &RgbaImage, radians: f32) -> RgbaImage {
    if radians.abs() < 1e-4 {
        return img.clone();
    }
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (sin, cos) = radians.sin_cos();
    // One pixel of slack past the exact bounding box. `ceil` alone can land
    // precisely on the boundary — at 45 degrees it does — and then the rotated
    // corner sits in the outermost row and gets shaved off.
    let nw = (w * cos.abs() + h * sin.abs()).ceil().max(1.0) + 1.0;
    let nh = (w * sin.abs() + h * cos.abs()).ceil().max(1.0) + 1.0;
    let mut out = RgbaImage::from_pixel(nw as u32, nh as u32, Rgba([0, 0, 0, 0]));

    let (scx, scy) = (w / 2.0, h / 2.0);
    let (dcx, dcy) = (nw / 2.0, nh / 2.0);
    for y in 0..out.height() {
        for x in 0..out.width() {
            let dx = x as f32 + 0.5 - dcx;
            let dy = y as f32 + 0.5 - dcy;
            // Inverse rotation.
            let sx = dx * cos + dy * sin + scx;
            let sy = -dx * sin + dy * cos + scy;
            if sx < 0.0 || sy < 0.0 || sx >= w || sy >= h {
                continue;
            }
            out.put_pixel(x, y, *img.get_pixel(sx as u32, sy as u32));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Rgba8;

    fn settings() -> Style {
        Style {
            watermark: true,
            watermark_text: "shotr".into(),
            ..Default::default()
        }
    }

    fn opaque(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]))
    }

    fn marked_pixels(img: &RgbaImage) -> usize {
        img.pixels().filter(|p| p.0 != [0, 0, 0, 255]).count()
    }

    /// A stamp with no text and no logo is nothing to draw, and must not become
    /// an empty rectangle or a panic.
    #[test]
    fn an_empty_watermark_leaves_the_image_untouched() {
        let mut img = opaque(200, 120);
        let s = Style {
            watermark: true,
            watermark_text: "   ".into(),
            ..Default::default()
        };
        draw(&mut img, &s, None, None);
        assert_eq!(marked_pixels(&img), 0);
        // And with the whole feature off.
        let mut img = opaque(200, 120);
        draw(&mut img, &Style::default(), None, None);
        assert_eq!(marked_pixels(&img), 0);
    }

    /// Each anchor has to land in its own corner. A sign error here puts the
    /// mark off-canvas, where it silently does nothing.
    #[test]
    fn every_anchor_lands_in_its_own_region() {
        let logo = RgbaImage::from_pixel(20, 10, Rgba([255, 0, 0, 255]));
        let cases = [
            (WatermarkPos::TopLeft, true, true),
            (WatermarkPos::TopRight, false, true),
            (WatermarkPos::BottomLeft, true, false),
            (WatermarkPos::BottomRight, false, false),
        ];
        for (pos, want_left, want_top) in cases {
            let mut img = opaque(400, 300);
            let s = Style {
                watermark_pos: pos,
                ..settings()
            };
            draw(&mut img, &s, None, Some(&logo));

            let mut sum_x = 0u64;
            let mut sum_y = 0u64;
            let mut n = 0u64;
            for (x, y, p) in img.enumerate_pixels() {
                if p.0 != [0, 0, 0, 255] {
                    sum_x += x as u64;
                    sum_y += y as u64;
                    n += 1;
                }
            }
            assert!(n > 0, "{pos:?} drew nothing");
            let (cx, cy) = (sum_x / n, sum_y / n);
            assert_eq!(cx < 200, want_left, "{pos:?} horizontal: centre at {cx}");
            assert_eq!(cy < 150, want_top, "{pos:?} vertical: centre at {cy}");
        }
    }

    #[test]
    fn centre_really_is_the_centre() {
        let logo = RgbaImage::from_pixel(20, 10, Rgba([255, 0, 0, 255]));
        let mut img = opaque(400, 300);
        let s = Style {
            watermark_pos: WatermarkPos::Center,
            ..settings()
        };
        draw(&mut img, &s, None, Some(&logo));
        // The mark should straddle the middle of the image.
        assert_ne!(img.get_pixel(200, 150).0, [0, 0, 0, 255]);
    }

    #[test]
    fn opacity_scales_how_much_shows_through() {
        let logo = RgbaImage::from_pixel(40, 40, Rgba([255, 255, 255, 255]));
        let sample = |op: u8| {
            let mut img = opaque(200, 200);
            let s = Style {
                watermark_pos: WatermarkPos::Center,
                watermark_opacity: op,
                ..settings()
            };
            draw(&mut img, &s, None, Some(&logo));
            img.get_pixel(100, 100).0[0] as i32
        };
        let (faint, strong) = (sample(40), sample(255));
        assert!(faint < strong, "40 gave {faint}, 255 gave {strong}");
        assert_eq!(strong, 255, "full opacity should reach the logo colour");
        assert_eq!(sample(0), 0, "zero opacity must leave the image alone");
    }

    /// Tiling has to reach every corner, not just the middle.
    #[test]
    fn tiling_covers_the_whole_image() {
        let logo = RgbaImage::from_pixel(16, 8, Rgba([255, 0, 0, 255]));
        let mut img = opaque(300, 200);
        let s = Style {
            watermark_tiled: true,
            watermark_angle: -30.0,
            ..settings()
        };
        draw(&mut img, &s, None, Some(&logo));

        // Split into quadrants; each must have picked up some ink.
        for (x0, y0) in [(0, 0), (150, 0), (0, 100), (150, 100)] {
            let hit = (x0..x0 + 150)
                .flat_map(|x| (y0..y0 + 100).map(move |y| (x, y)))
                .any(|(x, y)| img.get_pixel(x, y).0 != [0, 0, 0, 255]);
            assert!(hit, "quadrant at ({x0},{y0}) got no watermark");
        }
    }

    /// The bounding box of a rotated rectangle, analytically.
    fn expected_box(w: f32, h: f32, deg: f32) -> (f32, f32) {
        let (sin, cos) = deg.to_radians().sin_cos();
        (
            w * cos.abs() + h * sin.abs(),
            w * sin.abs() + h * cos.abs(),
        )
    }

    #[test]
    fn rotating_by_nothing_is_a_no_op_and_a_right_angle_swaps_the_sides() {
        let img = RgbaImage::from_pixel(20, 10, Rgba([1, 2, 3, 255]));
        assert_eq!(rotate(&img, 0.0).dimensions(), (20, 10));

        let turned = rotate(&img, std::f32::consts::FRAC_PI_2);
        let (w, h) = turned.dimensions();
        // f32 cos(pi/2) is -4e-8 rather than 0, so the ceil can add a pixel.
        assert!(
            w.abs_diff(10) <= 2 && h.abs_diff(20) <= 2,
            "a quarter turn should swap 20x10 into ~10x20, got {w}x{h}"
        );
        let kept = turned.pixels().filter(|p| p.0[3] > 0).count();
        assert!(kept > 150, "rotation dropped most of the image: {kept}/200");
    }

    /// A rotated stamp has to be given exactly the room its bounding box needs.
    ///
    /// Note the box can be *narrower* than the original: a long flat rectangle
    /// turned 30 degrees loses horizontal extent even as it gains height. The
    /// property that matters is that nothing is clipped.
    #[test]
    fn a_rotated_stamp_gets_exactly_its_bounding_box() {
        for deg in [-45.0_f32, -30.0, 15.0, 75.0] {
            let img = RgbaImage::from_pixel(100, 20, Rgba([9, 9, 9, 255]));
            let turned = rotate(&img, deg.to_radians());
            let (w, h) = turned.dimensions();
            let (ew, eh) = expected_box(100.0, 20.0, deg);
            assert!(
                (w as f32 - ew).abs() <= 2.5 && (h as f32 - eh).abs() <= 2.5,
                "{deg}°: got {w}x{h}, expected about {ew:.0}x{eh:.0}"
            );

            // Nothing may sit on the outermost ring, or it was cut off.
            for x in 0..w {
                assert_eq!(turned.get_pixel(x, 0).0[3], 0, "{deg}°: clipped at the top");
                assert_eq!(
                    turned.get_pixel(x, h - 1).0[3],
                    0,
                    "{deg}°: clipped at the bottom"
                );
            }
        }
    }

    #[test]
    fn a_logo_replaces_the_text_rather_than_stacking_with_it() {
        let logo = RgbaImage::from_pixel(30, 30, Rgba([0, 255, 0, 255]));
        let mut img = opaque(200, 200);
        let s = Style {
            watermark_pos: WatermarkPos::Center,
            watermark_opacity: 255,
            ..settings()
        };
        draw(&mut img, &s, None, Some(&logo));
        // Every marked pixel came from the logo, so all of them are its colour.
        let odd: Vec<Rgba8> = img
            .pixels()
            .map(|p| p.0)
            .filter(|p| *p != [0, 0, 0, 255] && *p != [0, 255, 0, 255])
            .collect();
        assert!(odd.is_empty(), "unexpected colours: {:?}", &odd[..odd.len().min(3)]);
    }
}
