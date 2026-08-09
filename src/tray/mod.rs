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

/// What the menu can ask for.
///
/// The capture entries are the *only* way to choose what gets captured: the
/// editor shows what it was given and offers no way to change it, because a
/// source picker there re-opens a question the user already answered.
///
/// The rest are doors into the app that used to exist only on the windowed
/// Select screen. macOS no longer reaches that screen — Apple's overlay picks a
/// region before any window opens — so without them History, "Open image…" and
/// "From clipboard" would be unreachable there. They are added on every platform
/// rather than only macOS: one menu that means the same thing everywhere.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    /// Freeze the screen, then drag a region on whichever monitor holds the
    /// pointer.
    CaptureRegion,
    /// Every monitor at once, straight to the editor.
    CaptureFull,
    /// One monitor whole, straight to the editor.
    CaptureMonitor(usize),
    /// One window, straight to the editor. The identifier is empty on macOS,
    /// where the native overlay does the choosing.
    CaptureWindow(String),
    OpenFile,
    /// Recent shots, to reopen one.
    History,
    /// Whatever image is on the clipboard.
    FromClipboard,
    Preferences,
    Quit,
}

impl Command {
    /// The `shotr …` invocation this stands for. Every entry is one fresh
    /// process, which is the same shape capture has always used.
    pub fn args(&self) -> Vec<String> {
        match self {
            Command::CaptureRegion => vec!["--capture".into()],
            Command::CaptureFull => vec!["--capture".into(), "--full".into()],
            Command::CaptureMonitor(i) => vec![
                "--capture".into(),
                "--full".into(),
                "--monitor".into(),
                i.to_string(),
            ],
            Command::CaptureWindow(id) => {
                vec!["--capture".into(), "--window".into(), id.clone()]
            }
            Command::OpenFile => vec!["--open".into()],
            Command::History => vec!["--history".into()],
            Command::FromClipboard => vec!["--clipboard".into()],
            Command::Preferences => vec!["--settings".into()],
            Command::Quit => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    /// Every command has to name a real invocation. A new variant that forgets
    /// one would silently launch `shotr` with no arguments — the tray daemon —
    /// and appear to do nothing.
    #[test]
    fn every_command_maps_to_one_invocation() {
        for command in [
            Command::CaptureRegion,
            Command::CaptureFull,
            Command::CaptureMonitor(2),
            Command::CaptureWindow("42".into()),
            Command::OpenFile,
            Command::History,
            Command::FromClipboard,
            Command::Preferences,
        ] {
            let args = command.args();
            assert!(
                !args.is_empty(),
                "{command:?} would launch the daemon instead of doing anything"
            );
            assert!(
                args[0].starts_with("--"),
                "{command:?} must start with a flag, got {args:?}"
            );
        }
        assert!(
            Command::Quit.args().is_empty(),
            "Quit is handled in the daemon loop and must not spawn anything"
        );
    }

    #[test]
    fn a_monitor_command_carries_its_index() {
        assert_eq!(
            Command::CaptureMonitor(3).args(),
            ["--capture", "--full", "--monitor", "3"]
        );
    }
}
