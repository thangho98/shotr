//! Minimal glyph blitter, used only for the watermark.
//!
//! `image` has no text support and `imageproc` would be a heavy dependency for
//! one line of text, so we rasterise through `ab_glyph` directly.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

use super::frame::blend;

/// Width in pixels the string will occupy at `px` size.
pub fn measure(font: &FontArc, px: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(px));
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            width += scaled.kern(p, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Draw `text` with its top-left corner at `(x, y)`.
pub fn draw(
    img: &mut RgbaImage,
    font: &FontArc,
    px: f32,
    x: f32,
    y: f32,
    color: Rgba<u8>,
    text: &str,
) {
    let scaled = font.as_scaled(PxScale::from(px));
    let mut caret = point(x, y + scaled.ascent());
    let mut prev = None;

    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(p) = prev {
            caret.x += scaled.kern(p, id);
        }
        prev = Some(id);

        let mut glyph = scaled.scaled_glyph(c);
        glyph.position = caret;
        caret.x += scaled.h_advance(id);

        let Some(outlined) = font.outline_glyph(glyph) else {
            continue; // whitespace and unmapped characters have no outline
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, cov| {
            let tx = bounds.min.x + gx as f32;
            let ty = bounds.min.y + gy as f32;
            if tx >= 0.0 && ty >= 0.0 {
                blend(img, tx as u32, ty as u32, color, cov);
            }
        });
    }
}

/// The first system font we can find that covers Vietnamese diacritics.
///
/// egui's bundled font does not, so the UI and the watermark share this lookup.
/// When it finds nothing the interface still runs, but every tone mark in it
/// turns into a blank box the moment the language is switched — which is what
/// happened on macOS and Windows while this list held Linux paths only.
///
/// One list for all three platforms, tried in order: a path belonging to
/// another operating system simply is not there, so the wrong entries cost a
/// failed `read` each and nothing else. Each group leads with that platform's
/// interface typeface, because this font labels buttons far more often than it
/// stamps a watermark, and trails into workhorses that keep shotr running on a
/// machine with none of the nicer ones.
///
/// Every macOS entry was checked on macOS 15: all load through `ab_glyph` —
/// including the `.ttc` collections, which it does not refuse — and all carry
/// `ă ơ đ ế ữ ạ`. The Windows entries are the documented system fonts and have
/// not been verified on a real machine.
pub const FONT_CANDIDATES: &[&str] = &[
    // Linux
    "/usr/share/fonts/inter/InterVariable.ttf",
    "/usr/share/fonts/Inter/InterVariable.ttf",
    "/usr/share/fonts/TTF/InterVariable.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    // macOS. SFNS is San Francisco, the system interface font.
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/SFNSText.ttf",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    // Windows
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\tahoma.ttf",
    r"C:\Windows\Fonts\arial.ttf",
];

pub fn load_system_font() -> Option<(Vec<u8>, FontArc)> {
    for path in FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(path)
            && let Ok(font) = FontArc::try_from_vec(data.clone())
        {
            return Some((data, font));
        }
    }
    None
}

#[cfg(test)]
mod font_lookup_tests {
    use super::FONT_CANDIDATES;

    /// This list was Linux-only for a while, and nothing broke loudly: the
    /// interface kept running and simply drew every Vietnamese tone mark as an
    /// empty box on the other two platforms. Nothing here can check a font that
    /// is not on the machine running the tests, so check the one thing that can
    /// be checked without one — that no platform has been forgotten.
    #[test]
    fn every_platform_has_somewhere_to_look() {
        for (prefix, platform) in [
            ("/usr/share/fonts/", "Linux"),
            ("/System/Library/Fonts/", "macOS"),
            (r"C:\Windows\Fonts\", "Windows"),
        ] {
            assert!(
                FONT_CANDIDATES.iter().any(|p| p.starts_with(prefix)),
                "no font path for {platform}, so its interface loses every Vietnamese diacritic"
            );
        }
    }

    #[test]
    fn the_paths_are_absolute() {
        for path in FONT_CANDIDATES {
            assert!(
                path.starts_with('/') || path.starts_with("C:\\"),
                "{path} is relative, so it would resolve against the working directory"
            );
        }
    }
}
