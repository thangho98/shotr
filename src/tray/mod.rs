//! The tray icon, whichever platform we are on.
//!
//! One API, two implementations, chosen at compile time:
//!
//! * **Linux** — [`sni`], speaking StatusNotifierItem over D-Bus through `ksni`.
//!   No GTK. `tray-icon`, used for the other two, hard-requires GTK3 plus
//!   libappindicator on Linux, which would hand this project a C build
//!   dependency on the one platform that needs the tray most.
//! * **Windows and macOS** — [`native`], on `tray-icon`, whose backends there
//!   are Shell_NotifyIcon and NSStatusItem and need no such thing.
//!
//! [`run`] is the whole interface: it owns the thread until the user quits.
//! That shape is not a preference — `tray-icon` needs a platform event loop on
//! the thread that created the icon, and on macOS that thread must be the main
//! one, so the tray cannot be a thing the daemon merely holds.

use std::time::Duration;

#[cfg(not(target_os = "linux"))]
mod native;
#[cfg(target_os = "linux")]
mod sni;

#[cfg(not(target_os = "linux"))]
pub use native::run;
#[cfg(target_os = "linux")]
pub use sni::run;

/// How long the loop sleeps between idle passes. Long enough to cost nothing,
/// short enough that a tray click and a shortcut both feel immediate.
const POLL: Duration = Duration::from_millis(80);

/// What the menu can ask for. These are the *only* way to choose what gets
/// captured: the editor shows what it was given and offers no way to change it,
/// because a source picker there re-opens a question the user already answered.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    /// Freeze the screen, then drag a region on whichever monitor holds the
    /// pointer.
    CaptureRegion,
    /// Every monitor at once, straight to the editor.
    CaptureFull,
    /// One monitor whole, straight to the editor.
    CaptureMonitor(usize),
    /// One window from its own buffer, straight to the editor.
    CaptureWindow(String),
    OpenFile,
    Quit,
}
