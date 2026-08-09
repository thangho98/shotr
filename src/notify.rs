//! A line of text from a process with no window to put it in.
//!
//! Two paths need this and neither can draw: `--capture --copy` renders to the
//! clipboard and exits, and the daemon binds a hotkey at startup. Both look
//! exactly like nothing happening, and both were mistaken for a broken feature
//! before this existed.
//!
//! Shelling out rather than binding a notification API: this is a handful of
//! words a few times a session, and neither `osascript` nor `notify-send` is a
//! build dependency.

/// Best-effort. A desktop with no notification daemon is not an error worth
/// reporting to someone who cannot act on it.
pub fn show(body: &str) {
    #[cfg(target_os = "macos")]
    {
        // `{body:?}` quotes and escapes, which is what an AppleScript string
        // literal needs.
        let script = format!("display notification {body:?} with title \"shotr\"");
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args(["shotr", body])
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = body;
}
