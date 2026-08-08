//! Screenshot history: the last N captures, kept on disk so they survive a
//! restart. Entries are named by their capture timestamp, which is also the
//! sort key — no index file to get out of sync with the directory.

use image::RgbaImage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::config_dir;

pub const MAX_ENTRIES: usize = 24;
const THUMB_W: u32 = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub ts: u64,
    pub image: PathBuf,
    pub thumb: PathBuf,
}

fn dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("history"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Store a capture. Best-effort: history is a convenience, never a reason to
/// fail a capture, so every error here is swallowed.
pub fn record(img: &RgbaImage) -> Option<Entry> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;

    let ts = now();
    let image = dir.join(format!("{ts}.png"));
    let thumb = dir.join(format!("{ts}.thumb.png"));

    img.save(&image).ok()?;
    let (tw, th) = thumb_size(img.width(), img.height());
    image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle)
        .save(&thumb)
        .ok()?;

    prune();
    Some(Entry { ts, image, thumb })
}

fn thumb_size(w: u32, h: u32) -> (u32, u32) {
    if w <= THUMB_W {
        return (w.max(1), h.max(1));
    }
    let scale = THUMB_W as f32 / w as f32;
    (THUMB_W, ((h as f32 * scale).round() as u32).max(1))
}

/// Newest first.
pub fn list() -> Vec<Entry> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            // Thumbnails are derived files, not entries in their own right.
            let ts: u64 = name.strip_suffix(".png")?.parse().ok()?;
            Some(Entry {
                ts,
                thumb: dir.join(format!("{ts}.thumb.png")),
                image: path,
            })
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.ts));
    entries
}

/// Drop everything past [`MAX_ENTRIES`], oldest first.
pub fn prune() {
    for entry in list().into_iter().skip(MAX_ENTRIES) {
        let _ = std::fs::remove_file(&entry.image);
        let _ = std::fs::remove_file(&entry.thumb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_keep_aspect_ratio_and_never_upscale() {
        let (w, h) = thumb_size(4000, 2000);
        assert_eq!(w, THUMB_W);
        assert_eq!(h, THUMB_W / 2);

        // Already small enough: left alone.
        assert_eq!(thumb_size(120, 90), (120, 90));
    }

    #[test]
    fn thumbnail_of_an_extreme_panorama_still_has_height() {
        let (_, h) = thumb_size(10000, 30);
        assert!(h >= 1, "height rounded away to zero");
    }
}
