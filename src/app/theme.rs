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

/// The rule every control is outlined with.
fn line_stroke() -> Stroke {
    Stroke::new(1.0_f32, pal().line)
}

/// The outline a control gains under the pointer.
///
/// On a dark surface it can brighten; on a light one there is nothing brighter
/// than the control itself, so it darkens instead.
fn hover_line_of(pal: &Palette) -> Color32 {
    if pal.dark {
        Color32::from_rgb(0x3d, 0x41, 0x4c)
    } else {
        Color32::from_rgb(0xb4, 0xb9, 0xc2)
    }
}

fn hover_line() -> Color32 {
    hover_line_of(pal())
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

    let radius = CornerRadius::same(RADIUS_CONTROL);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = pal.panel;
    w.noninteractive.weak_bg_fill = pal.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text_dim);
    w.noninteractive.corner_radius = radius;

    let hover_stroke = hover_line_of(pal);

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
pub(crate) fn install_fonts(ctx: &egui::Context) -> Option<ab_glyph::FontArc> {
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

/// A hairline separator. `ui.separator()` draws edge to edge and at this
/// contrast it reads as a hard division; sections want a hint, not a wall.
pub fn rule(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, pal().line);
}

// ---------------------------------------------------------------- controls
//
// The interface is built out of eight shapes: a card, a welded bar, a
// segmented track, a chip, a slider row, a checkbox, a swatch and a button.
// They live here rather than beside the sidebar that needed them first,
// because the Preferences window is built out of the same eight.
//
// Two of them are painted by hand, and both for the same reason: egui takes
// two colours the design distinguishes from one field. A slider's rail and its
// knob both come from `widgets.inactive.bg_fill`, and the design wants `canvas`
// and `text`; a checkbox's tick comes from the same ink as its box, and the
// design wants an accent box with the tick punched out of it in panel ink. No
// styling separates either pair.

/// A button, a segmented track, a chip, a welded bar.
pub const RADIUS_CONTROL: u8 = 8;
/// A number box, a swatch, a field inside a card.
pub const RADIUS_SMALL: u8 = 6;
/// A card.
pub const RADIUS_CARD: u8 = 9;

/// Full-width button.
const H_PRIMARY: f32 = 30.0;
/// Secondary, ghost button.
const H_GHOST: f32 = 28.0;
/// A welded bar standing on its own.
pub const H_BAR: f32 = 28.0;
/// A welded bar inside a card, where everything is a step tighter.
pub const H_BAR_CARD: f32 = 26.0;
const H_SEGMENT: f32 = 26.0;
const H_CHIP: f32 = 28.0;
/// The number box beside a slider.
const H_NUMBER: f32 = 22.0;
const W_NUMBER: f32 = 42.0;
/// The label that starts a slider row. Fixed, so the rails of a stack of
/// sliders line up whatever their labels say.
const W_SLIDER_LABEL: f32 = 74.0;
/// Between the parts of one row, and between rows inside a card.
pub const ROW_GAP: f32 = 9.0;
const SLIDER_RAIL: f32 = 4.0;
const SLIDER_KNOB: f32 = 14.0;
const CHECK_BOX: f32 = 16.0;
/// Breathing room either side of a label inside a segmented cell.
const CELL_PAD: f32 = 8.0;
/// Room either side of the text inside a field.
///
/// egui's own default is 4, which puts the caret almost on the border and reads
/// as text stuck to the edge — the more so in a welded [`Bar`], where there is no
/// frame of the field's own to separate them.
pub const FIELD_PAD: i8 = 9;

/// The frame for a field welded into a [`Bar`]: no fill, no stroke, but the
/// padding a field needs.
///
/// Not `Frame::NONE`. `TextEdit` takes its text inset from the frame when it is
/// given one and from `TextEdit::margin` *only when it is not* — see
/// `egui-0.34.3/src/widgets/text_edit/builder.rs:666`. So a field handed
/// `Frame::NONE` ignores its margin and sits flush against the divider, and
/// setting the margin looks like it should fix that and does nothing.
pub fn welded_field() -> egui::Frame {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(FIELD_PAD, 0))
}
/// A label inside a card row.
const FONT_ROW: f32 = 12.0;

/// Group a fold's controls into a card.
///
/// Without it the controls float loose in the column and a fold's body has no
/// edge, so two open folds read as one list.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(pal().glass)
        .stroke(line_stroke())
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(egui::Margin::symmetric(9, 10))
        .show(ui, |ui| {
            // A `Frame` shrinks to its contents, and a card holding one narrow
            // control — Watermark, with the watermark off — came out as a small
            // box floating in the column while every other card was full width.
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            add(ui)
        })
        .inner
}

/// Style the controls inside as fields: their own radius, a `surface` fill, and
/// an accent border while focused.
///
/// The radius is a parameter because the design gives the same `TextEdit` two:
/// [`RADIUS_SMALL`] for a number box or a field inside a card, and
/// [`RADIUS_CONTROL`] for one standing on its own.
pub fn field<R>(ui: &mut egui::Ui, radius: u8, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let radius = CornerRadius::same(radius);
        let w = &mut ui.visuals_mut().widgets;
        for state in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            state.corner_radius = radius;
        }
        // A `TextEdit` fills itself from `extreme_bg_color`, which is the
        // furthest-back tone in the palette — the same one the slider rail is a
        // well of. Left alone, a field reads as a hole in the card instead of a
        // control on it. Scoped rather than set globally, because scroll bar
        // troughs take their colour from the same field and do want the well.
        ui.visuals_mut().extreme_bg_color = pal().surface;
        // A `DragValue` carries `min_size(interact_size)`, which is 40 × 24 by
        // default — taller than the number box the design asks for, and enough
        // to push every slider row a little past its card.
        ui.spacing_mut().interact_size = egui::vec2(W_NUMBER, H_NUMBER);
        // What `TextEdit` swaps its border for while it has focus.
        ui.visuals_mut().selection.stroke = Stroke::new(1.0_f32, ACCENT);
        add(ui)
    })
    .inner
}

/// Strip a widget's own frame so it welds into the [`Bar`] it sits in.
pub fn frameless(ui: &mut egui::Ui) {
    let hi = pal().surface_hi;
    let w = &mut ui.visuals_mut().widgets;
    for state in [
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.bg_fill = Color32::TRANSPARENT;
        state.weak_bg_fill = Color32::TRANSPARENT;
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = CornerRadius::ZERO;
    }
    // Hover still has to show, or half the bar looks inert.
    w.hovered.weak_bg_fill = hi;
    w.active.weak_bg_fill = hi;
}

/// The one full-width button a screen leads with.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let width = ui.available_width();
    // `add_sized`, not `min_size`: a button merely *given* a larger minimum keeps
    // its label where the layout put it, which on a full-width button is hard
    // left. `add_sized` lays it out centred and justified.
    let resp = ui.add_sized(egui::vec2(width, H_PRIMARY), egui::Button::new(label));
    // The design lights the top edge alone; egui strokes all four the same, so
    // the lit edge goes on afterwards. Inset by the radius so it does not cut
    // across the corners.
    let r = resp.rect;
    let inset = f32::from(RADIUS_CONTROL) * 0.5;
    ui.painter().hline(
        (r.left() + inset)..=(r.right() - inset),
        r.top() + 0.5,
        Stroke::new(1.0_f32, pal().edge),
    );
    resp
}

/// A full-width button for the action nobody should hit by accident: an
/// outline and dimmed lettering, no fill.
pub fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let width = ui.available_width();
    ui.scope(|ui| {
        let dim = pal().text_dim;
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            state.weak_bg_fill = Color32::TRANSPARENT;
        }
        w.inactive.fg_stroke.color = dim;
        ui.add_sized(egui::vec2(width, H_GHOST), egui::Button::new(label))
    })
    .inner
}

/// A hand-painted checkbox: 16px box, accent when on, the tick punched out of
/// it in panel ink.
///
/// Returns the `Response` with `changed()` set by hand, because every caller
/// reads it and a widget built out of `interact` does not get it for free.
pub fn checkbox(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let text_w = (ui.available_width() - CHECK_BOX - ROW_GAP).max(40.0);
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        pal().text,
        text_w,
    );
    let size = egui::vec2(
        CHECK_BOX + ROW_GAP + galley.size().x,
        galley.size().y.max(CHECK_BOX),
    );
    let (rect, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    // egui's own widgets report themselves to the accessibility tree; one built
    // out of `allocate_exact_size` reports nothing at all unless told to.
    let enabled = ui.is_enabled();
    let state = *on;
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, state, label)
    });

    let box_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + CHECK_BOX / 2.0, rect.center().y),
        egui::Vec2::splat(CHECK_BOX),
    );
    let (fill, edge) = match (*on, resp.hovered()) {
        (true, _) => (ACCENT, ACCENT),
        (false, true) => (pal().surface_hi, hover_line()),
        (false, false) => (pal().surface, pal().line),
    };
    let painter = ui.painter();
    painter.rect(
        box_rect,
        CornerRadius::same(5),
        fill,
        Stroke::new(1.0_f32, edge),
        egui::StrokeKind::Inside,
    );
    if *on {
        let c = box_rect.center();
        painter.add(egui::Shape::line(
            vec![
                egui::pos2(c.x - 3.5, c.y - 0.2),
                egui::pos2(c.x - 1.0, c.y + 2.6),
                egui::pos2(c.x + 3.6, c.y - 3.0),
            ],
            Stroke::new(1.8_f32, pal().panel),
        ));
    }
    painter.galley(
        egui::pos2(
            rect.left() + CHECK_BOX + ROW_GAP,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        pal().text,
    );
    resp
}

/// The name that starts a row inside a card.
///
/// Fixed width, so the controls down a stack of rows line up whatever their
/// labels say — which is the whole reason the labels are short.
pub fn row_label(ui: &mut egui::Ui, text: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(W_SLIDER_LABEL, H_NUMBER), egui::Sense::hover());
    ui.painter().with_clip_rect(rect).text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(FONT_ROW),
        pal().text_dim,
    );
}

/// `label · rail · number box`, the row every dial in the app is made of.
///
/// The number box is the reason the row exists: dragging cannot enter an exact
/// value, and every one of these ranges has values worth typing.
pub fn slider_row<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    suffix: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        row_label(ui, label);
        let rail_w = (ui.available_width() - W_NUMBER - ROW_GAP).max(48.0);
        changed |= rail(ui, rail_w, value, &range);
        changed |= field(ui, RADIUS_SMALL, |ui| {
            ui.add_sized(
                egui::vec2(W_NUMBER, H_NUMBER),
                egui::DragValue::new(value).range(range).suffix(suffix),
            )
            .changed()
        });
    });
    changed
}

/// The rail on its own, for a row that needs something other than a number box
/// beside it.
pub fn rail<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    width: f32,
    value: &mut N,
    range: &std::ops::RangeInclusive<N>,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, SLIDER_KNOB),
        egui::Sense::click_and_drag(),
    );
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    // The knob travels an inset span so that neither end of it hangs off the
    // rail; the rail itself is drawn full width.
    let travel = egui::Rangef::new(
        rect.left() + SLIDER_KNOB / 2.0,
        rect.right() - SLIDER_KNOB / 2.0,
    );

    let mut changed = false;
    if let Some(p) = resp.interact_pointer_pos() {
        let picked = value_at(p.x, travel, lo, hi, N::INTEGRAL);
        if picked != value.to_f64() {
            *value = N::from_f64(picked);
            changed = true;
        }
    }

    let enabled = ui.is_enabled();
    let reading = value.to_f64();
    resp.widget_info(|| egui::WidgetInfo::slider(enabled, reading, ""));

    let t = fraction(value.to_f64(), lo, hi);
    let mid = rect.center().y;
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left(), mid - SLIDER_RAIL / 2.0),
        egui::pos2(rect.right(), mid + SLIDER_RAIL / 2.0),
    );
    let radius = CornerRadius::same((SLIDER_RAIL / 2.0) as u8);
    let painter = ui.painter();
    painter.rect_filled(bar, radius, pal().canvas);
    let knob_x = travel.min + t * travel.span();
    if knob_x > bar.left() {
        painter.rect_filled(
            egui::Rect::from_min_max(bar.min, egui::pos2(knob_x, bar.max.y)),
            radius,
            ACCENT,
        );
    }
    painter.circle(
        egui::pos2(knob_x, mid),
        SLIDER_KNOB / 2.0,
        pal().text,
        Stroke::new(1.0_f32, Color32::from_black_alpha(89)),
    );
    changed
}

/// Where the knob sits along its travel, 0..1.
fn fraction(value: f64, lo: f64, hi: f64) -> f32 {
    if hi <= lo {
        return 0.0;
    }
    (((value - lo) / (hi - lo)) as f32).clamp(0.0, 1.0)
}

/// The value a pointer at `x` picks. Clamped, so a drag that runs off the end
/// of the rail pins to the end of the range instead of leaving it.
fn value_at(x: f32, travel: egui::Rangef, lo: f64, hi: f64, integral: bool) -> f64 {
    let t = if travel.span() > 0.0 {
        ((x - travel.min) / travel.span()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let raw = lo + f64::from(t) * (hi - lo);
    if integral { raw.round() } else { raw }
}

/// A row of mutually exclusive options welded into one track.
///
/// Loose `selectable_label`s say "here are some buttons"; a joined track says
/// "exactly one of these". Every choice in the app is the second thing.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let width = ui.available_width();
    let font = egui::FontId::proportional(FONT_ROW);

    // Wrap onto more rows rather than squeeze: four watermark styles on one row
    // gave each 74pt for labels like "Rounded plate", and the clip took the last
    // word off every one of them. Measured, so it holds in either language.
    let widest = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), pal().text)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let per_row = cells_per_row(width, widest, options.len());
    let rows = row_sizes(options.len(), per_row);

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, H_SEGMENT * rows.len() as f32),
        egui::Sense::hover(),
    );
    // Allocated first, and only then given up on: returning before the
    // allocation would shift the auto-generated id of every widget after it.
    if options.is_empty() {
        return false;
    }
    let radius = CornerRadius::same(RADIUS_CONTROL);
    let last_row = rows.len() - 1;
    let mut changed = false;
    let mut index = 0;

    for (r, count) in rows.iter().copied().enumerate() {
        let top = rect.top() + H_SEGMENT * r as f32;
        let row = egui::Rect::from_min_max(
            egui::pos2(rect.left(), top),
            egui::pos2(rect.right(), top + H_SEGMENT),
        );
        if r > 0 {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(row.left(), row.top()),
                    egui::pos2(row.right(), row.top() + 1.0),
                ),
                0.0,
                pal().line,
            );
        }

        for i in 0..count {
            let (value, label) = options[index];
            let (x0, x1) = segment_bounds(row.x_range(), i, count);
            let cell =
                egui::Rect::from_min_max(egui::pos2(x0, row.top()), egui::pos2(x1, row.bottom()));
            let resp = ui.interact(cell, ui.id().with((id, index)), egui::Sense::click());
            let on = *current == value;
            let enabled = ui.is_enabled();
            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, on, label)
            });
            // Only the block's four outer corners are rounded; a filled cell with
            // corners of its own leaves the track showing through in notches.
            let corners = CornerRadius {
                nw: if r == 0 && i == 0 { radius.nw } else { 0 },
                ne: if r == 0 && i + 1 == count { radius.ne } else { 0 },
                sw: if r == last_row && i == 0 { radius.sw } else { 0 },
                se: if r == last_row && i + 1 == count {
                    radius.se
                } else {
                    0
                },
            };
            let fill = match (on, resp.hovered()) {
                (true, _) => ACCENT.gamma_multiply(0.30),
                (false, true) => pal().surface,
                (false, false) => Color32::TRANSPARENT,
            };
            if fill != Color32::TRANSPARENT {
                ui.painter().rect_filled(cell, corners, fill);
            }
            if i > 0 {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, row.top()),
                        egui::pos2(x0 + 1.0, row.bottom()),
                    ),
                    0.0,
                    pal().line,
                );
            }
            let ink = if on || resp.hovered() {
                pal().text
            } else {
                pal().text_dim
            };
            ui.painter().with_clip_rect(cell).text(
                cell.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                ink,
            );
            if resp.clicked() {
                *current = value;
                changed = true;
            }
            index += 1;
        }
    }

    ui.painter()
        .rect_stroke(rect, radius, line_stroke(), egui::StrokeKind::Inside);
    changed
}

/// How many cells fit on one row with room for the longest label.
fn cells_per_row(width: f32, widest_label: f32, n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let needed = widest_label + CELL_PAD * 2.0;
    if needed <= 0.0 {
        return n;
    }
    let fits = (width / needed).floor().max(1.0) as usize;
    fits.clamp(1, n)
}

/// Spread `n` cells over as few rows as `per_row` allows, evenly.
///
/// Evenly, not greedily: five options at three per row is 3 + 2, never 3 + 1 + 1
/// and never a row with one lonely cell in it.
fn row_sizes(n: usize, per_row: usize) -> Vec<usize> {
    if n == 0 {
        return vec![0];
    }
    let rows = n.div_ceil(per_row.max(1));
    let base = n / rows;
    let extra = n % rows;
    (0..rows).map(|r| base + usize::from(r < extra)).collect()
}

/// Cell `i` of `n` across `span`.
///
/// Rounded boundaries rather than a rounded width: `n` cells of `w / n` leave a
/// sliver of the track showing between the fills, which reads as a seam through
/// the selected cell. The outer two boundaries are the track's own edges
/// untouched — rounding them lands the end cell up to half a pixel past the
/// outline, and a fractional width is the normal case, not the odd one.
fn segment_bounds(span: egui::Rangef, i: usize, n: usize) -> (f32, f32) {
    let at = |k: usize| match k {
        0 => span.min,
        k if k == n => span.max,
        k => span.min + (span.span() * k as f32 / n as f32).round(),
    };
    (at(i), at(i + 1))
}

/// One option with its own subtitle: a name, and what it means in pixels.
pub fn chip(ui: &mut egui::Ui, width: f32, on: bool, name: &str, sub: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, H_CHIP), egui::Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, on, name)
    });
    let radius = CornerRadius::same(RADIUS_CONTROL);
    let fill = match (on, resp.hovered()) {
        (true, _) => ACCENT.gamma_multiply(0.18),
        (false, true) => pal().surface,
        (false, false) => Color32::TRANSPARENT,
    };
    let painter = ui.painter();
    if fill != Color32::TRANSPARENT {
        painter.rect_filled(rect, radius, fill);
    }
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, if on { ACCENT } else { pal().line }),
        egui::StrokeKind::Inside,
    );
    let inner = painter.with_clip_rect(rect.shrink(2.0));
    inner.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(FONT_ROW),
        pal().text,
    );
    inner.text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        sub,
        egui::FontId::proportional(10.0),
        if on { ACCENT } else { pal().text_dim },
    );
    resp
}

/// Controls welded into one framed bar with hairline dividers.
///
/// Three widgets with gaps between them read as three decisions; the preset
/// row is one decision, so it gets one outline.
pub struct Bar {
    /// One child `Ui` for the whole bar, and every cell is a child of *this*.
    ///
    /// Not a convenience. `Ui::new_child` bumps the **parent's** auto-id counter
    /// whichever salt the child was given, and every widget id after it is
    /// derived from that counter — so a bar that made one child of the *caller's*
    /// `Ui` per cell renamed everything downstream whenever a cell appeared. See
    /// the test, which names the gesture that broke.
    inner: egui::Ui,
    rect: egui::Rect,
    x: f32,
}

impl Bar {
    /// Allocate the bar and paint its frame. The cells are drawn afterwards and
    /// therefore on top of it.
    pub fn new(ui: &mut egui::Ui, salt: &str, height: f32) -> Self {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let radius = CornerRadius::same(RADIUS_CONTROL);
        let painter = ui.painter();
        painter.rect_filled(rect, radius, pal().surface);
        painter.rect_stroke(rect, radius, line_stroke(), egui::StrokeKind::Inside);
        let inner = ui.new_child(egui::UiBuilder::new().id_salt(salt).max_rect(rect));
        Self {
            inner,
            rect,
            x: rect.left(),
        }
    }

    pub fn cell(&mut self, salt: &str, width: f32) -> egui::Ui {
        self.slice(salt, width)
    }

    /// The flexible cell: everything except the `keep` the cells after it need.
    /// Stated by the caller rather than tracked here, because the bar cannot
    /// know what is still to come.
    pub fn rest(&mut self, salt: &str, keep: f32) -> egui::Ui {
        let width = (self.rect.right() - self.x - keep).max(0.0);
        self.slice(salt, width)
    }

    fn slice(&mut self, salt: &str, width: f32) -> egui::Ui {
        if self.x > self.rect.left() + 0.5 {
            self.inner.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(self.x, self.rect.top()),
                    egui::pos2(self.x + 1.0, self.rect.bottom()),
                ),
                0.0,
                pal().line,
            );
        }
        let cell = egui::Rect::from_min_max(
            egui::pos2(self.x, self.rect.top()),
            egui::pos2(self.x + width, self.rect.bottom()),
        );
        self.x += width;
        // A cell narrower than its own inset would come out inverted, and a
        // negative `available_size` makes every widget inside it lay out wrong.
        let inset = if cell.width() > 2.0 {
            cell.shrink(1.0)
        } else {
            cell
        };
        let mut child = self.inner.new_child(
            egui::UiBuilder::new()
                .id_salt(salt)
                .max_rect(inset)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        frameless(&mut child);
        child
    }
}

/// One background swatch: the gradient, and nothing else.
///
/// Not a `Button::image`. A button brings padding and a frame, which inset the
/// gradient inside a box and turned a grid of colours into a grid of widgets —
/// the swatch *is* the value, so the cell is all image.
pub fn swatch(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    side: f32,
    selected: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, selected, "")
    });
    let painter = ui.painter();
    let radius = CornerRadius::same(RADIUS_SMALL);
    let mut shape = egui::epaint::RectShape::filled(rect, radius, Color32::WHITE);
    shape.brush = Some(std::sync::Arc::new(egui::epaint::Brush {
        fill_texture_id: tex.id(),
        uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
    }));
    painter.add(shape);
    if selected {
        swatch_selected(painter, rect);
    } else if resp.hovered() {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, pal().text),
            egui::StrokeKind::Inside,
        );
    }
    resp
}

/// Mark the chosen swatch: a hairline round it, and an accent halo outside
/// that.
///
/// Reaches 2px beyond the cell, which the grid's 6px gap has room for. A tint
/// over the swatch would be wrong here — the swatch *is* the value, and
/// colouring it shows a background the export will not have.
fn swatch_selected(painter: &egui::Painter, rect: egui::Rect) {
    let radius = CornerRadius::same(RADIUS_SMALL);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(2.0_f32, ACCENT.gamma_multiply(0.35)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, pal().text),
        egui::StrokeKind::Inside,
    );
}

/// A colour button at the size the design asks for.
///
/// egui takes its size from `interact_size`, which is shared with every other
/// widget, so the size has to be scoped rather than set.
pub fn color_swatch(ui: &mut egui::Ui, color: &mut Color32, side: f32) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = egui::Vec2::splat(side);
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.corner_radius = CornerRadius::same(RADIUS_SMALL);
            state.bg_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(46));
        }
        ui.color_edit_button_srgba(color)
    })
    .inner
}

/// How wide a text button will come out, for a row that has to keep space for
/// one before it knows what else is on the row.
///
/// Measured rather than assumed. Every label here is translated, and a constant
/// that fits the English fits nothing else: "auto" is 34pt and "tự động" is half
/// again as wide.
pub fn text_button_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(size),
        pal().text,
    );
    galley.size().x + ui.spacing().button_padding.x * 2.0
}

/// An 11px dimmed line: what a control just did, or what it will do.
pub fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text)
            .size(11.0)
            .color(pal().text_dim),
    );
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

    /// The cells of a segmented track have to tile it exactly. Sizing each one
    /// `width / n` and rounding leaves a sliver of the track's own fill between
    /// them, which reads as a seam drawn through the selected cell.
    #[test]
    fn a_segmented_track_is_tiled_by_its_cells_with_no_seam() {
        for width in [120.0_f32, 199.0, 261.0, 288.5, 336.0] {
            for n in 2..=6_usize {
                let span = egui::Rangef::new(11.0, 11.0 + width);
                let mut prev = span.min;
                for i in 0..n {
                    let (x0, x1) = segment_bounds(span, i, n);
                    assert_eq!(x0, prev, "w={width} n={n} cell {i} leaves a seam behind it");
                    assert!(x1 > x0, "w={width} n={n} cell {i} is empty");
                    prev = x1;
                }
                assert_eq!(prev, span.max, "w={width} n={n}: the last cell misses the edge");
            }
        }
    }

    /// A slider has to reach both ends of its range, and the knob has to stay on
    /// the rail while it does. `x = left + t * width` gets the first and fails
    /// the second: the knob hangs half off at either end.
    #[test]
    fn a_slider_reaches_both_ends_of_its_range_without_leaving_the_rail() {
        let rail = egui::Rangef::new(40.0, 240.0);
        let travel = egui::Rangef::new(rail.min + 7.0, rail.max - 7.0);
        for (lo, hi) in [(0.0, 400.0), (0.0, 100.0), (-90.0, 90.0), (0.4, 4.0)] {
            assert_eq!(value_at(travel.min, travel, lo, hi, false), lo, "left end misses the minimum");
            assert_eq!(value_at(travel.max, travel, lo, hi, false), hi, "right end misses the maximum");
            for (value, t) in [(lo, 0.0_f32), (hi, 1.0)] {
                let knob = travel.min + fraction(value, lo, hi) * travel.span();
                assert!(
                    knob - 7.0 >= rail.min - 0.01 && knob + 7.0 <= rail.max + 0.01,
                    "the knob hangs off the rail at t={t}: centre {knob} on {rail:?}"
                );
            }
        }
    }

    /// Dragging past the end of the rail must pin to the end of the range, not
    /// walk out of it. The renderer clamps too, so this shows up as a slider
    /// that stops responding rather than as anything visibly wrong.
    #[test]
    fn dragging_off_the_end_of_a_rail_stays_inside_the_range() {
        let travel = egui::Rangef::new(50.0, 150.0);
        for x in [-400.0_f32, 0.0, 49.9, 150.1, 900.0] {
            let v = value_at(x, travel, 0.0, 80.0, true);
            assert!((0.0..=80.0).contains(&v), "x={x} picked {v}, outside 0..=80");
        }
    }

    /// An integral slider must not hand a fraction to a `u32` field, and a
    /// float one must keep its fraction.
    #[test]
    fn a_slider_rounds_only_for_whole_numbered_ranges() {
        let travel = egui::Rangef::new(0.0, 100.0);
        let whole = value_at(33.3, travel, 0.0, 10.0, true);
        assert_eq!(whole, whole.round(), "an integral range produced {whole}");
        let fine = value_at(33.3, travel, 0.0, 1.0, false);
        assert!(fine > 0.0 && fine < 1.0, "a float range was rounded away to {fine}");
    }

    /// The whole app reads `changed()` on a checkbox to decide whether to
    /// re-render the shot. A hand-painted widget does not get that for free, and
    /// a version that reported a change every frame would re-bake the preview
    /// forever — which costs 275ms a time.
    #[test]
    fn a_hand_painted_checkbox_reports_a_change_only_when_it_is_clicked() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::Dark);

        // The widget is placed by the layout, so its rect is only known after a
        // frame has drawn it.
        let mut on = false;
        let mut seen = egui::Rect::NOTHING;
        let quiet = |ctx: &egui::Context, on: &mut bool, seen: &mut egui::Rect| {
            let mut changed = false;
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                let resp = checkbox(ui, on, "Balance");
                *seen = resp.rect;
                changed = resp.changed();
            });
            changed
        };
        assert!(!quiet(&ctx, &mut on, &mut seen), "an untouched checkbox reported a change");
        assert!(!on, "an untouched checkbox toggled itself");

        let click = seen.center();
        let press = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run_ui(press, |ui| {
            checkbox(ui, &mut on, "Balance");
        });

        let mut changed = false;
        let release = egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos: click,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(release, |ui| {
            changed = checkbox(ui, &mut on, "Balance").changed();
        });
        assert!(changed, "clicking a checkbox reported no change");
        assert!(on, "clicking a checkbox did not toggle it");

        assert!(
            !quiet(&ctx, &mut on, &mut seen),
            "a checkbox went on reporting a change after the click was over"
        );
    }

    /// A ticked checkbox is an accent box with the tick *punched out of it* in
    /// panel ink — not a tick drawn in the same ink as the label.
    ///
    /// This is the whole reason the widget is painted by hand rather than styled:
    /// egui takes the tick from `fg_stroke`, which is the text colour, and a
    /// near-white tick on the accent has barely any contrast. Asserted on the
    /// shapes rather than on pixels, because it is a decision about which colour
    /// goes where and not about anti-aliasing.
    #[test]
    fn a_ticked_checkbox_punches_its_tick_out_of_the_accent_in_panel_ink() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::Dark);

        let mut on = true;
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            checkbox(ui, &mut on, "Balance");
        });

        let mut box_fill = None;
        let mut tick_ink = None;
        for clipped in &out.shapes {
            match &clipped.shape {
                egui::Shape::Rect(r) if r.fill == ACCENT => box_fill = Some(r.fill),
                egui::Shape::Path(p) => {
                    if let egui::epaint::ColorMode::Solid(c) = p.stroke.color {
                        tick_ink = Some(c);
                    }
                }
                _ => {}
            }
        }
        assert_eq!(box_fill, Some(ACCENT), "a ticked box is not filled with the accent");
        assert_eq!(
            tick_ink,
            Some(DARK.panel),
            "the tick is not drawn in panel ink, so it has no contrast against the accent"
        );
    }

    /// A welded field carries its padding on the *frame*.
    ///
    /// This is the whole reason `welded_field` exists rather than `Frame::NONE`:
    /// `TextEdit` reads its text inset from the frame when it has one and from
    /// `margin` only when it has none, so setting the margin on a `Frame::NONE`
    /// field compiles, reads correctly, and does nothing — the caret stays flush
    /// against the divider. Reported twice before the cause was found.
    #[test]
    fn a_welded_field_keeps_its_text_off_the_divider() {
        let frame = welded_field();
        assert_eq!(
            frame.inner_margin.left, FIELD_PAD,
            "the padding is not on the frame, so the field will ignore it"
        );
        assert_eq!(frame.inner_margin.right, FIELD_PAD, "right side too");
        assert_eq!(
            frame.fill,
            Color32::TRANSPARENT,
            "a welded field must show the bar's own fill, not one of its own"
        );
        assert_eq!(
            frame.stroke.width, 0.0,
            "a welded field must not draw a second border inside the bar's"
        );
    }

    /// A segmented track wraps rather than clipping its labels.
    ///
    /// Four watermark styles on one 336pt row left 74pt each, and "Rounded plate"
    /// lost its last word to the clip — reported as options being hidden. Every
    /// cell has to be wide enough for the longest label in the set, because they
    /// all share a width.
    #[test]
    fn a_segmented_track_gives_every_cell_room_for_the_longest_label() {
        // The real case: the sidebar's usable width, and the widest label in each
        // set as laid out at 12px.
        for (width, widest, n) in [
            (288.0_f32, 78.0_f32, 4), // watermark styles
            (288.0, 30.0, 5),         // aspect ratios: Auto · 4:3 · 3:2 · 16:9 · 1:1
            (288.0, 60.0, 3),         // PNG · JPEG · WebP
            (120.0, 78.0, 4),         // a panel far narrower than we ship
        ] {
            let per_row = cells_per_row(width, widest, n);
            let rows = row_sizes(n, per_row);
            assert_eq!(rows.iter().sum::<usize>(), n, "cells went missing");
            for count in rows {
                let cell = width / count as f32;
                // One cell per row is the floor: below that there is nothing left
                // to give, and clipping is the honest outcome.
                assert!(
                    count == 1 || cell >= widest,
                    "w={width} n={n}: {count} cells of {cell:.0}pt cannot hold a {widest:.0}pt label"
                );
            }
        }
    }

    /// Rows are filled evenly, so no row is left with one lonely cell in it.
    #[test]
    fn segmented_rows_are_balanced() {
        assert_eq!(row_sizes(4, 2), vec![2, 2], "four options at two per row");
        assert_eq!(row_sizes(5, 3), vec![3, 2], "five at three, not 3+1+1");
        assert_eq!(row_sizes(5, 5), vec![5], "everything fits on one row");
        assert_eq!(row_sizes(1, 3), vec![1], "one option is one row");
    }

    /// A bar whose cells come and go must not rename the widgets after it.
    ///
    /// `Ui::new_child` bumps the *parent's* auto-id counter whether or not the
    /// child was given a salt (`egui-0.34.3/src/ui.rs:303`), and every widget id
    /// downstream is derived from that counter. So a bar that made one child per
    /// cell renamed the whole sidebar the moment its delete button appeared —
    /// and that button appears exactly when the current style matches a saved
    /// preset, which stops being true on the *first frame of a drag*. The drag
    /// then died after one step, and a number box lost focus after one
    /// keystroke. The bar therefore takes one child of its own and puts every
    /// cell inside that, so the parent sees the same single bump either way.
    #[test]
    fn a_bar_whose_cells_come_and_go_does_not_rename_what_follows_it() {
        let _serial = alone();
        let ctx = egui::Context::default();
        apply(&ctx, ThemeMode::Dark);

        let mut ids = Vec::new();
        for delete_cell in [true, false] {
            let mut after = egui::Id::NULL;
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut bar = Bar::new(ui, "presets", H_BAR);
                let _pick = bar.rest("pick", 90.0);
                let _name = bar.cell("name", 60.0);
                if delete_cell {
                    let _delete = bar.cell("delete", 30.0);
                }
                after = ui.button("whatever comes next").id;
            });
            ids.push(after);
        }
        assert_eq!(
            ids[0], ids[1],
            "the delete cell coming and going renames every widget after the bar"
        );

        // And the mechanism itself, asserted directly, so this test cannot
        // quietly stop testing anything: an extra `new_child` on the caller's own
        // `Ui` *does* rename what follows it. If an egui upgrade ever stops
        // folding the parent's child count into later ids, the guard above has
        // nothing left to catch and `Bar` can go back to being simpler.
        let mut naive = Vec::new();
        for extra_child in [true, false] {
            let mut after = egui::Id::NULL;
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                if extra_child {
                    let _ = ui.new_child(egui::UiBuilder::new().id_salt("extra"));
                }
                after = ui.button("whatever comes next").id;
            });
            naive.push(after);
        }
        assert_ne!(
            naive[0], naive[1],
            "egui no longer renames later widgets, so this whole guard is obsolete"
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
