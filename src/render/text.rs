//! Minimal glyph blitter, used only for the watermark.
//!
//! `image` has no text support and `imageproc` would be a heavy dependency for
//! one line of text, so we rasterise through `ab_glyph` directly.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

use super::frame::blend;

/// The scale `ab_glyph` needs to draw a font at `em` pixels to the em square.
///
/// `PxScale` is *not* an em size: `ScaleFont::scale_factor` divides it by the
/// font's line height — ascent − descent + line gap — while egui, and everyone
/// else who says "font size", divides by the em square. The same number
/// therefore drew two different sizes, and only one of them ended up in the
/// picture: a label followed the pointer at egui's size, then shrank by about a
/// sixth the moment it was baked, leaving the selection frame — measured with
/// egui — standing off the ink. Every entry point here takes an em size.
fn px_scale(font: &FontArc, em: f32) -> PxScale {
    match font.units_per_em() {
        Some(upem) if upem > 0.0 && font.height_unscaled() > 0.0 => {
            PxScale::from(em * font.height_unscaled() / upem)
        }
        _ => PxScale::from(em),
    }
}

/// Width in pixels the string will occupy at `em` size.
pub fn measure(font: &FontArc, em: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(px_scale(font, em));
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
    em: f32,
    x: f32,
    y: f32,
    color: Rgba<u8>,
    text: &str,
) {
    let scaled = font.as_scaled(px_scale(font, em));
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

/// Draw a rule under a line whose top-left corner is at `(x, y)`.
///
/// Placed from the font's own ascent rather than from a fraction of the em, so
/// it clears the descenders of the face actually in use instead of the one this
/// was tuned against. `ab_glyph` does not read the `post` table, which is where
/// a font states its own underline position, so the offset below is a
/// convention rather than the designer's intent.
pub fn underline(
    img: &mut RgbaImage,
    font: &FontArc,
    em: f32,
    x: f32,
    y: f32,
    w: f32,
    color: Rgba<u8>,
) {
    let scaled = font.as_scaled(px_scale(font, em));
    let thickness = (em * 0.06).max(1.0);
    let top = y + scaled.ascent() + (em * 0.12).max(1.0);
    for row in 0..thickness.ceil() as u32 {
        let ty = top + row as f32;
        if ty < 0.0 {
            continue;
        }
        for col in 0..w.ceil() as u32 {
            let tx = x + col as f32;
            if tx >= 0.0 {
                blend(img, tx as u32, ty as u32, color, 1.0);
            }
        }
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
    use eframe::egui;

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

    /// The editor lays a label out with egui and the exporter bakes it with
    /// `ab_glyph`, and the whole design assumes the two agree — the selection
    /// frame is measured with one and drawn around the other. They did not:
    /// `PxScale` normalises by the line height and egui by the em square, so a
    /// label followed the pointer at one size and shrank the moment it settled,
    /// leaving the frame standing off the text.
    #[test]
    fn the_exporter_and_the_editor_lay_a_label_out_at_the_same_size() {
        let Some((_, font)) = super::load_system_font() else {
            return; // no system font: egui falls back and the exporter draws nothing
        };
        let ctx = egui::Context::default();
        crate::app::theme::install_fonts(&ctx);

        let (text, size) = ("Xin chào, world", 40.0_f32);
        let baked = super::measure(&font, size, text);
        // Twice: `set_fonts` is applied at the start of the next pass.
        let mut laid_out = 0.0;
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                laid_out = ui
                    .painter()
                    .layout_no_wrap(
                        text.to_owned(),
                        egui::FontId::new(size, egui::FontFamily::Proportional),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x;
            });
        }

        let off = (baked - laid_out).abs() / laid_out;
        assert!(
            off < 0.03,
            "the exporter draws {text:?} {baked}px wide and the editor {laid_out}px — \
             {:.0}% apart, so the selection frame cannot fit the baked label",
            off * 100.0
        );
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
