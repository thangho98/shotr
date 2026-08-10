//! Application state and the eframe entry point. The UI itself lives in
//! [`sidebar`] and [`canvas`], which are child modules so they can reach these
//! private fields directly.

use crate::i18n::{t, tf};

mod canvas;
pub(crate) mod icons;
mod ocr_job;
mod shell;
mod sidebar;
pub(crate) mod theme;

use ab_glyph::FontArc;
use eframe::egui;
use image::{Rgba, RgbaImage};
use std::path::PathBuf;

use crate::annotate::{self, Layer, Tool};
use crate::capture::make_preview;
use crate::export;
use crate::history;
use crate::ocr::Word;
use crate::ocr::detect::Finding;
use crate::render::background::BG_PRESETS;
use crate::render::{Geometry, Scene, render, render_detailed};
use crate::settings::{Background, Prefs, Preset, Rgba8, Style};
use ocr_job::OcrState;

/// What this process was launched to do.
/// Where a screenshot comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Every monitor stitched into one virtual desktop.
    All,
    /// One monitor, captured at 1:1.
    Monitor(usize),
}

impl Source {
    /// The arguments that reproduce this source in a fresh capture process.
    pub fn args(self) -> Vec<String> {
        match self {
            Source::All => vec!["--capture".into()],
            Source::Monitor(i) => vec!["--capture".into(), "--monitor".into(), i.to_string()],
        }
    }
}

pub enum Start {
    /// Fullscreen region picker over a screenshot taken before the window.
    Picker(RgbaImage),
    /// Straight to the editor with a snapshot of the whole desktop. The editor
    /// keeps it, not just the part being edited — see [`ShotrApp::new`].
    Editor(RgbaImage),
    /// Straight to the editor with an image that is not the desktop: one
    /// window, copied out of its own buffer.
    Window(RgbaImage),
    OpenPath(PathBuf),
    OpenDialog,
    /// Whatever image is on the clipboard, straight to the editor.
    Clipboard,
    /// The hub: no capture, just the recent shots and the other ways in.
    ///
    /// This screen used to be reachable only by capturing first and then going
    /// "back to selection". macOS no longer passes through it at all — Apple's
    /// overlay returns a finished region — so the tray opens it directly.
    History,
}

pub(crate) const SIDEBAR_W: f32 = 336.0;
/// How long a status message holds the line before the tool hint returns.
pub(crate) const STATUS_SECONDS: f64 = 6.0;
pub(crate) const PREVIEW_MAX_W: u32 = 1000;
pub(crate) const SWATCH_PX: u32 = 56;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Select,
    Edit,
}

/// How the Select screen interprets the pointer.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PickMode {
    Region,
    Window,
}

/// Background swatches, in sidebar order.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Swatch {
    Auto,
    Desktop,
    Preset(usize),
    None,
    Custom,
}

impl Swatch {
    pub(crate) fn label(self) -> String {
        match self {
            Self::Auto => "Auto".into(),
            Self::Desktop => "Desktop".into(),
            Self::Preset(i) => BG_PRESETS[i].name.into(),
            Self::None => "None".into(),
            Self::Custom => "Custom".into(),
        }
    }

    pub(crate) fn background(self) -> Background {
        match self {
            Self::Auto => Background::Auto,
            Self::Desktop => Background::Desktop,
            Self::Preset(i) => Background::Preset(i),
            Self::None => Background::None,
            Self::Custom => Background::Custom,
        }
    }
}

pub(crate) fn swatch_order() -> Vec<Swatch> {
    let mut v = vec![Swatch::Auto, Swatch::Desktop];
    v.extend((0..BG_PRESETS.len()).map(Swatch::Preset));
    v.push(Swatch::None);
    v.push(Swatch::Custom);
    v
}

/// One group of sidebar controls. Exactly one is open at a time: six groups is
/// more than any screen shows at once, and a column of open cards is a column
/// nobody can find anything in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Section {
    Background,
    Layout,
    Ratio,
    Ocr,
    Watermark,
    Export,
}

/// What clicking on the image does while OCR results are showing.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OcrMode {
    Off,
    SelectText,
    ManualRedact,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Zoom {
    Fit,
    Percent(u32),
}

impl Zoom {
    /// Zoom steps, in percent. Geometric rather than linear so one notch of the
    /// wheel feels the same at 25% as it does at 400%.
    pub(crate) const STEPS: [u32; 10] = [25, 33, 50, 75, 100, 150, 200, 300, 400, 800];
    pub(crate) const MIN: u32 = 10;
    pub(crate) const MAX: u32 = 1600;

    /// Scale one notch. `by` above 1.0 zooms in.
    pub(crate) fn scaled(self, current_pct: u32, by: f32) -> Zoom {
        let base = match self {
            Zoom::Fit => current_pct,
            Zoom::Percent(p) => p,
        };
        let next = (base as f32 * by).round() as u32;
        Zoom::Percent(next.clamp(Zoom::MIN, Zoom::MAX))
    }

    /// The next step up or down the preset ladder.
    pub(crate) fn stepped(self, current_pct: u32, up: bool) -> Zoom {
        let base = match self {
            Zoom::Fit => current_pct,
            Zoom::Percent(p) => p,
        };
        let next = if up {
            Zoom::STEPS.iter().find(|s| **s > base).copied()
        } else {
            Zoom::STEPS.iter().rev().find(|s| **s < base).copied()
        };
        Zoom::Percent(next.unwrap_or(base).clamp(Zoom::MIN, Zoom::MAX))
    }
}

pub struct ShotrApp {
    // Raw, uncropped screen capture plus its downscaled preview (Select mode).
    pub(crate) capture_full: RgbaImage,
    pub(crate) capture_preview: RgbaImage,
    pub(crate) raw_texture: Option<egui::TextureHandle>,
    pub(crate) raw_dirty: bool,

    // Working image plus preview (Edit mode).
    pub(crate) shot_full: RgbaImage,
    pub(crate) shot_preview: RgbaImage,
    pub(crate) preview_scale: f32,
    /// Width `shot_preview` was built to, in device pixels. Follows the
    /// display — see [`ShotrApp::fit_preview_to_display`].
    pub(crate) preview_budget: u32,
    pub(crate) texture: Option<egui::TextureHandle>,
    pub(crate) dirty: bool,

    // Selection.
    pub(crate) mode: Mode,
    pub(crate) pick_mode: PickMode,
    pub(crate) sel_start: Option<egui::Pos2>,
    pub(crate) sel_rect: Option<egui::Rect>,
    pub(crate) crop_px: Option<[u32; 4]>,

    // Windows as they were at capture time, for the hover-to-pick overlay.
    pub(crate) windows: Vec<crate::winlist::WindowEntry>,
    pub(crate) hover_window: Option<usize>,

    pub(crate) source: Source,
    /// The snapshot exactly as taken, spanning every monitor. Never modified —
    /// switching between screens cuts a view out of this rather than shooting
    /// again, so every view is the same instant.
    pub(crate) desktop_full: RgbaImage,
    /// Where each monitor sits inside [`Self::desktop_full`].
    pub(crate) monitor_views: Vec<crate::capture::MonitorView>,

    // Capture flow (X11 only): hide the window, then shoot at `pending_capture`.
    pub(crate) pending_capture: Option<f64>,

    pub(crate) style: Style,
    pub(crate) prev_style: Style,
    pub(crate) prefs: Prefs,
    pub(crate) prev_prefs: Prefs,
    pub(crate) save_settings_at: Option<f64>,

    pub(crate) presets: Vec<Preset>,
    pub(crate) preset_name: String,

    pub(crate) history: Vec<history::Entry>,
    pub(crate) history_thumbs: Vec<Option<egui::TextureHandle>>,

    // Loaded wallpaper / custom background image, keyed by the path it came from.
    pub(crate) bg_image: Option<RgbaImage>,
    /// Loaded watermark logo, cached alongside its path.
    pub(crate) wm_image: Option<RgbaImage>,
    pub(crate) wm_image_key: Option<PathBuf>,
    pub(crate) bg_image_key: Option<PathBuf>,

    pub(crate) swatches: Vec<(Swatch, egui::TextureHandle)>,
    pub(crate) swatches_dirty: bool,

    // Annotation. Layer coordinates are in original screenshot pixels.
    pub(crate) layers: Vec<Layer>,
    pub(crate) tool: Tool,
    pub(crate) undo: annotate::History,
    pub(crate) selected_layer: Option<usize>,
    /// The shape currently being dragged out. Kept out of `layers` so a drag
    /// can be previewed with cheap vector overlays instead of re-rendering the
    /// whole pipeline on every frame.
    pub(crate) draft: Option<Layer>,
    pub(crate) move_delta: Option<[f32; 2]>,
    pub(crate) drag_anchor: Option<[f32; 2]>,
    pub(crate) annot_color: Rgba8,
    pub(crate) annot_stroke: f32,
    pub(crate) annot_font_size: f32,
    pub(crate) annot_blur: f32,
    /// Opacity of the paint tool, 0–255. Kept apart from [`Self::annot_color`]
    /// so dialling paint down to a translucent marker does not also make every
    /// arrow and box drawn afterwards see-through.
    pub(crate) annot_paint_alpha: u8,
    /// Where the screenshot sits inside the last preview render.
    pub(crate) preview_geom: Geometry,

    // OCR. Word rects are in original screenshot pixels, like annotation layers.
    pub(crate) ocr_words: Vec<Word>,
    pub(crate) ocr_findings: Vec<Finding>,
    pub(crate) ocr_state: OcrState,
    pub(crate) ocr_rx: Option<std::sync::mpsc::Receiver<ocr_job::Outcome>>,
    /// Word indices the user redacted by hand.
    pub(crate) manual_redact: Vec<usize>,
    pub(crate) selected_words: Vec<usize>,
    pub(crate) ocr_mode: OcrMode,
    /// Rubber-band selection over words, in original screenshot pixels.
    pub(crate) ocr_drag: Option<([f32; 2], [f32; 2])>,
    /// Set when a new image lands; the next frame kicks off recognition.
    pub(crate) want_ocr: bool,

    pub(crate) font: Option<FontArc>,
    pub(crate) zoom: Zoom,
    /// How far the image is dragged from centre, in screen pixels. Zoom without
    /// pan can only ever show the middle of an enlarged image.
    pub(crate) pan: egui::Vec2,
    /// Zoom percentage the last frame actually drew at, so a wheel notch from
    /// `Fit` continues from what is on screen rather than jumping to 100%.
    pub(crate) shown_zoom: u32,
    pub(crate) status: String,
    pub(crate) clipboard: Option<arboard::Clipboard>,

    /// True while the window covers the screen for region picking.
    pub(crate) picking_fullscreen: bool,
    /// Opened from the tray with no capture behind it, so the Select screen is
    /// a hub rather than a region picker.
    pub(crate) hub: bool,
    /// `--copy` where the picker is shotr's own window. macOS never sets this:
    /// Apple's overlay hands back a finished region, so that copy happens
    /// before any window exists.
    pub copy_on_finish: bool,
    /// Background colour detected in the current screenshot, for the Inset UI.
    pub(crate) detected_inset: Option<Rgba8>,
    pub(crate) show_custom_size: bool,
    /// Which sidebar group is unfolded.
    pub(crate) open_section: Option<Section>,
    /// The 1×N ramp the sidebar card is painted with. Built once, on the first
    /// frame that has a `Context` to build it in.
    pub(crate) sidebar_grad: Option<egui::TextureHandle>,
    /// Where the pointer sat, relative to the shape's own angle, when a
    /// rotation drag began. `None` means the drag is a move, not a turn.
    pub(crate) turn_from: Option<f32>,
    /// The annotation the overlay owns this frame, and which the preview bitmap
    /// therefore leaves out.
    ///
    /// Two reasons to take one out: a drag, so it can follow the pointer
    /// without a ghost sitting at the old spot; and selection of a stroked
    /// shape, so the halo can be drawn *under* the ink rather than over it.
    pub(crate) detached_layer: Option<usize>,
    /// The last message shown, and when it stops being worth the space.
    ///
    /// Messages like "Saved: …" are news, not a permanent caption — but the
    /// status line is also where the editor explains the current tool, and a
    /// message that never expires holds that line for the whole session. The
    /// text is compared rather than every assignment site being changed.
    pub(crate) status_shown: String,
    pub(crate) status_until: f64,
    /// Our own idea of whether the window is maximised. With the titlebar gone
    /// this is the only thing the maximise button can toggle against — see
    /// [`shell`].
    pub(crate) maximised: bool,

    /// Caret position, as a byte index into the text draft being typed. Bytes
    /// rather than chars because that is what slicing a `String` needs, and
    /// Vietnamese is multi-byte throughout.
    pub(crate) text_caret: usize,
    /// The text a label had before this edit started, so Esc can put it back.
    /// `None` means the label is new and Esc should throw it away.
    pub(crate) text_before: Option<String>,
    /// In-flight IME composition, shown after the caret but not yet part of the
    /// label. Vietnamese input methods build every accented character here.
    pub(crate) text_preedit: String,
}

impl ShotrApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        start: Start,
        source: Source,
        views: Vec<crate::capture::MonitorView>,
    ) -> Self {
        let placeholder = RgbaImage::from_pixel(640, 360, Rgba([28, 30, 38, 255]));
        let style = Style::load();
        let prefs = Prefs::load();
        // Before any string is drawn, so the first frame is already translated,
        // and before any colour is chosen, so it is already the right theme.
        crate::i18n::set(prefs.lang);
        let font = theme::apply(&cc.egui_ctx, prefs.theme);

        let mut app = Self {
            capture_full: placeholder.clone(),
            capture_preview: placeholder.clone(),
            raw_texture: None,
            raw_dirty: true,
            shot_full: placeholder.clone(),
            shot_preview: placeholder.clone(),
            preview_scale: 1.0,
            preview_budget: PREVIEW_MAX_W,
            texture: None,
            dirty: true,
            mode: Mode::Select,
            pick_mode: PickMode::Region,
            sel_start: None,
            sel_rect: None,
            crop_px: None,
            windows: Vec::new(),
            hover_window: None,
            source,
            desktop_full: placeholder.clone(),
            monitor_views: views,
            pending_capture: None,
            prev_style: style.clone(),
            style,
            prev_prefs: prefs.clone(),
            prefs,
            save_settings_at: None,
            presets: crate::settings::load_presets(),
            preset_name: String::new(),
            history: history::list(),
            history_thumbs: Vec::new(),
            bg_image: None,
            wm_image: None,
            wm_image_key: None,
            bg_image_key: None,
            swatches: Vec::new(),
            swatches_dirty: true,
            layers: Vec::new(),
            tool: Tool::Select,
            undo: annotate::History::default(),
            selected_layer: None,
            draft: None,
            move_delta: None,
            drag_anchor: None,
            annot_color: [0xff, 0x3b, 0x30, 0xff],
            annot_stroke: 6.0,
            annot_font_size: 34.0,
            annot_blur: 12.0,
            annot_paint_alpha: 255,
            preview_geom: Geometry::default(),
            ocr_words: Vec::new(),
            ocr_findings: Vec::new(),
            ocr_state: OcrState::Absent,
            ocr_rx: None,
            manual_redact: Vec::new(),
            selected_words: Vec::new(),
            ocr_mode: OcrMode::Off,
            ocr_drag: None,
            want_ocr: false,
            font,
            zoom: Zoom::Fit,
            pan: egui::Vec2::ZERO,
            shown_zoom: 100,
            status: String::new(),
            clipboard: arboard::Clipboard::new().ok(),
            picking_fullscreen: matches!(start, Start::Picker(_)),
            hub: matches!(start, Start::History),
            copy_on_finish: false,
            detected_inset: None,
            show_custom_size: false,
            turn_from: None,
            detached_layer: None,
            status_shown: String::new(),
            status_until: 0.0,
            open_section: Some(Section::Background),
            sidebar_grad: None,
            maximised: false,
            text_caret: 0,
            text_before: None,
            text_preedit: String::new(),
        };

        // main() grabs the screen before the window exists, which is the only
        // way to stay out of our own shot on Wayland.
        match start {
            Start::Picker(shot) => app.adopt_initial(shot),
            Start::Editor(shot) => {
                // The editor keeps the desktop snapshot, not only the image it
                // is editing. Two things need it: `--monitor N` narrows a full
                // capture to one screen through `apply_source`, and "Back to
                // selection" hands the picker something to select from. Left as
                // the startup placeholder, both cropped outside the image.
                app.desktop_full = shot;
                app.apply_source();
                app.shot_full = app.capture_full.clone();
                app.enter_edit();
            }
            // No desktop snapshot to seed: a window is not a rectangle of the
            // screen, which is the whole reason it is captured separately.
            Start::Window(shot) => {
                app.shot_full = shot;
                app.enter_edit();
            }
            Start::OpenPath(path) => app.open_image(&path),
            Start::OpenDialog => match export::open_image_dialog() {
                Some(path) => app.open_image(&path),
                None => std::process::exit(0),
            },
            // Nothing on the clipboard leaves the hub up with the reason showing,
            // rather than closing on someone who just mis-clicked.
            Start::Clipboard => {
                app.open_from_clipboard();
                if app.mode != Mode::Edit {
                    app.mode = Mode::Select;
                }
            }
            Start::History => {
                app.mode = Mode::Select;
                app.status = t("Pick a recent shot, open a file, or paste from the clipboard.").into();
            }
        }
        app
    }

    // ---------------------------------------------------------------- capture

    /// Re-shoot from inside the editor. On Wayland the window cannot get out
    /// of frame, so hand the job to a fresh process — exactly what the tray and
    /// the desktop shortcut do — and leave this one alone.
    /// Start a fresh capture in a new process.
    ///
    /// It has to be a new process, not a re-capture in this one: on Wayland a
    /// window cannot hide itself, so shooting from here would put shotr's own
    /// window in the picture.
    pub(crate) fn start_capture(&mut self, source: Source) {
        match std::env::current_exe()
            .map(|exe| std::process::Command::new(exe).args(source.args()).spawn())
        {
            Ok(Ok(_)) => self.status = t("Opened a new capture window.").into(),
            Ok(Err(e)) => self.status = format!("Could not start the capture process: {e}"),
            Err(e) => self.status = format!("Could not find shotr: {e}"),
        }
    }

    /// Adopt a screenshot taken before the window existed.
    fn adopt_initial(&mut self, shot: RgbaImage) {
        self.windows = crate::winlist::list();
        // This is the snapshot for the rest of the session; switching screens
        // later cuts views out of it instead of taking another one.
        self.desktop_full = shot;
        self.apply_source();
        self.mode = Mode::Select;
        self.status = t("Drag to select a region. Space switches to picking a window.").into();
    }

    fn do_capture(&mut self) {
        // Enumerate windows while we are still hidden, so shotr is not in the list.
        self.windows = crate::winlist::list();
        match crate::capture::capture_desktop() {
            Ok((image, views)) => {
                self.desktop_full = image;
                self.monitor_views = views;
                self.apply_source();
                self.mode = Mode::Select;
                self.status = match self.pick_mode {
                    PickMode::Region => {
                        t("Drag to select a region. Space switches to picking a window.").into()
                    }
                    PickMode::Window => t("Click a window in the list.").into(),
                };
            }
            Err(e) => self.status = format!("Capture failed: {e}"),
        }
    }

    /// Point the picker at the whole desktop or at one monitor, by cutting the
    /// snapshot already in hand. No capture happens here — that is the entire
    /// point: the user asked for a shot once, and this is that shot.
    pub(crate) fn apply_source(&mut self) {
        let rect = match self.source {
            Source::All => None,
            Source::Monitor(i) => self.monitor_views.get(i).map(|v| v.rect),
        };
        self.capture_full = cut_monitor(&self.desktop_full, rect);
        self.capture_preview = make_preview(&self.capture_full, 1100).0;
        self.raw_dirty = true;
        self.sel_start = None;
        self.sel_rect = None;
        self.crop_px = None;
        self.hover_window = None;
    }

    /// Capture one window straight from its own buffer and go to the editor.
    ///
    /// Not a crop of the desktop shot: the compositor hands over that window's
    /// pixels, so a window sitting behind another still comes out whole.
    pub(crate) fn capture_window(&mut self, index: usize) {
        let Some(window) = self.windows.get(index).cloned() else {
            return;
        };
        match crate::winlist::capture(&window.identifier) {
            Ok(Some(img)) => {
                self.shot_full = img;
                self.enter_edit();
                if let Some(entry) = history::record(&self.shot_full) {
                    self.history.insert(0, entry);
                    self.history.truncate(history::MAX_ENTRIES);
                    self.history_thumbs.clear();
                }
            }
            // Cancelled out of the picker: stay where we are and say nothing.
            Ok(None) => {}
            Err(e) => self.status = format!("Could not capture the window: {e}"),
        }
    }

    pub(crate) fn finish_selection(&mut self, use_crop: bool) {
        self.shot_full = match (use_crop, self.crop_px) {
            (true, Some([x, y, w, h])) => {
                image::imageops::crop_imm(&self.capture_full, x, y, w, h).to_image()
            }
            _ => self.capture_full.clone(),
        };
        self.enter_edit();
        if let Some(entry) = history::record(&self.shot_full) {
            self.history.insert(0, entry);
            self.history.truncate(history::MAX_ENTRIES);
            self.history_thumbs.clear();
        }
    }

    /// The picker fills the screen with no decorations; the editor is an
    /// ordinary window. This is the hand-off between the two.
    fn leave_fullscreen(&mut self, ctx: &egui::Context) {
        if !self.picking_fullscreen {
            return;
        }
        self.picking_fullscreen = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        // The editor draws its own titlebar, so the system's stays off — see
        // `shell`. Turning decorations back on here would put a second,
        // duplicate titlebar above the sidebar card's own.
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        // Where the picker is an always-on-top overlay rather than a fullscreen
        // window, the editor it becomes must stop hovering over everything.
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::Normal,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1320.0, 860.0)));
    }

    fn enter_edit(&mut self) {
        self.pan = egui::Vec2::ZERO;
        let (pv, sc) = make_preview(&self.shot_full, self.preview_budget);
        self.shot_preview = pv;
        self.preview_scale = sc;
        self.dirty = true;
        self.mode = Mode::Edit;
        self.status.clear();
        // The Auto swatch previews this image's palette, so it must be redrawn.
        self.swatches_dirty = true;
        self.detected_inset = crate::render::frame::border_color(&self.shot_full, 8).map(|c| c.0);
        // Annotations belong to the image they were drawn on.
        self.layers.clear();
        self.undo.clear();
        self.selected_layer = None;
        self.draft = None;
        self.move_delta = None;
        self.drag_anchor = None;
        self.ocr_mode = OcrMode::Off;
        self.reset_ocr();
        self.want_ocr = true;
    }

    // ------------------------------------------------------------ annotation

    pub(crate) fn undo_annotation(&mut self) {
        self.undo.undo(&mut self.layers);
        self.selected_layer = None;
        self.dirty = true;
    }

    pub(crate) fn redo_annotation(&mut self) {
        self.undo.redo(&mut self.layers);
        self.selected_layer = None;
        self.dirty = true;
    }

    pub(crate) fn delete_selected_layer(&mut self) {
        if let Some(i) = self.selected_layer
            && i < self.layers.len()
        {
            self.undo.push(&self.layers);
            self.layers.remove(i);
            self.selected_layer = None;
            self.dirty = true;
        }
    }

    /// Commit a finished drag. Returns false if the shape was a stray click.
    pub(crate) fn commit_draft(&mut self) -> bool {
        let Some(layer) = self.draft.take() else {
            return false;
        };
        if layer.is_degenerate() {
            return false;
        }
        self.undo.push(&self.layers);
        self.layers.push(layer);
        self.selected_layer = Some(self.layers.len() - 1);
        self.dirty = true;
        true
    }

    /// The colour a new layer of this tool should be drawn in. Paint carries
    /// its own opacity; everything else uses the picker's colour as chosen.
    pub(crate) fn ink(&self, tool: Tool) -> Rgba8 {
        let mut c = self.annot_color;
        if tool == Tool::Highlight {
            c[3] = self.annot_paint_alpha;
        }
        c
    }

    /// Change zoom and recentre. Jumping to a preset with the image still
    /// dragged off to one side would leave it half out of view.
    pub(crate) fn set_zoom(&mut self, zoom: Zoom) {
        self.zoom = zoom;
        self.pan = egui::Vec2::ZERO;
    }

    /// True while a label is being typed on the canvas.
    pub(crate) fn typing_text(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| d.kind == Tool::Text)
    }

    /// Commit the label being typed. An empty one is dropped rather than baked
    /// as an invisible layer you can never find again to delete.
    pub(crate) fn finish_text_edit(&mut self) {
        if !self.typing_text() {
            return;
        }
        self.commit_draft();
        self.draft = None;
        self.text_before = None;
        self.text_caret = 0;
        self.text_preedit.clear();
        self.status.clear();
    }

    /// Esc: restore what the label said before, or discard it if it is new.
    pub(crate) fn cancel_text_edit(&mut self) {
        if !self.typing_text() {
            return;
        }
        match self.text_before.take() {
            Some(original) => {
                if let Some(d) = self.draft.as_mut() {
                    d.text = original;
                }
                self.commit_draft();
            }
            None => self.dirty = true,
        }
        self.draft = None;
        self.text_caret = 0;
        self.text_preedit.clear();
        self.status.clear();
    }

    pub(crate) fn open_image(&mut self, path: &std::path::Path) {
        match image::open(path) {
            Ok(img) => {
                self.shot_full = img.to_rgba8();
                self.enter_edit();
                self.status = tf("Opened {path}", &[("path", &path.display().to_string())]);
            }
            Err(e) => self.status = format!("Could not open the image: {e}"),
        }
    }

    pub(crate) fn open_from_clipboard(&mut self) {
        let Some(cb) = self.clipboard.as_mut() else {
            self.status = t("Cannot reach the clipboard").into();
            return;
        };
        match cb.get_image() {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                match RgbaImage::from_raw(w, h, img.bytes.into_owned()) {
                    Some(rgba) => {
                        self.shot_full = rgba;
                        self.enter_edit();
                        self.status = tf("Pasted a {w}×{h} image from the clipboard", &[("w", &w.to_string()), ("h", &h.to_string())]);
                    }
                    None => self.status = t("The clipboard image is not valid").into(),
                }
            }
            Err(e) => self.status = tf("No image on the clipboard: {err}", &[("err", &e.to_string())]),
        }
    }

    // ----------------------------------------------------------------- render

    /// Load the wallpaper or custom image if the current background needs one.
    fn sync_bg_image(&mut self) {
        let wanted = self.style.background_image_path();
        if wanted == self.bg_image_key {
            return;
        }
        self.bg_image = wanted
            .as_ref()
            .and_then(|p| image::open(p).ok())
            .map(|i| i.to_rgba8());
        self.bg_image_key = wanted;
        self.dirty = true;
        self.swatches_dirty = true;
    }

    /// Load the watermark logo when its path changes. Same shape as the
    /// background image cache: decode once, keep it until the path moves.
    fn sync_wm_image(&mut self) {
        let wanted = self.style.watermark_image.clone();
        if wanted == self.wm_image_key {
            return;
        }
        self.wm_image = wanted
            .as_ref()
            .and_then(|p| image::open(p).ok())
            .map(|i| i.to_rgba8());
        self.wm_image_key = wanted;
        self.dirty = true;
    }

    pub(crate) fn scene<'a>(
        &'a self,
        shot: &'a RgbaImage,
        scale: f32,
        layers: &'a [Layer],
    ) -> Scene<'a> {
        Scene {
            shot,
            style: &self.style,
            scale,
            bg_image: self.bg_image.as_ref(),
            font: self.font.as_ref(),
            layers,
            logo: self.wm_image.as_ref(),
        }
    }

    fn rebuild_raw_texture(&mut self, ctx: &egui::Context) {
        // 1:1 in the fullscreen picker so it reads as the live desktop; the
        // downscaled copy is plenty for the windowed case.
        let source = if self.picking_fullscreen {
            &self.capture_full
        } else {
            &self.capture_preview
        };
        // `capture_full` is the one texture with no bound on it: the others go
        // through `make_preview` first. A stitched desktop can outgrow what the
        // GPU accepts, so it is checked here and nowhere else.
        let shrunk = fit_texture(source, ctx.input(|i| i.max_texture_side) as u32);
        let color = to_color_image(shrunk.as_ref().unwrap_or(source));
        match &mut self.raw_texture {
            Some(t) => t.set(color, egui::TextureOptions::LINEAR),
            None => {
                self.raw_texture =
                    Some(ctx.load_texture("raw", color, egui::TextureOptions::LINEAR))
            }
        }
    }

    /// Keep the preview bitmap sized in the display's *own* pixels.
    ///
    /// [`PREVIEW_MAX_W`] is a texel budget, and egui lays out in points: at two
    /// device pixels per point the canvas asks for twice as many texels as the
    /// budget allows, and the GPU magnifies the shortfall back up. The exported
    /// file was pixel-perfect the whole time — only what the editor showed went
    /// soft, which is why this reads as a rendering bug and is not one.
    ///
    /// Recomputed every frame rather than once, because dragging the window to
    /// a display with a different scale changes the answer.
    fn fit_preview_to_display(&mut self, ctx: &egui::Context) {
        let want = ((PREVIEW_MAX_W as f32 * ctx.pixels_per_point()).round() as u32).max(1);
        if want == self.preview_budget {
            return;
        }
        self.preview_budget = want;
        let (pv, sc) = make_preview(&self.shot_full, want);
        self.shot_preview = pv;
        self.preview_scale = sc;
        self.dirty = true;
    }

    fn rebuild_texture(&mut self, ctx: &egui::Context) {
        let layers = self.layers_except(self.detached_layer);
        let rendered =
            render_detailed(&self.scene(&self.shot_preview, self.preview_scale, &layers));
        self.preview_geom = rendered.geom;
        let out = rendered.image;
        let color = to_color_image(&out);
        match &mut self.texture {
            Some(t) => t.set(color, egui::TextureOptions::LINEAR),
            None => {
                self.texture =
                    Some(ctx.load_texture("preview", color, egui::TextureOptions::LINEAR))
            }
        }
    }

    // ---------------------------------------------------------------- actions

    pub(crate) fn full_render(&self) -> RgbaImage {
        let layers = self.all_layers();
        render(&self.scene(&self.shot_full, 1.0, &layers))
    }

    pub(crate) fn do_copy(&mut self) {
        let out = self.full_render();
        match export::copy(&out, &mut self.clipboard) {
            Ok(()) => self.status = t("Copied to the clipboard").into(),
            Err(e) => self.status = format!("Clipboard lỗi: {e}"),
        }
    }

    pub(crate) fn do_save(&mut self, path: Option<PathBuf>) {
        let path = path.unwrap_or_else(|| export::default_path(&self.prefs));
        let out = self.full_render();
        match export::save(&out, &path, &self.prefs) {
            Ok(()) => self.status = tf("Saved: {path}", &[("path", &path.display().to_string())]),
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Copy recognised text — either the current selection or everything.
    pub(crate) fn copy_text(&mut self, selection_only: bool) {
        let text = if selection_only {
            self.selected_text()
        } else {
            self.all_text()
        };
        if text.is_empty() {
            self.status = t("No text to copy").into();
            return;
        }
        let chars = text.chars().count();
        match self.clipboard.as_mut() {
            Some(cb) => match cb.set_text(text) {
                Ok(()) => self.status = tf("{chars} characters copied", &[("chars", &chars.to_string())]),
                Err(e) => self.status = format!("Clipboard lỗi: {e}"),
            },
            None => self.status = t("Cannot reach the clipboard").into(),
        }
    }

    /// Open the folder exports land in, with whatever the desktop uses.
    pub(crate) fn open_output_dir(&mut self) {
        let dir = self.prefs.save_dir().join("shotr");
        let _ = std::fs::create_dir_all(&dir);
        match std::process::Command::new("xdg-open").arg(&dir).spawn() {
            Ok(_) => self.status = tf("Opened {path}", &[("path", &dir.display().to_string())]),
            Err(e) => self.status = format!("Could not open the folder: {e}"),
        }
    }

    pub(crate) fn copy_and_close(&mut self, ctx: &egui::Context) {
        self.do_copy();
        self.style.save();
        self.prefs.save();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // ---------------------------------------------------------------- presets

    pub(crate) fn save_preset(&mut self) {
        let name = self.preset_name.trim().to_string();
        if name.is_empty() {
            self.status = t("Name the preset first").into();
            return;
        }
        let preset = Preset {
            name: name.clone(),
            style: self.style.clone(),
        };
        match self.presets.iter_mut().find(|p| p.name == name) {
            Some(existing) => *existing = preset,
            None => self.presets.push(preset),
        }
        crate::settings::save_presets(&self.presets);
        self.status = tf("Preset “{name}” saved", &[("name", &name)]);
    }

    pub(crate) fn delete_preset(&mut self, index: usize) {
        if index < self.presets.len() {
            let removed = self.presets.remove(index);
            crate::settings::save_presets(&self.presets);
            self.status = tf("Preset “{name}” deleted", &[("name", &removed.name)]);
        }
    }
}

impl eframe::App for ShotrApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // The selection has just been made in shotr's own picker, and `--copy`
        // asked for the clipboard rather than the editor. Leaving before the
        // editor paints is the whole point: the window never becomes visible.
        if self.copy_on_finish && self.mode == Mode::Edit {
            self.copy_on_finish = false;
            self.copy_and_close(&ctx);
            return;
        }

        if let Some(t) = self.pending_capture {
            ctx.request_repaint();
            if ctx.input(|i| i.time) >= t {
                self.pending_capture = None;
                self.do_capture();
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // Under `ThemeMode::System` the answer changes when the desktop does,
        // with no warning — so it is asked every frame rather than at startup.
        theme::sync(&ctx);

        // A message is news for a few seconds, then the status line goes back
        // to explaining the tool in hand.
        let now = ctx.input(|i| i.time);
        if self.status != self.status_shown {
            self.status_shown = self.status.clone();
            self.status_until = now + STATUS_SECONDS;
        }

        self.poll_ocr(&ctx);
        if self.want_ocr && self.mode == Mode::Edit {
            self.want_ocr = false;
            self.start_ocr(&ctx);
        }

        // Switching tools mid-sentence must not leave a caret stranded on the
        // image, so the label is committed first.
        if self.tool != Tool::Text && self.typing_text() {
            self.finish_text_edit();
        }
        self.text_edit_input(&ctx);
        self.handle_shortcuts(&ctx);

        if self.style != self.prev_style {
            if self.style.custom_bg != self.prev_style.custom_bg {
                self.swatches_dirty = true;
            }
            self.prev_style = self.style.clone();
            self.dirty = true;
            self.save_settings_at = Some(ctx.input(|i| i.time) + 1.0);
        }
        // Preferences repaint too: the redaction policy lives here and decides
        // what gets covered.
        if self.prefs != self.prev_prefs {
            if self.prefs.theme != self.prev_prefs.theme {
                theme::set_mode(&ctx, self.prefs.theme);
                // The card is painted from a texture, and that texture is the
                // old palette's gradient.
                self.sidebar_grad = None;
            }
            self.prev_prefs = self.prefs.clone();
            self.dirty = true;
            self.save_settings_at = Some(ctx.input(|i| i.time) + 1.0);
        }
        if let Some(t) = self.save_settings_at {
            if ctx.input(|i| i.time) >= t {
                self.save_settings_at = None;
                self.style.save();
                self.prefs.save();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }

        self.sync_bg_image();
        self.sync_wm_image();

        if self.raw_dirty {
            self.rebuild_raw_texture(&ctx);
            self.raw_dirty = false;
        }
        if self.swatches_dirty {
            self.rebuild_swatches(&ctx);
            self.swatches_dirty = false;
        }
        if self.mode == Mode::Edit {
            self.fit_preview_to_display(&ctx);
            if self.dirty {
                self.rebuild_texture(&ctx);
                self.dirty = false;
            }
        }

        // The picker covers the screen and has no chrome at all; everything else
        // is the editor window, which draws its own.
        if self.picking_fullscreen {
            egui::CentralPanel::default()
                .frame(theme::canvas_frame(true))
                .show_inside(ui, |ui| self.select_central(ui));
        } else {
            self.shell_ui(ui, &ctx);
        }

        // Selecting a region is what turns the fullscreen picker into the editor.
        if self.picking_fullscreen && self.mode == Mode::Edit {
            self.leave_fullscreen(&ctx);
        }
    }

    /// The editor window is transparent, so the frame it draws for itself can
    /// have rounded corners and a shadow that composite against the desktop.
    ///
    /// The fullscreen picker is the exception: it covers the screen with the
    /// shot, and anywhere the shot does not reach would otherwise be a hole
    /// straight through to the desktop being selected on.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.picking_fullscreen {
            visuals.panel_fill.to_normalized_gamma_f32()
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }
}

/// The editor labels every shortcut `Ctrl+…`, on all three platforms — the
/// bottom bar and the About list both do. egui's `command` is ⌘ on macOS and
/// Ctrl elsewhere, so matching it alone leaves the physical Ctrl key, the one
/// those labels name, doing nothing on a Mac: Ctrl+C copied nothing at all.
/// Accepting either makes the labels true without taking ⌘C away.
fn editor_modifier(m: &egui::Modifiers) -> bool {
    m.command || m.ctrl
}

/// What to *call* that modifier in a label, on the platform reading it.
///
/// Both keys work everywhere — see [`editor_modifier`] — but only one of them
/// is the one a person on this platform reaches for, and printing "Ctrl+C" on a
/// Mac tells them to use the key that is not under their thumb. Spelled "Cmd"
/// rather than "⌘": the same word [`crate::hotkey`] already writes into
/// prefs.json, and no glyph to go missing from a system font.
pub(crate) const MOD_LABEL: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

/// True if the frame asked for a copy, however the platform spelled it.
///
/// egui translates the system's copy chord into [`egui::Event::Copy`] and, on
/// macOS, that is *all* it delivers: ⌘C arrives with no key press at all, only a
/// release once the chord is over. Watching for the key alone therefore left ⌘C
/// — the combination every Mac user reaches for first — doing nothing.
fn copy_requested(events: &[egui::Event]) -> bool {
    shortcut(events, egui::Key::C, None) || events.iter().any(|e| matches!(e, egui::Event::Copy))
}

/// True if `key` went down this frame with the editor's modifier held, and with
/// shift in the state asked for — `None` when shift does not matter.
///
/// The modifiers must be read off the *event*, never off `InputState::modifiers`.
/// That field is the state left at the end of the frame, and a quick tap delivers
/// the press and the release together: by the time the frame is read the modifier
/// has already been let go, the frame reports `Modifiers::NONE`, and the shortcut
/// silently never fires. Measured — a release build coalesced Ctrl+C into one
/// frame reporting NONE while a debug build, slow enough to split it over two
/// frames, worked. That difference is what made this look like a signing or
/// install problem for an afternoon.
fn shortcut(events: &[egui::Event], key: egui::Key, shift: Option<bool>) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            egui::Event::Key { key: k, pressed: true, modifiers, .. }
                if *k == key
                    && editor_modifier(modifiers)
                    && shift.is_none_or(|want| modifiers.shift == want)
        )
    })
}

/// True if `key` went down this frame with **no** modifier held.
///
/// Tool keys have to insist on that. `⌘1` is "back to 100%" and `⌘0` is "fit to
/// the window"; a digit handler that ignored modifiers would change the tool as
/// well, every time someone zoomed.
fn plain_key(events: &[egui::Event], key: egui::Key) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            egui::Event::Key { key: k, pressed: true, modifiers, .. }
                if *k == key && modifiers.is_none()
        )
    })
}

/// The key that types `label`. The pill prints these on its buttons, so the two
/// must agree; there is a test that walks the whole list.
fn tool_key(label: char) -> Option<egui::Key> {
    Some(match label {
        '1' => egui::Key::Num1,
        '2' => egui::Key::Num2,
        '3' => egui::Key::Num3,
        '4' => egui::Key::Num4,
        '5' => egui::Key::Num5,
        '6' => egui::Key::Num6,
        // Left of `1`, and the tool reached most often.
        '`' => egui::Key::Backtick,
        _ => return None,
    })
}

impl ShotrApp {
    /// The tool keys, and Esc to go back to Select.
    ///
    /// Only reached with the editor up and nothing being typed — the caller
    /// guards both — because a bare digit is a character before it is a
    /// shortcut, and a label being written must get it.
    fn tool_keys(&mut self, ctx: &egui::Context) {
        let (picked, escape) = ctx.input(|i| {
            let picked = shell::TOOLS.iter().find(|(_, key)| {
                tool_key(*key).is_some_and(|k| plain_key(&i.events, k))
            });
            (picked.map(|(tool, _)| *tool), plain_key(&i.events, egui::Key::Escape))
        });
        if let Some(tool) = picked {
            self.tool = tool;
        }
        if escape {
            self.tool = Tool::Select;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (copy, save, space, undo, redo, delete) = ctx.input(|i| {
            (
                copy_requested(&i.events),
                shortcut(&i.events, egui::Key::S, None),
                i.key_pressed(egui::Key::Space),
                shortcut(&i.events, egui::Key::Z, Some(false)),
                shortcut(&i.events, egui::Key::Z, Some(true)),
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            )
        });

        // Zoom keys, only while the editor is up and nothing is being typed.
        if self.mode == Mode::Edit && !self.typing_text() {
            let (zoom_in, zoom_out, fit, actual) = ctx.input(|i| {
                (
                    shortcut(&i.events, egui::Key::Plus, None)
                        || shortcut(&i.events, egui::Key::Equals, None),
                    shortcut(&i.events, egui::Key::Minus, None),
                    shortcut(&i.events, egui::Key::Num0, None),
                    shortcut(&i.events, egui::Key::Num1, None),
                )
            });
            if zoom_in {
                self.zoom = self.zoom.stepped(self.shown_zoom, true);
            }
            if zoom_out {
                self.zoom = self.zoom.stepped(self.shown_zoom, false);
            }
            if fit {
                self.set_zoom(Zoom::Fit);
            }
            if actual {
                self.set_zoom(Zoom::Percent(100));
            }
            self.tool_keys(ctx);
        }
        match self.mode {
            Mode::Edit => {
                // The shot is on the clipboard, so the editor has nothing left
                // to say: closing here is what lets the paste happen in the
                // window the user was already working in. Typing a label is the
                // exception — there Ctrl+C belongs to the text field.
                if copy && !self.typing_text() {
                    self.copy_and_close(ctx);
                }
                if save {
                    self.do_save(None);
                }
                // Undo while typing would rewind the layer stack out from under
                // the label being written, so it waits until the edit is done.
                if undo && !self.typing_text() {
                    self.undo_annotation();
                }
                if redo && !self.typing_text() {
                    self.redo_annotation();
                }
                // Only while a shape is selected, so typing into the text field
                // never deletes the layer being typed into.
                if delete && self.selected_layer.is_some() && self.tool == Tool::Select {
                    self.delete_selected_layer();
                }
            }
            Mode::Select => {
                if self.picking_fullscreen {
                    let (escape, enter) = ctx.input(|i| {
                        (
                            i.key_pressed(egui::Key::Escape),
                            i.key_pressed(egui::Key::Enter),
                        )
                    });
                    if escape {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if enter {
                        self.finish_selection(false);
                    }
                }
                // Nothing to switch to when no window could be listed.
                if space && !self.windows.is_empty() {
                    self.pick_mode = match self.pick_mode {
                        PickMode::Region => PickMode::Window,
                        PickMode::Window => PickMode::Region,
                    };
                    self.sel_rect = None;
                    self.crop_px = None;
                }
            }
        }
    }
}

// -------------------------------------------------------------------- helpers

/// Cut one monitor out of the desktop snapshot. `None` asks for all of it.
///
/// A rectangle that does not lie wholly inside the image is ignored rather than
/// clamped. `crop_imm` clamps, and clamping an origin that is already past the
/// right edge leaves a **0×0** image — which cannot be uploaded as a texture and
/// takes the process with it. That is not hypothetical: the editor used to start
/// without a desktop snapshot at all, so every monitor rectangle missed. Falling
/// back to the whole desktop is the rule an unknown monitor index already
/// followed, and it shows something rather than nothing.
fn cut_monitor(desktop: &RgbaImage, rect: Option<[u32; 4]>) -> RgbaImage {
    match rect {
        Some([x, y, w, h])
            if w > 0
                && h > 0
                && x.saturating_add(w) <= desktop.width()
                && y.saturating_add(h) <= desktop.height() =>
        {
            image::imageops::crop_imm(desktop, x, y, w, h).to_image()
        }
        _ => desktop.clone(),
    }
}

/// Shrink `img` until neither side exceeds `limit`. `None` when it already
/// fits, so the ordinary case copies nothing.
///
/// The ceiling is the GPU's, not egui's, and egui panics rather than refusing:
/// three monitors stitched side by side came to 8616px against a limit of 8192
/// and took the process down with `abort()`. Only the picture shrinks — the
/// selection is measured against `capture_full`, so a crop stays exact however
/// much this scales.
fn fit_texture(img: &RgbaImage, limit: u32) -> Option<RgbaImage> {
    let longest = img.width().max(img.height());
    if longest <= limit {
        return None;
    }
    let scale = limit as f32 / longest as f32;
    let fit = |side: u32| (((side as f32 * scale).round() as u32).max(1)).min(limit);
    Some(image::imageops::resize(
        img,
        fit(img.width()),
        fit(img.height()),
        image::imageops::FilterType::Triangle,
    ))
}

/// The icon every shotr window carries.
///
/// eframe sets the application icon at runtime from `ViewportBuilder`, and its
/// default is eframe's own logo — which silently overrides the `.icns` in the
/// bundle, so the Dock showed a black hexagon. Drawing it here keeps the Dock on
/// the same source as the tray, the launcher and the `.icns`: `render::icon`.
pub fn window_icon() -> egui::IconData {
    const PX: u32 = 256;
    let image = crate::render::icon::icon_image(PX);
    egui::IconData {
        rgba: image.into_raw(),
        width: PX,
        height: PX,
    }
}

pub(crate) fn to_color_image(img: &RgbaImage) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    )
}

#[cfg(test)]
mod shortcut_modifier_tests {
    use super::editor_modifier;
    use eframe::egui::Modifiers;

    /// Measured on macOS: Ctrl+C in the editor did nothing, because egui sets
    /// `command` for ⌘ there and the check looked at `command` alone — while
    /// the button beside it reads "Copy  Ctrl+C".
    #[test]
    fn the_ctrl_key_works_where_command_means_cmd() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert!(
            editor_modifier(&ctrl),
            "Ctrl+C would copy nothing on macOS, though every label promises it"
        );
    }

    /// The Mac key people actually reach for has to keep working.
    #[test]
    fn the_command_key_still_works() {
        let cmd = Modifiers {
            command: true,
            mac_cmd: true,
            ..Default::default()
        };
        assert!(editor_modifier(&cmd), "Cmd+C stopped copying");
    }

    /// A label has to name the key the reader will actually press. Both work,
    /// but "Ctrl+C" on a Mac points at the key that is not under their thumb —
    /// reported as the Copy and Save buttons being wrong.
    #[test]
    fn the_label_names_this_platforms_modifier() {
        let want = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        assert_eq!(
            super::MOD_LABEL,
            want,
            "the buttons would tell a user to press the wrong key"
        );
    }

    /// Otherwise a bare C, typed into anything, would fire every shortcut.
    #[test]
    fn no_modifier_is_not_a_shortcut() {
        assert!(
            !editor_modifier(&Modifiers::default()),
            "an unmodified keypress triggered a shortcut"
        );
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::shortcut;
    use eframe::egui::{Event, Key, Modifiers};

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    const CTRL: Modifiers = Modifiers {
        alt: false,
        ctrl: true,
        shift: false,
        mac_cmd: false,
        command: false,
    };

    /// The bug this whole function exists for. A quick tap delivers the press
    /// and the release in one frame, so the frame's own modifier state is back
    /// to nothing — reading that instead of the event's made Ctrl+C do nothing
    /// at all in a release build, while a slower debug build worked.
    #[test]
    fn a_press_and_release_in_one_frame_still_fires() {
        let events = vec![
            press(Key::C, CTRL),
            Event::Key {
                key: Key::C,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: CTRL,
            },
        ];
        assert!(
            shortcut(&events, Key::C, None),
            "Ctrl+C copied nothing whenever the tap was quick enough"
        );
    }

    #[test]
    fn a_key_without_the_modifier_is_not_a_shortcut() {
        let events = vec![press(Key::C, Modifiers::default())];
        assert!(
            !shortcut(&events, Key::C, None),
            "typing a bare C would copy and close the editor"
        );
    }

    /// Undo asks for shift off and redo for shift on, off the same key.
    #[test]
    fn shift_tells_undo_and_redo_apart() {
        let shifted = Modifiers { shift: true, ..CTRL };
        let events = vec![press(Key::Z, shifted)];
        assert!(
            shortcut(&events, Key::Z, Some(true)),
            "Ctrl+Shift+Z stopped redoing"
        );
        assert!(
            !shortcut(&events, Key::Z, Some(false)),
            "Ctrl+Shift+Z undid instead of redoing"
        );
    }

    #[test]
    fn another_key_does_not_match() {
        let events = vec![press(Key::S, CTRL)];
        assert!(!shortcut(&events, Key::C, None), "Ctrl+S triggered the copy");
    }

    /// Measured on macOS: ⌘C reaches the app as `Event::Copy` and nothing else —
    /// no key press at all — so watching for the key alone missed it entirely.
    #[test]
    fn the_platform_copy_event_counts_as_the_shortcut() {
        assert!(
            super::copy_requested(&[Event::Copy]),
            "Cmd+C did nothing on macOS, where it arrives as no key press at all"
        );
    }

    #[test]
    fn an_ordinary_frame_asks_for_no_copy() {
        assert!(
            !super::copy_requested(&[press(Key::S, CTRL)]),
            "Ctrl+S copied and closed the editor"
        );
    }

    /// The label printed on a tool button is a promise that the key works. If
    /// the two drift, the button says "3" and pressing 3 does nothing — a
    /// failure that produces no error anywhere.
    #[test]
    fn every_label_printed_on_the_pill_maps_to_a_real_key() {
        for (tool, label) in super::shell::TOOLS {
            assert!(
                super::tool_key(label).is_some(),
                "{tool:?} prints {label:?} on its button but no key produces it"
            );
        }
    }

    /// A tool key has to be unmodified. `⌘1` is "back to 100%" and `⌘0` is
    /// "fit to the window", so a handler that tolerated modifiers would change
    /// the tool every time someone zoomed.
    #[test]
    fn a_modified_key_does_not_pick_a_tool() {
        let key = super::tool_key('1').expect("1 is the Arrow tool's key");
        assert!(
            !super::plain_key(&[press(key, CTRL)], key),
            "Ctrl+1 would switch tool while zooming back to 100%"
        );
        assert!(
            super::plain_key(&[press(key, Modifiers::NONE)], key),
            "a bare 1 stopped picking the Arrow tool"
        );
    }
}

#[cfg(test)]
mod monitor_cut_tests {
    use super::cut_monitor;
    use image::RgbaImage;

    #[test]
    fn a_monitor_inside_the_snapshot_is_cut_out_of_it() {
        let desktop = RgbaImage::new(8616, 4320);
        let one = cut_monitor(&desktop, Some([5160, 0, 3456, 2234]));
        assert_eq!(
            (one.width(), one.height()),
            (3456, 2234),
            "the picker would show the wrong screen"
        );
    }

    #[test]
    fn a_rectangle_that_misses_the_snapshot_yields_the_whole_desktop() {
        // The editor opened with a 640x360 placeholder in place of the desktop,
        // so picking the second monitor cropped at x=5160 inside it. `crop_imm`
        // clamps rather than refusing, which made that a 0x0 image, and a
        // zero-sized texture ends the process.
        let placeholder = RgbaImage::new(640, 360);
        let out = cut_monitor(&placeholder, Some([5160, 0, 3456, 2234]));
        assert_eq!(
            (out.width(), out.height()),
            (640, 360),
            "a zero-sized crop reached the GPU and killed the editor"
        );
    }

    #[test]
    fn a_rectangle_hanging_over_the_edge_yields_the_whole_desktop() {
        // Half in bounds is still not a screen, and clamping it would show a
        // sliver of the wrong one.
        let desktop = RgbaImage::new(1920, 1080);
        let out = cut_monitor(&desktop, Some([1600, 0, 1920, 1080]));
        assert_eq!(
            (out.width(), out.height()),
            (1920, 1080),
            "a clamped crop passes a rectangle the caller never asked for"
        );
    }

    #[test]
    fn no_rectangle_means_every_monitor() {
        let desktop = RgbaImage::new(1920, 1080);
        let out = cut_monitor(&desktop, None);
        assert_eq!(
            (out.width(), out.height()),
            (1920, 1080),
            "'All monitors combined' must not crop"
        );
    }
}

#[cfg(test)]
mod texture_fit_tests {
    use super::fit_texture;
    use image::RgbaImage;

    #[test]
    fn an_image_within_the_limit_is_left_alone() {
        let img = RgbaImage::new(1920, 1080);
        assert!(
            fit_texture(&img, 8192).is_none(),
            "copying an image that already fits would waste a frame's worth of work"
        );
    }

    #[test]
    fn the_three_monitor_desktop_that_crashed_now_fits() {
        // 5160x2160 + 3456x2234 + 3840x2160 stitched, against a 8192 GPU limit.
        // egui does not clamp this, it panics and aborts the process.
        let img = RgbaImage::new(8616, 4320);
        let fitted = fit_texture(&img, 8192).expect("8616 is over the limit and must be shrunk");
        assert!(
            fitted.width() <= 8192 && fitted.height() <= 8192,
            "still {}x{}, which is what took the process down",
            fitted.width(),
            fitted.height()
        );
        // 8616:4320 is 1.9944; a shifted aspect would misplace the selection
        // rectangle drawn over the shot.
        let before = 8616.0 / 4320.0;
        let after = fitted.width() as f32 / fitted.height() as f32;
        assert!(
            (before - after).abs() < 0.01,
            "aspect drifted from {before} to {after}, so the picker would be stretched"
        );
    }

    #[test]
    fn a_tall_desktop_is_bounded_too() {
        // Monitors stacked vertically hit the same wall on the other axis.
        let fitted = fit_texture(&RgbaImage::new(2000, 9000), 8192)
            .expect("9000 is over the limit and must be shrunk");
        assert!(
            fitted.height() <= 8192,
            "height {} still exceeds the limit",
            fitted.height()
        );
    }
}

#[cfg(test)]
mod source_tests {
    use super::Source;

    /// The picker re-shoots by relaunching, so these arguments are the only
    /// thing carrying the choice across. Getting them wrong silently falls back
    /// to capturing everything.
    #[test]
    fn a_source_round_trips_through_command_line_arguments() {
        assert_eq!(Source::All.args(), vec!["--capture"]);
        assert_eq!(
            Source::Monitor(1).args(),
            vec!["--capture", "--monitor", "1"]
        );
        assert_eq!(
            Source::Monitor(0).args(),
            vec!["--capture", "--monitor", "0"],
            "monitor 0 must still be explicit, not left to the default"
        );
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::Zoom;

    /// A wheel notch from "Fit" has to continue from whatever Fit currently
    /// works out to. Starting from a fixed 100% would make the first notch jump
    /// the image, which is the classic annoyance in image viewers.
    #[test]
    fn zooming_from_fit_continues_from_what_is_on_screen() {
        assert_eq!(Zoom::Fit.stepped(33, true), Zoom::Percent(50));
        assert_eq!(Zoom::Fit.stepped(33, false), Zoom::Percent(25));
        assert_eq!(Zoom::Fit.scaled(60, 2.0), Zoom::Percent(120));
    }

    #[test]
    fn stepping_walks_the_ladder_in_both_directions() {
        let mut z = Zoom::Percent(100);
        for expect in [150, 200, 300, 400, 800] {
            z = z.stepped(100, true);
            assert_eq!(z, Zoom::Percent(expect));
        }
        for expect in [400, 300, 200, 150, 100, 75, 50, 33, 25] {
            z = z.stepped(100, false);
            assert_eq!(z, Zoom::Percent(expect));
        }
    }

    /// Both ends have to stop rather than wrap or run away.
    #[test]
    fn stepping_past_either_end_stays_put() {
        let bottom = Zoom::Percent(25).stepped(100, false);
        assert_eq!(bottom, Zoom::Percent(25), "nothing below the first step");
        let top = Zoom::Percent(800).stepped(100, true);
        assert_eq!(top, Zoom::Percent(800), "nothing above the last step");
    }

    /// Free scaling must clamp, or a fast scroll can drive the zoom to zero
    /// (a zero-size image, invisible and impossible to recover from) or to
    /// something that allocates absurd amounts.
    #[test]
    fn free_scaling_is_clamped_at_both_ends() {
        let tiny = Zoom::Percent(20).scaled(100, 0.001);
        assert_eq!(tiny, Zoom::Percent(Zoom::MIN));
        let huge = Zoom::Percent(1000).scaled(100, 1000.0);
        assert_eq!(huge, Zoom::Percent(Zoom::MAX));
        // And never zero, whatever the multiplier.
        for by in [0.0_f32, 0.0001, 1e-9] {
            match Zoom::Percent(100).scaled(100, by) {
                Zoom::Percent(p) => assert!(p >= Zoom::MIN, "got {p}%"),
                Zoom::Fit => panic!("scaling should always produce a percentage"),
            }
        }
    }

    /// One notch should feel the same at any magnification — that is why the
    /// step is a multiplier and not an addition.
    #[test]
    fn a_wheel_notch_is_proportional_not_fixed() {
        let low = match Zoom::Percent(50).scaled(100, 1.2) {
            Zoom::Percent(p) => p - 50,
            _ => unreachable!(),
        };
        let high = match Zoom::Percent(400).scaled(100, 1.2) {
            Zoom::Percent(p) => p - 400,
            _ => unreachable!(),
        };
        assert!(
            high > low * 4,
            "the same notch moved {low}% at 50% and only {high}% at 400%"
        );
    }
}
