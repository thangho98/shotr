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
/// egui's bundled font does not, so the UI and the watermark share this lookup.
/// Interface fonts, best first. Inter is a typeface drawn for user interfaces
/// and carries full Vietnamese coverage; the rest are workhorse fallbacks that
/// keep shotr running on a machine that has none of the nicer ones.
pub const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/inter/InterVariable.ttf",
    "/usr/share/fonts/Inter/InterVariable.ttf",
    "/usr/share/fonts/TTF/InterVariable.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
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
