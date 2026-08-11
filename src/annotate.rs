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
    /// A stroked line, with or without a head. This is where the arrow that
    /// used to be drawn from three capsules went when [`Tool::Arrow`] became a
    /// solid silhouette.
    Line,
    Text,
    Blur,
    Highlight,
    /// Flat, fully opaque rectangle. Not offered as a tool any more — the
    /// paint tool reaches this at the top of its range — but redaction still
    /// builds these from OCR hits, where "covered" is not negotiable.
    Fill,
}

impl Tool {
    pub const DRAWABLE: [Tool; 7] = [
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Line,
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
            Tool::Line => t("Line"),
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
        // Not the arrow: its thickness comes from its own proportions.
        matches!(self, Tool::Line | Tool::Rect | Tool::Ellipse)
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
    /// Whether a shape is a filled area rather than an outline.
    pub filled: bool,
    /// Corner radius, in original screenshot pixels. 0 is a sharp corner.
    pub corner: f32,
    pub head: Head,
    pub arrow: ArrowForm,
    pub cover: Cover,
    pub underline: bool,
    pub align: TextAlign,
    /// White rim around the shape, in original screenshot pixels. 0 is none.
    ///
    /// The reason it exists is legibility, not decoration: a red arrow over a
    /// red part of the picture disappears without it. Off by default for
    /// everything except the arrow, which is the tool most often dropped on top
    /// of whatever happens to be there.
    pub border: f32,
    pub border_color: Rgba8,
    /// How far the shadow feathers out, in original screenshot pixels.
    pub shadow: f32,
}

/// Which of the three arrows this is.
///
/// The forms are fixed and the proportions are locked: a drag sets where the
/// arrow points and how big it is, never how fat it is. That is what makes it
/// read as one mark from a marker pen rather than a line with a decoration on
/// the end, and it is why the tool has no stroke width.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum ArrowForm {
    #[default]
    Straight,
    /// Bending to the left of the way it points.
    BendLeft,
    BendRight,
}

impl ArrowForm {
    pub const ALL: [ArrowForm; 3] = [
        ArrowForm::Straight,
        ArrowForm::BendLeft,
        ArrowForm::BendRight,
    ];

    /// How far the spine's control point sits off the straight line between
    /// tail and tip, as a fraction of the arrow's length.
    fn bend(self) -> f32 {
        match self {
            ArrowForm::Straight => 0.0,
            ArrowForm::BendLeft => -0.30,
            ArrowForm::BendRight => 0.30,
        }
    }
}

/// What sits at the far end of an arrow.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum Head {
    /// A filled triangle, wider than the shaft. The mark a marker pen makes.
    #[default]
    Solid,
    /// Two barbs swept back from the tip, the width of the shaft.
    Open,
    /// A dashed shaft under a solid head, for a pointer that must not be
    /// mistaken for something in the picture.
    Dashed,
}

/// How a redaction hides what is under it.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum Cover {
    #[default]
    Blur,
    /// Block averaging. Worth offering beside blur because a gaussian is a
    /// convolution and can be partly undone by deconvolution, where averaging
    /// throws the detail away outright.
    Pixelate,
}

/// Which way a label grows from the point it was placed at.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
    Right,
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
            filled: false,
            corner: 0.0,
            head: Head::default(),
            arrow: ArrowForm::default(),
            cover: Cover::default(),
            underline: false,
            align: TextAlign::default(),
            border: 0.0,
            border_color: [255, 255, 255, 255],
            shadow: 0.0,
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
        // The arrow is not here because its direction *is* its geometry: the
        // drag from tail to tip already says which way it points, and a rotate
        // knob would be a second control for the same thing.
        !matches!(kind, Tool::Blur | Tool::Fill | Tool::Select | Tool::Arrow)
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
        let ink = Ink {
            color: layer.color,
            stroke,
            turn,
            border: layer.border * scale,
            border_color: layer.border_color,
            shadow: layer.shadow * scale,
            filled: layer.filled,
            corner: layer.corner * scale,
        };
        match layer.kind {
            Tool::Arrow => {
                // Built in shot pixels and scaled here, so the outline is the
                // same shape at preview size and at export size — the arrow's
                // proportions are the whole point of it.
                let pts: Vec<[f32; 2]> = arrow_points(layer)
                    .into_iter()
                    .map(|p| [p[0] * scale, p[1] * scale])
                    .collect();
                draw_arrow(img, &pts, &ink);
            }
            Tool::Rect => draw_rect(img, a, b, &ink),
            Tool::Ellipse => draw_ellipse(img, a, b, &ink),
            Tool::Line => draw_line(img, a, b, &ink, layer.head, scale),
            Tool::Highlight => draw_paint(img, a, b, layer.color, turn),
            Tool::Fill => draw_fill(img, a, b, layer.color),
            Tool::Blur => {
                let amount = (layer.blur * scale).max(1.0);
                match layer.cover {
                    Cover::Blur => draw_blur(img, a, b, amount),
                    Cover::Pixelate => draw_pixelate(img, a, b, amount),
                }
            }
            Tool::Text => {
                if let Some(font) = font {
                    let label = Label {
                        size: (layer.font_size * scale).max(6.0),
                        at: a,
                        color: Rgba(layer.color),
                        angle: layer.angle,
                        underline: layer.underline,
                        align: layer.align,
                        border: layer.border * scale,
                        border_color: layer.border_color,
                    };
                    draw_text(img, font, &label, &layer.text);
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
/// Signed distance to a rectangle, rounded by `r`. Negative inside.
///
/// The rounding is free: shrink the box by `r` and subtract `r` from the
/// distance, which is the same trick that gives the *sharp* box its rounded
/// outer corners — `outside` is a Euclidean distance to the corner point, so
/// the field around a corner is already circular.
fn sd_round_box(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    outside + inside - r
}

/// Approximate signed distance to an ellipse. Exact ellipse SDF needs an
/// iterative solve; scaling the space to a circle is close enough at the stroke
/// widths we draw and costs a fraction as much.
fn sd_ellipse(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let (rx, ry) = (rx.max(0.5), ry.max(0.5));
    let nx = (px - cx) / rx;
    let ny = (py - cy) / ry;
    let len = (nx * nx + ny * ny).sqrt();
    (len - 1.0) * rx.min(ry)
}

/// Rasterise a shape over its bounding box, `sdf` returning distance to the
/// figure and `cov` converting that to coverage.
/// Everything a drawn shape needs beyond its two corners, already scaled into
/// the pixels being drawn into.
///
/// Bundled because these travel together through every shape drawer, and
/// passing them one by one put four of those functions over the argument limit
/// as the options row grew.
struct Ink {
    color: Rgba8,
    stroke: f32,
    turn: Turn,
    border: f32,
    border_color: Rgba8,
    shadow: f32,
    /// Whether the shape is an area rather than an outline. A filled shape
    /// keeps its stroke: the fill is the interior *plus* the outline, which is
    /// what the distance field gives for free by not taking the absolute value.
    filled: bool,
    corner: f32,
}

impl Ink {
    /// The distance a filled shape is rasterised from, given the signed
    /// distance to its edge.
    ///
    /// Outline: distance to the *boundary*, so the band is centred on it.
    /// Filled: the signed distance itself, so everything inside is covered and
    /// the same band still hangs outside.
    fn shape_of(&self, signed: f32) -> f32 {
        if self.filled { signed } else { signed.abs() }
    }
}

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
    ink: &Ink,
    sdf: impl Fn(f32, f32) -> f32,
    half_stroke: f32,
) {
    let Turn { angle, centre } = ink.turn;
    // The rim and the shadow both reach outside the shape's own box.
    let grow = ink.border + ink.shadow * (1.0 + SHADOW_DROP) + 2.0;
    let bounds = [
        bounds[0] - grow,
        bounds[1] - grow,
        bounds[2] + grow,
        bounds[3] + grow,
    ];
    if angle.abs() < 1e-4 {
        rasterise(img, bounds, ink, sdf, half_stroke);
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
        ink,
        |px, py| {
            let (dx, dy) = (px - cx, py - cy);
            sdf(cx + dx * ucos - dy * usin, cy + dx * usin + dy * ucos)
        },
        half_stroke,
    );
}

/// How far the shadow falls below the shape, as a fraction of its reach.
const SHADOW_DROP: f32 = 0.35;
/// How dark the shadow is at its darkest.
const SHADOW_ALPHA: f32 = 0.38;

/// Rasterise a shape, its rim and its shadow from one distance field.
///
/// Three passes back to front, all off the same `sdf`, which is what keeps them
/// exactly concentric — a rim built by drawing the shape twice at two widths
/// would drift at the corners, where the two outlines are not parallel.
///
/// `sdf` gives distance to the *figure*; subtracting `half_stroke` turns that
/// into a signed distance to the ink's own edge, negative inside.
fn rasterise(
    img: &mut RgbaImage,
    bounds: [f32; 4],
    ink: &Ink,
    sdf: impl Fn(f32, f32) -> f32,
    half_stroke: f32,
) {
    let x0 = bounds[0].floor().max(0.0) as u32;
    let y0 = bounds[1].floor().max(0.0) as u32;
    let x1 = (bounds[2].ceil() as i64).clamp(0, img.width() as i64) as u32;
    let y1 = (bounds[3].ceil() as i64).clamp(0, img.height() as i64) as u32;
    let solid = |px: f32, py: f32| sdf(px, py) - half_stroke;

    if ink.shadow > 0.5 {
        let drop = ink.shadow * SHADOW_DROP;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = solid(x as f32 + 0.5, y as f32 + 0.5 - drop) - ink.border;
                let cov = (1.0 - d / ink.shadow).clamp(0.0, 1.0) * SHADOW_ALPHA;
                if cov > 0.0 {
                    blend(img, x, y, Rgba([0, 0, 0, 255]), cov);
                }
            }
        }
    }
    if ink.border > 0.5 {
        for y in y0..y1 {
            for x in x0..x1 {
                let d = solid(x as f32 + 0.5, y as f32 + 0.5) - ink.border;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    blend(img, x, y, Rgba(ink.border_color), cov);
                }
            }
        }
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let cov = (0.5 - solid(x as f32 + 0.5, y as f32 + 0.5)).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(img, x, y, Rgba(ink.color), cov);
            }
        }
    }
}

// ------------------------------------------------------------------- drawing

/// The arrow's silhouette in its own frame: tail at the origin, tip at (1, 0),
/// everything else scaled to that.
///
/// Generated rather than digitised from an SVG. Three fixed forms need three
/// outlines, and one spine with a signed bend gives all three from the same
/// twenty lines — a hand-listed set of points would be the same shapes with a
/// transcription error waiting in them, and the straight arrow is just the one
/// with no bend.
fn arrow_outline(form: ArrowForm) -> Vec<[f32; 2]> {
    /// Half the shaft's width.
    const SHAFT: f32 = 0.055;
    /// Half the head's width, at its base.
    const HEAD_HALF: f32 = 0.17;
    /// How much of the arrow's length the head takes.
    const HEAD_LEN: f32 = 0.30;
    const STEPS: usize = 40;

    let bend = form.bend();
    // A quadratic Bézier from tail to tip, pulled aside at the middle.
    let spine = |t: f32| -> [f32; 2] {
        let u = 1.0 - t;
        [
            2.0 * u * t * 0.5 + t * t,
            2.0 * u * t * bend,
        ]
    };
    let pts: Vec<[f32; 2]> = (0..=STEPS)
        .map(|i| spine(i as f32 / STEPS as f32))
        .collect();

    // Walk the spine from the tip back until the head has had its length, so
    // the head is the same size however far the spine bends.
    let mut run = 0.0;
    let mut neck = 0;
    for i in (1..pts.len()).rev() {
        let (dx, dy) = (pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
        run += (dx * dx + dy * dy).sqrt();
        if run >= HEAD_LEN {
            neck = i - 1;
            break;
        }
    }

    let normal = |i: usize| -> [f32; 2] {
        let (from, to) = (pts[i.saturating_sub(1)], pts[(i + 1).min(STEPS)]);
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        [-dy / len, dx / len]
    };

    let mut out = Vec::with_capacity(neck * 2 + 5);
    // Up one side of the shaft…
    for (i, p) in pts.iter().enumerate().take(neck + 1) {
        let n = normal(i);
        out.push([p[0] + n[0] * SHAFT, p[1] + n[1] * SHAFT]);
    }
    // …round the head…
    let n = normal(neck);
    out.push([
        pts[neck][0] + n[0] * HEAD_HALF,
        pts[neck][1] + n[1] * HEAD_HALF,
    ]);
    out.push(pts[STEPS]);
    out.push([
        pts[neck][0] - n[0] * HEAD_HALF,
        pts[neck][1] - n[1] * HEAD_HALF,
    ]);
    // …and back down the other side, so the two walks pair up point for point.
    for (i, p) in pts.iter().enumerate().take(neck + 1).rev() {
        let n = normal(i);
        out.push([p[0] - n[0] * SHAFT, p[1] - n[1] * SHAFT]);
    }
    out
}

/// The arrow's outline placed in the picture: `a` is the tail, `b` the tip.
///
/// One uniform scale, so the arrow keeps its proportions however it is dragged
/// — longer means bigger, never thinner.
pub fn arrow_points(layer: &Layer) -> Vec<[f32; 2]> {
    let (a, b) = (layer.a, layer.b);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return Vec::new();
    }
    let (ux, uy) = (dx / len, dy / len);
    arrow_outline(layer.arrow)
        .into_iter()
        .map(|[u, v]| {
            [
                a[0] + (u * ux - v * uy) * len,
                a[1] + (u * uy + v * ux) * len,
            ]
        })
        .collect()
}

/// Signed distance to a closed polygon. Negative inside.
///
/// Crossing count for the sign, nearest edge for the magnitude. The arrow is
/// not convex — the notch where the head meets the shaft — so none of the
/// cheaper convex tests apply.
fn sd_polygon(px: f32, py: f32, pts: &[[f32; 2]]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return f32::MAX;
    }
    let mut d = f32::MAX;
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (pi, pj) = (pts[i], pts[j]);
        d = d.min(sd_segment(px, py, pi, pj));
        if (pi[1] > py) != (pj[1] > py) {
            let x = (pj[0] - pi[0]) * (py - pi[1]) / (pj[1] - pi[1]) + pi[0];
            if px < x {
                inside = !inside;
            }
        }
        j = i;
    }
    if inside { -d } else { d }
}

fn draw_arrow(img: &mut RgbaImage, pts: &[[f32; 2]], ink: &Ink) {
    if pts.len() < 3 {
        return;
    }
    let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in pts {
        bounds[0] = bounds[0].min(p[0]);
        bounds[1] = bounds[1].min(p[1]);
        bounds[2] = bounds[2].max(p[0]);
        bounds[3] = bounds[3].max(p[1]);
    }
    rasterise_turned(img, bounds, ink, |px, py| sd_polygon(px, py, pts), 0.0);
}

/// A line, and whatever head it carries.
///
/// This is the arrow shotr drew before [`Tool::Arrow`] became a silhouette: a
/// stroke you can set the width of, with an open, solid or dashed treatment.
/// The two are different marks — this one is a technical pointer, that one is
/// a marker pen — and both are worth having.
fn draw_line(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], ink: &Ink, head: Head, scale: f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let stroke = ink.stroke;
    // The floor is 10 *shot* pixels, so it has to be scaled with everything
    // else. A bare 10 here is 10 preview pixels in the preview and 10 export
    // pixels in the export, which gave the same line two different heads.
    let reach = (stroke * 4.0).max(10.0 * scale).min(len);
    let along = dy.atan2(dx);

    let corner = |sign: f32| -> [f32; 2] {
        let t = along + std::f32::consts::PI + sign * 0.49;
        [b[0] + reach * t.cos(), b[1] + reach * t.sin()]
    };
    let (l, r) = (corner(-1.0), corner(1.0));

    let pad = stroke * 2.0;
    let x0 = a[0].min(b[0]).min(l[0]).min(r[0]) - pad;
    let y0 = a[1].min(b[1]).min(l[1]).min(r[1]) - pad;
    let x1 = a[0].max(b[0]).max(l[0]).max(r[0]) + pad;
    let y1 = a[1].max(b[1]).max(l[1]).max(r[1]) + pad;

    // A solid head is wider than the shaft, so the shaft stops at the middle of
    // the head's back edge; otherwise its round cap pokes past the point.
    let neck = [(l[0] + r[0]) / 2.0, (l[1] + r[1]) / 2.0];
    let shaft: Vec<([f32; 2], [f32; 2])> = match head {
        Head::Open => vec![(a, b)],
        Head::Solid => vec![(a, neck)],
        Head::Dashed => dashes(a, neck, stroke * 2.0),
    };

    rasterise_turned(
        img,
        [x0, y0, x1, y1],
        ink,
        |px, py| {
            let mut d = f32::MAX;
            for (from, to) in &shaft {
                d = d.min(sd_segment(px, py, *from, *to));
            }
            match head {
                // The barbs are strokes like the shaft, so they join it with
                // the same round cap and the whole mark is one silhouette.
                Head::Open => d
                    .min(sd_segment(px, py, b, l))
                    .min(sd_segment(px, py, b, r)),
                // The triangle is an area, so its distance is already inside
                // the band `rasterise` covers — no half-stroke to subtract.
                Head::Solid | Head::Dashed => d.min(sd_triangle(px, py, b, l, r) + stroke / 2.0),
            }
        },
        stroke / 2.0,
    );
}

/// A dashed line as a list of segments, `gap` long and `gap` apart./// A dashed line as a list of segments, `gap` long and `gap` apart.
fn dashes(a: [f32; 2], b: [f32; 2], gap: f32) -> Vec<([f32; 2], [f32; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let gap = gap.max(1.0);
    if len < gap * 2.0 {
        return vec![(a, b)];
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < len {
        let to = (at + gap).min(len);
        out.push((
            [a[0] + ux * at, a[1] + uy * at],
            [a[0] + ux * to, a[1] + uy * to],
        ));
        at = to + gap;
    }
    out
}

/// Signed distance to a triangle. Negative inside.
fn sd_triangle(px: f32, py: f32, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> f32 {
    let edge = |from: [f32; 2], to: [f32; 2]| sd_segment(px, py, from, to);
    let d = edge(p0, p1).min(edge(p1, p2)).min(edge(p2, p0));
    // Winding: the point is inside when it is on the same side of all three
    // edges. Comparing signs of the cross products says which.
    let side = |from: [f32; 2], to: [f32; 2]| {
        (to[0] - from[0]) * (py - from[1]) - (to[1] - from[1]) * (px - from[0])
    };
    let (s0, s1, s2) = (side(p0, p1), side(p1, p2), side(p2, p0));
    let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
    if inside { -d } else { d }
}

fn draw_rect(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], ink: &Ink) {
    let (x0, x1) = min_max(a[0], b[0]);
    let (y0, y1) = min_max(a[1], b[1]);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hw, hh) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let r = ink.corner.clamp(0.0, hw.min(hh));
    let pad = ink.stroke + 2.0;
    rasterise_turned(
        img,
        [x0 - pad, y0 - pad, x1 + pad, y1 + pad],
        ink,
        |px, py| ink.shape_of(sd_round_box(px, py, cx, cy, hw, hh, r)),
        ink.stroke / 2.0,
    );
}

fn draw_ellipse(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], ink: &Ink) {
    let (x0, x1) = min_max(a[0], b[0]);
    let (y0, y1) = min_max(a[1], b[1]);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let pad = ink.stroke + 2.0;
    rasterise_turned(
        img,
        [x0 - pad, y0 - pad, x1 + pad, y1 + pad],
        ink,
        |px, py| ink.shape_of(sd_ellipse(px, py, cx, cy, rx, ry)),
        ink.stroke / 2.0,
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
/// A line of text as the renderer needs it, already scaled.
struct Label {
    size: f32,
    /// The point the label was placed at. Which *part* of the line lands here
    /// is what [`Label::align`] decides.
    at: [f32; 2],
    color: Rgba<u8>,
    angle: f32,
    underline: bool,
    align: TextAlign,
    border: f32,
    border_color: Rgba8,
}

impl Label {
    /// How far left of the anchor the line starts, as a fraction of its width.
    fn shift(&self) -> f32 {
        match self.align {
            TextAlign::Left => 0.0,
            TextAlign::Centre => 0.5,
            TextAlign::Right => 1.0,
        }
    }
}

fn draw_text(img: &mut RgbaImage, font: &FontArc, label: &Label, body: &str) {
    let size = label.size;
    let w = text::measure(font, size, body);
    if w < 1.0 {
        return;
    }
    let dx = w * label.shift();

    if label.angle.abs() < 1e-4 {
        stamp_line(img, font, label, body, label.at[0] - dx, label.at[1], w);
        return;
    }

    // `measure` answers width only; a line is about 1.35 em tall with its
    // descenders, and the scratch image is padded generously anyway.
    let h = size * 1.35;
    // A margin, so the turned corners are not shaved off the scratch image.
    // The rim reaches outside the glyphs, so it has to be in the margin too.
    let pad = size + label.border;
    let mut stamp = RgbaImage::from_pixel(
        (w + pad * 2.0).ceil() as u32,
        (h + pad * 2.0).ceil() as u32,
        Rgba([0, 0, 0, 0]),
    );
    stamp_line(&mut stamp, font, label, body, pad, pad, w);
    let turned = crate::render::watermark::rotate(&stamp, label.angle);

    // The label turns about its anchor, but `rotate` turns the scratch image
    // about the *image's* centre. So follow where the anchor ended up and shift
    // the result until it lands back where the label belongs.
    //
    // Getting this wrong is not subtle but it is silent: the glyphs tilt
    // correctly and simply sit somewhere else, which shows up as the selection
    // frame no longer agreeing with the text it is drawn around.
    let (sin, cos) = label.angle.sin_cos();
    let (sw, sh) = (stamp.width() as f32, stamp.height() as f32);
    let (tw, th) = (turned.width() as f32, turned.height() as f32);
    // The anchor, as an offset from the scratch image's centre. Alignment moves
    // it along the line rather than moving the line, so a turned label still
    // pivots about the point it was placed at.
    let (vx, vy) = (pad + dx - sw / 2.0, pad - sh / 2.0);
    let ox = label.at[0] - (tw / 2.0 + vx * cos - vy * sin);
    let oy = label.at[1] - (th / 2.0 + vx * sin + vy * cos);
    image::imageops::overlay(img, &turned, ox.round() as i64, oy.round() as i64);
}

/// One line of text with whatever rim and rule it carries.
///
/// The rim is eight offset copies rather than a distance field, because a
/// glyph here is blitted coverage and not an SDF — there is no edge to offset.
/// Eight directions is what stops the corners of a letter thinning out; four
/// leaves visible notches on a diagonal stroke like the leg of an `R`.
fn stamp_line(
    img: &mut RgbaImage,
    font: &FontArc,
    label: &Label,
    body: &str,
    x: f32,
    y: f32,
    w: f32,
) {
    let size = label.size;
    if label.border > 0.5 {
        let r = label.border;
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        for (ox, oy) in [
            (-r, 0.0),
            (r, 0.0),
            (0.0, -r),
            (0.0, r),
            (-d, -d),
            (d, -d),
            (-d, d),
            (d, d),
        ] {
            text::draw(img, font, size, x + ox, y + oy, Rgba(label.border_color), body);
            if label.underline {
                text::underline(
                    img,
                    font,
                    size,
                    x + ox,
                    y + oy,
                    w,
                    Rgba(label.border_color),
                );
            }
        }
    }
    text::draw(img, font, size, x, y, label.color, body);
    if label.underline {
        text::underline(img, font, size, x, y, w, label.color);
    }
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

/// Block averaging, the other way of hiding a region.
///
/// Worth having beside blur rather than instead of it: a gaussian is a
/// convolution and can be partly undone by deconvolution or by upscaling,
/// where averaging discards the detail outright. The two share the Amount
/// dial, which reads as a radius for one and a block size for the other — at
/// the same number they hide about as much as each other.
fn draw_pixelate(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], block: f32) {
    let Some((x0, y0, x1, y1)) = clamp_region(img, a, b) else {
        return;
    };
    let block = (block.round() as u32).max(2);
    let mut by = y0;
    while by < y1 {
        let ey = (by + block).min(y1);
        let mut bx = x0;
        while bx < x1 {
            let ex = (bx + block).min(x1);
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in by..ey {
                for x in bx..ex {
                    for (s, c) in sum.iter_mut().zip(img.get_pixel(x, y).0) {
                        *s += c as u64;
                    }
                    n += 1;
                }
            }
            if n > 0 {
                let mean = sum.map(|s| (s / n) as u8);
                for y in by..ey {
                    for x in bx..ex {
                        img.put_pixel(x, y, Rgba(mean));
                    }
                }
            }
            bx = ex;
        }
        by = ey;
    }
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

    /// "Copy the shot as captured" hands over the capture's own pixels with the
    /// redaction boxes baked straight into them. That only works because baking
    /// leaves the image exactly as it was everywhere the box does not reach —
    /// unlike `render`, which deliberately lays the shot out on a larger canvas.
    #[test]
    fn baking_a_redaction_box_leaves_the_rest_of_the_capture_alone() {
        let before = RgbaImage::from_pixel(200, 200, Rgba([9, 200, 9, 255]));
        let mut shot = before.clone();
        let mut cover = Layer::new(Tool::Fill, [40.0, 40.0], [0, 0, 0, 255], 1.0, 12.0, 1.0);
        cover.b = [100.0, 80.0];
        apply(&mut shot, &[cover], 1.0, None);

        assert_eq!(
            shot.dimensions(),
            before.dimensions(),
            "the copy has to be the screenshot, not a canvas it was laid out on"
        );
        assert_ne!(
            shot.get_pixel(70, 60),
            before.get_pixel(70, 60),
            "the redaction box covered nothing, so the copy leaks what it hides"
        );
        assert_eq!(
            shot.get_pixel(180, 180),
            before.get_pixel(180, 180),
            "baking a box repainted the rest of the shot"
        );
    }

    /// Turning a shape has to move ink, and it has to move it *about the
    /// centre* — a rotation that also slid the shape sideways would be a
    /// translation wearing a disguise.
    #[test]
    fn turning_a_shape_moves_its_ink_but_not_its_centre() {
        for kind in [Tool::Rect, Tool::Ellipse, Tool::Line] {
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
        for kind in [Tool::Line, Tool::Rect, Tool::Ellipse, Tool::Text, Tool::Highlight] {
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
    fn the_line_head_is_the_same_size_in_the_preview_and_the_export() {
        let head_spread = |scale: f32, side: u32| {
            let mut img = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
            let mut l = Layer::new(Tool::Line, [10.0, 100.0], [255, 0, 0, 255], 1.0, 20.0, 8.0);
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

    /// A filled shape covers its middle; an outline leaves it alone. The two
    /// come from one distance field and differ only by an absolute value, so
    /// this is the assertion that says the sign is being used.
    #[test]
    fn filling_a_shape_covers_its_middle_and_an_outline_does_not() {
        for kind in [Tool::Rect, Tool::Ellipse] {
            let middle = |filled: bool| {
                let mut img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
                let mut l = layer(kind, [20.0, 20.0], [80.0, 80.0]);
                l.filled = filled;
                apply(&mut img, &[l], 1.0, None);
                img.get_pixel(50, 50).0
            };
            assert_eq!(
                middle(false),
                [0, 0, 0, 255],
                "{kind:?} painted its middle without being asked to fill"
            );
            assert_ne!(
                middle(true),
                [0, 0, 0, 255],
                "{kind:?} was filled and its middle stayed empty"
            );
        }
    }

    /// A corner radius has to cut the corner off. Sampling the very corner of
    /// the bounding box says whether it did.
    #[test]
    fn a_corner_radius_clears_the_corner_of_the_box() {
        let corner_ink = |r: f32| {
            let mut img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
            let mut l = layer(Tool::Rect, [20.0, 20.0], [80.0, 80.0]);
            l.stroke = 3.0;
            l.corner = r;
            apply(&mut img, &[l], 1.0, None);
            img.get_pixel(21, 21).0 != [0, 0, 0, 255]
        };
        assert!(corner_ink(0.0), "a sharp rectangle missed its own corner");
        assert!(
            !corner_ink(20.0),
            "a 20px radius still painted the square corner"
        );
    }

    /// The head is filled by giving the triangle a *signed* field, so this is
    /// the assertion that says the sign is right. A field that came back
    /// positive inside would draw the head as an outline.
    #[test]
    fn the_arrowheads_triangle_is_negative_inside_and_positive_outside() {
        let (p0, p1, p2) = ([10.0, 0.0], [0.0, -5.0], [0.0, 5.0]);
        assert!(
            sd_triangle(3.0, 0.0, p0, p1, p2) < 0.0,
            "a point inside the head reads as outside it, so the head is hollow"
        );
        assert!(sd_triangle(-3.0, 0.0, p0, p1, p2) > 0.0);
        assert!(sd_triangle(0.0, 0.0, p0, p1, p2).abs() < 1e-4, "on the edge");
    }

    /// Three heads have to be three different marks. A style that silently drew
    /// the same arrow would look like the buttons doing nothing.
    #[test]
    fn each_line_head_draws_something_different() {
        let render = |head: Head| {
            let mut img = RgbaImage::from_pixel(140, 60, Rgba([0, 0, 0, 0]));
            let mut l = layer(Tool::Line, [10.0, 30.0], [130.0, 30.0]);
            l.stroke = 4.0;
            l.head = head;
            apply(&mut img, &[l], 1.0, None);
            img
        };
        let lit = |img: &RgbaImage| ink(img);
        let (solid, open, dashed) = (
            render(Head::Solid),
            render(Head::Open),
            render(Head::Dashed),
        );
        assert!(!lit(&solid).is_empty(), "the solid head drew nothing");
        assert_ne!(lit(&solid), lit(&open), "solid and open are the same mark");
        assert_ne!(lit(&dashed), lit(&solid), "dashed and solid are the same mark");

        // A dashed shaft is gaps: walking the axis has to leave and re-enter
        // the ink at least twice.
        let runs = |img: &RgbaImage| {
            let mut runs = 0;
            let mut inside = false;
            for x in 10..100 {
                let on = img.get_pixel(x, 30).0[3] > 40;
                if on && !inside {
                    runs += 1;
                }
                inside = on;
            }
            runs
        };
        assert_eq!(runs(&solid), 1, "the solid arrow's shaft has a hole in it");
        assert!(
            runs(&dashed) >= 3,
            "the dashed shaft came out as {} unbroken run(s)",
            runs(&dashed)
        );
    }

    /// Pixelate must actually flatten detail, not merely tint it: the point of
    /// offering it beside blur is that what it removes cannot be recovered.
    #[test]
    fn pixelate_replaces_a_region_with_flat_blocks() {
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        for y in 0..40 {
            for x in 0..40 {
                // A fine checkerboard: nothing survives averaging.
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        let mut l = layer(Tool::Blur, [8.0, 8.0], [32.0, 32.0]);
        l.cover = Cover::Pixelate;
        l.blur = 8.0;
        apply(&mut img, &[l], 1.0, None);

        let a = img.get_pixel(12, 12).0;
        let b = img.get_pixel(13, 13).0;
        assert_eq!(a, b, "neighbouring pixels inside one block still differ");
        assert!(
            a[0] > 100 && a[0] < 160,
            "the block is {a:?}, not the average of a checkerboard"
        );
        assert_eq!(
            img.get_pixel(2, 2).0,
            [255, 255, 255, 255],
            "pixelate reached outside the region it was given"
        );
    }

    /// Alignment moves the line, not the anchor: a right-aligned label ends
    /// where a left-aligned one begins.
    #[test]
    fn alignment_moves_the_line_around_its_anchor() {
        let Some((_, font)) = crate::render::text::load_system_font() else {
            return;
        };
        let spread = |align: TextAlign| {
            let mut img = RgbaImage::from_pixel(300, 80, Rgba([0, 0, 0, 0]));
            let mut l = Layer::new(Tool::Text, [150.0, 20.0], [255, 0, 0, 255], 2.0, 24.0, 8.0);
            l.text = "anchored".to_owned();
            l.align = align;
            apply(&mut img, &[l], 1.0, Some(&font));
            let lit = ink(&img);
            (
                lit.iter().map(|p| p.0).min().unwrap_or(0),
                lit.iter().map(|p| p.0).max().unwrap_or(0),
            )
        };
        let (left0, _) = spread(TextAlign::Left);
        let (_, right1) = spread(TextAlign::Right);
        let (c0, c1) = spread(TextAlign::Centre);
        assert!(left0 >= 148, "a left-aligned label starts left of its anchor");
        assert!(right1 <= 152, "a right-aligned label ends right of its anchor");
        assert!(
            c0 < 150 && c1 > 150,
            "a centred label sits {c0}..{c1}, not astride its anchor at 150"
        );
    }

    /// The rule has to land under the glyphs, not through them.
    #[test]
    fn an_underline_adds_ink_below_the_text() {
        let Some((_, font)) = crate::render::text::load_system_font() else {
            return;
        };
        let lowest = |underline: bool| {
            let mut img = RgbaImage::from_pixel(200, 80, Rgba([0, 0, 0, 0]));
            let mut l = Layer::new(Tool::Text, [10.0, 10.0], [255, 0, 0, 255], 2.0, 24.0, 8.0);
            l.text = "ruled".to_owned();
            l.underline = underline;
            apply(&mut img, &[l], 1.0, Some(&font));
            ink(&img).iter().map(|p| p.1).max().unwrap_or(0)
        };
        assert!(
            lowest(true) > lowest(false),
            "the underline drew no lower than the text itself"
        );
    }

    /// The whole point of the new arrow: its proportions are locked, so a
    /// longer drag makes a *bigger* arrow, not a thinner one.
    #[test]
    fn the_arrow_keeps_its_proportions_however_far_it_is_dragged() {
        let ratio = |len: f32| {
            let mut l = layer(Tool::Arrow, [0.0, 0.0], [len, 0.0]);
            l.arrow = ArrowForm::Straight;
            let pts = arrow_points(&l);
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for p in &pts {
                lo = lo.min(p[1]);
                hi = hi.max(p[1]);
            }
            (hi - lo) / len
        };
        let short = ratio(60.0);
        let long = ratio(600.0);
        assert!(
            (short - long).abs() < 1e-3,
            "a 60px arrow is {short:.3} as wide as it is long and a 600px one \
             is {long:.3} — the shape is being stretched, not scaled"
        );
        assert!(short > 0.2 && short < 0.5, "the head is a strange width: {short}");
    }

    /// The three forms have to be three different silhouettes, and the bent
    /// ones have to leave the straight line between tail and tip.
    #[test]
    fn the_two_bends_go_opposite_ways_round_a_straight_arrow() {
        let spine_at_middle = |form: ArrowForm| {
            let mut l = layer(Tool::Arrow, [0.0, 100.0], [200.0, 100.0]);
            l.arrow = form;
            let pts = arrow_points(&l);
            // The outline is two walks of the same spine, so point `i` on one
            // wall pairs with `n-1-i` on the other and their midpoint is the
            // spine itself. Reading one wall alone would just measure the
            // shaft's own width.
            let n = pts.len();
            let i = 10;
            (pts[i][1] + pts[n - 1 - i][1]) / 2.0
        };
        let straight = spine_at_middle(ArrowForm::Straight);
        let left = spine_at_middle(ArrowForm::BendLeft);
        let right = spine_at_middle(ArrowForm::BendRight);
        assert!((straight - 100.0).abs() < 8.0, "the straight arrow is bent");
        assert!(
            left < straight - 10.0 && right > straight + 10.0,
            "the bends sit at {left:.0} and {right:.0} against {straight:.0} — \
             they are not going opposite ways"
        );
    }

    /// A concave polygon needs the crossing count to get its sign right; the
    /// arrow's notch, where the head meets the shaft, is where a convex test
    /// would quietly say "inside".
    #[test]
    fn the_arrow_is_solid_along_its_spine_and_hollow_beside_the_head() {
        let mut l = layer(Tool::Arrow, [0.0, 100.0], [200.0, 100.0]);
        l.arrow = ArrowForm::Straight;
        let pts = arrow_points(&l);
        assert!(
            sd_polygon(60.0, 100.0, &pts) < 0.0,
            "the middle of the shaft reads as outside the arrow"
        );
        assert!(
            sd_polygon(60.0, 100.0 - 25.0, &pts) > 0.0,
            "a point well off the shaft reads as inside it"
        );
        // Beside the head, past the shaft's width but short of the barb.
        assert!(sd_polygon(150.0, 100.0, &pts) < 0.0, "the head is hollow");
    }

    /// The rim exists so a red arrow on a red picture can still be seen. This
    /// is the assertion that it lands *outside* the ink rather than eating into
    /// it — a rim drawn inside would thin the shape instead of ringing it.
    #[test]
    fn the_rim_rings_the_shape_without_eating_into_it() {
        let draw = |border: f32| {
            let mut img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
            let mut l = layer(Tool::Rect, [30.0, 30.0], [70.0, 70.0]);
            l.stroke = 4.0;
            l.filled = true;
            l.border = border;
            l.border_color = [255, 255, 255, 255];
            apply(&mut img, &[l], 1.0, None);
            img
        };
        let bare = draw(0.0);
        let ringed = draw(4.0);

        assert_eq!(
            bare.get_pixel(50, 50).0,
            ringed.get_pixel(50, 50).0,
            "the rim changed the middle of the shape, so it is being drawn on top"
        );
        // Just outside the shape's own edge, which sits at y=70 plus half the
        // stroke. A rim reaches past that; nothing else does.
        assert_eq!(bare.get_pixel(50, 75).0, [0, 0, 0, 255]);
        let ring = ringed.get_pixel(50, 75).0;
        assert!(
            ring[0] > 200 && ring[1] > 200 && ring[2] > 200,
            "outside the shape reads {ring:?}, not the white rim"
        );
    }

    /// The shadow is the reason the rim reads at all on a light picture. It has
    /// to fall *outside* the rim, and it has to be soft — a hard edge would be
    /// a second outline rather than a shadow.
    #[test]
    fn the_shadow_falls_outside_the_shape_and_fades() {
        let mut img = RgbaImage::from_pixel(120, 120, Rgba([255, 255, 255, 255]));
        let mut l = layer(Tool::Rect, [40.0, 30.0], [80.0, 60.0]);
        l.stroke = 4.0;
        l.filled = true;
        l.shadow = 10.0;
        apply(&mut img, &[l], 1.0, None);

        // The shape's own edge is at y = 60 + half the stroke, and the shadow
        // reaches ten past that with a 3.5px drop, so all three samples sit
        // inside the falloff.
        let near = luminance(img.get_pixel(60, 64).0);
        let mid = luminance(img.get_pixel(60, 69).0);
        let far = luminance(img.get_pixel(60, 74).0);
        let away = luminance(img.get_pixel(60, 110).0);
        assert!(near < 0.98, "there is no shadow under the shape at all");
        assert!(
            near < mid && mid < far,
            "the shadow reads {near:.2} then {mid:.2} then {far:.2} — it is \
             not fading with distance"
        );
        assert!(away > 0.98, "the shadow reached the far side of the image");
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

    /// The field is *signed* now, because a filled shape is rasterised from the
    /// same function without taking the absolute value. A field that came back
    /// positive on both sides would fill the whole bounding box.
    #[test]
    fn box_distance_is_zero_on_the_edge_and_negative_inside() {
        // 20x20 box centred at (50,50): the edge sits at x=40.
        assert!(sd_round_box(40.0, 50.0, 50.0, 50.0, 10.0, 10.0, 0.0).abs() < 1e-5);
        assert!((sd_round_box(35.0, 50.0, 50.0, 50.0, 10.0, 10.0, 0.0) - 5.0).abs() < 1e-5);
        assert!(
            (sd_round_box(45.0, 50.0, 50.0, 50.0, 10.0, 10.0, 0.0) + 5.0).abs() < 1e-5,
            "inside the box the distance is not negative, so a fill would cover \
             the whole bounding box"
        );
    }

    /// Rounding pulls the corner in, and only the corner: a point on the middle
    /// of an edge is exactly as far away as it was with sharp corners.
    #[test]
    fn a_rounded_corner_moves_the_corner_and_leaves_the_edges_alone() {
        let sharp = sd_round_box(60.0, 60.0, 50.0, 50.0, 10.0, 10.0, 0.0);
        let round = sd_round_box(60.0, 60.0, 50.0, 50.0, 10.0, 10.0, 5.0);
        assert!(
            round > sharp + 1.0,
            "the corner point is {round} from a rounded box and {sharp} from a \
             sharp one — the radius did nothing"
        );
        let edge = |r| sd_round_box(60.0, 50.0, 50.0, 50.0, 10.0, 10.0, r);
        assert!((edge(0.0) - edge(5.0)).abs() < 1e-5, "the edge moved");
    }

    #[test]
    fn ellipse_distance_is_zero_on_the_axes_and_negative_inside() {
        assert!(sd_ellipse(60.0, 50.0, 50.0, 50.0, 10.0, 20.0).abs() < 1e-5);
        assert!(sd_ellipse(50.0, 70.0, 50.0, 50.0, 10.0, 20.0).abs() < 1e-5);
        assert!(sd_ellipse(50.0, 50.0, 50.0, 50.0, 10.0, 20.0) < 0.0);
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
            layer(Tool::Line, [-100.0, -100.0], [200.0, 200.0]),
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
        for t in [Tool::Line, Tool::Rect, Tool::Ellipse] {
            assert!(t.uses_stroke(), "{t:?} draws lines");
        }
        assert!(
            !Tool::Arrow.uses_stroke(),
            "the arrow's proportions are locked, so a stroke slider would be a \
             control that does nothing"
        );
        for t in [Tool::Text, Tool::Blur, Tool::Highlight, Tool::Fill] {
            assert!(!t.uses_stroke(), "{t:?} has no line to thicken");
        }
    }
}
