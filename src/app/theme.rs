//! Visual style for the editor chrome.
//!
//! egui ships a neutral grey theme with square corners and tight spacing. Next
//! to a screenshot sitting on a saturated gradient, that chrome competes with
//! the picture. Everything here pushes the UI back: near-black panels, exactly
//! one accent colour, soft corners, and room to breathe — so the image is the
//! only busy thing on screen.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// The single accent. Also used by the canvas overlays, so selection boxes and
/// buttons agree.
pub const ACCENT: Color32 = Color32::from_rgb(0x4a, 0x9e, 0xff);

const PANEL: Color32 = Color32::from_rgb(0x17, 0x18, 0x1c);
const CANVAS: Color32 = Color32::from_rgb(0x0d, 0x0e, 0x11);
const SURFACE: Color32 = Color32::from_rgb(0x23, 0x25, 0x2c);
const SURFACE_HI: Color32 = Color32::from_rgb(0x2e, 0x31, 0x3a);
const LINE: Color32 = Color32::from_rgb(0x2b, 0x2e, 0x36);
/// Surface for grouped controls — one step up from the panel behind them.
const CARD: Color32 = Color32::from_rgb(0x1d, 0x1f, 0x25);
const TEXT: Color32 = Color32::from_rgb(0xe7, 0xe9, 0xee);
const TEXT_DIM: Color32 = Color32::from_rgb(0x8b, 0x91, 0xa0);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = CANVAS;
    v.faint_bg_color = Color32::from_rgb(0x1e, 0x20, 0x26);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.40);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.hyperlink_color = ACCENT;
    // The stock shadows are heavy enough to read as a second UI layer.
    v.popup_shadow.spread = 2;
    v.window_shadow.spread = 2;

    let radius = CornerRadius::same(7);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.noninteractive.corner_radius = radius;

    for (state, fill, stroke) in [
        (&mut w.inactive, SURFACE, LINE),
        (&mut w.hovered, SURFACE_HI, Color32::from_rgb(0x3d, 0x41, 0x4c)),
        (&mut w.active, SURFACE_HI, ACCENT),
        (&mut w.open, SURFACE, LINE),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.0, TEXT);
        state.corner_radius = radius;
        // egui grows widgets on hover by default, which makes a row of tool
        // buttons jitter as the pointer crosses it.
        state.expansion = 0.0;
    }
    style.visuals = v;

    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(8.0, 7.0);
    sp.button_padding = egui::vec2(7.0, 4.0);
    sp.indent = 16.0;
    sp.slider_width = 132.0;
    sp.slider_rail_height = 4.0;
    sp.interact_size.y = 24.0;
    sp.scroll.bar_width = 8.0;
    // Two separate gaps: content to scrollbar, and scrollbar to the panel edge.
    // Without them the sliders and swatch grid run straight into the bar.
    sp.scroll.bar_inner_margin = 10.0;
    sp.scroll.bar_outer_margin = 4.0;
    sp.scroll.floating = false;

    use egui::FontFamily::{Monospace, Proportional};
    use egui::{FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(17.0, Proportional)),
        (TextStyle::Body, FontId::new(13.0, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Small, FontId::new(11.0, Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, Monospace)),
    ]
    .into();

    ctx.set_global_style(style);

    // shotr zooms the picture, not the chrome. Left on, egui's own keyboard zoom
    // would grab ctrl+plus/minus/0 and rescale the whole interface instead.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

/// Backdrop for the preview area. A shade darker than the sidebar so the
/// screenshot separates from the chrome instead of floating on the same tone.
/// The fullscreen picker gets nothing — there the shot covers everything and
/// any frame would only show as a seam.
pub fn canvas_frame(fullscreen_picker: bool) -> egui::Frame {
    if fullscreen_picker {
        egui::Frame::NONE
    } else {
        egui::Frame::NONE.fill(CANVAS)
    }
}

/// A card: a group of controls on a slightly raised surface.
///
/// Flat sections separated by rules make a long sidebar read as one
/// undifferentiated column. Giving each group its own surface lets the eye find
/// the boundaries without a single extra line being drawn.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(8.0);
    if !title.is_empty() {
        ui.label(
            egui::RichText::new(title.to_uppercase())
                .size(10.0)
                .color(TEXT_DIM)
                .strong(),
        );
        ui.add_space(3.0);
    }
    egui::Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(10, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A quiet section heading. The sidebar has a lot of groups in it; making the
/// headings small and grey keeps them scannable without shouting over the
/// controls they introduce.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(9.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(10.0)
            .color(TEXT_DIM)
            .strong(),
    );
    ui.add_space(1.0);
}

/// A label/value row: name on the left, current value right-aligned and dimmed.
/// Used above sliders so the number is readable without hovering.
pub fn slider_label(ui: &mut egui::Ui, name: &str, value: impl std::fmt::Display) {
    ui.horizontal(|ui| {
        ui.label(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value.to_string())
                    .color(TEXT_DIM)
                    .small(),
            );
        });
    });
}

/// A hairline separator. `ui.separator()` draws edge to edge and at this
/// contrast it reads as a hard division; sections want a hint, not a wall.
pub fn rule(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_stays_dark_enough_for_white_text() {
        // Rough perceptual luminance; anything above ~0.35 would make the
        // light body text hard to read.
        for c in [PANEL, CANVAS, CARD, SURFACE, SURFACE_HI] {
            let l = (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0;
            assert!(l < 0.35, "{c:?} is too light for the dark theme (l={l:.2})");
        }
        let l = (0.2126 * TEXT.r() as f32 + 0.7152 * TEXT.g() as f32 + 0.0722 * TEXT.b() as f32)
            / 255.0;
        assert!(l > 0.8, "body text must stay bright");
    }

    #[test]
    fn surfaces_are_ordered_from_panel_up() {
        // Each layer must sit above the one behind it or the depth cues invert.
        let lum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(lum(CANVAS) < lum(PANEL));
        assert!(lum(PANEL) < lum(CARD), "cards must lift off the panel");
        assert!(lum(CARD) < lum(SURFACE));
        assert!(lum(SURFACE) < lum(SURFACE_HI));
    }
}
