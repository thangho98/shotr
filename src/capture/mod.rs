//! Screen capture, and the compositing of several monitors into one image.
//!
//! One API, two backends, chosen at compile time:
//!
//! * **Linux and Windows** — [`xcap`], where the crate's monitor support works.
//! * **macOS** — [`macos`], driving `/usr/sbin/screencapture`. Not a fallback:
//!   xcap reaches macOS through `CGWindowListCreateImage`, which Apple obsoleted
//!   in 15.0, and the system tool is what every other Mac screenshot app uses.
//!
//! Window listing lives in [`crate::winlist`], not here.

use image::RgbaImage;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(not(target_os = "macos"))]
mod xcap;

#[cfg(target_os = "macos")]
use macos as backend;
#[cfg(not(target_os = "macos"))]
use xcap as backend;

pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
}

/// One monitor's pixels plus the rectangle the platform reported for it.
pub struct MonitorShot {
    pub name: String,
    pub image: RgbaImage,
    /// `(x, y, w, h)` exactly as the platform reported it, in whatever units it
    /// uses. Deliberately uncorrected: the factor that maps these onto the
    /// stitched canvas depends on *every* monitor, so it cannot be known here.
    pub reported: (i32, i32, u32, u32),
}

pub fn list_monitors() -> Vec<MonitorInfo> {
    backend::list_monitors()
}

/// Capture one monitor as RGBA. `index` is a position in [`list_monitors`].
pub fn capture_monitor(index: usize) -> Result<RgbaImage, String> {
    backend::capture_monitor(index)
}

fn capture_shots() -> Result<Vec<MonitorShot>, String> {
    backend::capture_shots()
}

/// How many captured pixels one reported unit buys on this monitor.
///
/// The captured frame is the only honest number — `xcap::Monitor` reports
/// `10320x4320` for a screen that captures at `3440x1440` — so the ratio comes
/// from the frame that just came back rather than from `scale_factor`.
fn scale_of(shot: &MonitorShot) -> f64 {
    let reported_w = shot.reported.2;
    if reported_w == 0 {
        return 1.0;
    }
    shot.image.width() as f64 / reported_w as f64
}

/// The one factor the whole composite is built at.
///
/// Per-monitor factors cannot be applied directly. A Retina laptop reports
/// scale 2 and an external panel beside it reports 1, so scaling each monitor's
/// origin by its own factor puts the two in different coordinate spaces —
/// measured at 1800px of overlap when they sit side by side, and at the laptop
/// coming out *wider* than an ultrawide that is really twice its size.
///
/// The largest factor wins rather than the smallest: upscaling the coarser
/// monitor only makes it soft, while downscaling the finer one throws away
/// detail that exists.
fn shared_scale(shots: &[MonitorShot]) -> f64 {
    shots
        .iter()
        .map(scale_of)
        .fold(f64::MIN_POSITIVE, f64::max)
}

/// Where one monitor lands on the composite, before the arrangement is shifted
/// to start at the origin.
fn placement(shot: &MonitorShot, s: f64) -> [i32; 4] {
    let (x, y, w, h) = shot.reported;
    [
        (x as f64 * s).round() as i32,
        (y as f64 * s).round() as i32,
        ((w as f64 * s).round() as i32).max(1),
        ((h as f64 * s).round() as i32).max(1),
    ]
}

/// Every monitor's rectangle plus the canvas holding them, normalised so the
/// top-left of the arrangement is `(0, 0)`.
///
/// One computation with two consumers — [`stitch`] draws from it and
/// [`views_for`] reports it — because a rectangle that disagrees between those
/// two means the editor shows one screen while claiming to show another.
struct Layout {
    rects: Vec<[u32; 4]>,
    size: (u32, u32),
}

fn layout(shots: &[MonitorShot]) -> Option<Layout> {
    if shots.is_empty() {
        return None;
    }
    let s = shared_scale(shots);
    let placed: Vec<[i32; 4]> = shots.iter().map(|shot| placement(shot, s)).collect();

    let min_x = placed.iter().map(|r| r[0]).min()?;
    let min_y = placed.iter().map(|r| r[1]).min()?;
    let max_x = placed.iter().map(|r| r[0] + r[2]).max()?;
    let max_y = placed.iter().map(|r| r[1] + r[3]).max()?;

    Some(Layout {
        rects: placed
            .iter()
            .map(|r| {
                [
                    (r[0] - min_x) as u32,
                    (r[1] - min_y) as u32,
                    r[2] as u32,
                    r[3] as u32,
                ]
            })
            .collect(),
        size: ((max_x - min_x).max(1) as u32, (max_y - min_y).max(1) as u32),
    })
}
/// Paste every monitor into one virtual-desktop image, normalised so the
/// top-left of the whole arrangement is `(0, 0)`.
///
/// Monitors rarely tile perfectly — here one sits below the other and inset by
/// 433px — so the gaps are filled with opaque black rather than left
/// transparent, which would turn into a hole once a background is composited
/// behind the shot.
pub fn stitch(shots: &[MonitorShot]) -> Option<RgbaImage> {
    let plan = layout(shots)?;
    let mut canvas = RgbaImage::from_pixel(plan.size.0, plan.size.1, image::Rgba([0, 0, 0, 255]));
    for (shot, rect) in shots.iter().zip(&plan.rects) {
        let [x, y, w, h] = *rect;
        // Only a monitor coarser than the finest one needs this, and Lanczos3
        // because the result is what the user then selects a region on.
        let scaled;
        let src = if (shot.image.width(), shot.image.height()) == (w, h) {
            &shot.image
        } else {
            scaled = image::imageops::resize(&shot.image, w, h, image::imageops::FilterType::Lanczos3);
            &scaled
        };
        image::imageops::overlay(&mut canvas, src, x as i64, y as i64);
    }
    Some(canvas)
}

/// Each monitor's rectangle inside the image [`stitch`] would produce.
fn views_for(shots: &[MonitorShot]) -> Vec<MonitorView> {
    let Some(plan) = layout(shots) else {
        return Vec::new();
    };
    shots
        .iter()
        .zip(plan.rects)
        .map(|(shot, rect)| MonitorView {
            name: shot.name.clone(),
            rect,
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
    let views = views_for(&shots);
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

    /// A monitor that reports exactly what it captures — scale 1, the honest case.
    fn shot(name: &str, x: i32, y: i32, w: u32, h: u32, fill: [u8; 4]) -> MonitorShot {
        reporting(name, (x, y, w, h), w, h, fill)
    }

    /// A monitor whose reported rectangle and captured frame disagree.
    fn reporting(
        name: &str,
        reported: (i32, i32, u32, u32),
        cap_w: u32,
        cap_h: u32,
        fill: [u8; 4],
    ) -> MonitorShot {
        MonitorShot {
            name: name.into(),
            image: RgbaImage::from_pixel(cap_w, cap_h, Rgba(fill)),
            reported,
        }
    }

    /// The correction this whole thing rests on. This compositor reports a
    /// 2560x1440 screen as 7680x4320 at (1299, 4320); the captured frame is the
    /// only honest number, so everything is scaled by the same ratio.
    #[test]
    fn a_reported_rectangle_is_corrected_by_the_captured_size() {
        let shots = vec![
            reporting("DP-3", (0, 0, 10320, 4320), 3440, 1440, [1, 1, 1, 255]),
            reporting("DP-2", (1299, 4320, 7680, 4320), 2560, 1440, [2, 2, 2, 255]),
        ];
        let plan = layout(&shots).unwrap();
        assert_eq!(plan.rects[0], [0, 0, 3440, 1440], "DP-3 landed wrong");
        assert_eq!(plan.rects[1], [433, 1440, 2560, 1440], "DP-2 landed wrong");
    }

    #[test]
    fn an_honest_compositor_needs_no_correction() {
        let shots = vec![shot("only", 1920, 0, 2560, 1440, [1, 1, 1, 255])];
        // Sole monitor, so it normalises to the origin whatever it reported.
        assert_eq!(layout(&shots).unwrap().rects[0], [0, 0, 2560, 1440]);
    }

    /// A zero reported width must not divide by zero and take the process out.
    #[test]
    fn a_monitor_reporting_no_width_does_not_divide_by_zero() {
        let shots = vec![reporting("broken", (7, 9, 0, 0), 100, 80, [1, 1, 1, 255])];
        let plan = layout(&shots).unwrap();
        assert_eq!(plan.size, (1, 1), "a zero-sized report cannot claim a canvas");
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
        let views = views_for(&shots);
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
        let views = views_for(&shots);
        assert_eq!(views[0].rect, [80, 0, 100, 100], "main shifts right");
        assert_eq!(views[1].rect, [0, 0, 80, 100], "left screen starts at 0");
    }

    /// Measured on a real Mac: a Retina laptop at scale 2 beside an ultrawide at
    /// scale 1. Scaling each monitor by its own factor put them in different
    /// coordinate spaces — the ultrawide, physically almost twice the laptop's
    /// width, came out *narrower* at a ratio of 0.96.
    #[test]
    fn mixed_dpi_monitors_keep_their_true_relative_size() {
        let shots = vec![
            reporting("laptop", (0, 0, 1800, 1169), 3600, 2338, [1, 1, 1, 255]),
            reporting("ultrawide", (-929, -1440, 3440, 1440), 3440, 1440, [2, 2, 2, 255]),
        ];
        let plan = layout(&shots).unwrap();
        let laptop_w = plan.rects[0][2] as f64;
        let ultra_w = plan.rects[1][2] as f64;
        let ratio = ultra_w / laptop_w;
        assert!(
            (ratio - 1.91).abs() < 0.01,
            "the ultrawide is 1.91x the laptop's width in reality; \
             the composite made it {ratio:.2}x"
        );
        assert_eq!(plan.size, (6880, 5218), "canvas must cover both at scale 2");
    }

    /// The same two monitors side by side. The old per-monitor scaling put the
    /// external panel 1800px inside the laptop's rectangle, silently painting
    /// one over the other.
    #[test]
    fn mixed_dpi_monitors_side_by_side_do_not_overlap() {
        let shots = vec![
            reporting("laptop", (0, 0, 1800, 1169), 3600, 2338, [1, 1, 1, 255]),
            reporting("external", (1800, 0, 1920, 1080), 1920, 1080, [2, 2, 2, 255]),
        ];
        let plan = layout(&shots).unwrap();
        let laptop_right = plan.rects[0][0] + plan.rects[0][2];
        assert_eq!(
            plan.rects[1][0], laptop_right,
            "the external panel must start where the laptop ends, not inside it"
        );

        let whole = stitch(&shots).unwrap();
        assert_eq!(
            whole.get_pixel(laptop_right + 10, 10).0,
            [2, 2, 2, 255],
            "the pixel past the laptop's edge belongs to the external panel"
        );
    }

    /// The guard that Linux and Windows did not change. Every monitor on those
    /// compositors reports at the same ratio, so nothing may be resampled — a
    /// resample here would mean every screenshot silently loses fidelity.
    #[test]
    fn a_uniform_scale_desktop_is_never_resampled() {
        let shots = vec![
            reporting("DP-3", (0, 0, 10320, 4320), 3440, 1440, [255, 0, 0, 255]),
            reporting("DP-2", (1299, 4320, 7680, 4320), 2560, 1440, [0, 255, 0, 255]),
        ];
        let whole = stitch(&shots).unwrap();
        for (view, original) in views_for(&shots).iter().zip(&shots) {
            let [x, y, w, h] = view.rect;
            let cut = image::imageops::crop_imm(&whole, x, y, w, h).to_image();
            assert_eq!(
                cut.as_raw(),
                original.image.as_raw(),
                "{} was resampled on a uniform-scale desktop",
                view.name
            );
        }
    }
}
