//! Annotation layers: arrows, shapes, text, blur and highlight.
//!
//! Layer geometry is stored in *original screenshot pixels*, independent of any
//! preview downscale or of the Balance crop. [`apply`] takes a `scale` that maps
//! those coordinates onto whatever bitmap it is drawing into, so the preview and
//! the export run the same code.
//!
//! Shapes are rasterised from signed distance fields rather than by stepping
//! along lines, which gives antialiased edges for free at any stroke width.

use crate::i18n::t;

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::render::frame::blend;
use crate::render::text;
use crate::settings::Rgba8;
use ab_glyph::FontArc;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Tool {
    Select,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Blur,
    Highlight,
    /// Flat, fully opaque rectangle. Not offered as a tool any more — the
    /// paint tool reaches this at the top of its range — but redaction still
    /// builds these from OCR hits, where "covered" is not negotiable.
    Fill,
}

impl Tool {
    pub const DRAWABLE: [Tool; 6] = [
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Blur,
        Tool::Highlight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => t("Select"),
            Tool::Arrow => t("Arrow"),
            Tool::Rect => t("Rectangle"),
            Tool::Ellipse => t("Ellipse"),
            Tool::Text => t("Text"),
            Tool::Blur => t("Blur"),
            Tool::Highlight => t("Paint"),
            Tool::Fill => t("Fill"),
        }
    }

    /// Text is placed with a click; the rest are dragged out.
    pub fn is_point_tool(self) -> bool {
        self == Tool::Text
    }

    /// Whether line thickness means anything for this tool. Paint and Blur
    /// cover whole regions and Text has its own size, so showing them a stroke
    /// slider would be offering a control that does nothing.
    pub fn uses_stroke(self) -> bool {
        matches!(self, Tool::Arrow | Tool::Rect | Tool::Ellipse)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default = "Layer::placeholder")]
pub struct Layer {
    pub kind: Tool,
    /// Drag start, in original screenshot pixels.
    pub a: [f32; 2],
    /// Drag end. For [`Tool::Text`] this is unused.
    pub b: [f32; 2],
    pub color: Rgba8,
    pub stroke: f32,
    pub text: String,
    pub font_size: f32,
    pub blur: f32,
    /// Turn about the shape's own centre, in radians, clockwise on screen.
    ///
    /// Only the tools that *draw* something carry this. Blur and Fill cover
    /// information rather than decorate it, so there is nothing to gain by
    /// tilting them and [`Layer::turnable`] says so.
    pub angle: f32,
}

impl Layer {
    pub fn new(
        kind: Tool,
        a: [f32; 2],
        color: Rgba8,
        stroke: f32,
        font_size: f32,
        blur: f32,
    ) -> Self {
        Self {
            kind,
            a,
            b: a,
            color,
            stroke,
            text: String::new(),
            font_size,
            blur,
            angle: 0.0,
        }
    }

    /// A blank layer, only so `serde(default)` has something to fill new fields
    /// from when an older saved layer is read back.
    fn placeholder() -> Self {
        Self::new(Tool::Select, [0.0, 0.0], [0, 0, 0, 255], 1.0, 12.0, 1.0)
    }

    /// The point a rotation turns about.
    pub fn centre(&self) -> [f32; 2] {
        match self.kind {
            // A label grows right and down from where it was placed, and its
            // width is not in the struct, so its own origin is the only anchor
            // both the renderer and the editor can agree on without measuring.
            Tool::Text => self.a,
            _ => [(self.a[0] + self.b[0]) / 2.0, (self.a[1] + self.b[1]) / 2.0],
        }
    }

    /// Whether turning this tool means anything.
    pub fn turnable(kind: Tool) -> bool {
        !matches!(kind, Tool::Blur | Tool::Fill | Tool::Select)
    }

    /// `p` brought back into the shape's own upright frame.
    pub fn unturn(&self, p: [f32; 2]) -> [f32; 2] {
        if self.angle.abs() < 1e-4 {
            return p;
        }
        let c = self.centre();
        let (sin, cos) = (-self.angle).sin_cos();
        let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
        [c[0] + dx * cos - dy * sin, c[1] + dx * sin + dy * cos]
    }

    /// Axis-aligned bounds in original screenshot pixels, padded for the stroke.
    pub fn bounds(&self) -> [f32; 4] {
        let pad = match self.kind {
            Tool::Text => self.font_size,
            _ => self.stroke * 3.0,
        };
        let (x0, x1) = min_max(self.a[0], self.b[0]);
        let (y0, y1) = min_max(self.a[1], self.b[1]);
        [x0 - pad, y0 - pad, x1 + pad, y1 + pad]
    }

    pub fn hit(&self, x: f32, y: f32) -> bool {
        let [x0, y0, x1, y1] = self.bounds();
        let [x, y] = self.unturn([x, y]);
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.a[0] += dx;
        self.a[1] += dy;
        self.b[0] += dx;
        self.b[1] += dy;
    }

    /// A drag that never moved is a stray click, not a shape — except for text,
    /// which is placed by clicking.
    pub fn is_degenerate(&self) -> bool {
        match self.kind {
            Tool::Text => self.text.trim().is_empty(),
            _ => (self.a[0] - self.b[0]).abs() < 3.0 && (self.a[1] - self.b[1]).abs() < 3.0,
        }
    }
}

fn min_max(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Draw every layer onto `img`. `scale` maps original screenshot pixels onto
/// `img` pixels (1.0 for export, the preview factor for the preview).
pub fn apply(img: &mut RgbaImage, layers: &[Layer], scale: f32, font: Option<&FontArc>) {
    for layer in layers {
        let a = [layer.a[0] * scale, layer.a[1] * scale];
        let b = [layer.b[0] * scale, layer.b[1] * scale];
        let stroke = (layer.stroke * scale).max(1.0);
        let c = layer.centre();
        let turn = Turn {
            angle: layer.angle,
            centre: [c[0] * scale, c[1] * scale],
        };
        match layer.kind {
            Tool::Arrow => draw_arrow(img, a, b, stroke, layer.color, turn, scale),
            Tool::Rect => draw_rect(img, a, b, stroke, layer.color, turn),
            Tool::Ellipse => draw_ellipse(img, a, b, stroke, layer.color, turn),
            Tool::Highlight => draw_paint(img, a, b, layer.color, turn),
            Tool::Fill => draw_fill(img, a, b, layer.color),
            Tool::Blur => draw_blur(img, a, b, (layer.blur * scale).max(1.0)),
            Tool::Text => {
                if let Some(font) = font {
                    let size = (layer.font_size * scale).max(6.0);
                    draw_text(img, font, size, a, Rgba(layer.color), &layer.text, layer.angle);
                }
            }
            Tool::Select => {}
        }
    }
}

// ------------------------------------------------------------ distance fields

fn sd_segment(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (pax, pay) = (px - a[0], py - a[1]);
    let (bax, bay) = (b[0] - a[0], b[1] - a[1]);
    let denom = bax * bax + bay * bay;
    let h = if denom <= f32::EPSILON {
        0.0
    } else {
        ((pax * bax + pay * bay) / denom).clamp(0.0, 1.0)
    };
    let dx = pax - bax * h;
    let dy = pay - bay * h;
    (dx * dx + dy * dy).sqrt()
}

/// Distance to the *outline* of an axis-aligned box.
fn sd_box_outline(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32) -> f32 {
    let qx = (px - cx).abs() - hw;
    let qy = (py - cy).abs() - hh;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    (outside + inside).abs()
}

/// Approximate distance to an ellipse outline. Exact ellipse SDF needs an
/// iterative solve; scaling the space to a circle is close enough at the stroke
/// widths we draw and costs a fraction as much.
fn sd_ellipse_outline(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let (rx, ry) = (rx.max(0.5), ry.max(0.5));
    let nx = (px - cx) / rx;
    let ny = (py - cy) / ry;
    let len = (nx * nx + ny * ny).sqrt();
    (len - 1.0).abs() * rx.min(ry)
}

/// Rasterise a shape over its bounding box, `sdf` returning distance to the
/// figure and `cov` converting that to coverage.
/// How a shape is turned: by `angle`, about `centre` in the pixels being drawn
/// into. The two always travel together, so they travel as one.
#[derive(Clone, Copy)]
struct Turn {
    angle: f32,
    centre: [f32; 2],
}

/// Rasterise a shape turned about `centre`.
///
/// Rotation costs nothing here but turning the *sample point* back into the
/// shape's upright frame — the distance field never learns about the angle.
/// That is why tilting an arrow needed no new geometry: every shape already
/// answers "how far is this pixel from me?", and asking it about a different
/// pixel is the whole of the change.
fn rasterise_turned(
    img: &mut RgbaImage,
    bounds: [f32; 4],
    color: Rgba8,
    turn: Turn,
    sdf: impl Fn(f32, f32) -> f32,
    half_stroke: f32,
) {
    let Turn { angle, centre } = turn;
    if angle.abs() < 1e-4 {
        rasterise(img, bounds, color, sdf, half_stroke);
        return;
    }
    // The upright bounds no longer contain the shape, so sweep the box that
    // holds them once turned.
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = (centre[0], centre[1]);
    let mut swept = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in [
        (bounds[0], bounds[1]),
        (bounds[2], bounds[1]),
        (bounds[2], bounds[3]),
        (bounds[0], bounds[3]),
    ] {
        let (dx, dy) = (x - cx, y - cy);
        let (rx, ry) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        swept[0] = swept[0].min(rx);
        swept[1] = swept[1].min(ry);
        swept[2] = swept[2].max(rx);
        swept[3] = swept[3].max(ry);
    }

    let (usin, ucos) = (-angle).sin_cos();
    rasterise(
        img,
        swept,
        color,
        |px, py| {
            let (dx, dy) = (px - cx, py - cy);
            sdf(cx + dx * ucos - dy * usin, cy + dx * usin + dy * ucos)
        },
        half_stroke,
    );
}

fn rasterise(
    img: &mut RgbaImage,
    bounds: [f32; 4],
    color: Rgba8,
    sdf: impl Fn(f32, f32) -> f32,
    half_stroke: f32,
) {
    let x0 = bounds[0].floor().max(0.0) as u32;
    let y0 = bounds[1].floor().max(0.0) as u32;
    let x1 = (bounds[2].ceil() as i64).clamp(0, img.width() as i64) as u32;
    let y1 = (bounds[3].ceil() as i64).clamp(0, img.height() as i64) as u32;

    for y in y0..y1 {
        for x in x0..x1 {
            let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
            let cov = (half_stroke + 0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(img, x, y, Rgba(color), cov);
            }
        }
    }
}

// ------------------------------------------------------------------- drawing

fn draw_arrow(
    img: &mut RgbaImage,
    a: [f32; 2],
    b: [f32; 2],
    stroke: f32,
    color: Rgba8,
    turn: Turn,
    scale: f32,
) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    // The floor is 10 *shot* pixels, so it has to be scaled with everything
    // else. A bare 10 here is 10 preview pixels in the preview and 10 export
    // pixels in the export, which gave the same arrow two different heads.
    let head = (stroke * 4.0).max(10.0 * scale).min(len);
    let along = dy.atan2(dx);

    // Two barbs swept back from the tip at ±28°.
    let barb = |sign: f32| -> [f32; 2] {
        let t = along + std::f32::consts::PI + sign * 0.49;
        [b[0] + head * t.cos(), b[1] + head * t.sin()]
    };
    let (l, r) = (barb(-1.0), barb(1.0));

    let pad = stroke * 2.0;
    let x0 = a[0].min(b[0]).min(l[0]).min(r[0]) - pad;
    let y0 = a[1].min(b[1]).min(l[1]).min(r[1]) - pad;
    let x1 = a[0].max(b[0]).max(l[0]).max(r[0]) + pad;
    let y1 = a[1].max(b[1]).max(l[1]).max(r[1]) + pad;

    rasterise_turned(
        img,
        [x0, y0, x1, y1],
        color,
        turn,
        |px, py| {
            sd_segment(px, py, a, b)
                .min(sd_segment(px, py, b, l))
                .min(sd_segment(px, py, b, r))
        },
        stroke / 2.0,
    );
}

fn draw_rect(
    img: &mut RgbaImage,
    a: [f32; 2],
    b: [f32; 2],
    stroke: f32,
    color: Rgba8,
    turn: Turn,
) {
    let (x0, x1) = min_max(a[0], b[0]);
    let (y0, y1) = min_max(a[1], b[1]);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let pad = stroke + 2.0;
    rasterise_turned(
        img,
        [x0 - pad, y0 - pad, x1 + pad, y1 + pad],
        color,
        turn,
        |px, py| sd_box_outline(px, py, cx, cy, hw, hh),
        stroke / 2.0,
    );
}

fn draw_ellipse(
    img: &mut RgbaImage,
    a: [f32; 2],
    b: [f32; 2],
    stroke: f32,
    color: Rgba8,
    turn: Turn,
) {
    let (x0, x1) = min_max(a[0], b[0]);
    let (y0, y1) = min_max(a[1], b[1]);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let pad = stroke + 2.0;
    rasterise_turned(
        img,
        [x0 - pad, y0 - pad, x1 + pad, y1 + pad],
        color,
        turn,
        |px, py| sd_ellipse_outline(px, py, cx, cy, rx, ry),
        stroke / 2.0,
    );
}

/// One paint tool, spanning what used to be two.
///
/// The colour's alpha is a single dial from marker to paint. Low down it is a
/// highlighter: the ink is picked by what it lands on — multiply where the
/// pixel is light, screen where it is dark — because a plain multiply can only
/// ever *darken*, and over a dark UI that turns bright ink into a muddy smear
/// that reads as "nothing happened". Both halves preserve contrast, so text
/// under a light stroke stays readable.
///
/// Turned up, the ink stops being translucent and becomes coverage, reaching
/// the flat colour exactly at the top of the range. That end of the dial is
/// what used to be a separate "cover" tool.
fn draw_paint(
    img: &mut RgbaImage,
    a: [f32; 2],
    b: [f32; 2],
    color: Rgba8,
    turn: Turn,
) {
    // Paint blends rather than stamping a distance field, so it cannot borrow
    // `rasterise_turned`. Same idea though: sweep the box the tilted region
    // needs and ask each pixel whether it lands inside the upright one.
    let Turn { angle, centre } = turn;
    let Some((x0, y0, x1, y1)) = clamp_region(img, a, b) else {
        return;
    };
    let (rx0, ry0, rx1, ry1) = (
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    );
    let turned = angle.abs() >= 1e-4;
    let (x0, y0, x1, y1) = if turned {
        let Some(swept) = clamp_region(img, [rx0, ry0], [rx1, ry1]).map(|_| {
            let (sin, cos) = angle.sin_cos();
            let mut bb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for (px, py) in [(rx0, ry0), (rx1, ry0), (rx1, ry1), (rx0, ry1)] {
                let (dx, dy) = (px - centre[0], py - centre[1]);
                let (qx, qy) = (
                    centre[0] + dx * cos - dy * sin,
                    centre[1] + dx * sin + dy * cos,
                );
                bb[0] = bb[0].min(qx);
                bb[1] = bb[1].min(qy);
                bb[2] = bb[2].max(qx);
                bb[3] = bb[3].max(qy);
            }
            bb
        }) else {
            return;
        };
        match clamp_region(img, [swept[0], swept[1]], [swept[2], swept[3]]) {
            Some(r) => r,
            None => return,
        }
    } else {
        (x0, y0, x1, y1)
    };

    let (usin, ucos) = (-angle).sin_cos();
    let strength = color[3] as f32 / 255.0;
    for y in y0..y1 {
        for x in x0..x1 {
            if turned {
                let (dx, dy) = (x as f32 + 0.5 - centre[0], y as f32 + 0.5 - centre[1]);
                let ux = centre[0] + dx * ucos - dy * usin;
                let uy = centre[1] + dx * usin + dy * ucos;
                if ux < rx0 || ux >= rx1 || uy < ry0 || uy >= ry1 {
                    continue;
                }
            }
            let p = img.get_pixel_mut(x, y);
            let lum = luminance(p.0);
            for (channel, tint) in p.0.iter_mut().zip(color).take(3) {
                let base = *channel as f32 / 255.0;
                let tint = tint as f32 / 255.0;
                let multiply = base * tint;
                let screen = 1.0 - (1.0 - base) * (1.0 - tint);
                let marker = multiply * lum + screen * (1.0 - lum);
                // The ink itself hardens towards the flat colour as the dial
                // rises, so the top of the range lands on exactly the tint.
                let ink = marker + (tint - marker) * strength;
                let mixed = base + (ink - base) * strength;
                *channel = (mixed * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A label, turned about the point it was placed at.
///
/// Glyphs are blitted, not sampled from a field, so the trick used for the
/// shapes does not apply. Instead the text goes onto a transparent scratch
/// image which is then turned whole — exactly what `render::watermark` already
/// does for the wordmark, so the two share one rotation.
fn draw_text(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    at: [f32; 2],
    color: Rgba<u8>,
    body: &str,
    angle: f32,
) {
    if angle.abs() < 1e-4 {
        text::draw(img, font, size, at[0], at[1], color, body);
        return;
    }
    let w = text::measure(font, size, body);
    // `measure` answers width only; a line is about 1.35 em tall with its
    // descenders, and the scratch image is padded generously anyway.
    let h = size * 1.35;
    if w < 1.0 {
        return;
    }
    // A margin, so the turned corners are not shaved off the scratch image.
    let pad = size;
    let mut stamp = RgbaImage::from_pixel(
        (w + pad * 2.0).ceil() as u32,
        (h + pad * 2.0).ceil() as u32,
        Rgba([0, 0, 0, 0]),
    );
    text::draw(&mut stamp, font, size, pad, pad, color, body);
    let turned = crate::render::watermark::rotate(&stamp, angle);

    // The label turns about its own origin, but `rotate` turns the scratch
    // image about the *image's* centre. So follow where the origin ended up and
    // shift the result until it lands back where the label belongs.
    //
    // Getting this wrong is not subtle but it is silent: the glyphs tilt
    // correctly and simply sit somewhere else, which shows up as the selection
    // frame no longer agreeing with the text it is drawn around.
    let (sin, cos) = angle.sin_cos();
    let (sw, sh) = (stamp.width() as f32, stamp.height() as f32);
    let (tw, th) = (turned.width() as f32, turned.height() as f32);
    // The origin, as an offset from the scratch image's centre.
    let (vx, vy) = (pad - sw / 2.0, pad - sh / 2.0);
    let ox = at[0] - (tw / 2.0 + vx * cos - vy * sin);
    let oy = at[1] - (th / 2.0 + vx * sin + vy * cos);
    image::imageops::overlay(img, &turned, ox.round() as i64, oy.round() as i64);
}

/// Perceptual brightness, 0.0 to 1.0.
fn luminance(px: [u8; 4]) -> f32 {
    (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0
}

fn draw_fill(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], color: Rgba8) {
    let Some((x0, y0, x1, y1)) = clamp_region(img, a, b) else {
        return;
    };
    for y in y0..y1 {
        for x in x0..x1 {
            blend(img, x, y, Rgba(color), 1.0);
        }
    }
}

fn draw_blur(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], sigma: f32) {
    let Some((x0, y0, x1, y1)) = clamp_region(img, a, b) else {
        return;
    };
    let region = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
    let blurred = image::imageops::blur(&region, sigma);
    image::imageops::replace(img, &blurred, x0 as i64, y0 as i64);
}

/// Integer pixel region covered by the drag, clipped to the image.
fn clamp_region(img: &RgbaImage, a: [f32; 2], b: [f32; 2]) -> Option<(u32, u32, u32, u32)> {
    let (fx0, fx1) = min_max(a[0], b[0]);
    let (fy0, fy1) = min_max(a[1], b[1]);
    let x0 = fx0.floor().max(0.0) as u32;
    let y0 = fy0.floor().max(0.0) as u32;
    let x1 = (fx1.ceil().max(0.0) as u32).min(img.width());
    let y1 = (fy1.ceil().max(0.0) as u32).min(img.height());
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// Undo/redo over whole-layer-stack snapshots. The stacks are small — a handful
/// of layers with short strings — so copying beats tracking deltas.
#[derive(Default)]
pub struct History {
    past: Vec<Vec<Layer>>,
    future: Vec<Vec<Layer>>,
}

const MAX_UNDO: usize = 64;

impl History {
    /// Record the state *before* a change is applied.
    pub fn push(&mut self, current: &[Layer]) {
        self.past.push(current.to_vec());
        if self.past.len() > MAX_UNDO {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self, current: &mut Vec<Layer>) {
        if let Some(prev) = self.past.pop() {
            self.future.push(std::mem::replace(current, prev));
        }
    }

    pub fn redo(&mut self, current: &mut Vec<Layer>) {
        if let Some(next) = self.future.pop() {
            self.past.push(std::mem::replace(current, next));
        }
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> RgbaImage {
        RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 0]))
    }

    fn ink(img: &RgbaImage) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for y in 0..img.height() {
            for x in 0..img.width() {
                if img.get_pixel(x, y).0[3] > 40 {
                    out.push((x, y));
                }
            }
        }
        out
    }

    fn shape(kind: Tool, angle: f32) -> Layer {
        let mut l = Layer::new(kind, [60.0, 90.0], [255, 0, 0, 255], 6.0, 30.0, 8.0);
        l.b = [140.0, 110.0];
        l.angle = angle;
        l
    }

    /// Turning a shape has to move ink, and it has to move it *about the
    /// centre* — a rotation that also slid the shape sideways would be a
    /// translation wearing a disguise.
    #[test]
    fn turning_a_shape_moves_its_ink_but_not_its_centre() {
        for kind in [Tool::Rect, Tool::Ellipse, Tool::Arrow] {
            let mut upright = canvas();
            let mut turned = canvas();
            apply(&mut upright, &[shape(kind, 0.0)], 1.0, None);
            apply(&mut turned, &[shape(kind, std::f32::consts::FRAC_PI_2)], 1.0, None);

            let (u, t) = (ink(&upright), ink(&turned));
            assert!(!u.is_empty() && !t.is_empty(), "{kind:?} drew nothing at all");
            assert_ne!(u, t, "{kind:?} ignored its angle");

            // A wide flat shape turned a quarter turn becomes a tall thin one.
            let span = |p: &[(u32, u32)], f: fn(&(u32, u32)) -> u32| {
                p.iter().map(f).max().unwrap() - p.iter().map(f).min().unwrap()
            };
            assert!(
                span(&u, |p| p.0) > span(&u, |p| p.1),
                "{kind:?}: the fixture should start wider than it is tall"
            );
            assert!(
                span(&t, |p| p.1) > span(&t, |p| p.0),
                "{kind:?}: a quarter turn should leave it taller than it is wide"
            );

            let mid = |p: &[(u32, u32)], f: fn(&(u32, u32)) -> u32| {
                (p.iter().map(f).min().unwrap() + p.iter().map(f).max().unwrap()) as i64 / 2
            };
            assert!(
                (mid(&u, |p| p.0) - mid(&t, |p| p.0)).abs() <= 3
                    && (mid(&u, |p| p.1) - mid(&t, |p| p.1)).abs() <= 3,
                "{kind:?} drifted off its centre while turning"
            );
        }
    }

    /// A turn is written to the layer on every frame of the drag, so the
    /// renderer sees every angle in between — at preview scale, where the
    /// numbers are smallest and any degenerate case shows up first.
    #[test]
    fn every_angle_renders_at_every_scale() {
        let font = crate::render::text::load_system_font().map(|(_, f)| f);
        for kind in [Tool::Arrow, Tool::Rect, Tool::Ellipse, Tool::Text, Tool::Highlight] {
            for steps in 0..24 {
                let angle = steps as f32 / 24.0 * std::f32::consts::TAU - std::f32::consts::PI;
                for scale in [0.12_f32, 0.5, 1.0] {
                    let mut img = RgbaImage::from_pixel(120, 90, Rgba([0, 0, 0, 255]));
                    let mut l = shape(kind, angle);
                    l.text = "xin chào".to_owned();
                    apply(&mut img, &[l], scale, font.as_ref());
                }
            }
        }
    }

    /// A label turns about the point it was placed at, so however far it is
    /// turned its ink stays the same distance from that point. The scratch
    /// image it is rotated on has its own centre, and lining the two up is easy
    /// to get wrong in a way that tilts the glyphs correctly and puts them
    /// somewhere else entirely.
    #[test]
    fn a_turned_label_stays_anchored_to_where_it_was_placed() {
        let Some((_, font)) = crate::render::text::load_system_font() else {
            return; // no system font on this machine; the renderer skips text too
        };
        let at = [100.0_f32, 100.0];
        let mut reach = Vec::new();
        for steps in 0..8 {
            let angle = steps as f32 / 8.0 * std::f32::consts::TAU;
            let mut img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 0]));
            let mut l = Layer::new(Tool::Text, at, [255, 0, 0, 255], 2.0, 20.0, 8.0);
            l.text = "anchor".to_owned();
            l.angle = angle;
            apply(&mut img, &[l], 1.0, Some(&font));

            let lit = ink(&img);
            assert!(!lit.is_empty(), "nothing drawn at {angle:.2} rad");
            let cx = lit.iter().map(|p| p.0 as f32).sum::<f32>() / lit.len() as f32;
            let cy = lit.iter().map(|p| p.1 as f32).sum::<f32>() / lit.len() as f32;
            reach.push(((cx - at[0]).powi(2) + (cy - at[1]).powi(2)).sqrt());
        }
        let (lo, hi) = (
            reach.iter().cloned().fold(f32::MAX, f32::min),
            reach.iter().cloned().fold(0.0_f32, f32::max),
        );
        assert!(
            hi - lo < 6.0,
            "the label drifts as it turns: its ink sits {lo:.0}..{hi:.0}px from \
             the point it was placed at, so it is not turning about that point"
        );
    }

    /// The arrowhead has a floor so a hairline arrow still reads as an arrow,
    /// and that floor is 10 *shot* pixels. It used to be 10 pixels of whatever
    /// bitmap was being drawn into, which is not the same thing: the preview is
    /// drawn at a fraction of full size, so it grew a head the export never got.
    #[test]
    fn the_arrowhead_is_the_same_size_in_the_preview_and_the_export() {
        let head_spread = |scale: f32, side: u32| {
            let mut img = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
            let mut l = Layer::new(Tool::Arrow, [10.0, 100.0], [255, 0, 0, 255], 1.0, 20.0, 8.0);
            l.b = [190.0, 100.0];
            apply(&mut img, &[l], scale, None);
            let lit = ink(&img);
            let ys: Vec<u32> = lit.iter().map(|p| p.1).collect();
            // A horizontal arrow is one pixel tall but for its head, so the
            // vertical spread is the head and nothing else.
            (ys.iter().max().copied().unwrap_or(0) - ys.iter().min().copied().unwrap_or(0)) as f32
        };

        let full = head_spread(1.0, 200);
        let half = head_spread(0.5, 100);
        assert!(full > 4.0 && half > 2.0, "the arrow drew no head at all");
        let ratio = full / half;
        assert!(
            (ratio - 2.0).abs() < 0.3,
            "the head is {full}px at full size and {half}px at half — {ratio:.2}× apart \
             rather than 2×, so the preview and the export disagree about the arrow"
        );
    }

    /// Blur and Fill hide information rather than decorate it, so they stay
    /// square however the angle is set — there is nothing to gain from a
    /// tilted redaction box, and a great deal of code to go wrong.
    #[test]
    fn the_covering_tools_ignore_the_angle() {
        assert!(!Layer::turnable(Tool::Blur));
        assert!(!Layer::turnable(Tool::Fill));
        assert!(Layer::turnable(Tool::Rect));
        assert!(Layer::turnable(Tool::Text));

        let mut square = canvas();
        let mut asked = canvas();
        apply(&mut square, &[shape(Tool::Fill, 0.0)], 1.0, None);
        apply(&mut asked, &[shape(Tool::Fill, 0.9)], 1.0, None);
        assert_eq!(
            ink(&square),
            ink(&asked),
            "a redaction box tilted, which it must never do"
        );
    }

    /// Hit-testing has to undo the rotation, or a turned shape can only be
    /// grabbed where it used to be.
    #[test]
    fn a_turned_shape_is_grabbed_where_it_now_sits() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let turned = shape(Tool::Rect, quarter);
        let c = turned.centre();
        // The upright shape is wide and flat, so a point far out to the side is
        // inside it; once turned a quarter, that point should be outside and
        // the same distance *above* the centre should be inside.
        assert!(shape(Tool::Rect, 0.0).hit(c[0] + 35.0, c[1]));
        assert!(!turned.hit(c[0] + 35.0, c[1]), "grabbed where it no longer is");
        assert!(turned.hit(c[0], c[1] + 35.0), "cannot be grabbed where it now sits");
    }


    fn layer(kind: Tool, a: [f32; 2], b: [f32; 2]) -> Layer {
        let mut l = Layer::new(kind, a, [255, 0, 0, 255], 4.0, 32.0, 8.0);
        l.b = b;
        l
    }

    #[test]
    fn segment_distance_is_zero_on_the_line_and_grows_off_it() {
        let (a, b) = ([0.0, 0.0], [10.0, 0.0]);
        assert!(sd_segment(5.0, 0.0, a, b).abs() < 1e-5);
        assert!((sd_segment(5.0, 3.0, a, b) - 3.0).abs() < 1e-5);
        // Past the end it measures from the endpoint, not the infinite line.
        assert!((sd_segment(14.0, 0.0, a, b) - 4.0).abs() < 1e-5);
    }

    #[test]
    fn a_zero_length_segment_does_not_divide_by_zero() {
        let d = sd_segment(3.0, 4.0, [0.0, 0.0], [0.0, 0.0]);
        assert!((d - 5.0).abs() < 1e-5);
    }

    #[test]
    fn box_outline_distance_is_zero_on_the_edge() {
        // 20x20 box centred at (50,50): the edge sits at x=40.
        assert!(sd_box_outline(40.0, 50.0, 50.0, 50.0, 10.0, 10.0).abs() < 1e-5);
        // Equidistant inside and outside — it is an outline, not a fill.
        assert!((sd_box_outline(45.0, 50.0, 50.0, 50.0, 10.0, 10.0) - 5.0).abs() < 1e-5);
        assert!((sd_box_outline(35.0, 50.0, 50.0, 50.0, 10.0, 10.0) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn ellipse_outline_distance_is_zero_on_the_axes() {
        assert!(sd_ellipse_outline(60.0, 50.0, 50.0, 50.0, 10.0, 20.0).abs() < 1e-5);
        assert!(sd_ellipse_outline(50.0, 70.0, 50.0, 50.0, 10.0, 20.0).abs() < 1e-5);
    }

    #[test]
    fn drawing_marks_the_image_and_respects_scale() {
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let layers = vec![layer(Tool::Rect, [10.0, 10.0], [50.0, 50.0])];
        apply(&mut img, &layers, 1.0, None);
        // The rect edge runs through x=10, y=30. Check green, not red: the ink
        // is pure red on white, so the red channel stays 255 either way.
        assert_ne!(img.get_pixel(10, 30).0[1], 255, "edge was not drawn");
        // Well inside the outline nothing should have changed.
        assert_eq!(img.get_pixel(30, 30).0, [255, 255, 255, 255]);
    }

    #[test]
    fn half_scale_puts_the_shape_at_half_the_coordinates() {
        let mut full = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let mut half = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let layers = vec![layer(Tool::Rect, [40.0, 40.0], [80.0, 80.0])];
        apply(&mut full, &layers, 1.0, None);
        apply(&mut half, &layers, 0.5, None);
        assert_ne!(full.get_pixel(40, 60).0[1], 255, "full-scale edge missing");
        assert_ne!(half.get_pixel(20, 30).0[1], 255, "half-scale edge missing");
        assert_eq!(
            half.get_pixel(40, 60).0,
            [255, 255, 255, 255],
            "the half-scale shape must not also be drawn at full coordinates"
        );
    }

    #[test]
    fn shapes_clipped_by_the_image_edge_do_not_panic() {
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        let layers = vec![
            layer(Tool::Arrow, [-100.0, -100.0], [200.0, 200.0]),
            layer(Tool::Blur, [-50.0, -50.0], [500.0, 500.0]),
            layer(Tool::Highlight, [30.0, 30.0], [900.0, 900.0]),
            layer(Tool::Ellipse, [-10.0, -10.0], [10.0, 10.0]),
        ];
        apply(&mut img, &layers, 1.0, None);
    }

    #[test]
    fn a_fully_offscreen_region_is_skipped() {
        let img = RgbaImage::new(40, 40);
        assert!(clamp_region(&img, [100.0, 100.0], [200.0, 200.0]).is_none());
    }

    #[test]
    fn degenerate_shapes_are_recognised() {
        assert!(layer(Tool::Rect, [10.0, 10.0], [11.0, 11.0]).is_degenerate());
        assert!(!layer(Tool::Rect, [10.0, 10.0], [60.0, 60.0]).is_degenerate());

        let mut t = layer(Tool::Text, [10.0, 10.0], [10.0, 10.0]);
        assert!(t.is_degenerate(), "empty text is nothing to draw");
        t.text = "hi".into();
        assert!(!t.is_degenerate());
    }

    #[test]
    fn hit_testing_uses_padded_bounds_so_thin_shapes_stay_grabbable() {
        let l = layer(Tool::Arrow, [50.0, 50.0], [50.0, 90.0]);
        assert!(l.hit(50.0, 70.0));
        assert!(l.hit(56.0, 70.0), "stroke padding should make this a hit");
        assert!(!l.hit(200.0, 70.0));
    }

    #[test]
    fn translate_moves_both_endpoints() {
        let mut l = layer(Tool::Rect, [10.0, 20.0], [30.0, 40.0]);
        l.translate(5.0, -5.0);
        assert_eq!(l.a, [15.0, 15.0]);
        assert_eq!(l.b, [35.0, 35.0]);
    }

    #[test]
    fn undo_and_redo_walk_the_stack_both_ways() {
        let mut history = History::default();
        let mut layers: Vec<Layer> = Vec::new();
        assert!(!history.can_undo());

        history.push(&layers);
        layers.push(layer(Tool::Rect, [0.0, 0.0], [10.0, 10.0]));
        history.push(&layers);
        layers.push(layer(Tool::Arrow, [0.0, 0.0], [10.0, 10.0]));
        assert_eq!(layers.len(), 2);

        history.undo(&mut layers);
        assert_eq!(layers.len(), 1);
        history.undo(&mut layers);
        assert_eq!(layers.len(), 0);
        assert!(!history.can_undo());

        history.redo(&mut layers);
        assert_eq!(layers.len(), 1);
        history.redo(&mut layers);
        assert_eq!(layers.len(), 2);
        assert!(!history.can_redo());
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut history = History::default();
        let mut layers = vec![layer(Tool::Rect, [0.0, 0.0], [10.0, 10.0])];
        history.push(&layers);
        layers.clear();
        history.undo(&mut layers);
        assert!(history.can_redo());

        history.push(&layers);
        assert!(!history.can_redo(), "redo must not survive a fresh edit");
    }

    #[test]
    fn the_undo_stack_is_bounded() {
        let mut history = History::default();
        let layers = vec![layer(Tool::Rect, [0.0, 0.0], [10.0, 10.0])];
        for _ in 0..(MAX_UNDO + 20) {
            history.push(&layers);
        }
        assert_eq!(history.past.len(), MAX_UNDO);
    }

    fn painted(bg: [u8; 4], ink: [u8; 4], alpha: u8) -> [u8; 4] {
        let mut img = RgbaImage::from_pixel(20, 20, Rgba(bg));
        let mut l = layer(Tool::Highlight, [2.0, 2.0], [18.0, 18.0]);
        l.color = [ink[0], ink[1], ink[2], alpha];
        apply(&mut img, &[l], 1.0, None);
        img.get_pixel(10, 10).0
    }

    const DARK: [u8; 4] = [18, 20, 24, 255];
    const LIGHT: [u8; 4] = [250, 250, 250, 255];
    const YELLOW: [u8; 4] = [255, 210, 0, 255];

    /// The top of the dial has to land on exactly the chosen colour — that end
    /// replaced a separate "cover" tool, and "almost opaque" would not do.
    #[test]
    fn full_strength_paints_the_flat_colour_over_anything() {
        for bg in [DARK, LIGHT, [7, 200, 90, 255]] {
            assert_eq!(
                painted(bg, YELLOW, 255),
                [255, 210, 0, 255],
                "background {bg:?} showed through at full strength"
            );
        }
    }

    /// The bottom of the dial has to leave the picture alone.
    #[test]
    fn the_lowest_strength_barely_touches_the_image() {
        let out = painted(DARK, YELLOW, 12);
        for (a, b) in out.iter().zip(DARK.iter()) {
            assert!(
                (*a as i32 - *b as i32).abs() < 24,
                "5% paint changed too much: {out:?} vs {DARK:?}"
            );
        }
    }

    /// Halfway down it is a highlighter, and the whole point of a highlighter
    /// is that you can still read what is under it.
    #[test]
    fn a_translucent_stroke_keeps_text_readable_underneath() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba(DARK));
        img.put_pixel(5, 5, Rgba([255, 255, 255, 255])); // a word
        let mut l = layer(Tool::Highlight, [0.0, 0.0], [10.0, 10.0]);
        l.color = [YELLOW[0], YELLOW[1], YELLOW[2], 128];
        apply(&mut img, &[l], 1.0, None);

        let text = luminance(img.get_pixel(5, 5).0);
        let back = luminance(img.get_pixel(1, 1).0);
        assert!(
            text > back + 0.25,
            "text lost against the highlight: {text:.2} vs {back:.2}"
        );
    }

    /// A plain multiply blend can only darken, which is why the old highlight
    /// vanished on dark screenshots. Mid-dial ink must brighten a dark
    /// background and still read as the ink's own hue.
    #[test]
    fn mid_strength_ink_shows_up_on_a_dark_background() {
        let out = painted(DARK, YELLOW, 128);
        assert!(
            luminance(out) > luminance(DARK) + 0.25,
            "ink did not lift the dark background: {out:?}"
        );
        assert!(
            out[0] > 120 && out[1] > 100 && out[2] < 90,
            "should read as yellow, got {out:?}"
        );
    }

    /// ...while on a light background it tints instead of blowing out, which is
    /// what multiply is right for.
    #[test]
    fn mid_strength_ink_tints_a_light_background() {
        let out = painted(LIGHT, YELLOW, 128);
        assert!(out[2] < 160, "blue must drop or it is not yellow: {out:?}");
        assert!(out[0] > 200, "red should stay high: {out:?}");
    }

    #[test]
    fn paint_is_one_tool_now_and_cover_is_no_longer_offered() {
        assert!(Tool::DRAWABLE.contains(&Tool::Highlight));
        assert!(
            !Tool::DRAWABLE.contains(&Tool::Fill),
            "Fill stays internal for redaction; paint at 100% replaces it"
        );
        // Redaction still needs the unconditional version.
        let out = {
            let mut img = RgbaImage::from_pixel(20, 20, Rgba(LIGHT));
            let mut l = layer(Tool::Fill, [2.0, 2.0], [18.0, 18.0]);
            l.color = [0, 0, 0, 255];
            apply(&mut img, &[l], 1.0, None);
            img.get_pixel(10, 10).0
        };
        assert_eq!(out, [0, 0, 0, 255], "redaction must still cover outright");
    }

    #[test]
    fn only_line_tools_offer_a_stroke_width() {
        for t in [Tool::Arrow, Tool::Rect, Tool::Ellipse] {
            assert!(t.uses_stroke(), "{t:?} draws lines");
        }
        for t in [Tool::Text, Tool::Blur, Tool::Highlight, Tool::Fill] {
            assert!(!t.uses_stroke(), "{t:?} has no line to thicken");
        }
    }
}
