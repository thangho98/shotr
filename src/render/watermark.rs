//! Watermarks: text or logo, set in a band beneath the shot.
//!
//! Everything is built as a small transparent RGBA *stamp*, which the pipeline
//! then places. That split is deliberate: the stamp belongs to the *shot*, not to
//! the canvas, so only [`super`] knows where it goes and only it can reserve the
//! room.
//!
//! It used to be an overlay anchored to the canvas on a nine-square grid, with an
//! option to tile it across the picture. Both are gone. A mark over a screenshot
//! covers the thing the screenshot was taken for, and every corner of one is
//! somebody's content — so it sits under the shot, aligned to the shot's right
//! edge, and the canvas grows to make room.

use ab_glyph::FontArc;
use image::{Rgba, RgbaImage};

use super::frame::{blend, rounded_coverage};
use super::text;
use crate::settings::{Style, WatermarkStyle};

/// The finished mark on transparent pixels, rotated and ready to place — or
/// `None` when there is nothing to stamp.
///
/// `shot_w` is the screenshot's own width, never the canvas's: the mark is part
/// of the shot, so sizing it against the canvas would make the same watermark on
/// the same screenshot come out a different size at a different padding.
///
/// `logo`, when present, replaces the text entirely — a picture mark and a
/// wordmark are alternatives, not layers.
pub fn stamp(
    shot_w: u32,
    s: &Style,
    font: Option<&FontArc>,
    logo: Option<&RgbaImage>,
) -> Option<RgbaImage> {
    if !s.watermark {
        return None;
    }
    let stamp = build_stamp(shot_w, s, font, logo)?;
    Some(rotate(&stamp, s.watermark_angle.to_radians()))
}

/// Composite a finished stamp at `(x, y)`, scaling its alpha by the style's
/// opacity.
pub fn place(canvas: &mut RgbaImage, stamp: &RgbaImage, x: i64, y: i64, s: &Style) {
    stamp_onto(canvas, stamp, x, y, f32::from(s.watermark_opacity) / 255.0);
}

/// The mark itself, on transparent pixels. `None` when there is nothing to say.
fn build_stamp(
    shot_w: u32,
    s: &Style,
    font: Option<&FontArc>,
    logo: Option<&RgbaImage>,
) -> Option<RgbaImage> {
    // Size follows the shot so a watermark looks the same on a phone-sized crop
    // and on an ultrawide.
    let base = (14.0 + shot_w as f32 * 0.006).clamp(11.0, 48.0) * s.watermark_size;

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

    /// Nothing to say means no stamp at all, not an empty rectangle.
    ///
    /// `None` rather than a 0×0 image matters: the pipeline reserves room from the
    /// stamp's height, so an empty stamp would open a gap under every shot that
    /// has no watermark.
    #[test]
    fn an_empty_watermark_produces_no_stamp() {
        let blank = Style {
            watermark: true,
            watermark_text: "   ".into(),
            ..Default::default()
        };
        assert!(stamp(800, &blank, None, None).is_none(), "blank text");
        assert!(
            stamp(800, &Style::default(), None, None).is_none(),
            "the feature is off"
        );
    }

    /// The mark is sized against the shot, never the canvas, so the same
    /// screenshot keeps the same mark however much padding is put around it.
    #[test]
    fn the_mark_is_sized_from_the_shot() {
        let logo = RgbaImage::from_pixel(20, 10, Rgba([255, 0, 0, 255]));
        let small = stamp(400, &settings(), None, Some(&logo)).expect("a logo is a stamp");
        let big = stamp(2000, &settings(), None, Some(&logo)).expect("a logo is a stamp");
        assert!(
            big.height() > small.height(),
            "a wider shot should take a bigger mark: {} vs {}",
            small.height(),
            big.height()
        );
    }

    #[test]
    fn opacity_scales_how_much_shows_through() {
        let logo = RgbaImage::from_pixel(40, 40, Rgba([255, 255, 255, 255]));
        let sample = |op: u8| {
            let mut img = opaque(200, 200);
            let s = Style {
                watermark_opacity: op,
                ..settings()
            };
            let mark = stamp(800, &s, None, Some(&logo)).expect("a logo is a stamp");
            place(&mut img, &mark, 80, 80, &s);
            img.get_pixel(100, 100).0[0] as i32
        };
        let (faint, strong) = (sample(40), sample(255));
        assert!(faint < strong, "40 gave {faint}, 255 gave {strong}");
        assert_eq!(strong, 255, "full opacity should reach the logo colour");
        assert_eq!(sample(0), 0, "zero opacity must leave the image alone");
    }

    #[test]
    fn a_logo_replaces_the_text_rather_than_stacking_with_it() {
        let logo = RgbaImage::from_pixel(30, 30, Rgba([0, 255, 0, 255]));
        let mut img = opaque(200, 200);
        let s = Style {
            watermark_opacity: 255,
            ..settings()
        };
        let mark = stamp(800, &s, None, Some(&logo)).expect("a logo is a stamp");
        place(&mut img, &mark, 40, 40, &s);
        assert!(marked_pixels(&img) > 0, "the logo drew nothing");
        // Every marked pixel came from the logo, so all of them are its colour.
        let odd: Vec<Rgba8> = img
            .pixels()
            .map(|p| p.0)
            .filter(|p| *p != [0, 0, 0, 255] && *p != [0, 255, 0, 255])
            .collect();
        assert!(
            odd.is_empty(),
            "unexpected colours: {:?}",
            &odd[..odd.len().min(3)]
        );
    }

    /// The bounding box of a rotated rectangle, analytically.
    fn expected_box(w: f32, h: f32, deg: f32) -> (f32, f32) {
        let (sin, cos) = deg.to_radians().sin_cos();
        (w * cos.abs() + h * sin.abs(), w * sin.abs() + h * cos.abs())
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
                "{deg} deg: got {w}x{h}, expected about {ew:.0}x{eh:.0}"
            );

            // Nothing may sit on the outermost ring, or it was cut off.
            for x in 0..w {
                assert_eq!(
                    turned.get_pixel(x, 0).0[3],
                    0,
                    "{deg} deg: clipped at the top"
                );
                assert_eq!(
                    turned.get_pixel(x, h - 1).0[3],
                    0,
                    "{deg} deg: clipped at the bottom"
                );
            }
        }
    }
}
