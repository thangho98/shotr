//! Visual style for the editor chrome.
//!
//! egui ships a neutral grey theme with square corners and tight spacing. Next
//! to a screenshot sitting on a saturated gradient, that chrome competes with
//! the picture. Everything here pushes the UI back: quiet panels, exactly one
//! accent colour, soft corners, and room to breathe — so the image is the only
//! busy thing on screen.
//!
//! There are two palettes. The design system only ever specified the dark one,
//! so the light one is derived from it by keeping every *relationship* the
//! design leans on rather than by inverting the numbers.

use eframe::egui::{self, Color32, CornerRadius, Stroke};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::settings::ThemeMode;

/// The single accent.
///
/// The one colour that does **not** change with the theme. It is drawn over the
/// screenshot as well as in the chrome — selection boxes, the active tool — and
/// there it has to mean the same thing whatever the interface is doing. It is
/// also the one colour a user might reasonably screenshot and send to someone.
pub const ACCENT: Color32 = Color32::from_rgb(0x4a, 0x9e, 0xff);

/// One complete set of surfaces and inks.
///
/// The fields are ordered the way they stack: [`Self::canvas`] is furthest
/// back, then [`Self::panel`], then the controls. Those *relationships* are
/// what the design depends on, and they are what the light palette reproduces —
/// see the tests, which check the relationships rather than the values.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Palette {
    /// The window frame, the bars, and everything that is "the app".
    pub panel: Color32,
    /// Behind the shot. A step away from the panel so the picture separates
    /// from the chrome instead of floating on the same tone.
    pub canvas: Color32,
    /// Controls: buttons, fields, combo boxes.
    pub surface: Color32,
    /// A control under the pointer.
    pub surface_hi: Color32,
    /// egui's own faint fill, for striped rows and inset wells.
    pub faint: Color32,
    /// Hairlines and rules.
    pub line: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub sidebar_top: Color32,
    pub sidebar_bottom: Color32,
    /// The floating tool bar over the picture.
    pub glass: Color32,
    /// Failures the user has to read, in the sidebar.
    pub danger: Color32,
    /// The hairline along a raised edge.
    ///
    /// This changes *direction* between themes rather than merely alpha: white
    /// at low alpha reads as a lit edge on a dark surface and as absolutely
    /// nothing on a light one, where the edge has to be a shadow instead.
    pub edge: Color32,
    /// Which of egui's own two visuals to build the widget styling on.
    pub dark: bool,
}

/// The palette the design system specifies.
pub(crate) const DARK: Palette = Palette {
    panel: Color32::from_rgb(0x17, 0x18, 0x1c),
    canvas: Color32::from_rgb(0x0d, 0x0e, 0x11),
    surface: Color32::from_rgb(0x23, 0x25, 0x2c),
    surface_hi: Color32::from_rgb(0x2e, 0x31, 0x3a),
    faint: Color32::from_rgb(0x1e, 0x20, 0x26),
    line: Color32::from_rgb(0x2b, 0x2e, 0x36),
    text: Color32::from_rgb(0xe7, 0xe9, 0xee),
    text_dim: Color32::from_rgb(0x8b, 0x91, 0xa0),
    sidebar_top: Color32::from_rgb(0x24, 0x26, 0x2f),
    sidebar_bottom: Color32::from_rgb(0x1a, 0x1b, 0x21),
    glass: Color32::from_rgb(0x1d, 0x1f, 0x25),
    danger: Color32::from_rgb(0xff, 0x6b, 0x6b),
    // `from_white_alpha` is not a const fn; this is what it computes.
    edge: Color32::from_rgba_premultiplied(28, 28, 28, 28),
    dark: true,
};

/// The same design on a light desktop.
///
/// Not an inversion. Two things do not survive being flipped:
///
/// *Hover.* On a dark surface a control brightens under the pointer, and there
/// is room above it to do so. On white there is nowhere brighter to go, so
/// [`Palette::surface_hi`] goes *down* instead — which is why the tests ask
/// whether hover is distinguishable rather than whether it is lighter.
///
/// *The lit edge.* See [`Palette::edge`].
pub(crate) const LIGHT: Palette = Palette {
    panel: Color32::from_rgb(0xee, 0xf0, 0xf4),
    canvas: Color32::from_rgb(0xe2, 0xe5, 0xea),
    surface: Color32::from_rgb(0xff, 0xff, 0xff),
    surface_hi: Color32::from_rgb(0xe6, 0xe9, 0xee),
    faint: Color32::from_rgb(0xf4, 0xf5, 0xf8),
    line: Color32::from_rgb(0xd7, 0xda, 0xe1),
    text: Color32::from_rgb(0x1b, 0x1d, 0x22),
    text_dim: Color32::from_rgb(0x6c, 0x72, 0x80),
    sidebar_top: Color32::from_rgb(0xff, 0xff, 0xff),
    sidebar_bottom: Color32::from_rgb(0xf8, 0xf9, 0xfb),
    glass: Color32::from_rgb(0xf2, 0xf4, 0xf7),
    danger: Color32::from_rgb(0xc0, 0x39, 0x2b),
    edge: Color32::from_black_alpha(20),
    dark: false,
};

/// Which palette is live, as an index rather than a lock.
///
/// Same reasoning as [`crate::i18n`]: shotr draws one window from one thread,
/// and threading a palette handle through every painter would add a parameter
/// to the whole UI to express something genuinely process-wide.
static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The palette everything paints with right now.
pub(crate) fn pal() -> &'static Palette {
    match ACTIVE.load(Ordering::Relaxed) {
        1 => &LIGHT,
        _ => &DARK,
    }
}

/// Point the painters at the theme egui has settled on.
///
/// Called every frame, because under [`ThemeMode::System`] the answer changes
/// when the desktop does — a Mac set to "Auto" switches at sunrise — and
/// nothing tells us in advance.
pub fn sync(ctx: &egui::Context) {
    let light = ctx.theme() == egui::Theme::Light;
    ACTIVE.store(u8::from(light), Ordering::Relaxed);
}

/// Switch themes without rebuilding the styles or reloading the font.
pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(mode.preference());
    sync(ctx);
}

// ------------------------------------------------------------- window shell
//
// The editor draws its own window: no system titlebar, a rounded frame, and a
// sidebar card that stands proud of that frame. Everything below is the
// geometry and colour of that shell.

/// Corner radius of the window frame.
pub(crate) const WINDOW_RADIUS: u8 = 18;
/// Corner radius of the sidebar card. Deliberately *larger* than the window's,
/// which is what makes the card read as sitting on top rather than cut into it.
pub(crate) const SIDEBAR_RADIUS: u8 = 20;
/// How far the sidebar card sticks out above and below the window frame.
pub(crate) const OVERHANG_V: f32 = 16.0;
/// How far the card overlaps the frame's left edge.
pub(crate) const OVERHANG_H: f32 = 20.0;
/// The strip carrying the window controls and the wordmark.
pub(crate) const STRIP_H: f32 = 44.0;
/// The bar above the picture: file name, undo/redo, Copy, Save.
pub(crate) const TOPBAR_H: f32 = 44.0;
/// The bar below it: zoom and the keyboard hint.
pub(crate) const STATUSBAR_H: f32 = 32.0;

/// Transparent margin kept around the window frame, purely so the sidebar
/// card's overhang has somewhere to go.
///
/// It is exactly [`OVERHANG_V`] and no more, because every pixel of it is
/// transparent window: it swallows clicks meant for whatever is behind, and
/// anything painted into it is read as chrome the app has drawn around itself.
pub(crate) const SHELL_MARGIN: f32 = OVERHANG_V;

/// The window casts no shadow of its own, and neither does the card.
///
/// The design asks for `0 32px 70px rgba(0,0,0,0.58)`, which assumes an
/// unbounded desktop behind the window. What we have instead is a margin of our
/// own transparent window, so a shadow is not free: it darkens real pixels of
/// whatever is behind, and it cannot fade out any further than the margin
/// allows. Two rounds of softening it (`alpha 158` → `76`, reach 15pt → 11pt)
/// were both still reported as "a translucent black border around the window" —
/// which is what a drop shadow becomes when it has nowhere to fall.
///
/// macOS draws no shadow for this window either: measured, the desktop is
/// pixel-identical right up to the frame's edge. So the edges are defined by
/// the hairlines alone, and nothing is painted outside the frame at all.
///
/// Kept as named constants rather than deleted so that the test below still has
/// something to hold, and so that re-adding one is a considered change.
const FRAME_SHADOW: egui::epaint::Shadow = egui::epaint::Shadow::NONE;
const CARD_SHADOW: egui::epaint::Shadow = egui::epaint::Shadow::NONE;

/// The three window controls, in macOS order.
///
/// `cfg`-ed rather than merely unused elsewhere: shipping Apple's lights on
/// Windows would be one desktop's chrome pasted onto another, so the other
/// platforms draw their own and must not even have these to hand.
#[cfg(target_os = "macos")]
pub(crate) const LIGHTS: [Color32; 3] = [
    Color32::from_rgb(0xff, 0x5f, 0x57),
    Color32::from_rgb(0xfe, 0xbc, 0x2e),
    Color32::from_rgb(0x28, 0xc8, 0x40),
];
#[cfg(target_os = "macos")]
pub(crate) const LIGHT_D: f32 = 12.0;
#[cfg(target_os = "macos")]
pub(crate) const LIGHT_GAP: f32 = 8.0;

/// The floating tool bar over the picture.
pub(crate) const PILL_RADIUS: u8 = 13;
pub(crate) const PILL_PAD: f32 = 5.0;
pub(crate) const PILL_GAP: f32 = 8.0;

/// The edge every raised surface here gets.
///
/// One stroke for all three surfaces. The design gives them 0.10, 0.13 and 0.09
/// alpha; at one physical pixel that difference is invisible, and having a
/// single value is what lets the edge flip from a lit rim on a dark surface to
/// a shadow on a light one.
fn hairline() -> Stroke {
    Stroke::new(1.0_f32, pal().edge)
}

/// Paint the window frame: fill, plus the hairline that separates it from
/// whatever is behind. Nothing reaches outside the rectangle — see
/// [`FRAME_SHADOW`].
pub(crate) fn window_frame(painter: &egui::Painter, rect: egui::Rect) {
    let radius = CornerRadius::same(WINDOW_RADIUS);
    painter.add(FRAME_SHADOW.as_shape(rect, radius));
    painter.rect_filled(rect, radius, pal().panel);
    painter.rect_stroke(rect, radius, hairline(), egui::StrokeKind::Inside);
}

/// Paint the sidebar card.
///
/// The card and the frame are the same family of greys, so what separates them
/// is the hairline and the gradient, not a shadow: the card overhangs onto the
/// user's desktop, and a shadow there is read as a border — see [`CARD_SHADOW`].
///
/// `gradient` is a 1×N texture of [`SIDEBAR_TOP`] → [`SIDEBAR_BOTTOM`]. A mesh
/// would not do: egui cannot clip a mesh to rounded corners, and the corners are
/// the whole point of the card.
pub(crate) fn sidebar_card(
    painter: &egui::Painter,
    rect: egui::Rect,
    gradient: Option<&egui::TextureHandle>,
) {
    let radius = CornerRadius::same(SIDEBAR_RADIUS);
    painter.add(CARD_SHADOW.as_shape(rect, radius));
    match gradient {
        Some(tex) => {
            let mut shape = egui::epaint::RectShape::filled(rect, radius, Color32::WHITE);
            shape.brush = Some(std::sync::Arc::new(egui::epaint::Brush {
                fill_texture_id: tex.id(),
                uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            }));
            painter.add(shape);
        }
        None => {
            painter.rect_filled(rect, radius, pal().sidebar_top);
        }
    }
    painter.rect_stroke(rect, radius, hairline(), egui::StrokeKind::Inside);
}

/// The 1×N gradient the card is painted with.
pub(crate) fn sidebar_gradient(ctx: &egui::Context) -> egui::TextureHandle {
    const N: usize = 64;
    let (top, bottom) = (pal().sidebar_top, pal().sidebar_bottom);
    let pixels: Vec<Color32> = (0..N)
        .map(|i| {
            let t = i as f32 / (N - 1) as f32;
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            Color32::from_rgb(
                mix(top.r(), bottom.r()),
                mix(top.g(), bottom.g()),
                mix(top.b(), bottom.b()),
            )
        })
        .collect();
    ctx.load_texture(
        "sidebar-gradient",
        egui::ColorImage {
            size: [1, N],
            source_size: egui::vec2(1.0, N as f32),
            pixels,
        },
        egui::TextureOptions::LINEAR,
    )
}

/// The floating tool bar's backdrop.
///
/// The design asks for `backdrop-filter: blur(18px)`, which egui has no way to
/// do — and the handoff is explicit that faking it by copying the pixels
/// underneath is not acceptable. So the fill goes opaque instead, which is the
/// stated fallback.
///
/// It has to be the colour the design's `rgba(35,37,44,0.72)` *composites to*
/// over the canvas, not that colour with the alpha dropped. Dropping the alpha
/// lands on `#23252c`, which is exactly [`SURFACE`] — so the slider rail, drawn
/// in that same colour, disappeared into the bar and the only visible part of
/// the control was its handle floating on nothing.
pub(crate) fn glass(painter: &egui::Painter, rect: egui::Rect) {
    let radius = CornerRadius::same(PILL_RADIUS);
    painter.add(
        egui::epaint::Shadow {
            offset: [0, 8],
            blur: 22,
            spread: 0,
            color: Color32::from_black_alpha(107),
        }
        .as_shape(rect, radius),
    );
    painter.rect_filled(rect, radius, pal().glass);
    painter.rect_stroke(rect, radius, hairline(), egui::StrokeKind::Inside);
}

// `Stroke::new` takes `impl Into<f32>`, which leaves a bare `1.0` for inference
// to resolve. It used to quietly pick `f32`; since Rust 1.96 that fallback is
// on its way out and warns, which under `-D warnings` is a failed build. Hence
// the `_f32` on every literal width here and in `canvas.rs` and `icons.rs` —
// they are not decoration, and removing them breaks CI before it breaks anyone
// locally, because CI tracks stable and a working checkout may not.

/// Install the look, and the font that goes with it.
///
/// The font is not separable from the theme: egui's bundled one has Latin
/// Extended-A but not Latin Extended Additional (U+1EA0–U+1EF9), where every
/// Vietnamese tone mark lives, so "Tiếng Việt" renders as "Ti□ng Vi□t". Any
/// window that styled itself without also loading a system font would show that
/// — the Preferences window did, until this moved in here.
///
/// The loaded font comes back because the watermark rasteriser needs the same
/// one; windows that do not draw watermarks ignore the return.
pub fn apply(ctx: &egui::Context, mode: ThemeMode) -> Option<ab_glyph::FontArc> {
    let font = install_fonts(ctx);

    // Build *both* styles, always, whichever mode is chosen.
    //
    // egui keeps a `dark_style` and a `light_style` and picks between them from
    // the theme; `set_global_style` writes to whichever one is selected at that
    // moment. Installing only one is what produced a half-themed window: the
    // style landed in `dark_style` at startup, and when the desktop reported
    // itself light egui served its own untouched `light_style` instead, so
    // every button and field went white while everything painted by hand stayed
    // dark. Filling both slots means the answer is right before the question is
    // even asked.
    ctx.set_style_of(egui::Theme::Dark, style_for(&DARK));
    ctx.set_style_of(egui::Theme::Light, style_for(&LIGHT));

    // shotr zooms the picture, not the chrome. Left on, egui's own keyboard zoom
    // would grab ctrl+plus/minus/0 and rescale the whole interface instead.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);

    set_mode(ctx, mode);
    font
}

/// The whole widget styling, for one palette.
fn style_for(pal: &Palette) -> egui::Style {
    let mut style = egui::Style::default();

    let mut v = if pal.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = pal.panel;
    v.window_fill = pal.panel;
    v.extreme_bg_color = pal.canvas;
    v.faint_bg_color = pal.faint;
    v.window_stroke = Stroke::new(1.0_f32, pal.line);
    v.window_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.40);
    v.selection.stroke = Stroke::new(1.0_f32, pal.text);
    v.hyperlink_color = ACCENT;
    // The stock shadows are heavy enough to read as a second UI layer.
    v.popup_shadow.spread = 2;
    v.window_shadow.spread = 2;

    let radius = CornerRadius::same(7);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = pal.panel;
    w.noninteractive.weak_bg_fill = pal.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text_dim);
    w.noninteractive.corner_radius = radius;

    // The outline a control gains under the pointer. On a dark surface it can
    // brighten; on a light one there is nothing brighter than the control
    // itself, so it darkens instead.
    let hover_stroke = if pal.dark {
        Color32::from_rgb(0x3d, 0x41, 0x4c)
    } else {
        Color32::from_rgb(0xb4, 0xb9, 0xc2)
    };

    for (state, fill, stroke) in [
        (&mut w.inactive, pal.surface, pal.line),
        (&mut w.hovered, pal.surface_hi, hover_stroke),
        (&mut w.active, pal.surface_hi, ACCENT),
        (&mut w.open, pal.surface, pal.line),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, stroke);
        state.fg_stroke = Stroke::new(1.0_f32, pal.text);
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

    style
}

/// Put a Vietnamese-capable system font in front of egui's bundled one.
fn install_fonts(ctx: &egui::Context) -> Option<ab_glyph::FontArc> {
    let (data, font) = crate::render::text::load_system_font()?;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "viet".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(data)),
    );
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(fam)
            .or_default()
            .insert(0, "viet".to_owned());
    }
    ctx.set_fonts(fonts);
    Some(font)
}

/// Backdrop for the preview area. A shade darker than the sidebar so the
/// screenshot separates from the chrome instead of floating on the same tone.
/// The fullscreen picker gets nothing — there the shot covers everything and
/// any frame would only show as a seam.
pub fn canvas_frame(fullscreen_picker: bool) -> egui::Frame {
    if fullscreen_picker {
        egui::Frame::NONE
    } else {
        egui::Frame::NONE.fill(pal().canvas)
    }
}

/// One accordion section of the sidebar: a heading that toggles, a caret that
/// turns, a hairline beneath, and the body only when it is open.
///
/// The sidebar holds six groups of controls and no screen is tall enough for
/// all of them at once. Cards made every group visible and none of them
/// findable; folding means exactly one group is on screen and the rest are a
/// scannable list of headings.
///
/// Returns true when the heading was clicked, so the caller owns which section
/// is open — the fold itself keeps no state.
pub fn fold<R>(
    ui: &mut egui::Ui,
    title: &str,
    open: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> bool {
    let width = ui.available_width();
    let (head, resp) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::click());
    let hot = resp.hovered() || open;
    let ink = if hot { pal().text } else { pal().text_dim };

    let painter = ui.painter();
    painter.text(
        head.left_center() + egui::vec2(2.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::proportional(10.0),
        ink,
    );
    // The caret points down when shut and up when open, the way a disclosure
    // triangle does everywhere else.
    let c = head.right_center() - egui::vec2(6.0, 0.0);
    let dir = if open { -1.0 } else { 1.0 };
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(c.x - 4.0, c.y - 2.0 * dir),
            egui::pos2(c.x + 4.0, c.y - 2.0 * dir),
            egui::pos2(c.x, c.y + 2.5 * dir),
        ],
        ink,
        Stroke::NONE,
    ));

    if open {
        ui.add_space(2.0);
        add(ui);
        ui.add_space(10.0);
    }
    let (rule, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rule, 0.0, pal().line);

    resp.clicked()
}

/// A quiet section heading. The sidebar has a lot of groups in it; making the
/// headings small and grey keeps them scannable without shouting over the
/// controls they introduce.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(9.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(10.0)
            .color(pal().text_dim)
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
                    .color(pal().text_dim)
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
    ui.painter().rect_filled(rect, 0.0, pal().line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Perceptual luminance, 0..1.
    fn lum(c: Color32) -> f32 {
        (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
    }

    /// Raw brightness, for questions about which of two surfaces sits on top.
    fn level(c: Color32) -> i32 {
        c.r() as i32 + c.g() as i32 + c.b() as i32
    }

    fn both() -> [(&'static str, Palette); 2] {
        [("dark", DARK), ("light", LIGHT)]
    }

    /// Which palette is live is process-wide state, and cargo runs tests in
    /// parallel — so every test that switches themes has to take this first or
    /// they read each other's answers. Poisoning is ignored deliberately: one
    /// test failing must not turn into three.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Body text and dimmed text both have to be readable on the panel they are
    /// written on — in either palette. A light theme built by inverting only
    /// *some* of the colours fails here.
    #[test]
    fn text_stays_readable_on_the_panel_in_both_palettes() {
        for (name, p) in both() {
            let gap = (lum(p.text) - lum(p.panel)).abs();
            assert!(gap > 0.6, "{name}: body text against the panel is only {gap:.2}");
            let dim = (lum(p.text_dim) - lum(p.panel)).abs();
            assert!(dim > 0.35, "{name}: dimmed text against the panel is only {dim:.2}");
        }
    }

    /// The design leans on an order of surfaces, not on particular greys: the
    /// canvas sits behind the panel, controls lift off the panel, the card lifts
    /// off the frame, and its gradient runs from lit to unlit. The light palette
    /// earns its place by reproducing every one of those, which is why this test
    /// runs over both rather than pinning values.
    #[test]
    fn both_palettes_stack_their_surfaces_the_same_way() {
        for (name, p) in both() {
            assert!(
                level(p.canvas) < level(p.panel),
                "{name}: the shot would float on the same tone as the chrome"
            );
            assert!(
                level(p.panel) < level(p.surface),
                "{name}: controls would sink into the panel behind them"
            );
            assert!(
                level(p.panel) < level(p.sidebar_bottom),
                "{name}: the card would read as a hole in the frame, not a card on it"
            );
            assert!(
                level(p.sidebar_bottom) < level(p.sidebar_top),
                "{name}: the card's gradient is lit from below"
            );
            assert!(
                level(p.sidebar_bottom) < level(p.surface),
                "{name}: controls would sink into the card"
            );
        }
    }

    /// The tool bar floats over the picture and holds controls drawn in
    /// `surface`. When the two matched, the slider rail vanished into the bar
    /// and only its handle showed — a control that looked broken rather than
    /// one that looked wrong.
    #[test]
    fn controls_stay_visible_against_the_floating_tool_bar() {
        for (name, p) in both() {
            assert!(
                level(p.glass) + 12 < level(p.surface),
                "{name}: the tool bar is too close to the controls sitting on it"
            );
            assert!(
                level(p.canvas) < level(p.glass),
                "{name}: the tool bar must float above the canvas, not sink into it"
            );
        }
    }

    /// Hover has to be *visible*, not necessarily brighter.
    ///
    /// This is the one relationship that cannot survive being inverted: on a
    /// dark surface a control brightens under the pointer, but the light
    /// palette's controls are already white and there is nothing above white,
    /// so there it darkens instead. Asking "is it lighter?" would force the
    /// light theme into a corner; asking "can you see it?" is the real
    /// requirement.
    #[test]
    fn hover_is_visible_in_both_palettes() {
        for (name, p) in both() {
            let step = (level(p.surface_hi) - level(p.surface)).abs();
            assert!(step >= 20, "{name}: hover only shifts the control by {step}");
        }
    }

    /// The lit edge has to change direction with the theme. White at 11% alpha
    /// is a highlight on near-black and is simply invisible on near-white.
    #[test]
    fn the_raised_edge_shows_up_against_its_own_panel() {
        for (name, p) in both() {
            // Composite the edge over the panel it is drawn on.
            let a = p.edge.a() as f32 / 255.0;
            let over = |chan: fn(Color32) -> u8| {
                chan(p.edge) as f32 + chan(p.panel) as f32 * (1.0 - a)
            };
            let edged = Color32::from_rgb(
                over(|c| c.r()) as u8,
                over(|c| c.g()) as u8,
                over(|c| c.b()) as u8,
            );
            let gap = (lum(edged) - lum(p.panel)).abs();
            assert!(
                gap > 0.015,
                "{name}: the hairline round every raised surface is invisible ({gap:.3})"
            );
        }
    }

    /// The accent is the one colour that does not move: it is drawn over the
    /// user's screenshot as well as in the chrome, so it has to survive being
    /// exported and looked at outside the app.
    #[test]
    fn the_accent_reads_against_both_palettes() {
        for (name, p) in both() {
            let gap = (lum(ACCENT) - lum(p.panel)).abs();
            assert!(gap > 0.15, "{name}: the accent disappears into the panel ({gap:.2})");
        }
    }

    /// Nothing may be painted outside the window frame.
    ///
    /// The margin is not ours to draw on: it is transparent window sitting over
    /// the user's desktop, and anything put there is seen as a border the app
    /// has ruled around itself. Reported twice — "vẫn còn 1 lớp viền đen hơi
    /// trong suốt xung quanh" — through two rounds of softening, before the
    /// shadows came out altogether.
    #[test]
    fn nothing_is_painted_outside_the_window_frame() {
        for (what, shadow) in [("window", FRAME_SHADOW), ("card", CARD_SHADOW)] {
            let m = shadow.margin();
            assert_eq!(
                (m.left, m.right, m.top, m.bottom),
                (0.0, 0.0, 0.0, 0.0),
                "the {what} shadow reaches out onto the desktop again"
            );
        }
    }

    /// The preference is what decides, and it decides for the widgets egui
    /// draws as well as for the chrome shotr paints. Those are two different
    /// mechanisms — a global palette and egui's own style slots — and the bug
    /// this whole file was rewritten for was them disagreeing.
    #[test]
    fn the_preference_drives_both_the_palette_and_the_widgets() {
        let _serial = alone();
        for (mode, want, dark) in [
            (ThemeMode::Light, LIGHT, false),
            (ThemeMode::Dark, DARK, true),
        ] {
            let ctx = egui::Context::default();
            apply(&ctx, mode);
            assert_eq!(*pal(), want, "{mode:?}: the painters got the wrong palette");
            let visuals = ctx.global_style().visuals.clone();
            assert_eq!(
                visuals.widgets.inactive.bg_fill, want.surface,
                "{mode:?}: egui's own widgets got the wrong palette"
            );
            assert_eq!(visuals.dark_mode, dark, "{mode:?}: wrong base visuals");
        }
    }

    /// Following the desktop has to keep following it. A Mac set to "Auto"
    /// switches at sunrise while the editor is open, and nothing announces it
    /// beforehand — which is exactly how a half-light window shipped once.
    #[test]
    fn system_mode_tracks_the_desktop_while_the_window_is_open() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::System);

        for (system, want) in [
            (egui::Theme::Light, LIGHT),
            (egui::Theme::Dark, DARK),
            (egui::Theme::Light, LIGHT),
        ] {
            let raw = egui::RawInput {
                system_theme: Some(system),
                ..Default::default()
            };
            let _ = ctx.run_ui(raw, |_| {});
            sync(&ctx);
            assert_eq!(*pal(), want, "the desktop turned {system:?} and shotr did not");
            assert_eq!(
                ctx.global_style().visuals.widgets.inactive.bg_fill,
                want.surface,
                "the desktop turned {system:?} and egui's widgets did not"
            );
        }
    }

    /// ...and a chosen theme must ignore the desktop entirely.
    #[test]
    fn a_chosen_theme_ignores_the_desktop() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::Dark);
        let raw = egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        };
        let _ = ctx.run_ui(raw, |_| {});
        sync(&ctx);
        assert_eq!(
            *pal(),
            DARK,
            "picking Dark in Preferences has to survive the desktop being light"
        );
    }

    /// egui's own keyboard zoom rescales the whole interface and would fight
    /// the editor for ctrl+plus/minus/0.
    #[test]
    fn egui_keyboard_zoom_is_turned_off() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::Dark);
        assert!(!ctx.options(|o| o.zoom_with_keyboard));
    }
}
