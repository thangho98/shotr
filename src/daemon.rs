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

use crate::hotkey::Hotkeys;
use crate::i18n::tf;
use crate::ipc;
use crate::settings::Prefs;
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

    // Before `tray::run`, not on its first tick. The status item is the other
    // way round — macOS refuses one until NSApplication is up — but a Carbon
    // hotkey wants the opposite: registered from the main thread *before* the
    // event loop takes it over. Both orderings were measured; see
    // `plans/reports/260809-1151-macos-global-hotkeys.md`.
    let mut prefs = Prefs::load();

    // A fresh install gets one working shortcut rather than a Preferences pane
    // it has to find first — and is told which, because macOS cannot say whether
    // the combination was already spoken for.
    let system = crate::hotkey::system_bindings();
    if let Some((action, hotkey)) = crate::hotkey::first_run_binding(&prefs, &system) {
        prefs.hotkeys.push((action, hotkey.to_string()));
        prefs.hotkeys_initialised = true;
        prefs.save();
        crate::notify::show(&tf(
            "{keys} now captures a region. Change it in Preferences.",
            &[("keys", &hotkey.to_string())],
        ));
    } else if !prefs.hotkeys_initialised {
        // Nothing was free, or this platform leaves it to the desktop. Either
        // way the question is settled and must not be asked again.
        prefs.hotkeys_initialised = true;
        prefs.save();
    }

    let mut hotkeys = Hotkeys::new();
    hotkeys.rebind(&prefs.hotkeys);

    // Three sources, one loop: the tray menu, later `shotr` launches, and the
    // global hotkeys. Which loop it is differs by platform — see `tray::run`.
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
                // A hotkey and a menu click become the same command line here,
                // so everything downstream — the fresh process, the poke at an
                // editor already up — is the path the tray has always used.
                for action in hotkeys.pressed() {
                    spawn_shot(&action.command().args());
                }
                if let Ok(request) = requests.try_recv() {
                    match request {
                        ipc::Request::Capture | ipc::Request::Show => {
                            spawn_shot(&Command::CaptureRegion.args());
                        }
                        ipc::Request::ReloadHotkeys => {
                            hotkeys.rebind(&Prefs::load().hotkeys);
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
