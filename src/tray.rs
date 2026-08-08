//! System tray via StatusNotifierItem.
//!
//! SNI is a plain D-Bus protocol, so this needs no GTK — `ksni` speaks it over
//! `zbus` directly. (The widely used `tray-icon` crate *does* hard-require GTK3
//! on Linux, which is a property of that crate, not of Linux tray icons.)
//!
//! COSMIC hosts the watcher through `cosmic-applet-status-area`, which owns
//! both `org.kde.StatusNotifierWatcher` and
//! `com.system76.CosmicStatusNotifierWatcher`.

use crate::i18n::t;

use ksni::blocking::TrayMethods;
use ksni::menu::{StandardItem, SubMenu};
use ksni::{Icon, MenuItem};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::render::background::{BG_PRESETS, mesh};
use crate::render::frame::rounded_coverage;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    /// Grab the screen, then let the user drag out a region.
    CaptureRegion,
    /// Grab the whole screen and go straight to the editor.
    CaptureFull,
    /// Drag a region on one specific monitor, at 1:1.
    CaptureMonitor(usize),
    OpenFile,
    Quit,
}

struct ShotrTray {
    tx: Sender<Command>,
}

impl ShotrTray {
    fn send(&self, command: Command) {
        // Menu handlers must not block; the daemon loop picks this up.
        let _ = self.tx.send(command);
    }
}

impl ksni::Tray for ShotrTray {
    fn id(&self) -> String {
        "shotr".into()
    }

    fn title(&self) -> String {
        "shotr".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Several sizes so the panel can pick without rescaling.
        vec![icon(22), icon(32), icon(48)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "shotr".into(),
            description: t("Click to pick a region").into(),
            ..Default::default()
        }
    }

    /// Panels that open the menu on left click will do so; those that instead
    /// call Activate get the most common action, which is a region capture.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::CaptureRegion);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: t("Pick a region…").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::CaptureRegion)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t("Whole screen").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::CaptureFull)),
                ..Default::default()
            }
            .into(),
            // Picking one screen keeps the picker at 1:1. The default capture
            // spans every monitor, which has to shrink to fit one window.
            SubMenu {
                label: t("A single monitor…").into(),
                submenu: monitor_items(),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: t("Open image…").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::OpenFile)),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: t("Help").into(),
                submenu: vec![
                    StandardItem {
                        label: t("Bind a shortcut to: shotr --capture").into(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: t("Quit").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// One entry per monitor. Built fresh each time the menu opens, so plugging a
/// screen in shows up without restarting shotr. Listing monitors does not
/// capture anything, so this is cheap.
fn monitor_items() -> Vec<MenuItem<ShotrTray>> {
    let monitors = crate::capture::list_monitors();
    if monitors.is_empty() {
        return vec![
            StandardItem {
                label: t("(no monitors could be read)").into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
    }
    monitors
        .into_iter()
        .map(|m| {
            let index = m.index;
            StandardItem {
                label: format!("{} — {}", index + 1, m.name),
                activate: Box::new(move |t: &mut ShotrTray| {
                    t.send(Command::CaptureMonitor(index))
                }),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

/// Keeps the tray alive. Dropping this removes the icon from the panel.
pub struct TrayHandle {
    _handle: ksni::blocking::Handle<ShotrTray>,
}

/// Register a tray icon. Returns `None` when no SNI host is running, which is
/// normal on plain wlroots sessions and on GNOME without an extension.
pub fn spawn_headless() -> Option<(TrayHandle, Receiver<Command>)> {
    let (tx, rx) = channel();
    let tray = ShotrTray { tx };
    match tray.spawn() {
        Ok(handle) => Some((TrayHandle { _handle: handle }, rx)),
        Err(_) => None,
    }
}

/// The app icon: a rounded square in shotr's gradient with a camera lens
/// punched into it. Drawn in code so no image file has to ship with the binary
/// — the same pixels feed the tray and the `.desktop` launcher icon.
pub fn icon_image(size: u32) -> image::RgbaImage {
    let gradient = mesh(size, size, &BG_PRESETS[4]); // "Love"
    let radius = (size as f32 * 0.22).round() as u32;
    let centre = size as f32 / 2.0;
    let lens_outer = size as f32 * 0.30;
    let lens_inner = size as f32 * 0.17;

    let mut out = image::RgbaImage::new(size, size);
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
                image::Rgba([to_u8(r), to_u8(g), to_u8(b), to_u8(cov * 255.0)]),
            );
        }
    }
    out
}

/// The same icon as ARGB32 in network byte order, which is what SNI wants.
fn icon(size: u32) -> Icon {
    let img = icon_image(size);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for px in img.pixels() {
        let [r, g, b, a] = px.0;
        let alpha = a as f32 / 255.0;
        // Premultiplied, so the panel background does not bleed through the
        // antialiased corners.
        data.push(a);
        data.push((r as f32 * alpha).round() as u8);
        data.push((g as f32 * alpha).round() as u8);
        data.push((b as f32 * alpha).round() as u8);
    }
    Icon {
        width: size as i32,
        height: size as i32,
        data,
    }
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
    fn icon_has_the_right_buffer_length() {
        let i = icon(32);
        assert_eq!(i.width, 32);
        assert_eq!(i.height, 32);
        assert_eq!(i.data.len(), 32 * 32 * 4, "ARGB32 is four bytes per pixel");
    }

    #[test]
    fn icon_corners_are_transparent_and_the_centre_is_not() {
        let size = 48;
        let i = icon(size);
        let at = |x: u32, y: u32| i.data[((y * size + x) * 4) as usize]; // alpha

        assert_eq!(at(0, 0), 0, "rounded corner should be cut away");
        assert_eq!(at(size - 1, 0), 0);
        assert_eq!(at(size / 2, size / 2), 255, "centre should be opaque");
    }

    #[test]
    fn the_lens_ring_is_brighter_than_the_surrounding_gradient() {
        let size = 48;
        let i = icon(size);
        let rgb = |x: u32, y: u32| {
            let o = ((y * size + x) * 4) as usize;
            (i.data[o + 1] as u32) + (i.data[o + 2] as u32) + (i.data[o + 3] as u32)
        };
        // A point on the ring versus a point just inside it.
        let centre = size / 2;
        let on_ring = rgb(centre + (size as f32 * 0.24) as u32, centre);
        let inside = rgb(centre, centre);
        assert!(on_ring > inside, "ring {on_ring} should outshine {inside}");
    }

    #[test]
    fn band_is_empty_outside_and_full_between() {
        assert_eq!(smooth_band(0.0, 5.0, 10.0), 0.0);
        assert_eq!(smooth_band(20.0, 5.0, 10.0), 0.0);
        assert_eq!(smooth_band(7.5, 5.0, 10.0), 1.0);
    }
}
