//! Saving and clipboard.
//!
//! Clipboard note: on Wayland `arboard` delegates to `wl-clipboard-rs` with
//! `foreground(false)`, which forks a helper process to serve the selection.
//! The copied image therefore survives our own exit, so "copy and close" is
//! safe here.

use crate::i18n::t;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbImage, RgbaImage};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::{ExportFormat, Settings, pictures_dir};

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Expand the filename template. Supported tokens: `{unix}`, `{date}`,
/// `{time}`. Anything else is left alone so a stray brace is not fatal.
pub fn expand_template(template: &str, unix: u64) -> String {
    let (date, time) = civil_from_unix(unix);
    let name = template
        .replace("{unix}", &unix.to_string())
        .replace("{date}", &date)
        .replace("{time}", &time);
    let name = name.trim();
    if name.is_empty() {
        format!("shotr-{unix}")
    } else {
        // Never let a template escape the chosen directory.
        name.replace(['/', '\\'], "-")
    }
}

/// Days-to-calendar conversion (UTC), so no chrono dependency is needed for one
/// filename. Based on Howard Hinnant's `civil_from_days`.
fn civil_from_unix(unix: u64) -> (String, String) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!(
            "{:02}-{:02}-{:02}",
            secs / 3600,
            (secs / 60) % 60,
            secs % 60
        ),
    )
}

/// `~/Pictures/shotr/<template>.<ext>`
pub fn default_path(settings: &Settings) -> PathBuf {
    let name = expand_template(&settings.filename_template, timestamp());
    pictures_dir()
        .join("shotr")
        .join(format!("{name}.{}", settings.format.extension()))
}

pub fn save(img: &RgbaImage, path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let writer = BufWriter::new(file);

    // Pick the format from the extension the user actually chose in Save As,
    // falling back to the configured one.
    let format = ExportFormat::from_extension(path.extension().and_then(|e| e.to_str()))
        .unwrap_or(settings.format);

    match format {
        ExportFormat::Png => {
            let compression = if settings.png_max_compression {
                CompressionType::Best
            } else {
                CompressionType::Default
            };
            PngEncoder::new_with_quality(writer, compression, FilterType::Adaptive)
                .write_image(
                    img.as_raw(),
                    img.width(),
                    img.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())
        }
        ExportFormat::Jpeg => {
            // JPEG has no alpha channel; flatten first so transparent corners
            // come out white instead of black.
            let flat = flatten_on_white(img);
            JpegEncoder::new_with_quality(writer, settings.jpeg_quality.clamp(1, 100))
                .write_image(
                    flat.as_raw(),
                    flat.width(),
                    flat.height(),
                    ExtendedColorType::Rgb8,
                )
                .map_err(|e| e.to_string())
        }
        ExportFormat::Webp => WebPEncoder::new_lossless(writer)
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string()),
    }
}

/// Composite over white. Used for formats without an alpha channel.
pub fn flatten_on_white(img: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
        let a = px.0[3] as f32 / 255.0;
        let blend = |c: u8| {
            ((c as f32 * a) + 255.0 * (1.0 - a))
                .round()
                .clamp(0.0, 255.0) as u8
        };
        out.put_pixel(
            x,
            y,
            image::Rgb([blend(px.0[0]), blend(px.0[1]), blend(px.0[2])]),
        );
    }
    out
}

/// Ask the user where to put the file. Returns `None` if they cancelled.
pub fn save_as_dialog(settings: &Settings) -> Option<PathBuf> {
    let name = expand_template(&settings.filename_template, timestamp());
    rfd::FileDialog::new()
        .set_file_name(format!("{name}.{}", settings.format.extension()))
        .set_directory(pictures_dir())
        .add_filter("PNG", &["png"])
        .add_filter("JPEG", &["jpg", "jpeg"])
        .add_filter("WebP", &["webp"])
        .save_file()
}

pub fn open_image_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(t("Image"), &["png", "jpg", "jpeg", "webp", "bmp"])
        .set_directory(pictures_dir())
        .pick_file()
}

pub fn copy(img: &RgbaImage, clipboard: &mut Option<arboard::Clipboard>) -> Result<(), String> {
    let cb = clipboard.as_mut().ok_or(t("Cannot reach the clipboard"))?;
    let data = arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: std::borrow::Cow::Borrowed(img.as_raw()),
    };
    cb.set_image(data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_tokens_expand() {
        // 2023-11-14T22:13:20 UTC
        let out = expand_template("shotr-{date}-{time}", 1_700_000_000);
        assert_eq!(out, "shotr-2023-11-14-22-13-20");
    }

    #[test]
    fn unix_token_expands() {
        assert_eq!(expand_template("shot{unix}", 42), "shot42");
    }

    #[test]
    fn the_epoch_converts_correctly() {
        let (date, time) = civil_from_unix(0);
        assert_eq!(date, "1970-01-01");
        assert_eq!(time, "00-00-00");
    }

    #[test]
    fn a_leap_day_converts_correctly() {
        // 2024-02-29T12:00:00 UTC
        let (date, time) = civil_from_unix(1_709_208_000);
        assert_eq!(date, "2024-02-29");
        assert_eq!(time, "12-00-00");
    }

    #[test]
    fn an_empty_template_still_yields_a_name() {
        assert_eq!(expand_template("   ", 7), "shotr-7");
    }

    #[test]
    fn path_separators_cannot_escape_the_directory() {
        let out = expand_template("../../etc/passwd", 1);
        assert!(!out.contains('/'), "got {out}");
        assert!(!out.contains('\\'));
    }

    #[test]
    fn unknown_tokens_are_left_alone() {
        assert_eq!(expand_template("a{nope}b", 1), "a{nope}b");
    }

    #[test]
    fn flattening_puts_transparency_on_white_not_black() {
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0])); // fully transparent
        img.put_pixel(1, 0, image::Rgba([10, 20, 30, 255])); // opaque
        let flat = flatten_on_white(&img);
        assert_eq!(flat.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(flat.get_pixel(1, 0).0, [10, 20, 30]);
    }

    #[test]
    fn half_transparent_pixels_blend_toward_white() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 128]));
        let flat = flatten_on_white(&img);
        let v = flat.get_pixel(0, 0).0[0];
        assert!((120..=136).contains(&v), "got {v}");
    }

    #[test]
    fn every_format_writes_a_readable_file() {
        let dir = std::env::temp_dir().join(format!("shotr-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = RgbaImage::from_pixel(24, 16, image::Rgba([200, 60, 90, 255]));

        for format in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Webp] {
            let settings = Settings {
                format,
                ..Default::default()
            };
            let path = dir.join(format!("t.{}", format.extension()));
            save(&img, &path, &settings).expect("save");
            let back = image::open(&path).expect("reopen").to_rgba8();
            assert_eq!(
                (back.width(), back.height()),
                (24, 16),
                "{format:?} round trip"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_extension_of_the_chosen_path_wins_over_the_setting() {
        let dir = std::env::temp_dir().join(format!("shotr-export2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]));

        // Settings say PNG, but the user typed a .jpg name in Save As.
        let settings = Settings {
            format: ExportFormat::Png,
            ..Default::default()
        };
        let path = dir.join("forced.jpg");
        save(&img, &path, &settings).unwrap();

        let format = image::ImageReader::open(&path)
            .unwrap()
            .format()
            .expect("format detected");
        assert_eq!(format, image::ImageFormat::Jpeg);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
