//! The app icon, drawn in code.
//!
//! This lives in `render` rather than next to the tray that first needed it
//! because `tray` is `cfg(target_os = "linux")`: the macOS bundle and the
//! Windows build need the same pixels, and a module they cannot compile cannot
//! give them to them.

use image::{Rgba, RgbaImage};

use super::background::{BG_PRESETS, mesh};
use super::frame::rounded_coverage;

/// The app icon: a rounded square in shotr's gradient with a camera lens
/// punched into it. Drawn in code so no image file has to ship with the binary
/// — the same pixels feed the tray, the `.desktop` launcher icon and the macOS
/// `.icns`.
pub fn icon_image(size: u32) -> RgbaImage {
    let gradient = mesh(size, size, &BG_PRESETS[4]); // "Love"
    let radius = (size as f32 * 0.22).round() as u32;
    let centre = size as f32 / 2.0;
    let lens_outer = size as f32 * 0.30;
    let lens_inner = size as f32 * 0.17;

    let mut out = RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let cov = rounded_coverage(x, y, size, size, radius);
            let px = gradient.get_pixel(x, y).0;
            let (mut r, mut g, mut b) = (px[0] as f32, px[1] as f32, px[2] as f32);

            // Distance from the centre decides where the lens sits.
            let dx = x as f32 + 0.5 - centre;
            let dy = y as f32 + 0.5 - centre;
            let d = (dx * dx + dy * dy).sqrt();

            let ring = smooth_band(d, lens_inner, lens_outer);
            if ring > 0.0 {
                r += (255.0 - r) * ring;
                g += (255.0 - g) * ring;
                b += (255.0 - b) * ring;
            }

            let to_u8 = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            out.put_pixel(
                x,
                y,
                Rgba([to_u8(r), to_u8(g), to_u8(b), to_u8(cov * 255.0)]),
            );
        }
    }
    out
}

/// 1.0 inside the annulus between `inner` and `outer`, feathered by a pixel.
fn smooth_band(d: f32, inner: f32, outer: f32) -> f32 {
    let outer_edge = (outer - d).clamp(0.0, 1.0);
    let inner_edge = (d - inner).clamp(0.0, 1.0);
    outer_edge.min(inner_edge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_is_empty_outside_and_full_between() {
        assert_eq!(smooth_band(0.0, 5.0, 10.0), 0.0);
        assert_eq!(smooth_band(20.0, 5.0, 10.0), 0.0);
        assert_eq!(smooth_band(7.5, 5.0, 10.0), 1.0);
    }

    #[test]
    fn every_size_the_iconset_asks_for_renders() {
        // A macOS `.iconset` is rejected outright if one variant is missing, so
        // 16 and 1024 have to survive the rounding in `radius` and the lens
        // radii; 16 is where `size * 0.17` is under two pixels.
        for size in [16u32, 32, 1024] {
            let img = icon_image(size);
            assert_eq!(
                (img.width(), img.height()),
                (size, size),
                "icon_image({size}) must be square at the size asked for"
            );
            assert_eq!(
                img.get_pixel(0, 0).0[3],
                0,
                "the rounded corner must stay cut away at {size}px"
            );
            assert_eq!(
                img.get_pixel(size / 2, size / 2).0[3],
                255,
                "the centre must stay opaque at {size}px"
            );
        }
    }
}
