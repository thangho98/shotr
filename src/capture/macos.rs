//! Screen capture on macOS through `/usr/sbin/screencapture`.
//!
//! Measured, not assumed: Shottr runs `screencapture -x -i FILE` and Xnapper
//! `screencapture -i -o -a FILE`, and `otool -L` on Xnapper shows zero
//! references to ScreenCaptureKit. The crosshair, the live dimensions, space to
//! switch to window mode and escape to cancel are all Apple's UI, which is why
//! shotr does not draw its own picker here.
//!
//! Nothing in this module needs macOS 14: the system tool has been there for
//! decades, so `LSMinimumSystemVersion` stays where it was.
//!
//! TCC attributes screen recording to the *responsible* process, so the
//! `screencapture` child inherits the grant given to `shotr.app`. Running the
//! binary straight from a terminal instead borrows the terminal's grant — which
//! is why permission has to be granted to the bundle, not to the executable.

use std::path::{Path, PathBuf};
use std::process::Command;

use image::RgbaImage;

use super::{MonitorInfo, MonitorShot};

/// What to point the system tool at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shot {
    /// Drag a rectangle. Space switches to window mode inside Apple's overlay.
    Region,
    /// Click a window, shadow and attached panels left out.
    Window,
    /// One whole display, by index into [`list_monitors`].
    Display(usize),
}

/// The exact command line for one source.
///
/// `-x` silences the shutter, `-o` drops the window shadow and `-a` drops
/// attached windows — the same flags the two shipping apps use.
///
/// No `-r`: it was measured to change only the dpi metadata, never a pixel, and
/// `export` re-encodes so the source dpi never reaches the output file.
fn args(shot: Shot, out: &Path) -> Vec<String> {
    let mut a = vec!["-x".to_string()];
    match shot {
        Shot::Region => a.push("-i".into()),
        Shot::Window => {
            a.extend(["-i", "-W", "-o", "-a"].map(String::from));
        }
        // `-D` is 1-based ("1 is main") and follows CGGetActiveDisplayList
        // order — verified across two displays.
        Shot::Display(i) => a.extend(["-D".to_string(), (i + 1).to_string()]),
    }
    a.push(out.to_string_lossy().into_owned());
    a
}

fn scratch_path() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("shotr-{}-{nanos}.png", std::process::id()))
}

/// Read back what the tool wrote, then remove it.
///
/// A missing file is a cancel, not a failure: escape out of the overlay and
/// `screencapture` exits having written nothing.
fn read_back(path: &Path) -> Result<Option<RgbaImage>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    let _ = std::fs::remove_file(path);
    Ok(Some(img))
}

/// Run the system tool and hand back what it captured, or `None` if the user
/// cancelled.
pub fn run(shot: Shot) -> Result<Option<RgbaImage>, String> {
    let path = scratch_path();
    // The exit status cannot be trusted to tell a cancel from a refusal — both
    // are non-zero and both write nothing — so the file is what decides. A
    // missing Screen Recording grant therefore looks like a cancel here, and is
    // explained in the Preferences window instead.
    Command::new("/usr/sbin/screencapture")
        .args(args(shot, &path))
        .status()
        .map_err(|e| format!("Could not run screencapture: {e}"))?;
    read_back(&path)
}

pub fn list_monitors() -> Vec<MonitorInfo> {
    active_displays()
        .iter()
        .enumerate()
        .map(|(index, _)| MonitorInfo {
            index,
            name: crate::i18n::tf("Monitor {n}", &[("n", &(index + 1).to_string())]),
        })
        .collect()
}

pub fn capture_monitor(index: usize) -> Result<RgbaImage, String> {
    run(Shot::Display(index))?.ok_or_else(|| "Nothing was captured".to_string())
}

/// Every display, each paired with the rectangle CoreGraphics reports for it.
///
/// One `screencapture` per display rather than the multi-filename form, which is
/// undocumented and untested here. At one to three displays the extra spawns
/// cost nothing.
pub fn capture_shots() -> Result<Vec<MonitorShot>, String> {
    let displays = active_displays();
    if displays.is_empty() {
        return Err("No monitor found".into());
    }
    let mut out = Vec::new();
    for (index, &id) in displays.iter().enumerate() {
        let image = capture_monitor(index)?;
        let bounds = display_bounds(id);
        // Logical points from CGDisplayBounds against backing pixels from the
        // capture: exactly the pair `super::layout` corrects, and the reason a
        // Retina laptop beside a 1x panel composites correctly.
        out.push(MonitorShot {
            name: crate::i18n::tf("Monitor {n}", &[("n", &(index + 1).to_string())]),
            image,
            reported: bounds,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- CoreGraphics
//
// Declared by hand rather than adding a binding crate, the way `ipc` declares
// the Win32 calls it needs. These are current API — the obsoleted one is
// `CGWindowListCreateImage`, which is what leaving xcap behind avoids.

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

/// Display IDs in the order `screencapture -D` counts them.
fn active_displays() -> Vec<u32> {
    const MAX: u32 = 16;
    let mut ids = [0u32; MAX as usize];
    let mut count: u32 = 0;
    let err = unsafe { CGGetActiveDisplayList(MAX, ids.as_mut_ptr(), &mut count) };
    if err != 0 {
        return Vec::new();
    }
    ids[..count as usize].to_vec()
}

/// `(x, y, w, h)` in logical points, primary display's top-left at the origin.
fn display_bounds(id: u32) -> (i32, i32, u32, u32) {
    let r = unsafe { CGDisplayBounds(id) };
    (
        r.origin.x as i32,
        r.origin.y as i32,
        r.size.width.max(0.0) as u32,
        r.size.height.max(0.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flags are the whole contract with the system tool. `-o -a` vanishing
    /// would silently put window shadows back into every window capture.
    #[test]
    fn each_source_builds_its_own_command_line() {
        let p = Path::new("/tmp/x.png");
        assert_eq!(args(Shot::Region, p), ["-x", "-i", "/tmp/x.png"]);
        assert_eq!(
            args(Shot::Window, p),
            ["-x", "-i", "-W", "-o", "-a", "/tmp/x.png"]
        );
        assert!(
            !args(Shot::Region, p).contains(&"-r".to_string()),
            "-r only touches dpi metadata and export re-encodes; it must stay off"
        );
    }

    /// Off by one here captures the wrong screen without any error.
    #[test]
    fn display_numbering_is_one_based() {
        let p = Path::new("/tmp/x.png");
        assert_eq!(args(Shot::Display(0), p), ["-x", "-D", "1", "/tmp/x.png"]);
        assert_eq!(args(Shot::Display(2), p), ["-x", "-D", "3", "/tmp/x.png"]);
    }

    /// Escape out of Apple's overlay writes no file. That is a cancel, and must
    /// not surface as an error the user has to dismiss.
    #[test]
    fn a_missing_file_is_a_cancel_not_a_failure() {
        let missing = std::env::temp_dir().join("shotr-does-not-exist-1234.png");
        assert!(matches!(read_back(&missing), Ok(None)));
    }

    /// The machine this runs on must agree with what `-D` will be handed.
    #[test]
    fn the_display_list_and_its_bounds_are_consistent() {
        let ids = active_displays();
        assert!(!ids.is_empty(), "a Mac running tests has at least one display");
        for &id in &ids {
            let (_, _, w, h) = display_bounds(id);
            assert!(w > 0 && h > 0, "display {id} reported a zero-sized rectangle");
        }
    }
}
