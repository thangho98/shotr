//! Best-effort lookup of the current desktop wallpaper, for the "Desktop"
//! background preset.
//!
//! This is genuinely best-effort. COSMIC (the desktop this was developed on)
//! can point its background at a *directory* and rotate through it on a timer,
//! in which case nothing in the config says which image is on screen right now.
//! We return the first image in alphabetical order — COSMIC's default sampling
//! is `Alphanumeric` — and the user can always fall back to Custom.

#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

/// The wallpaper currently on screen, if it can be worked out.
#[cfg(target_os = "linux")]
pub fn current() -> Option<PathBuf> {
    cosmic().or_else(gnome)
}

/// macOS has no file to point at, so the wallpaper is read off the screen.
///
/// `System Events` can name a *still* wallpaper, and that is what this used to
/// ask. It answers `missing value` for a dynamic or aerial one — measured on a
/// Mac whose wallpaper is an aerial video: the store at
/// `com.apple.wallpaper/Store/Index.plist` names the provider
/// `com.apple.wallpaper.choice.aerials` and an `assetID`, with `Files` empty and
/// no still frame downloaded anywhere. There is no path to return, so the
/// "Desktop" swatch came out flat grey and the feature looked missing.
///
/// What is always true is that the wallpaper is *on screen*: the Dock owns one
/// `Wallpaper-<UUID>` window per display, below every application window, and
/// `screencapture -l` will hand over its pixels. That works for a still, a
/// dynamic wallpaper and an aerial alike, and it needs no permission beyond the
/// screen recording grant capture already has.
///
/// Cached for the life of the process: this is called from the render path, and
/// the alternative is a subprocess per frame. A wallpaper changed mid-session
/// therefore needs a restart to show up — a fair trade for a background that
/// does not flicker while an aerial one animates underneath it.
#[cfg(target_os = "macos")]
pub fn current() -> Option<PathBuf> {
    use std::sync::OnceLock;
    static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHED.get_or_init(macos::grab).clone()
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PathBuf;
    use objc2_core_foundation::{CFNumber, CFRetained, CFString, CFType};
    use objc2_core_graphics::{
        CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowName, kCGWindowNumber,
        kCGWindowOwnerName,
    };

    pub(super) fn grab() -> Option<PathBuf> {
        let id = wallpaper_window()?;
        let dest = std::env::temp_dir().join(format!("shotr-wallpaper-{id}.png"));
        let ok = std::process::Command::new("/usr/sbin/screencapture")
            .args([
                "-x", // no shutter: nothing was captured on the user's behalf
                "-o", // no window shadow, which would be baked into the edges
                "-l",
                &id.to_string(),
            ])
            .arg(&dest)
            .status()
            .ok()?
            .success();
        (ok && dest.is_file()).then_some(dest)
    }

    /// The window id of the largest `Wallpaper-*` window the Dock owns.
    ///
    /// Largest because there is one per display and any of them is a reasonable
    /// backdrop; the biggest is the one most likely to cover a shot without being
    /// upscaled. `kCGWindowListOptionAll` rather than `OnScreenOnly`, because a
    /// wallpaper window on a display that is asleep is still the right image.
    fn wallpaper_window() -> Option<u32> {
        let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionAll, 0)?;
        let mut best: Option<(u32, i64)> = None;
        for i in 0..list.count() {
            // SAFETY: the array came from CoreGraphics, which documents it as an
            // array of window-description dictionaries, and `i` is in range.
            let dict = unsafe { list.value_at_index(i) };
            let Some(dict) = std::ptr::NonNull::new(dict.cast_mut()) else {
                continue;
            };
            let dict: &objc2_core_foundation::CFDictionary = unsafe { dict.cast().as_ref() };

            if string_of(dict, unsafe { kCGWindowOwnerName }).as_deref() != Some("Dock") {
                continue;
            }
            let name = string_of(dict, unsafe { kCGWindowName })?;
            if !name.starts_with("Wallpaper") {
                continue;
            }
            let Some(id) = number_of(dict, unsafe { kCGWindowNumber }) else {
                continue;
            };
            if best.is_none_or(|(_, seen)| id > seen) {
                best = Some((id as u32, id));
            }
        }
        best.map(|(id, _)| id)
    }

    fn value_of(
        dict: &objc2_core_foundation::CFDictionary,
        key: &CFString,
    ) -> Option<CFRetained<CFType>> {
        // SAFETY: the key is a CoreGraphics constant and the dictionary is one of
        // CoreGraphics' own; a missing key gives a null pointer, which is checked.
        let raw = unsafe { dict.value(key as *const CFString as *const std::ffi::c_void) };
        let raw = std::ptr::NonNull::new(raw.cast_mut())?;
        Some(unsafe { CFRetained::retain(raw.cast::<CFType>()) })
    }

    fn string_of(dict: &objc2_core_foundation::CFDictionary, key: &CFString) -> Option<String> {
        let value = value_of(dict, key)?;
        let text = value.downcast_ref::<CFString>()?;
        Some(text.to_string())
    }

    fn number_of(dict: &objc2_core_foundation::CFDictionary, key: &CFString) -> Option<i64> {
        let value = value_of(dict, key)?;
        let number = value.downcast_ref::<CFNumber>()?;
        number.as_i64()
    }
}

/// Windows records the path the shell last wrote out, which is a re-encoded
/// copy of the wallpaper rather than the original file — good enough to show.
#[cfg(target_os = "windows")]
pub fn current() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let path = PathBuf::from(appdata).join("Microsoft/Windows/Themes/TranscodedWallpaper");
    path.is_file().then_some(path)
}

#[cfg(target_os = "linux")]
/// COSMIC stores a RON blob at
/// `~/.config/cosmic/com.system76.CosmicBackground/v1/all`, e.g.
/// `( output: "all", source: Path("/usr/share/backgrounds"), ... )`
fn cosmic() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    let path = dirs
        .config_dir()
        .join("cosmic/com.system76.CosmicBackground/v1/all");
    let text = std::fs::read_to_string(path).ok()?;

    let start = text.find("Path(\"")? + "Path(\"".len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    let source = PathBuf::from(&rest[..end]);

    if source.is_file() {
        Some(source)
    } else if source.is_dir() {
        first_image_in(&source)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn gnome() -> Option<PathBuf> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-uri"])
        .output()
        .ok()?;
    let raw = String::from_utf8(out.stdout).ok()?;
    let uri = raw.trim().trim_matches('\'').trim_matches('"');
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let path = PathBuf::from(percent_decode(path));
    path.is_file().then_some(path)
}

#[cfg(target_os = "linux")]
fn first_image_in(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "bmp"
                    )
                })
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files.into_iter().next()
}

#[cfg(test)]
mod tests {
    /// Asking for the wallpaper must never crash, and must never name a file
    /// that is not there — every caller opens the path without re-checking.
    ///
    /// This is deliberately weak about the *answer*: a build machine has no
    /// desktop session and `None` is the right result there. What it is strict
    /// about is that the macOS path walks a CoreFoundation array by raw pointer,
    /// and a mistake there is a crash rather than a wrong answer.
    #[test]
    fn asking_for_the_wallpaper_is_safe_and_never_lies() {
        if let Some(path) = super::current() {
            assert!(
                path.is_file(),
                "returned {} , which nothing can open",
                path.display()
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
