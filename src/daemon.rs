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

    // Two sources, one loop: the tray menu, and later `shotr` launches. Which
    // loop it is differs by platform — see `tray::run`.
    tray::run(|command| {
        match command {
            Some(Command::CaptureRegion) => spawn_shot(&["--capture"]),
            Some(Command::CaptureFull) => spawn_shot(&["--capture", "--full"]),
            Some(Command::CaptureMonitor(i)) => {
                spawn_shot(&["--capture", "--full", "--monitor", &i.to_string()])
            }
            // The identifier survives the trip because both backends hand out
            // one meant to be shared: `ext_foreign_toplevel_list_v1` says so in
            // as many words, and elsewhere it is the window id the system uses.
            Some(Command::CaptureWindow(id)) => spawn_shot(&["--capture", "--window", &id]),
            Some(Command::OpenFile) => spawn_shot(&["--open"]),
            Some(Command::Quit) => return false,
            None => {
                if let Ok(request) = requests.try_recv() {
                    match request {
                        ipc::Request::Capture | ipc::Request::Show => spawn_shot(&["--capture"]),
                    }
                }
            }
        }
        true
    })
}

/// Launch a capture process. Detached: the daemon must not block on it, and it
/// outliving or predeceasing us is fine either way.
fn spawn_shot(args: &[&str]) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Could not resolve the shotr executable: {e}");
            return;
        }
    };
    match Process::new(exe).args(args).spawn() {
        Ok(_child) => {}
        Err(e) => eprintln!("Could not run shotr {}: {e}", args.join(" ")),
    }
}
