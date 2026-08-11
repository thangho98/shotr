//! A shot pinned to the screen: a floating window that stays above other
//! applications, holding the raw capture at 1:1.
//!
//! It paints the image and nothing else. No titlebar, no badge, no border —
//! every pixel of chrome would land in the next capture, which is the point of
//! pinning. Feedback that cannot be drawn goes through [`crate::notify`],
//! exactly as the windowless `--capture --copy` path does.
//!
//! Design and every measurement behind it:
//! `plans/reports/260811-1026-pin-to-screen.md`.

use crate::app::{shortcut, theme};
use crate::export;
use crate::i18n::t;
use crate::notify;
use crate::settings::Prefs;
use eframe::egui;
use eframe::egui::emath::GuiRounding;
use image::RgbaImage;
use std::path::{Path, PathBuf};

/// How much of a monitor a pin may cover before it is shrunk to fit.
/// `--capture --full --pin` would otherwise ask for a window larger than the
/// screen it has to appear on.
const MAX_SHARE: f32 = 0.9;

/// Below this the pin is too faint to find, and finding it is the only way to
/// close it — there is no chrome to click.
const ALPHA_FLOOR: u8 = 40;

/// The hover controls, in points. Two round marks in a corner: small enough not
/// to cover what was pinned, and near the size `icons` glyphs are drawn for.
const CTRL: f32 = 28.0;
const CTRL_GAP: f32 = 6.0;
const CTRL_INSET: f32 = 10.0;
/// Faint on purpose. These sit on top of the shot, and the shot is the point;
/// they only have to be findable once the pointer is already over the pin.
const CTRL_BG: u8 = 64;
const CTRL_INK: u8 = 130;
const CTRL_LIFT: u8 = 72;

/// What the hover controls were asked for.
#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Edit,
    Close,
}

/// The pin's two marks live here rather than in [`crate::app::icons`], and the
/// reason is the close one: `Glyph::Close` is `cfg`-ed away on macOS on purpose,
/// so the editor cannot reach for it instead of Apple's window lights. A pin has
/// no titlebar on any platform and needs its own. Adding an ungated twin beside
/// the gated one would leave the guard decorative, and splitting the pair across
/// two files would invite the next reader to do exactly that.
///
/// They follow the authoring convention of `icons`: a 0..1 square inside a
/// margin, stroke width proportional to the rect, so they carry the same visual
/// weight as the app's own glyphs.
fn cross(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let box_ = rect.shrink(rect.width() * 0.33);
    let stroke = egui::Stroke::new((rect.width() * 0.075).max(1.2), color);
    painter.line_segment([box_.left_top(), box_.right_bottom()], stroke);
    painter.line_segment([box_.right_top(), box_.left_bottom()], stroke);
}

/// A pencil, tip down. Drawn as one convex silhouette — a long shaft ending in a
/// point — because at 28px an outline of a pencil is mud. The collar is a gap cut
/// out of it rather than a line drawn on it: a contrasting stroke over a
/// translucent fill reads as grime.
fn pencil(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let box_ = rect.shrink(rect.width() * 0.26);
    let at =
        |x: f32, y: f32| egui::pos2(box_.min.x + x * box_.width(), box_.min.y + y * box_.height());

    // One axis, tip at the bottom left; every point below is that axis offset by
    // half a width along its perpendicular, so the shaft cannot come out skewed.
    painter.add(egui::Shape::convex_polygon(
        vec![at(0.04, 0.96), at(0.32, 0.84), at(0.16, 0.68)],
        color,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            at(0.20, 0.64),
            at(0.36, 0.80),
            at(1.00, 0.16),
            at(0.84, 0.00),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

/// Opacity per point of wheel travel, and the most one frame may move it.
///
/// Proportional rather than one step per event: `smooth_scroll_delta` keeps
/// arriving for several frames after the fingers lift, so a fixed step per frame
/// overshoots and a fling lands on the floor. The cap keeps one violent flick
/// from doing the same.
const ALPHA_PER_POINT: f32 = 0.5;
const ALPHA_MAX_JUMP: f32 = 16.0;

/// The window size, in points, that shows `px` at one image pixel per *device*
/// pixel — clamped to [`MAX_SHARE`] of the monitor, keeping the aspect ratio.
///
/// `monitor` is whatever `ViewportInfo::monitor_size` reported, and it is an
/// `Option` because a platform may decline to say. With no size to clamp
/// against the pin opens at its natural size: a window slightly too large beats
/// one shrunk against a guess.
///
/// It is the monitor, not the work area — the menu bar and a dock are inside it.
/// [`MAX_SHARE`] is what keeps a pin off them, roughly, and roughly is enough
/// for something the user can drag.
/// Write an image where another process can read it.
///
/// The name carries a timestamp because the reader starts *after* this returns:
/// two hand-offs in quick succession would otherwise race over one path, and the
/// first process could open the second's image.
pub(crate) fn to_temp(img: &RgbaImage) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!("shotr-pin-{stamp}.png"));
    img.save(&path)
        .map(|()| path)
        .map_err(|e| format!("Could not write the shot to a file: {e}"))
}

fn spawn_flag(flag: &str, path: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not find shotr: {e}"))?;
    std::process::Command::new(exe)
        .arg(flag)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start shotr {flag}: {e}"))
}

pub fn pin_points(px: [u32; 2], ppp: f32, monitor: Option<egui::Vec2>) -> egui::Vec2 {
    let ppp = if ppp > 0.0 { ppp } else { 1.0 };
    let want = egui::vec2(px[0] as f32 / ppp, px[1] as f32 / ppp);

    let Some(area) = monitor.filter(|a| a.x > 0.0 && a.y > 0.0) else {
        return want;
    };
    let room = area * MAX_SHARE;
    let shrink = (room.x / want.x).min(room.y / want.y).min(1.0);
    want * shrink
}

pub struct PinApp {
    img: RgbaImage,
    tex: Option<egui::TextureHandle>,
    alpha: u8,
    /// The `pixels_per_point` the window was last sized for, and the whole of
    /// the 1:1 policy.
    ///
    /// Re-asserting 1:1 every frame would fight the user's own resize, so this
    /// has to be keyed on something that changes only when 1:1 *means* something
    /// different. That is the scale, and nothing else: keying it on the size
    /// asked for instead meant that dragging the pin between two same-scale
    /// monitors of different resolution changed the clamp, silently threw away a
    /// hand resize, and — for a pin straddling a 1×/2× boundary, where each
    /// resize changes which monitor owns the window — alternated between two
    /// values forever, neither of which ever matched.
    sized_for: Option<f32>,
    clipboard: Option<arboard::Clipboard>,
    /// Where the image came from, when it came from a file. `--capture --pin`
    /// hands over an image and no path, so opening the editor from here has to
    /// write one first.
    source: Option<PathBuf>,
}

impl PinApp {
    pub fn new(cc: &eframe::CreationContext<'_>, img: RgbaImage, source: Option<PathBuf>) -> Self {
        // The pin says almost nothing out loud, but "Edit" and its two clipboard
        // notifications have to speak the language the rest of the app does. And
        // `theme::apply` is the only place that installs the font carrying
        // Vietnamese tone marks — without it "Sửa" draws as an empty box — as
        // well as the only place that turns egui's keyboard zoom off, which for
        // this window would resize it rather than magnify anything.
        let prefs = Prefs::load();
        crate::i18n::set(prefs.lang);
        theme::apply(&cc.egui_ctx, prefs.theme);

        Self {
            img,
            tex: None,
            alpha: u8::MAX,
            sized_for: None,
            clipboard: arboard::Clipboard::new().ok(),
            source,
        }
    }

    fn px(&self) -> [u32; 2] {
        [self.img.width(), self.img.height()]
    }

    fn texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if let Some(tex) = &self.tex {
            return tex.clone();
        }
        let size = [self.img.width() as usize, self.img.height() as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, self.img.as_raw());
        let tex = ctx.load_texture("pin", image, egui::TextureOptions::LINEAR);
        self.tex = Some(tex.clone());
        tex
    }

    fn keep_1to1(&mut self, ctx: &egui::Context, monitor: Option<egui::Vec2>) {
        let ppp = ctx.pixels_per_point();
        if self.sized_for == Some(ppp) {
            return;
        }
        self.sized_for = Some(ppp);

        let want = pin_points(self.px(), ppp, monitor);
        // Asking for the size it already has would be one more chance for a
        // window manager to round it and hand back a different number.
        if (want - ctx.content_rect().size()).abs().max_elem() < 0.5 {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(want));
    }

    fn copy(&mut self) {
        match export::copy(&self.img, &mut self.clipboard) {
            Ok(()) => notify::show(t("Copied to the clipboard")),
            Err(e) => {
                eprintln!("Could not reach the clipboard: {e}");
                notify::show(t("Cannot reach the clipboard"));
            }
        }
    }

    /// Hand the shot to the editor and stand down.
    ///
    /// The pin closes: the editor is about to show the same picture, and two
    /// copies of one image on screen is the clutter pinning was meant to avoid.
    fn edit(&mut self, ctx: &egui::Context) {
        let path = match &self.source {
            Some(path) => Ok(path.clone()),
            None => to_temp(&self.img),
        };
        match path.and_then(|p| spawn_flag("--open", &p)) {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            // A pin has no status line, so the only way to say so is out of band.
            Err(e) => {
                eprintln!("{e}");
                notify::show(t("Could not open the editor"));
            }
        }
    }

    /// The only chrome the pin has, and the rule it obeys: it may never be on
    /// screen while anything is being captured.
    ///
    /// Hover alone is not enough of a guard — a capture leaves the pointer
    /// wherever it was. Focus is: while `screencapture` or shotr's own picker is
    /// up, that overlay owns the focus and this window does not, so the controls
    /// are gone before the shutter. Both conditions, or the pin stops being
    /// something you can photograph.
    ///
    /// Returns what was asked for, and whether the pointer is over either mark —
    /// the second so the caller can leave a drag on them alone.
    fn controls(&self, ui: &egui::Ui, rect: egui::Rect) -> (Option<Ask>, bool) {
        let (focused, pointer) = ui.ctx().input(|i| {
            (
                i.viewport().focused.unwrap_or(false),
                i.pointer.hover_pos().is_some(),
            )
        });
        if !focused || !pointer {
            return (None, false);
        }

        let mut asked = None;
        let mut over = false;
        // Laid out from the corner inwards, so Close is the outermost: it is the
        // one a hand reaches for without aiming.
        for (n, ask) in [Ask::Close, Ask::Edit].into_iter().enumerate() {
            let at = egui::Rect::from_min_size(
                egui::pos2(
                    rect.max.x - CTRL_INSET - CTRL - n as f32 * (CTRL + CTRL_GAP),
                    rect.min.y + CTRL_INSET,
                ),
                egui::Vec2::splat(CTRL),
            );
            // Allocated after the body, so egui hands these the click: within one
            // layer the last widget to claim a spot wins it.
            let hit = ui.interact(at, egui::Id::new(("pin_ctrl", n)), egui::Sense::click());
            let lift = if hit.hovered() { CTRL_LIFT } else { 0 };

            // Hand-mixed translucent black and white rather than palette colours:
            // these sit on an arbitrary screenshot, and every colour in the
            // palette disappears against some screenshot or other.
            let ink = egui::Color32::from_white_alpha(CTRL_INK + lift);
            ui.painter().circle_filled(
                at.center(),
                CTRL / 2.0,
                egui::Color32::from_black_alpha(CTRL_BG + lift),
            );
            match ask {
                Ask::Edit => pencil(ui.painter(), at, ink),
                Ask::Close => cross(ui.painter(), at, ink),
            }

            over |= hit.hovered();
            if hit.clicked() {
                asked = Some(ask);
            }
        }
        (asked, over)
    }
}

impl eframe::App for PinApp {
    /// Transparent, and for a different reason than the editor's: winit has no
    /// `set_opacity`, so the ghost can only be a tint on the image, so the
    /// window behind it has to be able to let the desktop through.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Every modifier here is read off the events, never off
        // `InputState::modifiers` — see `app::shortcut` for what that costs.
        // Nothing is sent from inside the closure: `send_viewport_cmd` takes the
        // lock `ctx.input` is already holding.
        let (close, reset, copy, scroll, monitor) = ctx.input(|i| {
            (
                shortcut(&i.events, egui::Key::W, None),
                shortcut(&i.events, egui::Key::Num1, None),
                shortcut(&i.events, egui::Key::C, None)
                    || i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.smooth_scroll_delta.y,
                i.viewport().monitor_size,
            )
        });

        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if copy {
            self.copy();
        }
        if reset {
            // One key back to a pin worth capturing. It restores the structure,
            // not the bytes: a screen capture is colour-managed, so the round
            // trip moves values whatever this does. And for a shot larger than
            // the screen there is no 1:1 to go back to — `pin_points` shrank it
            // to fit and this returns it to that size, not to the shot's.
            self.alpha = u8::MAX;
            self.sized_for = None;
        }
        if scroll != 0.0 {
            let step = (scroll * ALPHA_PER_POINT).clamp(-ALPHA_MAX_JUMP, ALPHA_MAX_JUMP);
            let next = f32::from(self.alpha) + step;
            self.alpha = next.clamp(f32::from(ALPHA_FLOOR), f32::from(u8::MAX)) as u8;
        }

        self.keep_1to1(&ctx, monitor);
        let tex = self.texture(&ctx);

        // Edge to edge, and snapped to whole device pixels. A gap anywhere
        // around this is a hole through to the desktop and the next capture
        // takes the hole with it; a *fractional* rect is worse, because
        // `TextureOptions::LINEAR` then samples half a texel off and blurs the
        // whole image. Sampling was measured lossless at scale 1 and 2, where
        // the rect lands on pixels anyway — 1.25 and 1.5 are why this is here.
        let rect = ui.max_rect().round_to_pixels(ctx.pixels_per_point());
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::from_white_alpha(self.alpha),
        );

        let body = ui.interact(rect, egui::Id::new("pin_body"), egui::Sense::click_and_drag());
        let (ask, over_ctrl) = self.controls(ui, rect);
        // Dragging a control must move nothing: the click belongs to it, and a pin
        // that slides out from under a mark cannot be clicked at all.
        if body.drag_started() && !over_ctrl {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        match ask {
            Some(Ask::Edit) => self.edit(&ctx),
            Some(Ask::Close) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
    }
}

/// Open a pin in a process of its own, for a caller that already has a window.
///
/// Two reasons it cannot be [`run`] from inside one: a second `eframe` app
/// cannot be started inside a running one, and a pin has to outlive whatever
/// opened it — closing the editor must not take the pin down with it.
pub fn spawn(path: &Path) -> Result<(), String> {
    spawn_flag("--pin", path)
}

/// Open one pin. Its own process, so it outlives whatever started it.
///
/// `source` is the file the image came from, when there is one: the Edit
/// affordance hands that path to the editor rather than writing the image out
/// again.
pub fn run(img: RgbaImage, source: Option<PathBuf>) -> eframe::Result {
    // `pixels_per_point` cannot be known before the window exists — it depends
    // on the display the window lands on — so this is the 1× guess and the
    // first frame corrects it.
    let size = pin_points([img.width(), img.height()], 1.0, None);
    let viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .with_always_on_top()
        .with_inner_size(size)
        .with_title("shotr")
        .with_icon(crate::app::window_icon());

    eframe::run_native(
        "shotr",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(PinApp::new(cc, img, source)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_image_pixel_per_device_pixel() {
        let big = egui::vec2(4000.0, 4000.0);
        assert_eq!(
            pin_points([480, 320], 2.0, Some(big)),
            egui::vec2(240.0, 160.0),
            "on a 2x display a pin sized in raw pixels is twice as big as the shot, \
             and a capture of it would be a resampled copy"
        );
        assert_eq!(
            pin_points([480, 320], 1.0, Some(big)),
            egui::vec2(480.0, 320.0),
            "on a 1x display points and pixels are the same thing"
        );
    }

    #[test]
    fn the_file_handed_over_holds_the_image_handed_in() {
        // The first version handed over the newest history entry instead, on the
        // assumption it was the same shot. On macOS a region arrives from Apple's
        // overlay and is never recorded, so that entry was a capture from an
        // earlier session: the pin opened showing a picture the user had not just
        // taken. Nothing about that failure was visible in a type signature.
        let mut img = RgbaImage::new(4, 3);
        img.put_pixel(0, 0, image::Rgba([255, 0, 255, 255]));
        img.put_pixel(3, 2, image::Rgba([9, 8, 7, 255]));

        let path = to_temp(&img).expect("a pin hand-off must produce a file");
        let back = image::open(&path)
            .expect("the pin process has to be able to read it back")
            .to_rgba8();
        std::fs::remove_file(&path).ok();

        assert_eq!(
            back, img,
            "the pin must show the shot it was given, pixel for pixel"
        );
    }

    #[test]
    fn two_hand_offs_do_not_share_a_path() {
        // The reader starts after the writer returns, so one shared path lets the
        // second pin overwrite the first's image before it has been opened.
        let img = RgbaImage::new(2, 2);
        let first = to_temp(&img).expect("write");
        let second = to_temp(&img).expect("write");
        let same = first == second;
        std::fs::remove_file(&first).ok();
        std::fs::remove_file(&second).ok();
        assert!(!same, "two pins in a row would race over one file");
    }

    #[test]
    fn a_shot_larger_than_the_screen_is_shrunk_to_fit() {
        let area = egui::vec2(1440.0, 900.0);
        let got = pin_points([5760, 3600], 1.0, Some(area));
        assert!(
            got.x <= area.x * MAX_SHARE + 0.01 && got.y <= area.y * MAX_SHARE + 0.01,
            "a full-desktop pin must fit the screen it appears on, got {got:?}"
        );
        assert!(
            (got.x / got.y - 5760.0 / 3600.0).abs() < 0.01,
            "shrinking to fit must not stretch the shot, got {got:?}"
        );
    }

    #[test]
    fn a_shot_that_fits_is_left_alone() {
        assert_eq!(
            pin_points([400, 300], 1.0, Some(egui::vec2(1440.0, 900.0))),
            egui::vec2(400.0, 300.0),
            "a pin small enough to fit must stay at 1:1 rather than be scaled to \
             some share of the screen"
        );
    }

    #[test]
    fn an_unreported_monitor_leaves_the_pin_at_its_natural_size() {
        // `monitor_size` is an `Option` and a platform may decline to fill it
        // in. A window slightly too large beats one shrunk against a guess.
        assert_eq!(
            pin_points([480, 320], 1.0, None),
            egui::vec2(480.0, 320.0),
            "with no monitor size known the pin must still open at 1:1"
        );
        assert_eq!(
            pin_points([480, 320], 1.0, Some(egui::Vec2::ZERO)),
            egui::vec2(480.0, 320.0),
            "a zero-sized monitor report would otherwise collapse the window to \
             nothing, and a zero-sized texture ends the process"
        );
    }
}
