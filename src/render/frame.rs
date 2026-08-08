//! Geometry: balance trimming, antialiased rounded rectangles, drop shadows.

use image::{Rgba, RgbaImage};

/// Source-over blend of `src` onto `dst` at `(x, y)`, scaled by `cov` (0..=1).
pub fn blend(dst: &mut RgbaImage, x: u32, y: u32, src: Rgba<u8>, cov: f32) {
    if x >= dst.width() || y >= dst.height() {
        return;
    }
    let sa = (src.0[3] as f32 / 255.0) * cov.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let d = dst.get_pixel_mut(x, y);
    let da = d.0[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        *d = Rgba([0, 0, 0, 0]);
        return;
    }
    for i in 0..3 {
        let s = src.0[i] as f32 / 255.0;
        let dc = d.0[i] as f32 / 255.0;
        let c = (s * sa + dc * da * (1.0 - sa)) / out_a;
        d.0[i] = (c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    d.0[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Signed distance from a point to a rounded rectangle centred on the box.
/// Negative inside. `hw`/`hh` are half-extents.
fn rounded_sdf(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let r = r.min(hw).min(hh).max(0.0);
    let qx = px.abs() - (hw - r);
    let qy = py.abs() - (hh - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// Antialiased coverage of pixel `(x, y)` inside a `w`×`h` rounded rect.
///
/// The old implementation returned a hard boolean, which left visibly jagged
/// corners on exported images. Sampling the distance field with a one-pixel
/// ramp costs nothing and removes the stair-stepping.
pub fn rounded_coverage(x: u32, y: u32, w: u32, h: u32, r: u32) -> f32 {
    if r == 0 {
        return 1.0;
    }
    let hw = w as f32 / 2.0;
    let hh = h as f32 / 2.0;
    // +0.5 samples the pixel centre.
    let px = x as f32 + 0.5 - hw;
    let py = y as f32 + 0.5 - hh;
    (0.5 - rounded_sdf(px, py, hw, hh, r as f32)).clamp(0.0, 1.0)
}

/// Find the content box by trimming margins that match the screenshot's own
/// border colour. Used by the Balance toggle so lopsided whitespace does not
/// push the subject off-centre.
///
/// Returns `(x, y, w, h)`. Trimming is capped at 25% per side so a screenshot
/// that is genuinely mostly-uniform never collapses to nothing.
pub fn content_box(img: &RgbaImage, tolerance: u8) -> (u32, u32, u32, u32) {
    let (w, h) = (img.width(), img.height());
    if w < 8 || h < 8 {
        return (0, 0, w, h);
    }

    // The border colour is whatever the four corners agree on; if they disagree
    // there is no uniform margin to trim.
    let corners = [
        img.get_pixel(0, 0),
        img.get_pixel(w - 1, 0),
        img.get_pixel(0, h - 1),
        img.get_pixel(w - 1, h - 1),
    ];
    let base = *corners[0];
    if !corners.iter().all(|c| close(c, &base, tolerance)) {
        return (0, 0, w, h);
    }

    let max_x = w / 4;
    let max_y = h / 4;

    let row_uniform = |y: u32| (0..w).all(|x| close(img.get_pixel(x, y), &base, tolerance));
    let col_uniform = |x: u32| (0..h).all(|y| close(img.get_pixel(x, y), &base, tolerance));

    let mut top = 0;
    while top < max_y && row_uniform(top) {
        top += 1;
    }
    let mut bottom = 0;
    while bottom < max_y && row_uniform(h - 1 - bottom) {
        bottom += 1;
    }
    let mut left = 0;
    while left < max_x && col_uniform(left) {
        left += 1;
    }
    let mut right = 0;
    while right < max_x && col_uniform(w - 1 - right) {
        right += 1;
    }

    let nw = w.saturating_sub(left + right);
    let nh = h.saturating_sub(top + bottom);
    if nw < 8 || nh < 8 {
        return (0, 0, w, h);
    }
    (left, top, nw, nh)
}

/// The screenshot's own background colour, if it has a consistent one.
///
/// Used to tint the inset frame so it reads as an extension of the window being
/// captured rather than a white border stuck around it. Returns `None` when the
/// edges disagree — a photo or a busy screenshot has no such colour, and
/// guessing one would look worse than the default.
pub fn border_color(img: &RgbaImage, tolerance: u8) -> Option<Rgba<u8>> {
    let (w, h) = (img.width(), img.height());
    if w < 8 || h < 8 {
        return None;
    }

    // Sample the ring of edge pixels, subsampled — reading every pixel of a 4K
    // border buys no extra accuracy.
    let step = (w.max(h) / 256).max(1);
    let mut samples: Vec<Rgba<u8>> = Vec::new();
    for x in (0..w).step_by(step as usize) {
        samples.push(*img.get_pixel(x, 0));
        samples.push(*img.get_pixel(x, h - 1));
    }
    for y in (0..h).step_by(step as usize) {
        samples.push(*img.get_pixel(0, y));
        samples.push(*img.get_pixel(w - 1, y));
    }
    if samples.is_empty() {
        return None;
    }

    // Whichever sample the most others agree with wins, provided it is a real
    // majority rather than the largest of many small groups.
    let mut best = (0usize, samples[0]);
    for candidate in &samples {
        let agree = samples
            .iter()
            .filter(|s| close(s, candidate, tolerance))
            .count();
        if agree > best.0 {
            best = (agree, *candidate);
        }
    }
    if (best.0 as f32) / (samples.len() as f32) < 0.6 {
        return None;
    }

    // Average the agreeing samples so noise and antialiasing wash out.
    let mut sum = [0u64; 4];
    let mut n = 0u64;
    for s in samples.iter().filter(|s| close(s, &best.1, tolerance)) {
        for (acc, v) in sum.iter_mut().zip(s.0) {
            *acc += v as u64;
        }
        n += 1;
    }
    let avg = |i: usize| (sum[i] / n.max(1)) as u8;
    Some(Rgba([avg(0), avg(1), avg(2), 255]))
}

fn close(a: &Rgba<u8>, b: &Rgba<u8>, tol: u8) -> bool {
    (0..4).all(|i| a.0[i].abs_diff(b.0[i]) <= tol)
}

/// Shadow parameters derived from the single 0..=100 slider, pre-multiplied by
/// the preview scale so the preview matches the export.
pub struct Shadow {
    pub sigma: f32,
    pub alpha: u8,
    pub offset_y: f32,
}

impl Shadow {
    pub fn from_strength(strength: u32, scale: f32) -> Option<Self> {
        if strength == 0 {
            return None;
        }
        let s = (strength.min(100) as f32) / 100.0;
        Some(Self {
            sigma: (4.0 + s * 34.0) * scale,
            alpha: (40.0 + s * 150.0) as u8,
            offset_y: (2.0 + s * 26.0) * scale,
        })
    }
}

/// Where a rounded rectangle sits on the canvas.
pub struct Placement {
    pub origin: (i64, i64),
    pub size: (u32, u32),
    pub radius: u32,
}

/// Render a blurred drop shadow cast by `place` onto a `cw`×`ch` canvas.
pub fn shadow_layer(canvas: (u32, u32), place: &Placement, shadow: &Shadow) -> RgbaImage {
    let (cw, ch) = canvas;
    let (ox, oy) = place.origin;
    let (w, h) = place.size;
    let r = place.radius;

    let mut layer = RgbaImage::new(cw, ch);
    let dy = shadow.offset_y.round() as i64;
    for y in 0..h {
        for x in 0..w {
            let cov = rounded_coverage(x, y, w, h, r);
            if cov <= 0.0 {
                continue;
            }
            let tx = ox + x as i64;
            let ty = oy + y as i64 + dy;
            if tx < 0 || ty < 0 || tx >= cw as i64 || ty >= ch as i64 {
                continue;
            }
            let a = (shadow.alpha as f32 * cov).round() as u8;
            layer.put_pixel(tx as u32, ty as u32, Rgba([0, 0, 0, a]));
        }
    }
    image::imageops::blur(&layer, shadow.sigma.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200×200 red square centred in a 300×300 white field, then shifted so
    /// the margins are lopsided: 20px left, 80px right, 10px top, 90px bottom.
    fn lopsided() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(300, 300, Rgba([255, 255, 255, 255]));
        for y in 10..210 {
            for x in 20..220 {
                img.put_pixel(x, y, Rgba([200, 30, 30, 255]));
            }
        }
        img
    }

    #[test]
    fn content_box_trims_uniform_margins() {
        // Trimming is capped at 25% per side, so from 300px only 75px can go.
        let (x, y, w, h) = content_box(&lopsided(), 6);
        assert_eq!((x, y), (20, 10));
        assert_eq!(w, 300 - 20 - 75);
        assert_eq!(h, 300 - 10 - 75);
    }

    #[test]
    fn content_box_is_noop_when_corners_disagree() {
        let mut img = lopsided();
        img.put_pixel(299, 299, Rgba([0, 0, 0, 255]));
        assert_eq!(content_box(&img, 6), (0, 0, 300, 300));
    }

    #[test]
    fn content_box_never_collapses_a_uniform_image() {
        let img = RgbaImage::from_pixel(120, 120, Rgba([9, 9, 9, 255]));
        let (_, _, w, h) = content_box(&img, 6);
        assert!(w >= 8 && h >= 8, "got {w}x{h}");
    }

    #[test]
    fn border_color_finds_a_uniform_frame() {
        // Dark window chrome around a light body.
        let mut img = RgbaImage::from_pixel(200, 150, Rgba([32, 33, 36, 255]));
        for y in 20..130 {
            for x in 20..180 {
                img.put_pixel(x, y, Rgba([250, 250, 250, 255]));
            }
        }
        let c = border_color(&img, 8).expect("edges agree");
        assert_eq!([c.0[0], c.0[1], c.0[2]], [32, 33, 36]);
    }

    #[test]
    fn border_color_declines_when_the_edges_disagree() {
        // Four quadrants of wildly different colour: no background to speak of.
        let mut img = RgbaImage::new(120, 120);
        for y in 0..120 {
            for x in 0..120 {
                let c = match (x < 60, y < 60) {
                    (true, true) => [255, 0, 0, 255],
                    (false, true) => [0, 255, 0, 255],
                    (true, false) => [0, 0, 255, 255],
                    (false, false) => [255, 255, 0, 255],
                };
                img.put_pixel(x, y, Rgba(c));
            }
        }
        assert!(border_color(&img, 8).is_none());
    }

    #[test]
    fn border_color_tolerates_slight_noise() {
        let mut img = RgbaImage::from_pixel(160, 160, Rgba([240, 240, 242, 255]));
        // A few stray dark pixels on the edge must not defeat detection.
        for x in (0..160).step_by(37) {
            img.put_pixel(x, 0, Rgba([10, 10, 10, 255]));
        }
        let c = border_color(&img, 6).expect("majority still agrees");
        assert!(c.0[0] > 200, "got {:?}", c.0);
    }

    #[test]
    fn border_color_is_none_for_a_tiny_image() {
        assert!(border_color(&RgbaImage::new(4, 4), 8).is_none());
    }

    #[test]
    fn rounded_coverage_is_antialiased_at_the_corner() {
        // Dead centre is fully covered, the extreme corner is not, and somewhere
        // along the arc there is a partial value — that partial is the whole
        // point of replacing the old boolean mask.
        assert_eq!(rounded_coverage(50, 50, 100, 100, 20), 1.0);
        assert_eq!(rounded_coverage(0, 0, 100, 100, 20), 0.0);

        let partial = (0..25)
            .flat_map(|y| (0..25).map(move |x| rounded_coverage(x, y, 100, 100, 20)))
            .filter(|c| *c > 0.05 && *c < 0.95)
            .count();
        assert!(
            partial >= 10,
            "expected a band of partially covered pixels along the arc, got {partial}"
        );
    }

    #[test]
    fn zero_radius_covers_everything() {
        assert_eq!(rounded_coverage(0, 0, 100, 100, 0), 1.0);
    }

    #[test]
    fn shadow_strength_zero_disables_it() {
        assert!(Shadow::from_strength(0, 1.0).is_none());
        assert!(Shadow::from_strength(1, 1.0).is_some());
    }

    #[test]
    fn shadow_scales_with_preview_factor() {
        let full = Shadow::from_strength(50, 1.0).unwrap();
        let half = Shadow::from_strength(50, 0.5).unwrap();
        assert!((full.sigma / 2.0 - half.sigma).abs() < 1e-4);
        assert_eq!(full.alpha, half.alpha, "opacity must not depend on scale");
    }
}
