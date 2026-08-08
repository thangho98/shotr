//! Interface language.
//!
//! English is the source language: every user-facing string is written in
//! English at the call site and looked up here when another language is
//! selected. That keeps the code readable to anyone and makes an untranslated
//! string degrade to English rather than to a blank label or a key like
//! `sidebar.export.button`.
//!
//! The current language lives in a global rather than being threaded through
//! every call. shotr draws one window from one thread, and passing a language
//! handle into every label would add a parameter to most of the UI to express
//! something that is genuinely process-wide.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    En,
    Vi,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Vi];

    /// Shown on the switcher. Each language names itself, as language pickers
    /// should — someone looking for Vietnamese is scanning for "Tiếng Việt",
    /// not for "Vietnamese".
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Vi => "Tiếng Việt",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Vi => "VI",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Vi,
        _ => Lang::En,
    }
}

pub fn set(lang: Lang) {
    CURRENT.store(
        match lang {
            Lang::En => 0,
            Lang::Vi => 1,
        },
        Ordering::Relaxed,
    );
}

/// Translate an English source string into the current language.
pub fn t(en: &'static str) -> &'static str {
    match current() {
        Lang::En => en,
        Lang::Vi => lookup(en).unwrap_or(en),
    }
}

/// Translate, then fill in `{name}` placeholders.
///
/// `t` cannot be used inside `format!` — the macro needs a literal — and word
/// order differs between languages, so the placeholders are named and
/// substituted after translation rather than positional.
pub fn tf(en: &'static str, args: &[(&str, &str)]) -> String {
    let mut out = t(en).to_string();
    for (key, value) in args {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

fn lookup(en: &str) -> Option<&'static str> {
    VI.binary_search_by_key(&en, |(k, _)| *k)
        .ok()
        .map(|i| VI[i].1)
}

/// English → Vietnamese. Sorted by key; a test enforces that, since the lookup
/// is a binary search and an unsorted entry would simply never be found.
static VI: &[(&str, &str)] = &[
    ("(background colour detected)", "(đã dò màu nền)"),
    ("(no background colour found)", "(không dò được màu nền)"),
    ("(no monitors could be read)", "(không đọc được màn hình nào)"),
    ("(no presets yet)", "(chưa có preset nào)"),
    ("(no region selected)", "(chưa chọn vùng nào)"),
    ("(untitled window)", "(cửa sổ không tên)"),
    ("100%", "100%"),
    ("100% covers completely; lower is translucent, like a highlighter.", "100% che kín, thấp hơn thì trong suốt như bút dạ quang."),
    ("A single monitor…", "Một màn hình…"),
    ("Add a shadow", "Đổ bóng"),
    ("All monitors combined", "Gộp tất cả màn hình"),
    ("Angle", "Góc xoay"),
    ("Arrow", "Mũi tên"),
    ("Back to selection", "◀ Chọn lại vùng"),
    ("Background", "Nền"),
    ("Balance", "Balance"),
    ("Bind a shortcut to: shotr --capture", "Gán phím tắt: chạy `shotr --capture`"),
    ("Blur", "Làm mờ"),
    ("Blur amount", "Độ mờ"),
    ("Border radius", "Bo góc"),
    ("Cannot reach the clipboard", "Không truy cập được clipboard"),
    ("Capture again", "📸 Chụp lại"),
    ("Card", "Thẻ"),
    ("Change…", "Đổi…"),
    ("Clear all annotations", "Xoá hết chú thích"),
    ("Click a window in the list   ·   Space: back to region   ·   Esc: cancel", "Di chuột lên cửa sổ rồi bấm   ·   Space: chọn vùng   ·   Esc: huỷ"),
    ("Click a window in the list.", "Bấm một cửa sổ trong danh sách."),
    ("Click a word to hide or reveal it.", "Bấm vào từ để che hoặc bỏ che."),
    ("Click the image and type. Enter to finish, Esc to cancel. Click existing text to edit it.", "Bấm lên ảnh rồi gõ. Enter xong, Esc bỏ. Bấm lại chữ cũ để sửa."),
    ("Click to pick a region", "Bấm để chọn vùng chụp"),
    ("Colour", "Màu"),
    ("Copied to the clipboard", "Đã copy vào clipboard"),
    ("Copy", "Copy"),
    ("Copy all", "Copy tất cả"),
    ("Copy and close on double-click", "Double-click ảnh = Copy và đóng"),
    ("Copy the text in the image", "📋 Copy chữ trong ảnh"),
    ("Corner", "Góc"),
    ("Crop to selection", "Cắt vùng đã chọn"),
    ("Custom…", "Tuỳ chỉnh…"),
    ("Delete layer", "Xoá lớp"),
    ("Double-click the image to copy and close", "Double-click ảnh = Copy và đóng"),
    ("Download the OCR model", "⬇ Tải model OCR"),
    ("Downloading the model…", "Đang tải model…"),
    ("Drag on the image to pick a region.", "Kéo chuột trên ảnh để khoanh vùng cần chụp."),
    ("Drag on the image to select text.", "Kéo chuột trên ảnh để chọn chữ."),
    ("Drag to select   ·   Enter: whole screen   ·   Esc: cancel", "Kéo để chọn vùng   ·   Enter: cả màn hình   ·   Esc: huỷ"),
    ("Drag to select   ·   Space: pick a window   ·   Enter: whole screen   ·   Esc: cancel", "Kéo để chọn vùng   ·   Space: chọn cửa sổ   ·   Enter: cả màn hình   ·   Esc: huỷ"),
    ("Drag to select a region. Space switches to picking a window.", "Kéo chuột để chọn vùng. Space để đổi sang chọn cửa sổ."),
    ("Ellipse", "Elip"),
    ("Enable watermark", "Bật đóng dấu"),
    ("Enter text", "Nội dung"),
    ("Export", "Xuất ảnh"),
    ("Filename", "Tên file"),
    ("Fill", "Che"),
    ("Fit", "Vừa khung"),
    ("Font size", "Cỡ chữ"),
    ("From clipboard", "📋 Từ clipboard"),
    ("Help", "Trợ giúp"),
    ("Hide {n} words", "Bỏ che {n} từ"),
    ("History", "Lịch sử"),
    ("Image", "Ảnh"),
    ("Image source", "Nguồn ảnh"),
    ("Insert the copyright sign", "Chèn ký hiệu bản quyền"),
    ("Inset", "Inset"),
    ("JPEG has no alpha channel — a transparent background becomes white.", "JPEG không có kênh alpha — nền trong suốt sẽ thành trắng."),
    ("Language", "Ngôn ngữ"),
    ("Layout", "Bố cục"),
    ("Logo image", "Ảnh logo"),
    ("Maximum compression (slower)", "Nén tối đa (chậm hơn)"),
    ("Monitor {n}", "Màn hình {n}"),
    ("More…", "Thêm…"),
    ("Name the preset first", "Đặt tên cho preset trước đã"),
    ("Needs a 12 MB model; everything runs on this machine.", "Cần tải model 12 MB, chạy hoàn toàn trên máy."),
    ("No text found.", "Không thấy chữ nào."),
    ("No text to copy", "Không có chữ để copy"),
    ("No window to capture", "Không có cửa sổ nào để chụp"),
    ("OCR model downloaded", "Đã tải model OCR"),
    ("Off", "Tắt"),
    ("Opacity", "Độ đục"),
    ("Open file…", "📂 Mở file…"),
    ("Open image folder", "📂 Mở thư mục ảnh"),
    ("Open image…", "📂 Mở ảnh…"),
    ("Opened a new capture window.", "Đã mở cửa sổ chụp mới."),
    ("Opened {path}", "Đã mở {path}"),
    ("Outlined", "Viền chữ"),
    ("Padding", "Padding"),
    ("Paint", "Tô màu"),
    ("Paint opacity", "Độ đậm"),
    ("Pasted a {w}×{h} image from the clipboard", "Đã dán ảnh {w}×{h} từ clipboard"),
    ("Phone number", "Số điện thoại"),
    ("Pick a region…", "✂ Chọn vùng…"),
    ("Plain text", "Chữ trơn"),
    ("Position", "Vị trí"),
    ("Preset name", "Tên preset"),
    ("Preset “{name}” deleted", "Đã xoá preset “{name}”"),
    ("Preset “{name}” saved", "Đã lưu preset “{name}”"),
    ("Protective tiling is usually 20–40% opacity at -30°.", "Lát chống dùng lại ảnh thường để 20–40% độ đục, xoay -30°."),
    ("Quality", "Chất lượng"),
    ("Quit", "Thoát"),
    ("Ratio / Size", "Tỉ lệ / Kích thước"),
    ("Read with ocrs — no Vietnamese diacritics.", "Đọc bằng ocrs — không có dấu tiếng Việt."),
    ("Read with tesseract ({langs})", "Đọc bằng tesseract ({langs})"),
    ("Reading text…", "Đang đọc chữ…"),
    ("Rectangle", "Khung"),
    ("Redact sensitive data ({found} found)", "Che thông tin nhạy cảm (tìm thấy {found})"),
    ("Redaction colour", "Màu che"),
    ("Region", "Vùng"),
    ("Repeat diagonally across the image — the anti-reuse look", "Lặp lại chéo khắp ảnh — kiểu chống dùng lại ảnh"),
    ("Reset to defaults", "↺ Về mặc định"),
    ("Rounded plate", "Nền bo tròn"),
    ("Save", "Lưu"),
    ("Save As…", "Lưu thành…"),
    ("Saved: {path}", "Đã lưu: {path}"),
    ("Select", "Chọn"),
    ("Select text", "Chọn chữ"),
    ("Selection: {w} × {h} px", "Vùng chọn: {w} × {h} px"),
    ("Size", "Cỡ"),
    ("Solid", "Màu đặc"),
    ("Solid colour", "Màu đơn"),
    ("Step 1 — pick a region", "Bước 1 — Chọn vùng"),
    ("Stroke", "Nét"),
    ("Take a new shot", "📸 Chụp tấm mới"),
    ("Taken from the window's own buffer, so one behind another still comes out whole.", "Ảnh lấy thẳng từ cửa sổ đó, nên cửa sổ bị che vẫn ra đủ."),
    ("Text", "Chữ"),
    ("Text colour", "Màu chữ"),
    ("Text recognition (OCR)", "Nhận diện chữ (OCR)"),
    ("This compositor will not list windows. Use Region instead.", "Compositor này không cho liệt kê cửa sổ. Dùng chế độ Vùng."),
    ("Tile across the image", "Lát kín cả ảnh"),
    ("Tools", "Công cụ"),
    ("Trim uniform edges so the subject sits centred", "Cắt bớt viền đồng màu để nội dung nằm giữa"),
    ("Try again", "Thử lại"),
    ("Type — Enter to finish, Esc to cancel", "Gõ chữ — Enter xong, Esc bỏ"),
    ("Use the whole screen", "Dùng cả màn hình"),
    ("Watermark", "Đóng dấu"),
    ("WebP is lossless here, so there is no quality slider.", "WebP ở đây chỉ lossless, nên không có thanh chất lượng."),
    ("Whole screen", "🖥 Toàn màn hình"),
    ("Window", "Cửa sổ"),
    ("Your presets", "Preset của anh"),
    ("Zoom", "Phóng"),
    ("{chars} characters copied", "Đã copy {chars} ký tự"),
    ("{n} windows. Click one in the list.", "{n} cửa sổ. Bấm một cái trong danh sách giữa màn hình."),
    ("{n} words copied", "Đã copy {n} từ"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The lookup is a binary search, so an out-of-order entry is not a style
    /// problem — it is a translation that silently never appears.
    #[test]
    fn the_table_is_sorted_and_has_no_duplicates() {
        for pair in VI.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "out of order (or duplicated): {:?} then {:?}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn every_entry_is_translated_to_something() {
        for (en, vi) in VI {
            assert!(!en.trim().is_empty(), "empty key");
            assert!(!vi.trim().is_empty(), "{en:?} has an empty translation");
        }
    }

    #[test]
    fn placeholders_are_filled_after_translation() {
        set(Lang::Vi);
        assert_eq!(
            tf("Saved: {path}", &[("path", "/tmp/a.png")]),
            "Đã lưu: /tmp/a.png"
        );
        // An argument with no matching placeholder must not corrupt the string.
        assert_eq!(tf("Copy", &[("nope", "x")]), "Copy");
        set(Lang::En);
    }

    /// Word order differs between languages, so a placeholder can sit at either
    /// end. Positional substitution would silently scramble that.
    #[test]
    fn a_placeholder_can_move_within_the_sentence() {
        set(Lang::En);
        assert_eq!(tf("{n} words copied", &[("n", "12")]), "12 words copied");
        set(Lang::Vi);
        assert_eq!(tf("{n} words copied", &[("n", "12")]), "Đã copy 12 từ");
        set(Lang::En);
    }

    #[test]
    fn english_returns_the_source_string_untouched() {
        set(Lang::En);
        assert_eq!(t("Save"), "Save");
        assert_eq!(t("something never translated"), "something never translated");
    }

    #[test]
    fn vietnamese_translates_what_it_knows_and_falls_back_otherwise() {
        set(Lang::Vi);
        assert_eq!(t("Save"), "Lưu");
        assert_eq!(
            t("a string with no entry"),
            "a string with no entry",
            "a missing translation must show English, never a blank or a key"
        );
        set(Lang::En);
    }

    /// Every `t("…")` in the source has to exist in the table, or it will
    /// silently stay English while everything around it switches. The compiler
    /// cannot check this, so the test reads the source instead.
    #[test]
    fn every_translated_string_in_the_source_has_an_entry() {
        let mut missing: Vec<String> = Vec::new();
        let mut files = Vec::new();
        collect_rs(std::path::Path::new("src"), &mut files);
        assert!(!files.is_empty(), "found no source to scan");

        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            // Skip this file: its table and tests mention strings deliberately.
            if path.ends_with("i18n.rs") {
                continue;
            }
            for literal in translated_literals(&text) {
                if lookup(&literal).is_none() {
                    missing.push(format!("{}: {literal:?}", path.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these strings are wrapped in t() but have no Vietnamese entry:\n  {}",
            missing.join("\n  ")
        );
    }

    fn collect_rs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Pull the string out of every `t("…")` call. Deliberately simple: it only
    /// understands a literal directly inside the call, which is the only form
    /// the lookup can resolve anyway.
    fn translated_literals(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(pos) = text[i..].find("t(\"") {
            let start = i + pos;
            // Require a non-identifier character before `t`, so `format!(` and
            // `insert(` do not match.
            let ok = start == 0 || {
                let c = bytes[start - 1] as char;
                !c.is_alphanumeric() && c != '_'
            };
            let open = start + 3;
            let Some(end_rel) = text[open..].find('"') else {
                break;
            };
            if ok {
                found.push(text[open..open + end_rel].to_string());
            }
            i = open + end_rel + 1;
        }
        found
    }
}
