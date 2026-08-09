//! Listing and capturing individual windows, whichever platform we are on.
//!
//! One API, three implementations, chosen at compile time:
//!
//! * **Linux** — [`crate::wl_windows`], talking the Wayland toplevel protocols
//!   directly. xcap's window path returns an empty list on Wayland compositors,
//!   which is easy to mistake for "Wayland forbids this"; it does not.
//! * **Windows** — xcap, whose window support does work there.
//! * **macOS** — Apple's own overlay, through
//!   [`crate::capture::macos`]. There is nothing to enumerate: `screencapture
//!   -i -W` shows the list, highlights what will be captured, and hands back the
//!   window the user clicked. That also retires the Stage Manager problem, where
//!   the window server answered for a parked window's *tile* — 128×169 for a
//!   1440×900 Slack — and handed over the tilted preview rather than the window.
//!
//! [`capture`] returns `Ok(None)` for a cancel. Only macOS can produce one, but
//! the shape is uniform so callers need no platform knowledge.

use crate::i18n::t;

use image::RgbaImage;

/// A window the system is willing to name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowEntry {
    pub title: String,
    pub app_id: String,
    /// Opaque handle used to capture this window later. Stable for as long as
    /// the window lives, and never shown to the user.
    pub identifier: String,
}

impl WindowEntry {
    /// What the picker shows. Title and application rarely both carry weight —
    /// "Untitled" tells you nothing without the app, and an app name alone
    /// cannot separate three browser windows.
    pub fn label(&self) -> String {
        match (self.title.trim(), self.app_id.trim()) {
            ("", "") => t("(untitled window)").to_string(),
            ("", app) => app.to_string(),
            (title, "") => title.to_string(),
            (title, app) if title == app => title.to_string(),
            (title, app) => format!("{title} — {app}"),
        }
    }
}

#[cfg(target_os = "linux")]
pub fn list() -> Vec<WindowEntry> {
    crate::wl_windows::list()
}

#[cfg(target_os = "linux")]
pub fn capture(identifier: &str) -> Result<Option<RgbaImage>, String> {
    crate::wl_windows::capture(identifier).map(Some)
}

/// macOS enumerates nothing: the choice happens inside Apple's overlay, so a
/// list here would be a second answer to a question the overlay already asks.
#[cfg(target_os = "macos")]
pub fn list() -> Vec<WindowEntry> {
    Vec::new()
}

#[cfg(target_os = "macos")]
pub fn capture(_identifier: &str) -> Result<Option<RgbaImage>, String> {
    crate::capture::macos::run(crate::capture::macos::Shot::Window)
}

/// Windows: xcap enumerates and captures toplevels natively.
#[cfg(target_os = "windows")]
pub fn list() -> Vec<WindowEntry> {
    let Ok(windows) = xcap::Window::all() else {
        return Vec::new();
    };
    windows
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(false))
        .filter_map(|w| {
            let id = w.id().ok()?;
            let app = w.app_name().unwrap_or_default();
            if !worth_offering(
                &app,
                w.y().unwrap_or(0),
                w.width().unwrap_or(0),
                w.height().unwrap_or(0),
            ) {
                return None;
            }
            Some(WindowEntry {
                title: w.title().unwrap_or_default(),
                app_id: app,
                identifier: id.to_string(),
            })
        })
        .collect()
}

/// Is this something a person would want a screenshot of?
///
/// The rules are geometric where they can be, because a name is a name in
/// whatever language the system is set to.
#[cfg(target_os = "windows")]
fn worth_offering(app: &str, y: i32, width: u32, height: u32) -> bool {
    // Our own window would only ever be in the way.
    if app.eq_ignore_ascii_case("shotr") {
        return false;
    }
    // Nothing that thin, that high up, is a window someone means to capture.
    if y <= 0 && height <= 40 {
        return false;
    }
    // Placeholders and badges. A real window is bigger than an icon.
    width >= 64 && height >= 64
}

#[cfg(target_os = "windows")]
pub fn capture(identifier: &str) -> Result<Option<RgbaImage>, String> {
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;
    let target = windows
        .into_iter()
        .find(|w| w.id().is_ok_and(|id| id.to_string() == identifier))
        .ok_or_else(|| "That window is gone".to_string())?;
    let img = target.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    RgbaImage::from_raw(w, h, img.into_raw())
        .map(Some)
        .ok_or_else(|| "Capture data is not a valid image".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, app: &str) -> WindowEntry {
        WindowEntry {
            title: title.into(),
            app_id: app.into(),
            identifier: "x".into(),
        }
    }

    #[test]
    fn a_window_label_survives_missing_pieces() {
        assert_eq!(
            entry("Firefox", "org.mozilla").label(),
            "Firefox — org.mozilla"
        );
        assert_eq!(entry("", "org.mozilla").label(), "org.mozilla");
        assert_eq!(entry("Firefox", "").label(), "Firefox");
        assert_eq!(entry("  ", " ").label(), t("(untitled window)"));
    }

    /// Windows often reports the same string for both fields, and "Slack — Slack"
    /// reads as a bug.
    #[test]
    fn an_identical_title_and_app_is_not_repeated() {
        assert_eq!(entry("Slack", "Slack").label(), "Slack");
    }

    /// macOS answers "which window" inside Apple's overlay, so offering a list
    /// as well would ask twice.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_offers_no_window_list() {
        assert!(
            list().is_empty(),
            "a list here would duplicate the choice the native overlay already makes"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_window_server_furniture_is_not_offered() {
        for (app, y, w, h, what) in [
            ("shotr", 200, 900, 600, "our own window"),
            ("Something", 0, 40, 37, "a strip pinned to the top edge"),
            ("Placeholder", 1116, 1, 1, "a 1x1 placeholder"),
        ] {
            assert!(
                !worth_offering(app, y, w, h),
                "the menu would list {what} as something to screenshot"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn ordinary_windows_survive_the_filter() {
        assert!(
            worth_offering("Code", 109, 1440, 900),
            "an editor window is the ordinary case and must be offered"
        );
    }
}
