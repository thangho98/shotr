//! Screen capture through xcap: Linux and Windows.
//!
//! `xcap::Monitor::width()` is not trustworthy — on this COSMIC/Wayland setup
//! it reports 10320x4320 with `scale_factor` 0.333 for a monitor whose captured
//! frame is 3440x1440. Dimensions come from the captured image; the reported
//! rectangle is kept beside it and corrected later, once every monitor is known.

use image::RgbaImage;

use super::{MonitorInfo, MonitorShot};

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

/// Capture every monitor, keeping the geometry the platform reported alongside.
pub fn capture_shots() -> Result<Vec<MonitorShot>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let name = m
            .name()
            .unwrap_or_else(|_| crate::i18n::tf("Monitor {n}", &[("n", &(i + 1).to_string())]));
        let reported = (
            m.x().unwrap_or(0),
            m.y().unwrap_or(0),
            m.width().unwrap_or(0),
            m.height().unwrap_or(0),
        );
        let img = m.capture_image().map_err(|e| e.to_string())?;
        let (w, h) = (img.width(), img.height());
        let image = RgbaImage::from_raw(w, h, img.into_raw())
            .ok_or_else(|| "Capture data is not a valid image".to_string())?;
        out.push(MonitorShot {
            name,
            image,
            reported,
        });
    }
    Ok(out)
}
