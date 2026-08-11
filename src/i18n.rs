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
    ("(no presets yet)", "(chưa có preset nào)"),
    ("(no region selected)", "(chưa chọn vùng nào)"),
    ("(no windows could be read)", "(không đọc được cửa sổ nào)"),
    ("(untitled window)", "(cửa sổ không tên)"),
    ("100%", "100%"),
    ("100% covers completely; lower is translucent, like a highlighter.", "100% che kín, thấp hơn thì trong suốt như bút dạ quang."),
    ("A shortcut for capturing", "Phím tắt để chụp"),
    ("API keys", "Khoá API"),
    ("About", "Giới thiệu"),
    ("Add a shadow", "Đổ bóng"),
    ("All screens together", "Gộp tất cả màn hình"),
    ("Amount", "Mức"),
    ("Angle", "Góc xoay"),
    ("Arrow", "Mũi tên"),
    ("Ask for permission", "Xin quyền"),
    ("Auto grows the canvas to fit the shot plus its padding.", "Auto mở rộng khung theo ảnh cộng phần đệm."),
    ("Back to 100%", "Về 100%"),
    ("Back to selection", "◀ Chọn lại vùng"),
    ("Back to the Select tool", "Về công cụ Chọn"),
    ("Back to the Select tool, or cancel", "Về công cụ Chọn, hoặc huỷ"),
    ("Background", "Nền"),
    ("Balance", "Balance"),
    ("Bind a shortcut to: shotr --capture", "Gán phím tắt: chạy `shotr --capture`"),
    ("Bind a system shortcut to: shotr --capture", "Gán phím tắt hệ thống cho: shotr --capture"),
    ("Blur", "Làm mờ"),
    ("Blur amount", "Độ mờ"),
    ("Bring forward", "Đưa lên trên"),
    ("Cancel", "Huỷ"),
    ("Cannot reach the clipboard", "Không truy cập được clipboard"),
    ("Capture a region", "Chụp một vùng"),
    ("Capture a region…", "✂ Chọn vùng chụp…"),
    ("Capture a whole screen", "🖥 Chụp nguyên màn hình"),
    ("Capture a window", "🗔 Chụp một cửa sổ"),
    ("Capture a window…", "Chụp một cửa sổ…"),
    ("Capture again", "📸 Chụp lại"),
    ("Capture every screen", "Chụp mọi màn hình"),
    ("Card", "Thẻ"),
    ("Card numbers", "Số thẻ"),
    ("Changes are saved as you make them", "Mọi thay đổi được lưu ngay"),
    ("Changes with the desktop, while shotr is running.", "Đổi theo hệ thống, ngay khi shotr đang chạy."),
    ("Change…", "Đổi…"),
    ("Choose a folder…", "Chọn thư mục…"),
    ("Choose…", "Chọn…"),
    ("Clear", "Xoá"),
    ("Clear all annotations", "Xoá hết chú thích"),
    ("Click a shape to select it · drag to move · Backspace deletes", "Bấm vào hình để chọn · kéo để di chuyển · Backspace để xoá"),
    ("Click a window in the list   ·   Space: back to region   ·   Esc: cancel", "Di chuột lên cửa sổ rồi bấm   ·   Space: chọn vùng   ·   Esc: huỷ"),
    ("Click a window in the list.", "Bấm một cửa sổ trong danh sách."),
    ("Click a word to hide or reveal it.", "Bấm vào từ để che hoặc bỏ che."),
    ("Click the image and type. Enter to finish, Esc to cancel. Click existing text to edit it.", "Bấm lên ảnh rồi gõ. Enter xong, Esc bỏ. Bấm lại chữ cũ để sửa."),
    ("Click to open. Right-click to pin.", "Bấm để mở. Bấm phải để ghim."),
    ("Click to pick a region", "Bấm để chọn vùng chụp"),
    ("Colour", "Màu"),
    ("Colours", "Màu"),
    ("Copied the shot as captured", "Đã copy ảnh gốc"),
    ("Copied the shot as captured, still redacted", "Đã copy ảnh gốc, phần che vẫn giữ"),
    ("Copied to the clipboard", "Đã copy vào clipboard"),
    ("Copy", "Copy"),
    ("Copy a region", "Copy một vùng"),
    ("Copy all", "Copy tất cả"),
    ("Copy and close on double-click", "Double-click ảnh = Copy và đóng"),
    ("Copy every screen", "Copy mọi màn hình"),
    ("Copy the finished image and close", "Copy ảnh đã xong rồi đóng"),
    ("Copy the shot as captured", "Copy ảnh gốc (không nền)"),
    ("Copy the text in the image", "📋 Copy chữ trong ảnh"),
    ("Corner", "Góc"),
    ("Could not open the editor", "Không mở được trình sửa"),
    ("Crop to selection", "Cắt vùng đã chọn"),
    ("Custom", "Tuỳ chỉnh"),
    ("Dark", "Tối"),
    ("Dashed", "Nét đứt"),
    ("Default format", "Định dạng mặc định"),
    ("Delete layer", "Xoá lớp"),
    ("Delete the preset that matches", "Xoá preset đang khớp"),
    ("Delete the selected shape  ⌫", "Xoá hình đang chọn  ⌫"),
    ("Double-click the image to copy and close", "Double-click ảnh = Copy và đóng"),
    ("Download the OCR model", "⬇ Tải model OCR"),
    ("Downloading the model…", "Đang tải model…"),
    ("Drag on the image to pick a region.", "Kéo chuột trên ảnh để khoanh vùng cần chụp."),
    ("Drag on the image to select text.", "Kéo chuột trên ảnh để chọn chữ."),
    ("Drag to select   ·   Enter: whole screen   ·   Esc: cancel", "Kéo để chọn vùng   ·   Enter: cả màn hình   ·   Esc: huỷ"),
    ("Drag to select   ·   Space: pick a window   ·   Enter: whole screen   ·   Esc: cancel", "Kéo để chọn vùng   ·   Space: chọn cửa sổ   ·   Enter: cả màn hình   ·   Esc: huỷ"),
    ("Drag to select a region. Space switches to picking a window.", "Kéo chuột để chọn vùng. Space để đổi sang chọn cửa sổ."),
    ("Duplicate", "Nhân bản"),
    ("Ellipse", "Elip"),
    ("Email addresses", "Địa chỉ email"),
    ("Enable watermark", "Bật đóng dấu"),
    ("Enter text", "Nội dung"),
    ("Export", "Xuất ảnh"),
    ("File name", "Tên file"),
    ("Filename", "Tên file"),
    ("Fill", "Tô đặc"),
    ("Fit", "Vừa khung"),
    ("Fit to the window", "Vừa khung"),
    ("Fit · {p}%", "Vừa khung · {p}%"),
    ("Follow the desktop", "Theo hệ thống"),
    ("Font size", "Cỡ chữ"),
    ("From clipboard", "📋 Từ clipboard"),
    ("General", "Chung"),
    ("Help", "Trợ giúp"),
    ("Hide {n} words", "Bỏ che {n} từ"),
    ("History", "Lịch sử"),
    ("IP addresses", "Địa chỉ IP"),
    ("If no dialog appeared, use the button beside this one.", "Nếu không thấy hộp thoại nào, dùng nút bên cạnh."),
    ("Image", "Ảnh"),
    ("In the editor", "Trong trình sửa"),
    ("Insert the copyright sign", "Chèn ký hiệu bản quyền"),
    ("Inset", "Inset"),
    ("JPEG has no alpha channel — a transparent background becomes white.", "JPEG không có kênh alpha — nền trong suốt sẽ thành trắng."),
    (
        "Keep this shot floating above other windows",
        "Giữ ảnh này nổi trên các cửa sổ khác",
    ),
    ("Language", "Ngôn ngữ"),
    ("Layout", "Bố cục"),
    ("Leave the inset off rather than falling back to a plain colour.", "Bỏ luôn viền trong thay vì tô một màu mặc định."),
    ("Light", "Sáng"),
    ("Logo image", "Ảnh logo"),
    ("Maximum compression (slower)", "Nén tối đa (chậm hơn)"),
    ("Monitor {n}", "Màn hình {n}"),
    ("More…", "Thêm…"),
    ("Name the preset first", "Đặt tên cho preset trước đã"),
    ("Needs a 12 MB model; everything runs on this machine.", "Cần tải model 12 MB, chạy hoàn toàn trên máy."),
    ("No image on the clipboard: {err}", "Clipboard không có ảnh: {err}"),
    ("No text found.", "Không thấy chữ nào."),
    ("No text to copy", "Không có chữ để copy"),
    ("No window to capture", "Không có cửa sổ nào để chụp"),
    ("Not set", "Chưa đặt"),
    ("OCR model downloaded", "Đã tải model OCR"),
    ("Off", "Tắt"),
    ("Only when a colour is found", "Chỉ khi dò được màu"),
    ("Opacity", "Độ đục"),
    ("Open System Settings", "Mở System Settings"),
    ("Open a shot", "Mở một ảnh"),
    ("Open file…", "📂 Mở file…"),
    ("Open head", "Đầu hở"),
    ("Open image folder", "📂 Mở thư mục ảnh"),
    ("Open image…", "📂 Mở ảnh…"),
    ("Open recent shots", "Mở ảnh chụp gần đây"),
    ("Opened a new capture window.", "Đã mở cửa sổ chụp mới."),
    ("Opened {path}", "Đã mở {path}"),
    ("Outlined", "Viền chữ"),
    ("Padding", "Padding"),
    ("Paint", "Tô màu"),
    ("Paint opacity", "Độ đậm"),
    ("Pan the image", "Kéo ảnh"),
    ("Pasted a {w}×{h} image from the clipboard", "Đã dán ảnh {w}×{h} từ clipboard"),
    ("Permission", "Quyền"),
    ("Phone number", "Số điện thoại"),
    ("Phone numbers", "Số điện thoại"),
    ("Phone numbers are off by default: that pattern is the loosest of the set and the most likely to cover something that is not a phone number.", "Số điện thoại tắt sẵn: mẫu này lỏng nhất trong nhóm và dễ che nhầm thứ không phải số điện thoại."),
    ("Pick a drawing tool", "Chọn công cụ vẽ"),
    ("Pick a recent shot, open a file, or paste from the clipboard.", "Chọn một ảnh gần đây, mở file, hoặc dán từ clipboard."),
    ("Pick a tool", "Chọn công cụ"),
    ("Pin", "Ghim"),
    ("Pin a region", "Ghim một vùng"),
    ("Pin a region…", "Ghim một vùng…"),
    ("Pin to screen", "Ghim ra màn hình"),
    ("Pinned to the screen.", "Đã ghim ra màn hình."),
    ("Pixelate", "Vỡ hạt"),
    ("Pixelate survives a re-encode.", "Vỡ hạt không phục hồi được khi mã hoá lại."),
    ("Plain text", "Chữ trơn"),
    ("Preferences…", "Tuỳ chọn…"),
    ("Preset name", "Tên preset"),
    ("Preset “{name}” deleted", "Đã xoá preset “{name}”"),
    ("Preset “{name}” saved", "Đã lưu preset “{name}”"),
    ("Press a combination…", "Bấm tổ hợp phím…"),
    ("Quality", "Chất lượng"),
    ("Quit", "Thoát"),
    ("Radius", "Bo góc"),
    ("Ratio / Size", "Tỉ lệ / Kích thước"),
    ("Read with ocrs — no Vietnamese diacritics.", "Đọc bằng ocrs — không có dấu tiếng Việt."),
    ("Read with tesseract ({langs})", "Đọc bằng tesseract ({langs})"),
    ("Reading text…", "Đang đọc chữ…"),
    ("Recent shots…", "Ảnh chụp gần đây…"),
    ("Rectangle", "Khung"),
    ("Redact by default", "Mặc định bật che"),
    ("Redact sensitive data ({found} found)", "Che thông tin nhạy cảm (tìm thấy {found})"),
    ("Redaction", "Che thông tin"),
    ("Redo", "Làm lại"),
    ("Region", "Vùng"),
    ("Releases", "Các bản phát hành"),
    ("Repeat diagonally across the image — the anti-reuse look", "Lặp lại chéo khắp ảnh — kiểu chống dùng lại ảnh"),
    ("Reset to defaults", "↺ Về mặc định"),
    ("Rim", "Viền"),
    ("Rim colour", "Màu viền"),
    ("Rounded plate", "Nền bo tròn"),
    ("Save", "Lưu"),
    ("Save As…", "Lưu thành…"),
    ("Save the finished image", "Lưu ảnh đã xong"),
    ("Saved: {path}", "Đã lưu: {path}"),
    ("Screen recording is allowed", "Đã được phép ghi màn hình"),
    ("Screenshots are taken by the system tool, which can only see the screen if shotr is allowed to.", "Ảnh do công cụ của hệ thống chụp, và nó chỉ thấy được màn hình nếu shotr được cấp quyền."),
    ("Select", "Chọn"),
    ("Select text", "Chọn chữ"),
    ("Selection: {w} × {h} px", "Vùng chọn: {w} × {h} px"),
    ("Send back", "Đưa xuống dưới"),
    ("Shadow", "Đổ bóng"),
    ("Shortcuts", "Phím tắt"),
    ("Sits under the shot, sharing its right edge.", "Nằm dưới ảnh, thẳng cạnh phải của ảnh."),
    ("Size", "Cỡ"),
    ("Social sizes", "Kích thước mạng xã hội"),
    ("Solid", "Màu đặc"),
    ("Solid colour", "Màu đơn"),
    ("Solid head", "Đầu đặc"),
    ("Step 1 — pick a region", "Bước 1 — Chọn vùng"),
    ("Stroke", "Nét"),
    ("Switch between region and window picking", "Đổi giữa chọn vùng và chọn cửa sổ"),
    ("Take a new shot", "📸 Chụp tấm mới"),
    ("Take the whole screen", "Chụp cả màn hình"),
    ("Taken from the window's own buffer, so one behind another still comes out whole.", "Ảnh lấy thẳng từ cửa sổ đó, nên cửa sổ bị che vẫn ra đủ."),
    ("Text", "Chữ"),
    ("Text colour", "Màu chữ"),
    ("Text recognition (OCR)", "Nhận diện chữ (OCR)"),
    ("The clipboard image is not valid", "Ảnh trong clipboard không hợp lệ"),
    ("The permission is remembered for shotr.app in /Applications. A binary run straight from a terminal borrows the terminal's permission instead.", "Quyền được ghi nhận cho shotr.app trong /Applications. Chạy binary trực tiếp từ terminal thì nó dùng quyền của terminal."),
    ("The shot is fitted inside the pinned canvas.", "Ảnh được đặt vừa trong khung đã ghim."),
    ("Theme", "Giao diện"),
    ("These are fixed for now.", "Hiện chưa đổi được."),
    ("This compositor will not list windows. Use Region instead.", "Compositor này không cho liệt kê cửa sổ. Dùng chế độ Vùng."),
    ("Tokens: {date}, {time}, {unix}", "Mã thay thế: {date}, {time}, {unix}"),
    ("Tools", "Công cụ"),
    ("Trim uniform edges so the subject sits centred", "Cắt bớt viền đồng màu để nội dung nằm giữa"),
    ("Try again", "Thử lại"),
    ("Type — Enter to finish, Esc to cancel", "Gõ chữ — Enter xong, Esc bỏ"),
    ("Undo", "Hoàn tác"),
    ("Undo, and Shift to redo", "Hoàn tác, thêm Shift để làm lại"),
    ("Use the default", "Dùng mặc định"),
    ("Use the whole screen", "Dùng cả màn hình"),
    ("Use {keys}", "Dùng {keys}"),
    ("Version", "Phiên bản"),
    ("Watermark", "Đóng dấu"),
    ("WebP is lossless here, so there is no quality slider.", "WebP ở đây chỉ lossless, nên không có thanh chất lượng."),
    ("Where shots are saved", "Nơi lưu ảnh"),
    ("Which kinds of text are covered when redaction is switched on for a shot.", "Những loại thông tin được che khi bật chế độ che cho một ảnh."),
    ("Window", "Cửa sổ"),
    ("Your presets", "Preset của anh"),
    ("Zoom", "Phóng"),
    ("Zoom in and out", "Phóng to, thu nhỏ"),
    ("` and 1–6 pick a tool · Esc returns to Select", "` và 1–6 chọn công cụ · Esc về Chọn"),
    ("auto", "tự động"),
    ("built from the shot", "dựng từ chính ảnh"),
    ("current wallpaper", "ảnh nền hiện tại"),
    ("macOS cannot report shortcuts held by other apps. If one press does two things, choose another combination.", "macOS không cho biết phím tắt do app khác giữ. Nếu một lần bấm làm hai việc, hãy chọn tổ hợp khác."),
    ("macOS is using {keys} for its own screenshot. Both will run.", "macOS đang dùng {keys} cho ảnh chụp của nó. Cả hai sẽ cùng chạy."),
    ("macOS reads this permission once when an app starts. After allowing it, quit shotr from the menu bar and start it again.", "macOS chỉ đọc quyền này một lần lúc app khởi động. Sau khi cấp, hãy thoát shotr từ menu bar rồi mở lại."),
    ("shotr cannot record the screen yet", "shotr chưa được phép ghi màn hình"),
    ("shotr — Preferences", "shotr — Tuỳ chọn"),
    ("then choose Screenshots", "rồi chọn Screenshots"),
    ("{chars} characters copied", "Đã copy {chars} ký tự"),
    ("{keys} now captures a region. Change it in Preferences.", "{keys} giờ dùng để chụp một vùng. Đổi trong Preferences."),
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

    /// The current language is process-wide state and cargo runs tests in
    /// parallel, so every test that switches it has to take this first or they
    /// read each other's answers. Poisoning is ignored deliberately: one test
    /// failing must not turn into four.
    ///
    /// It was not needed while the suite happened to interleave harmlessly.
    /// Adding tests elsewhere in the crate changed the timing and
    /// `a_placeholder_can_move_within_the_sentence` started reading Vietnamese
    /// where it had asked for English — a failure with nothing wrong in the code
    /// it names.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn placeholders_are_filled_after_translation() {
        let _serial = alone();
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
        let _serial = alone();
        set(Lang::En);
        assert_eq!(tf("{n} words copied", &[("n", "12")]), "12 words copied");
        set(Lang::Vi);
        assert_eq!(tf("{n} words copied", &[("n", "12")]), "Đã copy 12 từ");
        set(Lang::En);
    }

    #[test]
    fn english_returns_the_source_string_untouched() {
        let _serial = alone();
        set(Lang::En);
        assert_eq!(t("Save"), "Save");
        assert_eq!(t("something never translated"), "something never translated");
    }

    #[test]
    fn vietnamese_translates_what_it_knows_and_falls_back_otherwise() {
        let _serial = alone();
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
            "these strings are wrapped in t() or tf() but have no Vietnamese entry:\n  {}",
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

    /// Pull the string out of every `t("…")` and `tf("…")` call. Deliberately
    /// simple: it only understands a literal directly inside the call, which is
    /// the only form the lookup can resolve anyway.
    ///
    /// **Both spellings, and that is the point.** Searching for `t("` cannot
    /// find `tf("` — the substring is not in it — so every `tf` string was
    /// exempt from the one check that exists to catch an untranslated string,
    /// and one duly shipped that way.
    fn translated_literals(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let bytes = text.as_bytes();
        for (start, _) in text.match_indices('(') {
            // Walk back over the callee. Stopping at the first non-identifier
            // byte keeps `format!(` and `insert(` out, and it cannot split a
            // character, because a non-ASCII byte ends the walk.
            let mut name_start = start;
            while name_start > 0 {
                let c = bytes[name_start - 1] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    name_start -= 1;
                } else {
                    break;
                }
            }
            if !matches!(&text[name_start..start], "t" | "tf") {
                continue;
            }
            // Skip whitespace after the paren. rustfmt puts a long literal on
            // its own line, so a scanner that stops at the newline exempts
            // precisely the strings most in need of a translation.
            let mut open = start + 1;
            while bytes.get(open).is_some_and(|b| b.is_ascii_whitespace()) {
                open += 1;
            }
            if bytes.get(open) != Some(&b'"') {
                continue;
            }
            open += 1;
            if let Some(end_rel) = text[open..].find('"') {
                found.push(text[open..open + end_rel].to_string());
            }
        }
        found
    }
}
