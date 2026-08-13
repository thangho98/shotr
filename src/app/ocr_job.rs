//! Background OCR and the redaction layers derived from it.
//!
//! Recognition takes a few hundred milliseconds on a full-screen capture, so it
//! runs on a worker thread and reports back through a channel. Redaction boxes
//! are *derived* every render rather than stored: re-running detection must
//! never leave stale boxes behind, and they should not consume undo steps.

use crate::i18n::t;

use eframe::egui;
use std::sync::mpsc::{Receiver, channel};

use super::ShotrApp;
use crate::annotate::{Layer, Tool};
use crate::ocr::{self, Word, detect::Finding, detect::Secret};
use crate::settings::RedactStyle;

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum OcrState {
    /// Models have not been downloaded yet.
    Absent,
    Downloading,
    Reading,
    Ready,
    Failed(String),
}

pub(crate) enum Outcome {
    Downloaded,
    Words(Vec<Word>),
    Failed(String),
}

/// The pure-Rust fallback engine.
fn run_ocrs(shot: &image::RgbaImage) -> Outcome {
    match ocr::Engine::load() {
        Ok(engine) => match engine.read(shot) {
            Ok(words) => Outcome::Words(words),
            Err(e) => Outcome::Failed(e),
        },
        Err(e) => Outcome::Failed(e),
    }
}

impl ShotrApp {
    /// Kick off recognition for the current screenshot.
    pub(crate) fn start_ocr(&mut self, ctx: &egui::Context) {
        if matches!(self.ocr_state, OcrState::Reading | OcrState::Downloading) {
            return;
        }
        // Tesseract first when it has a language pack: `ocrs` cannot produce
        // Vietnamese at all, so on Vietnamese text it is not a lesser result but
        // a wrong one.
        let langs = ocr::tesseract::best_langs();
        if langs.is_none() && !ocr::models_present() {
            self.ocr_state = OcrState::Absent;
            return;
        }
        self.ocr_state = OcrState::Reading;
        self.ocr_rx = Some(spawn(ctx.clone(), {
            let shot = self.shot_full.clone();
            move || match langs {
                Some(langs) => match ocr::tesseract::read(&shot, &langs) {
                    Ok(words) => Outcome::Words(words),
                    // Falling back keeps a missing language pack or a broken
                    // install from turning into "OCR is dead".
                    Err(_) if ocr::models_present() => run_ocrs(&shot),
                    Err(e) => Outcome::Failed(e),
                },
                None => run_ocrs(&shot),
            }
        }));
    }

    pub(crate) fn start_model_download(&mut self, ctx: &egui::Context) {
        if self.ocr_state == OcrState::Downloading {
            return;
        }
        self.ocr_state = OcrState::Downloading;
        self.ocr_rx = Some(spawn(ctx.clone(), || {
            match ocr::download_models(|_, _| {}) {
                Ok(()) => Outcome::Downloaded,
                Err(e) => Outcome::Failed(e),
            }
        }));
    }

    /// Drain the worker channel. Called once per frame.
    pub(crate) fn poll_ocr(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.ocr_rx else { return };
        let Ok(outcome) = rx.try_recv() else { return };
        self.ocr_rx = None;

        match outcome {
            Outcome::Downloaded => {
                self.status = t("OCR model downloaded").into();
                self.ocr_state = OcrState::Ready;
                self.start_ocr(ctx);
            }
            Outcome::Words(words) => {
                self.ocr_findings = crate::ocr::detect::scan(&words);
                self.ocr_findings
                    .extend(crate::ocr::detect::scan_phones(&words));
                self.ocr_words = words;
                self.ocr_state = OcrState::Ready;
                self.sync_ocr_mode();
                self.dirty = true;
            }
            Outcome::Failed(e) => {
                self.ocr_state = OcrState::Failed(e.clone());
                self.status = format!("OCR failed: {e}");
            }
        }
    }

    pub(crate) fn reset_ocr(&mut self) {
        self.ocr_rx = None;
        self.ocr_words.clear();
        self.ocr_findings.clear();
        self.manual_redact.clear();
        self.selected_words.clear();
        self.ocr_state = if ocr::models_present() {
            OcrState::Ready
        } else {
            OcrState::Absent
        };
    }

    fn kind_enabled(&self, kind: Secret) -> bool {
        let policy = &self.prefs;
        match kind {
            Secret::Email => policy.redact_email,
            Secret::CreditCard => policy.redact_card,
            Secret::IpAddress => policy.redact_ip,
            Secret::ApiKey => policy.redact_key,
            Secret::Phone => policy.redact_phone,
        }
    }

    /// How many findings the current toggles would actually cover.
    pub(crate) fn active_finding_count(&self) -> usize {
        self.ocr_findings
            .iter()
            .filter(|f| self.kind_enabled(f.kind))
            .count()
    }

    pub(crate) fn count_of(&self, kind: Secret) -> usize {
        self.ocr_findings.iter().filter(|f| f.kind == kind).count()
    }

    /// Redaction boxes followed by the user's own annotation layers, which is
    /// the order they must be drawn in — an arrow should sit on top of a box.
    pub(crate) fn all_layers(&self) -> Vec<Layer> {
        self.layers_except(None)
    }

    /// The same, minus one annotation.
    ///
    /// A shape being dragged is drawn as a vector at the pointer instead, so
    /// the preview bitmap must not also carry it at the place it started —
    /// otherwise the move shows a ghost sitting still behind the one that is
    /// following the mouse. The export never skips anything.
    pub(crate) fn layers_except(&self, skip: Option<usize>) -> Vec<Layer> {
        let mut out = self.redaction_layers();
        out.extend(annotations_except(&self.layers, skip));
        out
    }

    /// What the redaction policy and the user's own picks cover, with nothing
    /// drawn on top.
    ///
    /// Separate because these are the one thing that survives "copy the shot as
    /// captured": they exist to keep something off a screen, so an image that
    /// drops them is the one image the person who turned redaction on asked not
    /// to have. The same rule already keeps `--copy` out of the windowless path.
    pub(crate) fn redaction_layers(&self) -> Vec<Layer> {
        let mut out = Vec::new();
        let bands = line_bands(&self.ocr_words);

        if self.prefs.redact {
            for finding in &self.ocr_findings {
                if self.kind_enabled(finding.kind)
                    && let Some(rect) = self.finding_rect(finding, &bands)
                {
                    out.push(self.redaction_layer(rect));
                }
            }
        }
        for &i in &self.manual_redact {
            if let Some(word) = self.ocr_words.get(i) {
                out.push(self.redaction_layer(pad_rect(banded(word.rect, bands.get(i)))));
            }
        }
        out
    }

    pub(crate) fn finding_rect(&self, finding: &Finding, bands: &[[f32; 2]]) -> Option<[f32; 4]> {
        let mut out: Option<[f32; 4]> = None;
        for &i in &finding.words {
            let Some(word) = self.ocr_words.get(i) else {
                continue;
            };
            let r = banded(word.rect, bands.get(i));
            out = Some(match out {
                None => r,
                Some(a) => [
                    a[0].min(r[0]),
                    a[1].min(r[1]),
                    a[2].max(r[2]),
                    a[3].max(r[3]),
                ],
            });
        }
        Some(pad_rect(out?))
    }

    fn redaction_layer(&self, rect: [f32; 4]) -> Layer {
        let look = &self.style;
        let kind = match look.redact_style {
            RedactStyle::Solid => Tool::Fill,
            RedactStyle::Blur => Tool::Blur,
        };
        let mut layer = Layer::new(
            kind,
            [rect[0], rect[1]],
            look.redact_color,
            1.0,
            12.0,
            look.redact_blur,
        );
        layer.b = [rect[2], rect[3]];
        layer
    }

    /// Every recognised word, joined in reading order.
    pub(crate) fn all_text(&self) -> String {
        join_words(
            &self.ocr_words,
            &(0..self.ocr_words.len()).collect::<Vec<_>>(),
        )
    }

    pub(crate) fn selected_text(&self) -> String {
        join_words(&self.ocr_words, &self.selected_words)
    }
}

/// The given words grouped into text lines, each line ordered left to right.
///
/// Shared by the text join and the redaction bands so the two can never
/// disagree about where a line ends.
fn lines_of(words: &[Word], indices: &[usize]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| *i < words.len())
        .collect();
    order.sort_by(|a, b| {
        let (a, b) = (&words[*a], &words[*b]);
        a.rect[1]
            .partial_cmp(&b.rect[1])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.rect[0]
                    .partial_cmp(&b.rect[0])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut top = 0.0_f32;
    let mut height = 1.0_f32;
    for i in order {
        let w = &words[i];
        // More than half a line's worth of vertical drift starts a new line.
        let same = !out.is_empty() && (w.rect[1] - top).abs() <= height * 0.6;
        if same {
            out.last_mut().expect("checked non-empty").push(i);
        } else {
            top = w.rect[1];
            height = w.height().max(1.0);
            out.push(vec![i]);
        }
    }
    out
}

/// Top and bottom of the text line each word sits on, indexed like `words`.
///
/// A recognised box hugs the ink, not the line, so on one line `393` (digits),
/// `measured` (an ascender) and `worker` (neither) all come back different
/// heights — and `pad_rect` then scales its padding by that height and
/// multiplies the difference. Redaction bars built straight off those boxes come
/// out visibly ragged.
///
/// Vietnamese makes this worse rather than causing it: a tone mark raises the
/// top of some words on a line and not others, so `Việt` and `nam` differ where
/// `Viet` and `nam` would not. Giving every word on a line the line's own extent
/// is what makes the bars even.
fn line_bands(words: &[Word]) -> Vec<[f32; 2]> {
    let mut bands: Vec<[f32; 2]> = words.iter().map(|w| [w.rect[1], w.rect[3]]).collect();
    let all: Vec<usize> = (0..words.len()).collect();
    for line in lines_of(words, &all) {
        let top = line
            .iter()
            .map(|i| words[*i].rect[1])
            .fold(f32::INFINITY, f32::min);
        let bottom = line
            .iter()
            .map(|i| words[*i].rect[3])
            .fold(f32::NEG_INFINITY, f32::max);
        for i in line {
            bands[i] = [top, bottom];
        }
    }
    bands
}

/// A word's box with its top and bottom taken from the line it sits on.
fn banded(rect: [f32; 4], band: Option<&[f32; 2]>) -> [f32; 4] {
    match band {
        Some([top, bottom]) => [rect[0], *top, rect[2], *bottom],
        None => rect,
    }
}

/// Rebuild reading order: sort by line, then by x, inserting newlines between
/// lines so pasted text keeps its shape.
fn join_words(words: &[Word], indices: &[usize]) -> String {
    lines_of(words, indices)
        .iter()
        .map(|line| {
            line.iter()
                .map(|i| words[*i].text.trim())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A box that hugs the glyphs exactly still leaves legible edges, so grow it.
fn pad_rect(r: [f32; 4]) -> [f32; 4] {
    let pad = ((r[3] - r[1]) * 0.18).max(2.0);
    [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad]
}

fn spawn(ctx: egui::Context, job: impl FnOnce() -> Outcome + Send + 'static) -> Receiver<Outcome> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let outcome = job();
        let _ = tx.send(outcome);
        ctx.request_repaint();
    });
    rx
}

/// The user's annotations with one left out.
///
/// `skip` indexes the *annotation* list, not the combined output: redaction
/// boxes are prepended by the caller, so filtering the finished vector by the
/// same number would drop a redaction and leave the dragged shape behind.
fn annotations_except(layers: &[Layer], skip: Option<usize>) -> Vec<Layer> {
    layers
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .map(|(_, l)| l.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, x: f32, y: f32) -> Word {
        Word {
            text: text.into(),
            rect: [x, y, x + 40.0, y + 12.0],
        }
    }

    fn shape(x: f32) -> Layer {
        let mut l = Layer::new(Tool::Rect, [x, 0.0], [255, 0, 0, 255], 2.0, 20.0, 8.0);
        l.b = [x + 10.0, 10.0];
        l
    }

    /// While a shape is dragged it is drawn as a vector at the pointer, so the
    /// preview bitmap has to leave it out — otherwise a ghost sits still at the
    /// old spot behind the one following the mouse.
    #[test]
    fn the_dragged_annotation_is_the_only_one_left_out() {
        let layers = vec![shape(0.0), shape(100.0), shape(200.0)];
        let kept = annotations_except(&layers, Some(1));
        assert_eq!(kept.len(), 2, "exactly one annotation should be missing");
        assert_eq!(kept[0].a[0], 0.0);
        assert_eq!(kept[1].a[0], 200.0, "the wrong annotation was dropped");

        assert_eq!(
            annotations_except(&layers, None).len(),
            3,
            "nothing may be dropped when nothing is being dragged — this is the \
             path the export takes"
        );
    }

    /// `skip` counts annotations, not the finished list.
    ///
    /// Redaction boxes are prepended, so filtering the combined vector by the
    /// same number would remove a redaction and leave the dragged shape in.
    #[test]
    fn the_skip_index_is_not_confused_by_redaction_boxes() {
        let layers = vec![shape(0.0), shape(100.0)];
        let kept = annotations_except(&layers, Some(0));
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0].a[0], 100.0,
            "index 0 must mean the first annotation, whatever is prepended later"
        );
    }

    #[test]
    fn padding_grows_the_box_on_every_side() {
        let out = pad_rect([100.0, 100.0, 200.0, 120.0]);
        assert!(out[0] < 100.0 && out[1] < 100.0);
        assert!(out[2] > 200.0 && out[3] > 120.0);
    }

    #[test]
    fn tiny_text_still_gets_a_minimum_pad() {
        let out = pad_rect([10.0, 10.0, 20.0, 11.0]);
        assert!(
            out[0] <= 8.0,
            "expected at least 2px of padding, got {out:?}"
        );
    }

    /// A recognised box hugs the ink, so words on one line come back at
    /// different heights — worst with Vietnamese, where a tone mark raises the
    /// top of some words and not others. Redaction bars must not inherit that.
    #[test]
    fn every_word_on_a_line_redacts_to_the_same_height() {
        // One line: a tall word with a tone mark, a short one with neither
        // ascender nor descender, and a digit group.
        let words = vec![
            Word {
                text: "Việt".into(),
                rect: [0.0, 90.0, 40.0, 120.0],
            },
            Word {
                text: "nam".into(),
                rect: [50.0, 100.0, 90.0, 118.0],
            },
            Word {
                text: "393".into(),
                rect: [100.0, 96.0, 140.0, 120.0],
            },
        ];
        let bands = line_bands(&words);
        assert_eq!(
            bands[0], bands[1],
            "a tone mark must not make one word's bar taller than its neighbour's"
        );
        assert_eq!(bands[1], bands[2], "digits must share the line's band too");
        assert_eq!(
            bands[0],
            [90.0, 120.0],
            "the band must be the union of the line, not any one word"
        );

        // And the padded boxes therefore come out the same height.
        let heights: Vec<f32> = (0..3)
            .map(|i| {
                let r = pad_rect(banded(words[i].rect, bands.get(i)));
                r[3] - r[1]
            })
            .collect();
        assert!(
            heights.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
            "padded redaction boxes differ in height: {heights:?}"
        );
    }

    /// Words on genuinely different lines must keep their own bands, or one
    /// redaction would grow to cover the paragraph.
    #[test]
    fn separate_lines_keep_separate_bands() {
        let words = vec![w("first", 0.0, 10.0), w("second", 0.0, 100.0)];
        let bands = line_bands(&words);
        assert_ne!(
            bands[0], bands[1],
            "two lines 90px apart must not share one band"
        );
    }

    /// The band must widen a short word's box, never shift it sideways.
    #[test]
    fn banding_leaves_the_horizontal_edges_alone() {
        let out = banded([10.0, 50.0, 90.0, 60.0], Some(&[40.0, 70.0]));
        assert_eq!(out[0], 10.0, "left edge moved");
        assert_eq!(out[2], 90.0, "right edge moved");
        assert_eq!([out[1], out[3]], [40.0, 70.0], "band was not applied");
    }

    #[test]
    fn banding_a_word_with_no_line_is_a_no_op() {
        let r = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(banded(r, None), r);
    }

    #[test]
    fn words_join_in_reading_order_with_line_breaks() {
        // Deliberately out of order, spanning two lines.
        let words = vec![
            w("world", 60.0, 10.0),
            w("second", 0.0, 100.0),
            w("Hello", 0.0, 10.0),
            w("line", 60.0, 100.0),
        ];
        let all: Vec<usize> = (0..words.len()).collect();
        assert_eq!(join_words(&words, &all), "Hello world\nsecond line");
    }

    #[test]
    fn joining_a_subset_only_includes_that_subset() {
        let words = vec![w("a", 0.0, 0.0), w("b", 40.0, 0.0), w("c", 80.0, 0.0)];
        assert_eq!(join_words(&words, &[0, 2]), "a c");
    }

    #[test]
    fn joining_nothing_is_empty_not_a_panic() {
        assert_eq!(join_words(&[], &[]), "");
        assert_eq!(join_words(&[w("x", 0.0, 0.0)], &[]), "");
        // Out-of-range indices are ignored rather than panicking.
        assert_eq!(join_words(&[w("x", 0.0, 0.0)], &[7]), "");
    }
}
