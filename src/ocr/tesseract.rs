//! Vietnamese-capable OCR, by handing the image to the `tesseract` binary.
//!
//! Why not just use the `ocrs` engine for everything: its recognition model has
//! a fixed output alphabet, and that alphabet is pure ASCII —
//!
//! ```text
//! " 0123456789!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~EABCDEF…xyz"
//! ```
//!
//! — with no `ă â đ ê ô ơ ư` and no tone marks. The network has one output slot
//! per character in that list, so it *cannot* emit Vietnamese however it is
//! configured; `Tiếng Việt` can only ever come back mangled. That is a property
//! of the trained weights, not a setting.
//!
//! Tesseract ships a Vietnamese model and reports per-word boxes and confidence
//! in TSV, which maps straight onto [`Word`]. It is invoked as a subprocess
//! rather than linked, so shotr still builds with no C dependencies and simply
//! goes without if the binary is absent.

use image::{ImageEncoder, RgbaImage};
use std::io::Write;
use std::process::{Command, Stdio};

use super::Word;

/// Languages worth trying, in the order Tesseract should weigh them. Vietnamese
/// screenshots are usually a mix — UI chrome in English around Vietnamese
/// content — and `vie+eng` reads that far better than either alone.
pub const PREFERRED: [&str; 2] = ["vie", "eng"];

/// Language packs Tesseract has installed.
pub fn languages() -> Vec<String> {
    let Ok(out) = Command::new("tesseract").arg("--list-langs").output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1) // the first line is a header, not a language
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The `-l` argument to use, or `None` when nothing useful is installed.
///
/// `osd` is orientation detection, not a script — asking to recognise with it
/// makes Tesseract fail outright.
pub fn best_langs() -> Option<String> {
    let have = languages();
    let picked: Vec<&str> = PREFERRED
        .into_iter()
        .filter(|l| have.iter().any(|h| h == l))
        .collect();
    if picked.is_empty() {
        return None;
    }
    Some(picked.join("+"))
}

/// True when Tesseract can actually read Vietnamese here.
pub fn supports_vietnamese() -> bool {
    languages().iter().any(|l| l == "vie")
}

/// Recognise `img`, returning one [`Word`] per word box.
pub fn read(img: &RgbaImage, langs: &str) -> Result<Vec<Word>, String> {
    // PNG on stdin, TSV on stdout: no temporary files to clean up or collide.
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("could not encode the image: {e}"))?;

    let mut child = Command::new("tesseract")
        .args(["-", "stdout", "-l", langs, "--psm", "3", "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run tesseract: {e}"))?;

    child
        .stdin
        .take()
        .ok_or("could not open tesseract's stdin")?
        .write_all(&png)
        .map_err(|e| format!("could not send the image: {e}"))?;

    let out = child
        .wait_with_output()
        .map_err(|e| format!("tesseract failed: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tesseract failed: {}", why.trim()));
    }
    Ok(parse_tsv(&String::from_utf8_lossy(&out.stdout)))
}

/// Pull word boxes out of Tesseract's TSV.
///
/// Columns are `level page block par line word left top width height conf text`.
/// Only level 5 rows are words; the lower levels are the blocks and lines that
/// contain them, and taking those too would return every word several times
/// over inside ever-larger boxes.
fn parse_tsv(tsv: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }
        // Confidence is -1 for rows that carry no recognition result.
        let conf: f32 = cols[10].parse().unwrap_or(-1.0);
        if conf < 0.0 {
            continue;
        }
        let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
            cols[6].parse::<f32>(),
            cols[7].parse::<f32>(),
            cols[8].parse::<f32>(),
            cols[9].parse::<f32>(),
        ) else {
            continue;
        };
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        out.push(Word {
            text: text.to_string(),
            rect: [x, y, x + w, y + h],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slice of real Tesseract output, diacritics and all.
    const SAMPLE: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
1\t1\t0\t0\t0\t0\t0\t0\t800\t600\t-1\t
2\t1\t1\t0\t0\t0\t20\t30\t400\t40\t-1\t
3\t1\t1\t1\t0\t0\t20\t30\t400\t40\t-1\t
4\t1\t1\t1\t1\t0\t20\t30\t400\t40\t-1\t
5\t1\t1\t1\t1\t1\t20\t30\t120\t40\t96.4\tTiếng
5\t1\t1\t1\t1\t2\t150\t30\t100\t40\t95.1\tViệt
5\t1\t1\t1\t1\t3\t260\t30\t60\t40\t-1\t
5\t1\t1\t1\t1\t4\t330\t30\t80\t40\t88.0\t
5\t1\t1\t1\t1\t5\t420\t30\t90\t40\t91.2\tđường";

    #[test]
    fn words_come_back_with_their_diacritics_intact() {
        let words = parse_tsv(SAMPLE);
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Tiếng", "Việt", "đường"],
            "this is the whole point: the ocrs alphabet cannot produce these"
        );
    }

    /// Only level-5 rows are words. Taking the block and line rows too would
    /// return each word several times inside ever-larger boxes.
    #[test]
    fn container_rows_are_not_mistaken_for_words() {
        let words = parse_tsv(SAMPLE);
        assert_eq!(words.len(), 3, "levels 1-4 must be skipped");
        assert!(
            !words.iter().any(|w| w.rect == [0.0, 0.0, 800.0, 600.0]),
            "the page-level box leaked through as a word"
        );
    }

    #[test]
    fn rows_with_no_confidence_or_no_text_are_dropped() {
        let words = parse_tsv(SAMPLE);
        // The `-1` confidence row and the whitespace-only row are both gone.
        assert!(!words.iter().any(|w| w.text.trim().is_empty()));
        assert!(!words.iter().any(|w| w.rect[0] == 260.0), "conf -1 row kept");
        assert!(!words.iter().any(|w| w.rect[0] == 330.0), "blank row kept");
    }

    #[test]
    fn boxes_are_converted_from_x_y_w_h_to_corners() {
        let words = parse_tsv(SAMPLE);
        assert_eq!(words[0].rect, [20.0, 30.0, 140.0, 70.0]);
        assert_eq!(words[0].width(), 120.0);
        assert_eq!(words[0].height(), 40.0);
    }

    #[test]
    fn malformed_output_yields_nothing_rather_than_panicking() {
        for junk in ["", "not tsv at all", "level\ttext", "5\t\t\t\t\t\t\t\t\t\t\t"] {
            let _ = parse_tsv(junk);
        }
        assert!(parse_tsv("").is_empty());
    }

    /// `osd` is orientation detection, not a script. Passing it to `-l` makes
    /// Tesseract fail, so it must never be picked.
    #[test]
    fn language_choice_skips_non_scripts_and_missing_packs() {
        // Mirrors `best_langs` against a stand-in for what is installed.
        let choose = |have: &[&str]| -> Option<String> {
            let picked: Vec<&str> = PREFERRED
                .into_iter()
                .filter(|l| have.contains(l))
                .collect();
            (!picked.is_empty()).then(|| picked.join("+"))
        };
        assert_eq!(choose(&["vie", "eng", "osd"]).as_deref(), Some("vie+eng"));
        assert_eq!(choose(&["eng", "osd"]).as_deref(), Some("eng"));
        assert_eq!(choose(&["vie"]).as_deref(), Some("vie"));
        assert_eq!(choose(&["osd", "afr"]), None, "neither is one we asked for");
        assert_eq!(choose(&[]), None);
    }
}
