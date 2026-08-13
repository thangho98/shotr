// shotr — capture and beautify screenshots.
// Copyright (C) 2026 thangho98
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License version 3 as published by the
// Free Software Foundation.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for details.
//
// You should have received a copy of the GNU General Public License along with
// this program. If not, see <https://www.gnu.org/licenses/>.

use eframe::egui;
use shotr::app::{ShotrApp, Source, Start};
use shotr::capture;
use shotr::daemon;
use shotr::export;
use shotr::i18n::t;
use shotr::notify;
use shotr::pin;
use shotr::render;
use shotr::settings::{Prefs, Style};
use shotr::winlist;

const HELP: &str = "\
shotr — chụp và làm đẹp ảnh màn hình

    shotr                Chạy ở khay hệ thống (tray). Bấm icon để chụp.
    shotr --capture      Chụp mọi màn hình rồi cho kéo chọn vùng
    shotr --capture --full   Chụp hết, vào thẳng trình sửa
    shotr --capture --monitor N   Mở sẵn ở màn hình thứ N (đếm từ 0)
    shotr --capture --window ID   Chụp một cửa sổ, vào thẳng trình sửa
    shotr --capture --copy        Chụp vùng, làm đẹp, copy luôn — không mở cửa sổ
    shotr --capture --full --copy Chụp hết, làm đẹp, copy luôn
    shotr --capture --pin         Chụp vùng rồi ghim ảnh gốc nổi trên màn hình
    shotr --pin FILE     Ghim một ảnh có sẵn, đúng kích thước gốc
    shotr --open [FILE]  Mở một ảnh có sẵn
    shotr --clipboard    Mở ảnh đang có trong clipboard
    shotr --history      Mở danh sách ảnh chụp gần đây
    shotr --settings     Mở cửa sổ tuỳ chọn
    shotr --help         Hiển thị trợ giúp này

Mỗi lần chụp chạy trong một tiến trình riêng và chụp *trước khi* mở cửa sổ.
Đó là cách duy nhất để shotr không lọt vào ảnh của chính nó: Wayland không cho
một app tự ẩn cửa sổ của mình.

Trong trình sửa: Ctrl+lăn chuột để phóng to/thu nhỏ, giữ chuột giữa để kéo ảnh,
Ctrl+0 vừa khung, Ctrl+1 về 100%.

Phím tắt toàn cục:
  macOS  — Preferences → Shortcuts, chọn tổ hợp ngay trong app.
           Muốn dùng ⌘⇧4 thì phải tắt phím của Apple trong System Settings →
           Keyboard → Keyboard Shortcuts → Screenshots trước.
  Khác   — để desktop lo: COSMIC Settings → Keyboard → Shortcuts → Custom,
           chạy `shotr --capture`.
";

/// The value after `flag`, if there is one.
fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a String> {
    args.iter().skip_while(|a| *a != flag).nth(1)
}

/// `--monitor N`, if given and a number.
fn monitor_arg(args: &[String]) -> Option<usize> {
    arg_after(args, "--monitor").and_then(|v| v.parse().ok())
}

/// The file to pin, if one was named. A flag is not a filename: `shotr --pin
/// --capture` means "capture, then pin", not "pin a file called `--capture`".
fn pin_path(args: &[String]) -> Option<&String> {
    arg_after(args, "--pin").filter(|v| !v.starts_with("--"))
}

/// `--pin` with no file named only means something alongside `--capture` — the
/// same trade [`copy_flag`] makes, and for the same reason: on its own there is
/// nothing to pin, and accepting it would exit having silently done nothing.
fn pin_flag(args: &[String]) -> Result<bool, ()> {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match (has("--pin"), pin_path(args).is_some(), has("--capture")) {
        (true, false, false) => Err(()),
        (pin, _, _) => Ok(pin),
    }
}

/// `--copy` only means something alongside `--capture`. On its own there is
/// nothing to copy, and accepting it would exit having silently done nothing.
fn copy_flag(args: &[String]) -> Result<bool, ()> {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match (has("--copy"), has("--capture")) {
        (true, false) => Err(()),
        (copy, _) => Ok(copy),
    }
}

/// Render with the saved style and hand it to the clipboard. No window is
/// created, no eframe context is built.
fn copy_beautified(shot: &image::RgbaImage) {
    let out = render::beautify(shot, &Style::load());
    let mut clipboard = arboard::Clipboard::new().ok();
    match export::copy(&out, &mut clipboard) {
        Ok(()) => notify::show(t("Copied to the clipboard")),
        Err(e) => {
            eprintln!("Could not reach the clipboard: {e}");
            notify::show(t("Cannot reach the clipboard"));
        }
    }
}

/// One window, straight to the editor. `None` means nothing to show — the user
/// cancelled, or the failure was already reported.
///
/// The identifier comes from the tray and survives the trip between processes
/// because every backend hands out one meant to: `ext_foreign_toplevel_list_v1`
/// says so in as many words, and elsewhere it is the window id the system uses.
/// macOS ignores it — Apple's overlay does the choosing there.
fn window_shot(id: Option<&String>) -> Option<image::RgbaImage> {
    let id = id.map(String::as_str).unwrap_or_default();
    match winlist::capture(id) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("Could not capture that window: {e}");
            None
        }
    }
}

/// The desktop, or one screen of it, plus where each monitor landed. `None`
/// means nothing to show.
#[cfg(not(target_os = "macos"))]
fn screen_shot(full: bool, _monitor: Option<usize>) -> Option<(Start, Vec<capture::MonitorView>)> {
    // Always grab every monitor. The editor slices this one snapshot to show a
    // single screen, so `--monitor N` picks the starting view, not a narrower
    // capture — and switching later never re-shoots.
    match capture::capture_desktop() {
        Ok((shot, views)) => {
            let start = if full {
                Start::Editor(shot)
            } else {
                Start::Picker(shot)
            };
            Some((start, views))
        }
        Err(e) => {
            eprintln!("Capture failed: {e}");
            None
        }
    }
}

/// macOS has no windowed picker: without `--full` this is Apple's overlay, and
/// what comes back is already the region the user chose.
///
/// `--monitor N` shoots that one screen rather than the whole desktop and then
/// cutting, because the source was settled in the tray menu and the editor
/// offers no way to change it.
#[cfg(target_os = "macos")]
fn screen_shot(full: bool, monitor: Option<usize>) -> Option<(Start, Vec<capture::MonitorView>)> {
    if !full {
        return interactive(capture::macos::Shot::Region).map(|img| (Start::Editor(img), Vec::new()));
    }
    if let Some(i) = monitor {
        return match capture::capture_monitor(i) {
            Ok(img) => Some((Start::Editor(img), Vec::new())),
            Err(e) => {
                eprintln!("Capture failed: {e}");
                None
            }
        };
    }
    match capture::capture_desktop() {
        Ok((shot, views)) => Some((Start::Editor(shot), views)),
        Err(e) => {
            eprintln!("Capture failed: {e}");
            None
        }
    }
}

/// Hand the choice to Apple's overlay. Escape there is a cancel, and a cancel
/// must leave no window and say nothing.
#[cfg(target_os = "macos")]
fn interactive(shot: capture::macos::Shot) -> Option<image::RgbaImage> {
    match capture::macos::run(shot) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("Capture failed: {e}");
            None
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if has("--help") || has("-h") {
        println!("{HELP}");
        return Ok(());
    }

    // Preferences captures nothing, so it never reaches the paths below.
    if has("--settings") {
        return shotr::prefs_ui::run();
    }

    // Nor does pinning a file that is already on disk.
    if let Some(path) = pin_path(&args) {
        return match image::open(path) {
            Ok(img) => pin::run(img.to_rgba8(), Some(path.into())),
            Err(e) => {
                eprintln!("Could not open {path}: {e}");
                Ok(())
            }
        };
    }

    // Bare `--pin` has to ride along with a capture; this is what stops it
    // reaching the daemon branch below and starting the tray instead.
    let pin = match pin_flag(&args) {
        Ok(pin) => pin,
        Err(()) => {
            eprintln!("--pin needs either a file to pin or --capture to take one.");
            return Ok(());
        }
    };

    // Before the daemon branch below, because `--copy` names no window and
    // would otherwise fall through it and start the tray — the silent nothing
    // that `Command::args` has its own test to prevent.
    let copy = match copy_flag(&args) {
        Ok(copy) => copy,
        Err(()) => {
            eprintln!("--copy needs --capture: on its own there is nothing to copy.");
            return Ok(());
        }
    };

    // Plain `shotr` is the tray daemon. On Linux it has to be: a Wayland client
    // cannot hide its own window, so the only way to stay out of its own
    // screenshot is for no window to exist when the shutter fires. Windows and
    // macOS could hide a window instead, but a tray that is only there on one
    // of the three platforms is a worse deal than one capture path everywhere.
    let opens_a_window = ["--capture", "--open", "--clipboard", "--history", "--pin"]
        .iter()
        .any(|f| has(f));
    if !opens_a_window {
        std::process::exit(daemon::run());
    }

    let named_monitor = monitor_arg(&args);
    let source = match named_monitor {
        Some(i) => Source::Monitor(i),
        None => Source::All,
    };
    let mut views = Vec::new();

    let start = if has("--open") {
        // `--open path/to.png`, or no path to get a file dialog.
        match arg_after(&args, "--open") {
            Some(path) => Start::OpenPath(path.into()),
            None => Start::OpenDialog,
        }
    } else if has("--clipboard") {
        Start::Clipboard
    } else if has("--history") {
        Start::History
    } else if has("--window") {
        match window_shot(arg_after(&args, "--window")) {
            Some(img) => Start::Window(img),
            None => return Ok(()),
        }
    } else {
        match screen_shot(has("--full"), named_monitor) {
            Some((start, v)) => {
                views = v;
                start
            }
            None => return Ok(()),
        }
    };

    // `--copy` skips the editor, which it can only do once the image is final.
    // Every source hands one back already except shotr's own region picker,
    // where the selection has not been made yet — there the copy waits for it.
    //
    // Redaction boxes come from OCR, and OCR is something only the editor runs.
    // With that policy on, a windowless copy would hand back an unredacted
    // image to the one user who asked for it not to be, so the editor opens.
    let mut copy_on_finish = false;
    if copy {
        // The notification is the only thing this path says out loud, so it has
        // to speak the language the rest of the app does.
        let prefs = Prefs::load();
        shotr::i18n::set(prefs.lang);
        if prefs.redact {
            eprintln!(
                "Redaction is on, so this opens the editor instead: a windowless \
                 copy would not have run text recognition."
            );
        } else if let Start::Editor(shot) | Start::Window(shot) = &start {
            copy_beautified(shot);
            // `--copy --pin` asks for both, and the pin is a window: leaving here
            // would honour the first flag and drop the second in silence.
            if !pin {
                return Ok(());
            }
        } else {
            copy_on_finish = true;
        }
    }

    // `--pin` is the same shape as `--copy`: it can leave the editor out only
    // once the image is final, which every source manages except shotr's own
    // region picker. There the pin waits for the selection to be made — and then
    // a *fresh process* opens it, because one eframe app cannot start another and
    // because a pin has to outlive the window that asked for it.
    //
    // Redaction is not a reason to open the editor here, unlike `--copy`: a pin
    // is not an export. Nothing is saved and nothing leaves the machine.
    let mut pin_on_finish = false;
    let start = if pin {
        match start {
            // No path to carry: this shot has never been a file. The pin writes
            // one only if the user asks for the editor.
            Start::Editor(shot) | Start::Window(shot) => return pin::run(shot, None),
            other => {
                pin_on_finish = true;
                other
            }
        }
    } else {
        start
    };

    // The region picker covers the screen and shows the shot at 1:1, so it
    // looks like you are selecting on the live desktop. macOS never reaches
    // here: Apple's overlay did the picking before this process opened anything.
    //
    // The editor has no system titlebar either: it draws its own frame, with
    // the sidebar standing proud of it, so the window has to be transparent for
    // the rounded corners and the shadow to composite. Transparency can only be
    // asked for at creation, and the picker becomes the editor without being
    // recreated — so both branches ask for it.
    //
    // Transparency is a request, not a guarantee: an X11 session with no
    // compositor running has no way to honour it, and there the margin around
    // the frame will read as black rather than as the desktop. Wayland, Windows
    // and macOS all composite, so this only bites bare X11.
    let viewport = if matches!(start, Start::Picker(_)) {
        egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_title("shotr")
            .with_fullscreen(true)
    } else {
        egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([1320.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("shotr")
    }
    .with_icon(shotr::app::window_icon());

    eframe::run_native(
        "shotr",
        shotr::app::native_options(viewport),
        Box::new(move |cc| {
            let mut app = ShotrApp::new(cc, start, source, views.clone());
            app.copy_on_finish = copy_on_finish;
            app.pin_on_finish = pin_on_finish;
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod arg_tests {
    use super::{arg_after, copy_flag, monitor_arg, pin_flag, pin_path};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_flag_yields_the_value_after_it() {
        let a = args(&["--capture", "--window", "51695"]);
        assert_eq!(
            arg_after(&a, "--window").map(String::as_str),
            Some("51695"),
            "the tray passes the window identifier this way, so losing it captures nothing"
        );
    }

    #[test]
    fn a_flag_with_nothing_after_it_is_not_a_value() {
        // The tray always supplies one, but a hand-typed `shotr --capture
        // --window` must fall through to an ordinary capture rather than panic.
        let a = args(&["--capture", "--window"]);
        assert_eq!(arg_after(&a, "--window"), None);
    }

    #[test]
    fn an_absent_flag_yields_nothing() {
        let a = args(&["--capture", "--full"]);
        assert_eq!(arg_after(&a, "--window"), None);
        assert_eq!(monitor_arg(&a), None, "no --monitor means every monitor");
    }

    #[test]
    fn copy_needs_something_to_copy() {
        assert_eq!(
            copy_flag(&args(&["--copy"])),
            Err(()),
            "`--copy` alone would exit having silently done nothing"
        );
        assert_eq!(
            copy_flag(&args(&["--open", "--copy"])),
            Err(()),
            "`--copy` is about a capture, not about a file already on disk"
        );
    }

    #[test]
    fn copy_rides_along_with_a_capture() {
        assert_eq!(copy_flag(&args(&["--capture", "--copy"])), Ok(true));
        assert_eq!(
            copy_flag(&args(&["--capture", "--full", "--copy"])),
            Ok(true)
        );
        assert_eq!(
            copy_flag(&args(&["--capture", "--full"])),
            Ok(false),
            "a capture without --copy must still open the editor"
        );
    }

    #[test]
    fn a_named_file_is_what_gets_pinned() {
        let a = args(&["--pin", "/tmp/shot.png"]);
        assert_eq!(
            pin_path(&a).map(String::as_str),
            Some("/tmp/shot.png"),
            "losing the path here pins nothing and the command looks dead"
        );
        assert_eq!(pin_flag(&a), Ok(true));
    }

    #[test]
    fn a_flag_after_pin_is_not_a_filename() {
        // `shotr --pin --capture` means "capture, then pin". Reading the flag as
        // a path would try to open a file called `--capture` and give up.
        let a = args(&["--pin", "--capture"]);
        assert_eq!(pin_path(&a), None);
        assert_eq!(pin_flag(&a), Ok(true));
    }

    #[test]
    fn pin_needs_something_to_pin() {
        assert_eq!(
            pin_flag(&args(&["--pin"])),
            Err(()),
            "bare `--pin` would otherwise fall through to the daemon branch and \
             start the tray, which looks exactly like a dead shortcut"
        );
        assert_eq!(
            pin_flag(&args(&["--capture", "--pin"])),
            Ok(true),
            "this is the tray's own entry, and the flag order it uses"
        );
        assert_eq!(
            pin_flag(&args(&["--capture"])),
            Ok(false),
            "a capture without --pin must still open the editor"
        );
    }

    #[test]
    fn pin_rides_along_with_copy() {
        // Both are asked for, so both must happen: `--copy` returning early
        // would honour one flag and drop the other in silence.
        let a = args(&["--capture", "--copy", "--pin"]);
        assert_eq!(copy_flag(&a), Ok(true));
        assert_eq!(pin_flag(&a), Ok(true));
    }

    #[test]
    fn a_monitor_index_that_is_not_a_number_is_ignored() {
        // Better the whole desktop than a panic on a typo.
        let a = args(&["--capture", "--monitor", "left"]);
        assert_eq!(monitor_arg(&a), None);
        assert_eq!(monitor_arg(&args(&["--capture", "--monitor", "2"])), Some(2));
    }
}
