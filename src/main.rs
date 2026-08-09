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
use shotr::winlist;

const HELP: &str = "\
shotr — chụp và làm đẹp ảnh màn hình

    shotr                Chạy ở khay hệ thống (tray). Bấm icon để chụp.
    shotr --capture      Chụp mọi màn hình rồi cho kéo chọn vùng
    shotr --capture --full   Chụp hết, vào thẳng trình sửa
    shotr --capture --monitor N   Mở sẵn ở màn hình thứ N (đếm từ 0)
    shotr --capture --window ID   Chụp một cửa sổ, vào thẳng trình sửa
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

Phím tắt toàn cục: COSMIC Settings → Keyboard → Shortcuts → Custom,
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

    // Plain `shotr` is the tray daemon. On Linux it has to be: a Wayland client
    // cannot hide its own window, so the only way to stay out of its own
    // screenshot is for no window to exist when the shutter fires. Windows and
    // macOS could hide a window instead, but a tray that is only there on one
    // of the three platforms is a worse deal than one capture path everywhere.
    let opens_a_window = ["--capture", "--open", "--clipboard", "--history"]
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

    // The region picker covers the screen and shows the shot at 1:1, so it
    // looks like you are selecting on the live desktop. macOS never reaches
    // here: Apple's overlay did the picking before this process opened anything.
    let viewport = if matches!(start, Start::Picker(_)) {
        egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_title("shotr")
            .with_fullscreen(true)
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("shotr")
    }
    .with_icon(shotr::app::window_icon());

    eframe::run_native(
        "shotr",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ShotrApp::new(cc, start, source, views.clone())))),
    )
}

#[cfg(test)]
mod arg_tests {
    use super::{arg_after, monitor_arg};

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
    fn a_monitor_index_that_is_not_a_number_is_ignored() {
        // Better the whole desktop than a panic on a typo.
        let a = args(&["--capture", "--monitor", "left"]);
        assert_eq!(monitor_arg(&a), None);
        assert_eq!(monitor_arg(&args(&["--capture", "--monitor", "2"])), Some(2));
    }
}
