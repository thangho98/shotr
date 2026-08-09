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

use super::{Command, POLL};
use crate::render::icon::icon_image;

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

    /// The three ways to choose what gets captured, and nothing else. The
    /// editor deliberately offers no way to change its mind afterwards, so this
    /// menu is the whole decision.
    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: t("Capture a region…").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::CaptureRegion)),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: t("Capture a whole screen").into(),
                submenu: screen_items(),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: t("Capture a window").into(),
                submenu: window_items(),
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
            StandardItem {
                label: t("Recent shots…").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::History)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t("From clipboard").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::FromClipboard)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: t("Preferences…").into(),
                activate: Box::new(|t: &mut Self| t.send(Command::Preferences)),
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

/// Every screen at once, then one entry per monitor.
///
/// Built fresh each time the menu opens, so a screen plugged in later shows up
/// without restarting shotr. Listing monitors captures nothing, so this is
/// cheap enough to do on every open.
fn screen_items() -> Vec<MenuItem<ShotrTray>> {
    let mut items: Vec<MenuItem<ShotrTray>> = vec![
        StandardItem {
            label: t("All screens together").into(),
            activate: Box::new(|t: &mut ShotrTray| t.send(Command::CaptureFull)),
            ..Default::default()
        }
        .into(),
    ];
    let monitors = crate::capture::list_monitors();
    if monitors.is_empty() {
        return items;
    }
    items.push(MenuItem::Separator);
    items.extend(monitors.into_iter().map(|m| {
        let index = m.index;
        StandardItem {
            label: format!("{} — {}", index + 1, m.name),
            activate: Box::new(move |t: &mut ShotrTray| t.send(Command::CaptureMonitor(index))),
            ..Default::default()
        }
        .into()
    }));
    items
}

/// One entry per window, listed the moment the menu opens — any earlier and it
/// would offer windows that have since closed.
fn window_items() -> Vec<MenuItem<ShotrTray>> {
    let windows = crate::winlist::list();
    if windows.is_empty() {
        return vec![
            StandardItem {
                label: t("(no windows could be read)").into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
    }
    windows
        .into_iter()
        .map(|w| {
            let id = w.identifier.clone();
            StandardItem {
                label: w.label(),
                activate: Box::new(move |t: &mut ShotrTray| {
                    t.send(Command::CaptureWindow(id.clone()))
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
fn spawn_headless() -> Option<(TrayHandle, Receiver<Command>)> {
    let (tx, rx) = channel();
    let tray = ShotrTray { tx };
    match tray.spawn() {
        Ok(handle) => Some((TrayHandle { _handle: handle }, rx)),
        Err(_) => None,
    }
}

/// Own the thread until the user quits, and return the process exit code.
///
/// `tick` sees each command the menu produced, and `None` on every idle pass —
/// which is where the daemon checks its socket. Returning `false` stops the
/// loop. ksni runs its own D-Bus thread, so nothing here needs an event loop.
pub fn run(mut tick: impl FnMut(Option<Command>) -> bool) -> i32 {
    let Some((_handle, commands)) = spawn_headless() else {
        eprintln!(
            "Không tìm thấy tray (StatusNotifierItem) trên desktop này.\n\
             Dùng trực tiếp: shotr --capture"
        );
        return 1;
    };

    eprintln!("shotr is running in the system tray. Click the icon to capture.");

    loop {
        while let Ok(command) = commands.try_recv() {
            if !tick(Some(command)) {
                return 0;
            }
        }
        if !tick(None) {
            return 0;
        }
        std::thread::sleep(POLL);
    }
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
}
