//! Screen and window capture.
//!
//! Note: `xcap::Monitor::width()` is not trustworthy — on this COSMIC/Wayland
//! setup it reports 10320×4320 with `scale_factor` 0.333 for a monitor whose
//! captured frame is 3440×1440. Always take dimensions from the captured image
//! rather than from the `Monitor` metadata.
//!
//! Window listing lives in [`crate::wl_windows`], not here: xcap's Wayland
//! window path returns nothing on COSMIC, so shotr talks the toplevel protocols
//! directly instead.

use image::RgbaImage;

pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
}

pub fn list_monitors() -> Vec<MonitorInfo> {
    xcap::Monitor::all()
        .map(|ms| {
            ms.iter()
                .enumerate()
                .map(|(index, m)| MonitorInfo {
                    index,
                    name: m
                        .name()
                        .unwrap_or_else(|_| crate::i18n::tf("Monitor {n}", &[("n", &(index + 1).to_string())])),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Capture one monitor as RGBA. `index` is a position in [`list_monitors`].
pub fn capture_monitor(index: usize) -> Result<RgbaImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let mon = monitors
        .into_iter()
        .nth(index)
        .ok_or_else(|| "No monitor found".to_string())?;
    let img = mon.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    RgbaImage::from_raw(w, h, img.into_raw())
        .ok_or_else(|| "Capture data is not a valid image".to_string())
}

pub fn capture_primary() -> Result<RgbaImage, String> {
    capture_monitor(0)
}

/// One monitor's pixels plus where it sits on the virtual desktop.
pub struct MonitorShot {
    pub name: String,
    pub image: RgbaImage,
    pub origin: (i32, i32),
}

/// Capture every monitor, with each frame's true position worked out.
///
/// The position needs the same correction as the size: this compositor reports
/// a monitor at `(1299, 4320)` sized `7680×4320` for a screen that actually
/// captures at `2560×1440`. Rather than trust `scale_factor`, the ratio is
/// derived from the frame that just came back — the capture is ground truth, so
/// the correction is right even if the metadata changes meaning later.
pub fn capture_shots() -> Result<Vec<MonitorShot>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let name = m
            .name()
            .unwrap_or_else(|_| crate::i18n::tf("Monitor {n}", &[("n", &(i + 1).to_string())]));
        let (rx, ry, rw) = (
            m.x().unwrap_or(0),
            m.y().unwrap_or(0),
            m.width().unwrap_or(0),
        );
        let img = m.capture_image().map_err(|e| e.to_string())?;
        let (w, h) = (img.width(), img.height());
        let image = RgbaImage::from_raw(w, h, img.into_raw())
            .ok_or_else(|| "Capture data is not a valid image".to_string())?;
        let origin = scaled_origin(rx, ry, rw, w);
        out.push(MonitorShot {
            name,
            image,
            origin,
        });
    }
    Ok(out)
}

/// Correct a reported monitor position using how far the reported width is from
/// the captured width.
fn scaled_origin(rx: i32, ry: i32, reported_w: u32, captured_w: u32) -> (i32, i32) {
    if reported_w == 0 {
        return (rx, ry);
    }
    let k = captured_w as f64 / reported_w as f64;
    (
        (rx as f64 * k).round() as i32,
        (ry as f64 * k).round() as i32,
    )
}

/// Paste every monitor into one virtual-desktop image, normalised so the
/// top-left of the whole arrangement is `(0, 0)`.
///
/// Monitors rarely tile perfectly — here one sits below the other and inset by
/// 433px — so the gaps are filled with opaque black rather than left
/// transparent, which would turn into a hole once a background is composited
/// behind the shot.
pub fn stitch(shots: &[MonitorShot]) -> Option<RgbaImage> {
    let min_x = shots.iter().map(|s| s.origin.0).min()?;
    let min_y = shots.iter().map(|s| s.origin.1).min()?;
    let max_x = shots
        .iter()
        .map(|s| s.origin.0 + s.image.width() as i32)
        .max()?;
    let max_y = shots
        .iter()
        .map(|s| s.origin.1 + s.image.height() as i32)
        .max()?;

    let w = (max_x - min_x).max(1) as u32;
    let h = (max_y - min_y).max(1) as u32;
    let mut canvas = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    for s in shots {
        image::imageops::overlay(
            &mut canvas,
            &s.image,
            (s.origin.0 - min_x) as i64,
            (s.origin.1 - min_y) as i64,
        );
    }
    Some(canvas)
}

/// Each monitor's rectangle inside the image [`stitch`] would produce.
fn views_for(shots: &[MonitorShot], min_x: i32, min_y: i32) -> Vec<MonitorView> {
    shots
        .iter()
        .map(|s| MonitorView {
            name: s.name.clone(),
            rect: [
                (s.origin.0 - min_x) as u32,
                (s.origin.1 - min_y) as u32,
                s.image.width(),
                s.image.height(),
            ],
        })
        .collect()
}

/// Where one monitor sits inside a stitched desktop capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorView {
    pub name: String,
    /// `[x, y, w, h]` in pixels of the stitched image.
    pub rect: [u32; 4],
}

/// The whole desktop across every monitor, plus where each monitor landed in it.
///
/// Everything is captured once, at the instant the user asked for a shot. The
/// per-monitor rectangles are what let the editor switch between "all screens"
/// and one screen later by *cutting up this image*, with no second capture — so
/// every view shows the same moment, and switching costs nothing.
pub fn capture_desktop() -> Result<(RgbaImage, Vec<MonitorView>), String> {
    let shots = capture_shots()?;
    if shots.is_empty() {
        return Err("No monitor found".into());
    }
    let min_x = shots.iter().map(|s| s.origin.0).min().unwrap_or(0);
    let min_y = shots.iter().map(|s| s.origin.1).min().unwrap_or(0);
    let views = views_for(&shots, min_x, min_y);
    let image = stitch(&shots).ok_or_else(|| "Could not stitch the monitors together".to_string())?;
    Ok((image, views))
}

/// Downscale for a fast preview. Returns the scaled image plus the factor used.
pub fn make_preview(full: &RgbaImage, max_w: u32) -> (RgbaImage, f32) {
    if full.width() <= max_w {
        return (full.clone(), 1.0);
    }
    let scale = max_w as f32 / full.width() as f32;
    let nh = ((full.height() as f32 * scale).round() as u32).max(1);
    let small = image::imageops::resize(full, max_w, nh, image::imageops::FilterType::Triangle);
    (small, scale)
}

#[cfg(test)]
mod stitch_tests {
    use super::*;
    use image::Rgba;

    fn shot(name: &str, x: i32, y: i32, w: u32, h: u32, fill: [u8; 4]) -> MonitorShot {
        MonitorShot {
            name: name.into(),
            image: RgbaImage::from_pixel(w, h, Rgba(fill)),
            origin: (x, y),
        }
    }

    /// The correction this whole thing rests on. This compositor reports a
    /// 2560x1440 screen as 7680x4320 at (1299, 4320); the captured frame is the
    /// only honest number, so the position is scaled by the same ratio.
    #[test]
    fn a_reported_position_is_corrected_by_the_captured_size() {
        assert_eq!(scaled_origin(1299, 4320, 7680, 2560), (433, 1440));
        assert_eq!(scaled_origin(0, 0, 10320, 3440), (0, 0));
    }

    #[test]
    fn an_honest_compositor_needs_no_correction() {
        // Reported width already equals the captured width: leave it alone.
        assert_eq!(scaled_origin(1920, 0, 2560, 2560), (1920, 0));
        // And a zero width must not divide by zero.
        assert_eq!(scaled_origin(7, 9, 0, 1000), (7, 9));
    }

    /// The user's actual layout: an ultrawide on top, a second screen below and
    /// inset. The canvas has to cover both.
    #[test]
    fn the_real_two_monitor_layout_stitches_to_the_full_bounding_box() {
        let shots = vec![
            shot("DP-3", 0, 0, 3440, 1440, [255, 0, 0, 255]),
            shot("DP-2", 433, 1440, 2560, 1440, [0, 255, 0, 255]),
        ];
        let out = stitch(&shots).unwrap();
        assert_eq!((out.width(), out.height()), (3440, 2880));
        assert_eq!(out.get_pixel(10, 10).0, [255, 0, 0, 255], "top screen");
        assert_eq!(out.get_pixel(500, 2000).0, [0, 255, 0, 255], "bottom screen");
    }

    /// Where no monitor reaches, the canvas must still be opaque — a
    /// transparent gap would punch a hole through to the background once the
    /// shot is composited.
    #[test]
    fn gaps_between_monitors_are_opaque_black() {
        let shots = vec![
            shot("A", 0, 0, 3440, 1440, [255, 0, 0, 255]),
            shot("B", 433, 1440, 2560, 1440, [0, 255, 0, 255]),
        ];
        let out = stitch(&shots).unwrap();
        // Left of the lower screen, below the upper one: covered by neither.
        let gap = out.get_pixel(100, 2000).0;
        assert_eq!(gap, [0, 0, 0, 255], "gap must be opaque, got {gap:?}");
    }

    /// Monitors can sit at negative coordinates when the primary is not the
    /// leftmost. The result must be normalised, not clipped.
    #[test]
    fn a_monitor_left_of_the_origin_shifts_everything_into_frame() {
        let shots = vec![
            shot("main", 0, 0, 1920, 1080, [1, 1, 1, 255]),
            shot("left", -1280, 0, 1280, 1080, [2, 2, 2, 255]),
        ];
        let out = stitch(&shots).unwrap();
        assert_eq!((out.width(), out.height()), (3200, 1080));
        assert_eq!(out.get_pixel(10, 10).0, [2, 2, 2, 255], "left screen first");
        assert_eq!(out.get_pixel(2000, 10).0, [1, 1, 1, 255]);
    }

    #[test]
    fn one_monitor_stitches_to_itself_and_none_is_an_error() {
        let one = vec![shot("solo", 0, 0, 800, 600, [9, 9, 9, 255])];
        let out = stitch(&one).unwrap();
        assert_eq!((out.width(), out.height()), (800, 600));
        assert!(stitch(&[]).is_none(), "no monitors is not a 0x0 image");
    }

    /// The whole point of storing rectangles: cutting the stitched image by a
    /// monitor's rect must give back exactly that monitor. If this drifts, the
    /// editor shows one screen while claiming to show another.
    #[test]
    fn each_view_cuts_its_own_monitor_back_out_of_the_stitched_image() {
        let shots = vec![
            shot("DP-3", 0, 0, 344, 144, [255, 0, 0, 255]),
            shot("DP-2", 43, 144, 256, 144, [0, 255, 0, 255]),
        ];
        let min_x = shots.iter().map(|s| s.origin.0).min().unwrap();
        let min_y = shots.iter().map(|s| s.origin.1).min().unwrap();
        let views = views_for(&shots, min_x, min_y);
        let whole = stitch(&shots).unwrap();

        for (view, original) in views.iter().zip(&shots) {
            let [x, y, w, h] = view.rect;
            let cut = image::imageops::crop_imm(&whole, x, y, w, h).to_image();
            assert_eq!(
                (cut.width(), cut.height()),
                (original.image.width(), original.image.height()),
                "{} came back the wrong size",
                view.name
            );
            assert_eq!(
                cut.as_raw(),
                original.image.as_raw(),
                "{} came back with different pixels",
                view.name
            );
        }
    }

    /// A layout starting left of the origin still has to index from zero.
    #[test]
    fn views_are_relative_to_the_stitched_image_not_the_desktop() {
        let shots = vec![
            shot("main", 0, 0, 100, 100, [1, 1, 1, 255]),
            shot("left", -80, 0, 80, 100, [2, 2, 2, 255]),
        ];
        let views = views_for(&shots, -80, 0);
        assert_eq!(views[0].rect, [80, 0, 100, 100], "main shifts right");
        assert_eq!(views[1].rect, [0, 0, 80, 100], "left screen starts at 0");
    }
}
