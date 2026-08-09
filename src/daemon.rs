//! Tray-only background mode.
//!
//! This process owns the tray icon and nothing else — it never opens a window.
//! On Wayland that is not a style choice: a client cannot hide itself, so the
//! only way to keep shotr out of its own screenshot is for no shotr window to
//! exist when the shutter fires. Each capture therefore runs as a fresh
//! short-lived process that grabs the screen *before* it opens anything.
//!
//! Windows and macOS could hide a window instead, but they take the same route.
//! One capture path that is known to work everywhere beats two, and the tray
//! wants a resident process on those platforms regardless.

use std::process::Command as Process;

use crate::ipc;
use crate::tray::{self, Command};

/// Run until the tray asks us to quit. Returns the process exit code.
pub fn run() -> i32 {
    // The socket doubles as the single-instance lock: a second `shotr` hands
    // its request over and exits instead of stacking up tray icons.
    let requests = match ipc::start(ipc::Request::Capture) {
        ipc::Instance::Secondary => {
            eprintln!("shotr is already running — asked it to take a shot.");
            return 0;
        }
        ipc::Instance::Primary(rx) => rx,
    };

    // Preferences is the one window a second copy of makes no sense for: two of
    // them would write the same file and disagree. Captures are the opposite —
    // asking for two shots means two shots — so only this one is tracked.
    let mut prefs_window: Option<std::process::Child> = None;

    // Two sources, one loop: the tray menu, and later `shotr` launches. Which
    // loop it is differs by platform — see `tray::run`.
    tray::run(move |command| {
        match command {
            Some(Command::Quit) => return false,
            Some(Command::Preferences) => {
                let open = prefs_window
                    .as_mut()
                    .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
                if !open {
                    prefs_window = spawn_shot(&Command::Preferences.args());
                }
            }
            // Every other entry is one fresh process. The identifier inside a
            // window command survives the trip because every backend hands out
            // one meant to be shared: `ext_foreign_toplevel_list_v1` says so in
            // as many words, and elsewhere it is the window id the system uses.
            Some(command) => {
                spawn_shot(&command.args());
            }
            None => {
                if let Ok(request) = requests.try_recv() {
                    match request {
                        ipc::Request::Capture | ipc::Request::Show => {
                            spawn_shot(&Command::CaptureRegion.args());
                        }
                    }
                }
            }
        }
        true
    })
}

/// Launch a shotr process. Detached: the daemon must not block on it, and it
/// outliving or predeceasing us is fine either way. The handle comes back only
/// so a window that should be unique can be checked for.
fn spawn_shot(args: &[String]) -> Option<std::process::Child> {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not resolve the shotr executable: {e}");
            return None;
        }
    };
    match Process::new(exe).args(args).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("Could not run shotr {}: {e}", args.join(" "));
            None
        }
    }
}
