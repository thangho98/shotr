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
/// `CGWindowListCopyWindowInfo` is not a list of application windows. It is
/// every layer the window server composites, and xcap passes it through
/// untouched — `Window::all` has no filter at all. Measured on one three-screen
/// Mac, the unfiltered list offered: `Item-0` once per menu bar icon, `Menubar`
/// (1728×37 at y=0), `MenuBarCover` (1728×38 at y=-1), `StatusIndicator`
/// (10×19), one `App Icon Window` (64×64) and one `Gesture Blocking Overlay`
/// per window parked in Stage Manager, and a 1×1 window at (1e9, 1e9).
///
/// The rules are geometric where they can be, because a name is a name in
/// whatever language the system is set to. `WindowManager` and `Window Server`
/// are the exception: those are process names, which do not translate.
#[cfg(not(target_os = "linux"))]
fn worth_offering(app: &str, y: i32, width: u32, height: u32) -> bool {
    // Our own window would only ever be in the way.
    if app.eq_ignore_ascii_case("shotr")
        || app == "WindowManager"
        || app == "Window Server"
    {
        return false;
    }
    // The menu bar and everything sitting in it. Nothing that thin, that high
    // up, is a window someone means to capture.
    if y <= 0 && height <= 40 {
        return false;
    }
    // Placeholders and badges. A real window is bigger than an icon.
    width >= 64 && height >= 64
}

#[cfg(not(target_os = "linux"))]
pub fn capture(identifier: &str) -> Result<RgbaImage, String> {
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;
    let target = windows
        .into_iter()
        .find(|w| w.id().is_ok_and(|id| id.to_string() == identifier))
        .ok_or_else(|| "That window is gone".to_string())?;
    #[cfg(target_os = "macos")]
    raise(&target);
    let img = target.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    RgbaImage::from_raw(w, h, img.into_raw())
        .ok_or_else(|| "Capture data is not a valid image".to_string())
}

/// Bring a window's application forward, and wait for it to arrive.
///
/// Stage Manager parks windows in a strip, and the window server then answers
/// for the *tile*: Slack, really 1440×900, came back as 128×169, and what it
/// handed over was the tilted preview Stage Manager draws rather than the
/// window. Apple's own `screencapture -l<id>` returns the same skewed image, so
/// there is nothing to fix in how the shot is taken — the window has to be on
/// the main stage before anyone can photograph it, and asking for that is the
/// only lever there is.
///
/// Done for every window, not only parked ones, because a parked window is not
/// distinguishable from a small one: all we can read is a size, and 128×169 is
/// a legitimate size for a window to be.
///
/// The wait polls rather than sleeping a fixed time — the animation's length is
/// not ours to know — and gives up rather than hanging, because a tilted
/// screenshot still beats no screenshot.
#[cfg(target_os = "macos")]
fn raise(window: &xcap::Window) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    use std::time::{Duration, Instant};

    /// Long enough for Stage Manager's slide, measured generously.
    const SETTLE: Duration = Duration::from_millis(900);
    const STEP: Duration = Duration::from_millis(60);

    let Ok(pid) = window.pid() else {
        return;
    };
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
    else {
        return;
    };
    // `ActivateAllWindows` alone is ignored here: macOS only lets an
    // application activate another when the *asking* one is already frontmost,
    // and this process has no window at all yet — it is about to take a
    // screenshot. `ActivateIgnoringOtherApps` is the way past that.
    //
    // Its deprecation notice claims it "will have no effect" from macOS 14. On
    // macOS 15 it plainly does: without it Slack stayed at 128×152 in the Stage
    // Manager strip and was photographed there, with it the window came forward
    // and came out at 1337×1043. Measured, not assumed — and there is no
    // replacement offered for activating from a process that owns no window.
    #[allow(deprecated)]
    let ok = app.activateWithOptions(
        NSApplicationActivationOptions::ActivateAllWindows
            | NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );
    if !ok {
        eprintln!("Could not bring pid {pid} forward; capturing it where it is.");
    }

    // Two identical readings while the app holds the foreground: one alone
    // would catch the pause before the animation starts.
    let deadline = Instant::now() + SETTLE;
    let mut previous = None;
    while Instant::now() < deadline {
        std::thread::sleep(STEP);
        let size = (window.width().unwrap_or(0), window.height().unwrap_or(0));
        if app.isActive() && previous == Some(size) {
            return;
        }
        previous = Some(size);
    }
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

    /// Every one of these was offered by the tray menu on a real Mac before
    /// there was a filter, each with a name and an id, indistinguishable from a
    /// window until you captured it.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_window_server_furniture_is_not_offered() {
        use super::worth_offering;
        for (app, y, w, h, what) in [
            ("Control Center", 0, 76, 37, "a menu bar clock"),
            ("Lightshot Screenshot", 0, 40, 37, "a menu bar icon"),
            ("Bartender 5", -1, 1728, 38, "the strip covering the menu bar"),
            ("Window Server", 0, 1728, 37, "the menu bar itself"),
            ("Window Server", 9, 10, 19, "the recording indicator"),
            ("WindowManager", 738, 64, 64, "a Stage Manager app badge"),
            ("BetterDisplay", 1116, 1, 1, "a 1x1 placeholder"),
            ("shotr", 200, 900, 600, "our own window"),
        ] {
            assert!(
                !worth_offering(app, y, w, h),
                "the menu would list {what} as something to screenshot"
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn ordinary_windows_survive_the_filter() {
        use super::worth_offering;
        assert!(
            worth_offering("Code", 109, 1440, 900),
            "an editor window is the ordinary case and must be offered"
        );
        // Stage Manager shrinks a parked window to a tile this size. It stays
        // on the list: it is still that application's window, and dropping it
        // would mean the app cannot be picked at all while parked.
        assert!(
            worth_offering("Slack", 225, 128, 169),
            "a window parked in Stage Manager is still the app's window"
        );
    }
}
