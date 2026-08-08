//! Listing and capturing individual windows, whichever platform we are on.
//!
//! One API, two implementations, chosen at compile time:
//!
//! * **Linux** — [`crate::wl_windows`], talking the Wayland toplevel protocols
//!   directly. xcap's window path returns an empty list on Wayland compositors,
//!   which is easy to mistake for "Wayland forbids this"; it does not.
//! * **Windows and macOS** — xcap, whose window support does work there.
//!
//! Both capture the window from its own buffer rather than cropping it out of a
//! screenshot, so a window sitting behind another still comes out whole.

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
pub fn capture(identifier: &str) -> Result<RgbaImage, String> {
    crate::wl_windows::capture(identifier)
}

/// Windows and macOS: xcap enumerates and captures toplevels natively.
#[cfg(not(target_os = "linux"))]
pub fn list() -> Vec<WindowEntry> {
    let Ok(windows) = xcap::Window::all() else {
        return Vec::new();
    };
    windows
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(false))
        .filter_map(|w| {
            let id = w.id().ok()?;
            // Our own window would only ever be in the way.
            let app = w.app_name().unwrap_or_default();
            if app.eq_ignore_ascii_case("shotr") {
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

#[cfg(not(target_os = "linux"))]
pub fn capture(identifier: &str) -> Result<RgbaImage, String> {
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;
    let target = windows
        .into_iter()
        .find(|w| w.id().is_ok_and(|id| id.to_string() == identifier))
        .ok_or_else(|| "That window is gone".to_string())?;
    let img = target.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    RgbaImage::from_raw(w, h, img.into_raw())
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

    /// Windows and macOS often report the same string for both fields, and
    /// "Slack — Slack" reads as a bug.
    #[test]
    fn an_identical_title_and_app_is_not_repeated() {
        assert_eq!(entry("Slack", "Slack").label(), "Slack");
    }
}
