//! Does a pinned window survive being captured? Four questions, one probe.
//!
//! Opens the exact viewport combination the pin design proposes — undecorated,
//! transparent, always on top, resizable — holding a pattern built to expose
//! resampling, colour conversion and any gap between the image and the window
//! edge. Then the shot of it is compared against the pattern it was made from.
//!
//!     cargo run --release --example pin_probe
//!     screencapture -R <the rect it prints> /tmp/pin_probe_shot.png
//!     cargo run --release --example pin_probe -- --diff /tmp/pin_probe_shot.png
//!
//! Keys: wheel = opacity, `1` = back to solid, `f` = nearest/linear,
//! `l` = release the 1:1 lock so the window can be resized by hand, Esc = quit.
//!
//! It is still here because two of its questions are still open, and neither
//! can be answered on a Mac: whether a pin appears twice in shotr's own picker,
//! which shows a pre-shot that already contains it, and whether an undecorated
//! `resizable(true)` window gets free edge resizing off Windows and Linux as it
//! does on macOS. Run it there, write the answers into
//! `plans/reports/260811-1026-pin-to-screen.md`, and then delete this file —
//! everything it measured on macOS is already recorded.

use eframe::egui;

/// Big enough to hold every band, small enough to sit at 1:1 on a laptop.
const W: u32 = 480;
const H: u32 = 320;
/// The outermost ring. Any desktop showing through a seam between the image and
/// the window edge replaces this, and nothing else on a screen is this colour.
const SENTINEL: [u8; 4] = [255, 0, 255, 255];

/// Bands of hard edges, then flat colour. Stripes and a checkerboard turn to
/// mush under any resampling; the flat patches stay sharp under resampling and
/// move only if something converts colour.
fn pattern() -> image::RgbaImage {
    let mut img = image::RgbaImage::new(W, H);
    let black = [0, 0, 0, 255];
    let white = [255, 255, 255, 255];
    let patches = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
        [0, 0, 0, 255],
        [128, 128, 128, 255],
        [255, 0, 128, 255],
        [18, 52, 86, 255],
    ];

    for y in 0..H {
        for x in 0..W {
            let px = match y {
                // 1px vertical stripes.
                0..40 => {
                    if x % 2 == 0 {
                        white
                    } else {
                        black
                    }
                }
                // 1px horizontal stripes.
                40..80 => {
                    if y % 2 == 0 {
                        white
                    } else {
                        black
                    }
                }
                // 1px checkerboard: the harshest thing a sampler can meet.
                80..120 => {
                    if (x + y) % 2 == 0 {
                        white
                    } else {
                        black
                    }
                }
                // 3px stripes, where a half-pixel slip shows as a shifted edge
                // rather than as uniform grey.
                120..160 => {
                    if (x / 3) % 2 == 0 {
                        white
                    } else {
                        black
                    }
                }
                // A diagonal, whose aliasing signature is unmistakable.
                160..200 => {
                    if x % H == y {
                        white
                    } else {
                        [32, 32, 32, 255]
                    }
                }
                // Flat colour: this is where a colour profile conversion lands.
                _ => patches[(x as usize * patches.len() / W as usize).min(patches.len() - 1)],
            };
            img.put_pixel(x, y, image::Rgba(px));
        }
    }

    for x in 0..W {
        img.put_pixel(x, 0, image::Rgba(SENTINEL));
        img.put_pixel(x, H - 1, image::Rgba(SENTINEL));
    }
    for y in 0..H {
        img.put_pixel(0, y, image::Rgba(SENTINEL));
        img.put_pixel(W - 1, y, image::Rgba(SENTINEL));
    }
    img
}

/// The bands of [`pattern`], so the report can keep two questions apart.
const BANDS: [(&str, u32, u32); 6] = [
    ("1px vertical", 0, 40),
    ("1px horizontal", 40, 80),
    ("checkerboard", 80, 120),
    ("3px stripes", 120, 160),
    ("diagonal", 160, 200),
    ("flat patches", 200, H),
];

fn luma(p: [u8; 4]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

/// Mean absolute luma step between neighbours, across and down. Resampling
/// destroys this; a colour conversion, being a per-pixel map, largely keeps it —
/// which is what lets one number tell blur from a palette shift.
fn contrast(img: &image::RgbaImage, y0: u32, y1: u32) -> (f32, f32) {
    let (mut h, mut v, mut n) = (0.0, 0.0, 0.0);
    for y in y0..y1.min(img.height()).saturating_sub(1) {
        for x in 0..img.width().saturating_sub(1) {
            let c = luma(img.get_pixel(x, y).0);
            h += (c - luma(img.get_pixel(x + 1, y).0)).abs();
            v += (c - luma(img.get_pixel(x, y + 1).0)).abs();
            n += 1.0;
        }
    }
    if n == 0.0 { (0.0, 0.0) } else { (h / n, v / n) }
}

/// The commonest colours in a shot, for a region a hand dragged rather than a
/// rect we chose. Answers whether Apple's dimming overlay is baked into the
/// file: the pattern's white patch is the brightest thing in it, so if white
/// comes back as white nothing was darkened, whatever the region was.
fn colours(path: &str) {
    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return;
        }
    };
    let mut seen: std::collections::HashMap<[u8; 3], u64> = std::collections::HashMap::new();
    for p in img.pixels() {
        *seen.entry([p.0[0], p.0[1], p.0[2]]).or_default() += 1;
    }
    let mut top: Vec<_> = seen.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("{}x{}, commonest colours:", img.width(), img.height());
    for (c, n) in top.iter().take(8) {
        println!("  {c:?}  x{n}");
    }
    let white = top.iter().any(|(c, _)| c.iter().all(|&v| v >= 250));
    let black = top.iter().any(|(c, _)| c.iter().all(|&v| v <= 5));
    println!(
        "\nwhite present: {white}   black present: {black}\n{}",
        if white {
            "  nothing was dimmed: the overlay is excluded from the file."
        } else {
            "  no white came back — either the region missed the white patch, or the \
             overlay's dimming is baked into the shot."
        }
    );
}

/// What the shot says about the four questions.
fn diff(path: &str) {
    let want = pattern();
    let got = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return;
        }
    };

    println!("pattern {}x{}", want.width(), want.height());
    println!("capture {}x{}", got.width(), got.height());
    if got.dimensions() != want.dimensions() {
        println!(
            "\nSIZE MISMATCH. 1:1 did not hold: either the window was not \
             img_px/pixels_per_point, or `-R` is not in points."
        );
    }

    let w = want.width().min(got.width());
    let h = want.height().min(got.height());

    println!("\nband                differ  worst   contrast h/v  want h/v");
    let mut structural = false;
    for (name, y0, y1) in BANDS {
        if y0 >= h {
            continue;
        }
        let y1 = y1.min(h);
        let (mut differ, mut worst) = (0u64, 0i32);
        for y in y0..y1 {
            for x in 0..w {
                let a = want.get_pixel(x, y).0;
                let b = got.get_pixel(x, y).0;
                let d = (0..3)
                    .map(|c| (i32::from(a[c]) - i32::from(b[c])).abs())
                    .max()
                    .unwrap_or(0);
                if d > 0 {
                    differ += 1;
                    worst = worst.max(d);
                }
            }
        }
        let (gh, gv) = contrast(&got, y0, y1);
        let (wh, wv) = contrast(&want, y0, y1);
        // A band whose edges are gone has lost more than half its local step.
        let lost = wh > 1.0 && gh < wh * 0.5 || wv > 1.0 && gv < wv * 0.5;
        structural |= lost;
        println!(
            "{name:<18} {differ:>7} {worst:>6}   {gh:>5.1}/{gv:<5.1} {wh:>5.1}/{wv:<5.1}{}",
            if lost { "  <-- EDGES LOST" } else { "" }
        );
    }

    println!("\nflat patches, want -> got:");
    let y = (H - 60).min(h - 1);
    for i in 0..8u32 {
        let x = (i * W / 8 + W / 16).min(w - 1);
        println!(
            "  {:?} -> {:?}",
            &want.get_pixel(x, y).0[..3],
            &got.get_pixel(x, y).0[..3]
        );
    }

    // A ring in one flat colour means the image reached the window edge,
    // whatever that colour was mapped to. Desktop leaking in would vary along
    // its length, so uniformity is the test — not equality with the sentinel.
    let mut ring: Vec<[u8; 4]> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                ring.push(got.get_pixel(x, y).0);
            }
        }
    }
    let first = ring.first().copied().unwrap_or_default();
    let uniform = ring
        .iter()
        .filter(|p| {
            (0..3)
                .map(|c| (i32::from(p[c]) - i32::from(first[c])).abs())
                .max()
                .unwrap_or(0)
                <= 4
        })
        .count();
    println!(
        "\nring: {uniform}/{} pixels one colour, {:?} (sentinel was {:?})",
        ring.len(),
        &first[..3],
        &SENTINEL[..3]
    );

    println!("\nreading:");
    if structural {
        println!("  RESAMPLED. Some band lost its edges: a pin would look soft, and the");
        println!("  next capture would bake the softness into the file.");
    } else {
        println!("  structure intact — every band kept its local contrast, so sampling is");
        println!("  1:1 and pixel-aligned.");
    }
    if uniform * 20 < ring.len() * 19 {
        println!("  ring is not one colour: the image does not cover the window edge to");
        println!("  edge and the desktop is leaking into the shot.");
    } else if first[..3] != SENTINEL[..3] {
        println!("  colour is NOT preserved through the round trip: the ring came back as");
        println!("  {:?} where it went out as {:?}. Pin, capture again, and", &first[..3], &SENTINEL[..3]);
        println!("  the values have moved even though nothing was resampled.");
    } else {
        println!("  colour preserved exactly.");
    }
}

struct Probe {
    img: image::RgbaImage,
    tex: Option<egui::TextureHandle>,
    alpha: u8,
    nearest: bool,
    /// Whether to keep forcing the window to img_px / pixels_per_point. Release
    /// it to resize by hand and watch for jitter.
    locked: bool,
    asked: Option<egui::Vec2>,
    said: String,
}

impl Probe {
    fn new(img: image::RgbaImage) -> Self {
        Self {
            img,
            tex: None,
            alpha: 255,
            nearest: false,
            locked: true,
            asked: None,
            said: String::new(),
        }
    }

    fn options(&self) -> egui::TextureOptions {
        if self.nearest {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        }
    }

    fn texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if let Some(tex) = &self.tex {
            return tex.clone();
        }
        let size = [self.img.width() as usize, self.img.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, self.img.as_raw());
        let tex = ctx.load_texture("pattern", image, self.options());
        self.tex = Some(tex.clone());
        tex
    }

    /// 1:1 means one image pixel per *device* pixel, and `inner_size` is points.
    fn keep_1to1(&mut self, ctx: &egui::Context) {
        if !self.locked {
            return;
        }
        let ppp = ctx.pixels_per_point();
        let want = egui::vec2(self.img.width() as f32 / ppp, self.img.height() as f32 / ppp);
        let have = ctx.content_rect().size();
        let off = (want - have).abs();
        if off.x < 0.5 && off.y < 0.5 {
            return;
        }
        // Only ask once per target, or a window manager that rounds the size
        // differently turns this into an oscillation.
        if self.asked == Some(want) {
            return;
        }
        self.asked = Some(want);
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(want));
    }

    fn report(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let size = ctx.content_rect().size();
        let device = size * ppp;
        let rect = ctx.input(|i| i.viewport().inner_rect);
        let cmd = match rect {
            Some(r) => format!(
                "screencapture -R {},{},{},{} /tmp/pin_probe_shot.png",
                r.min.x.round(),
                r.min.y.round(),
                r.width().round(),
                r.height().round()
            ),
            None => "no inner_rect — the platform will not say where the window is".into(),
        };
        let line = format!(
            "ppp {ppp}  window {:.1}x{:.1} pt = {:.1}x{:.1} px  (want {}x{})  alpha {}  \
             filter {}  lock {}\n  {cmd}",
            size.x,
            size.y,
            device.x,
            device.y,
            self.img.width(),
            self.img.height(),
            self.alpha,
            if self.nearest { "nearest" } else { "linear" },
            self.locked,
        );
        if line != self.said {
            println!("{line}");
            self.said = line;
        }
    }
}

impl eframe::App for Probe {
    /// Transparent, because the ghost mode has no other mechanism: winit has no
    /// `set_opacity`, so opacity can only be a tint on the image.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.input(|i| {
            let step = i.smooth_scroll_delta.y;
            if step != 0.0 {
                let next = f32::from(self.alpha) + step.signum() * 8.0;
                self.alpha = next.clamp(16.0, 255.0) as u8;
            }
            if i.key_pressed(egui::Key::Num1) {
                self.alpha = 255;
                self.locked = true;
                self.asked = None;
            }
            if i.key_pressed(egui::Key::F) {
                self.nearest = !self.nearest;
                self.tex = None;
            }
            if i.key_pressed(egui::Key::L) {
                self.locked = !self.locked;
            }
            if i.key_pressed(egui::Key::Escape) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        self.keep_1to1(&ctx);
        let tex = self.texture(&ctx);

        // The image covers the whole window, edge to edge: a pin may paint no
        // chrome, because every pixel of it would land in the next capture.
        let rect = ui.max_rect();
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::from_white_alpha(self.alpha),
        );
        let hit = ui.interact(
            rect,
            egui::Id::new("pin_probe_body"),
            egui::Sense::click_and_drag(),
        );
        if hit.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        self.report(&ctx);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--diff") {
        match args.get(i + 1) {
            Some(path) => diff(path),
            None => eprintln!("usage: pin_probe -- --diff <capture.png>"),
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--colors") {
        match args.get(i + 1) {
            Some(path) => colours(path),
            None => eprintln!("usage: pin_probe -- --colors <capture.png>"),
        }
        return Ok(());
    }

    let img = pattern();
    let source = std::env::temp_dir().join("pin_probe_source.png");
    if let Err(e) = img.save(&source) {
        eprintln!("could not write {}: {e}", source.display());
    } else {
        println!("pattern written to {}", source.display());
    }
    println!("wheel = opacity, 1 = solid + relock, f = filter, l = unlock size, Esc = quit\n");

    eframe::run_native(
        "pin probe",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_transparent(true)
                .with_resizable(true)
                .with_always_on_top()
                .with_inner_size([img.width() as f32, img.height() as f32])
                .with_title("pin probe"),
            ..Default::default()
        },
        Box::new(move |_cc| Ok(Box::new(Probe::new(img)))),
    )
}
