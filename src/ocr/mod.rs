//! On-device OCR.
//!
//! Two backends, picked at runtime. `ocrs` is pure Rust and needs nothing
//! installed, but its recognition model has a fixed ASCII alphabet and so can
//! never produce Vietnamese. Where the `tesseract` binary is present with the
//! `vie` pack, [`tesseract`] is used instead — see that module for why this is
//! a hard limit rather than a tuning problem.
//!
//! The two models are ~12 MB combined and are not vendored; they are fetched
//! once into the config directory on first use.

pub mod detect;
pub mod tesseract;

use image::RgbaImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use std::path::PathBuf;

use crate::settings::config_dir;

const DETECTION_URL: &str = "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";
const RECOGNITION_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";

/// One recognised word and where it sits, in original screenshot pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    /// `[x0, y0, x1, y1]`
    pub rect: [f32; 4],
}

impl Word {
    pub fn width(&self) -> f32 {
        self.rect[2] - self.rect[0]
    }

    pub fn height(&self) -> f32 {
        self.rect[3] - self.rect[1]
    }
}

pub fn models_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("models"))
}

fn model_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = models_dir()?;
    Some((
        dir.join("text-detection.rten"),
        dir.join("text-recognition.rten"),
    ))
}

pub fn models_present() -> bool {
    model_paths().is_some_and(|(d, r)| d.is_file() && r.is_file())
}

/// Fetch both models. `progress` is called with `(done_bytes, total_bytes)` for
/// the whole download, so the UI can show one bar rather than two.
pub fn download_models(mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let (det_path, rec_path) = model_paths().ok_or("Could not determine the config directory")?;
    let dir = det_path.parent().ok_or("Invalid model path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    // Known sizes so the bar is meaningful from the first byte.
    let jobs = [(DETECTION_URL, &det_path), (RECOGNITION_URL, &rec_path)];
    let mut total = 0u64;
    let mut sizes = Vec::new();
    for (url, path) in &jobs {
        if path.is_file() {
            sizes.push(0);
            continue;
        }
        let len = content_length(url).unwrap_or(0);
        sizes.push(len);
        total += len;
    }

    let mut done = 0u64;
    for ((url, path), _size) in jobs.iter().zip(sizes) {
        if path.is_file() {
            continue;
        }
        let bytes = fetch(url)?;
        // Write to a temp name first so an interrupted download never leaves a
        // truncated file that would fail to parse on the next run.
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        done += bytes.len() as u64;
        progress(done, total.max(done));
    }
    Ok(())
}

fn content_length(url: &str) -> Option<u64> {
    let resp = ureq::head(url).call().ok()?;
    resp.headers()
        .get("content-length")?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let mut resp = ureq::get(url)
        .call()
        .map_err(|e| format!("downloading {url} failed: {e}"))?;
    let mut buf = Vec::new();
    std::io::copy(&mut resp.body_mut().as_reader(), &mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

pub struct Engine {
    inner: OcrEngine,
}

impl Engine {
    pub fn load() -> Result<Self, String> {
        let (det, rec) = model_paths().ok_or("Could not determine the config directory")?;
        let detection =
            rten::Model::load_file(&det).map_err(|e| format!("detection model failed: {e}"))?;
        let recognition =
            rten::Model::load_file(&rec).map_err(|e| format!("recognition model failed: {e}"))?;
        let inner = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection),
            recognition_model: Some(recognition),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Self { inner })
    }

    /// Recognise every word in the image. Slow (hundreds of ms to seconds) —
    /// call it off the UI thread.
    pub fn read(&self, img: &RgbaImage) -> Result<Vec<Word>, String> {
        let source = ImageSource::from_bytes(img.as_raw(), img.dimensions())
            .map_err(|e| format!("invalid image: {e}"))?;
        let input = self
            .inner
            .prepare_input(source)
            .map_err(|e| e.to_string())?;
        let word_rects = self.inner.detect_words(&input).map_err(|e| e.to_string())?;
        let lines = self.inner.find_text_lines(&input, &word_rects);
        let recognised = self
            .inner
            .recognize_text(&input, &lines)
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for line in recognised.into_iter().flatten() {
            for word in line.words() {
                let text = word.to_string();
                if text.trim().is_empty() {
                    continue;
                }
                let r = word.bounding_rect();
                out.push(Word {
                    text,
                    rect: [
                        r.left() as f32,
                        r.top() as f32,
                        r.right() as f32,
                        r.bottom() as f32,
                    ],
                });
            }
        }
        Ok(out)
    }
}
