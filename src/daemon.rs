//! Tray-only background mode.
//!
//! This process owns the tray icon and nothing else — it never opens a window.
//! That is the whole point: on Wayland a client cannot hide itself, so the only
//! way to keep shotr out of its own screenshot is for no shotr window to exist
//! when the shutter fires. Each capture therefore runs as a fresh short-lived
//! process that grabs the screen *before* it opens anything.

use std::process::Command;

use crate::ipc;
use crate::tray;

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

    let Some((_handle, commands)) = tray::spawn_headless() else {
        eprintln!(
            "Không tìm thấy tray (StatusNotifierItem) trên desktop này.\n\
             Dùng trực tiếp: shotr --capture"
        );
        return 1;
    };

    eprintln!("shotr is running in the system tray. Click the icon to capture.");

    loop {
        // Two sources, one loop: the tray menu and later `shotr` launches.
        if let Ok(command) = commands.try_recv() {
            match command {
                tray::Command::CaptureRegion => spawn_shot(&["--capture"]),
                tray::Command::CaptureFull => spawn_shot(&["--capture", "--full"]),
                tray::Command::CaptureMonitor(i) => {
                    spawn_shot(&["--capture", "--monitor", &i.to_string()])
                }
                tray::Command::OpenFile => spawn_shot(&["--open"]),
                tray::Command::Quit => return 0,
            }
        }
        if let Ok(request) = requests.try_recv() {
            match request {
                ipc::Request::Capture => spawn_shot(&["--capture"]),
                ipc::Request::Show => spawn_shot(&["--capture"]),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
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
    match Command::new(exe).args(args).spawn() {
        Ok(_child) => {}
        Err(e) => eprintln!("Could not run shotr {}: {e}", args.join(" ")),
    }
}
