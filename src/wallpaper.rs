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

/// macOS keeps it in a per-user sqlite database; `osascript` is the supported
/// way to ask and needs no extra dependency.
#[cfg(target_os = "macos")]
pub fn current() -> Option<PathBuf> {
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get picture of current desktop",
        ])
        .output()
        .ok()?;
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    path.is_file().then_some(path)
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
