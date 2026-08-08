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

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if has("--help") || has("-h") {
        println!("{HELP}");
        return Ok(());
    }

    // Plain `shotr` is the tray daemon. On Linux it has to be: a Wayland client
    // cannot hide its own window, so the only way to stay out of its own
    // screenshot is for no window to exist when the shutter fires. Windows and
    // macOS could hide a window instead, but a tray that is only there on one
    // of the three platforms is a worse deal than one capture path everywhere.
    if !has("--capture") && !has("--open") {
        std::process::exit(daemon::run());
    }

    // The picker shows one screen at 1:1. On macOS it has to be told which,
    // because its window is positioned by hand rather than made fullscreen —
    // see `capture::monitor_under_cursor`. Only the picker: `--full` means the
    // whole desktop, and a pointer resting somewhere must not narrow that.
    let named_monitor = monitor_arg(&args);
    #[cfg(target_os = "macos")]
    let picker_screen = if has("--capture") && !has("--full") && !has("--window") {
        named_monitor.or_else(capture::monitor_under_cursor)
    } else {
        None
    };
    #[cfg(not(target_os = "macos"))]
    let picker_screen: Option<usize> = None;

    let source = match named_monitor.or(picker_screen) {
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
    } else if let Some(id) = arg_after(&args, "--window") {
        // Nothing to grab first: the compositor hands over that window's own
        // buffer, which is why one sitting behind another still comes out whole.
        // The identifier is the tray's — both backends hand out one meant to
        // cross process boundaries.
        match winlist::capture(id) {
            Ok(img) => Start::Window(img),
            Err(e) => {
                eprintln!("Could not capture that window: {e}");
                return Ok(());
            }
        }
    } else {
        // Always grab every monitor. The editor slices this one snapshot to
        // show a single screen, so `--monitor N` picks the starting view, not a
        // narrower capture — and switching later never re-shoots.
        match capture::capture_desktop() {
            Ok((shot, v)) => {
                views = v;
                if has("--full") {
                    Start::Editor(shot)
                } else {
                    Start::Picker(shot)
                }
            }
            Err(e) => {
                eprintln!("Capture failed: {e}");
                return Ok(());
            }
        }
    };

    // The region picker covers the screen and shows the shot at 1:1, so it
    // looks like you are selecting on the live desktop.
    //
    // Covering it means a borderless window over that monitor's rectangle, not
    // fullscreen, wherever the rectangle can be known. Fullscreen on macOS is
    // the *native* kind: a Space of its own and an animation to get there, which
    // is a long way from Lightshot dropping an overlay in front of you. The menu
    // bar and Dock still float above the window — they outrank every level a
    // normal window can ask for — so they are the one part not covered.
    #[cfg(target_os = "macos")]
    let picker_bounds = picker_screen.and_then(capture::monitor_bounds);
    #[cfg(not(target_os = "macos"))]
    let picker_bounds: Option<[f32; 4]> = None;

    let viewport = if matches!(start, Start::Picker(_)) {
        let base = egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_title("shotr");
        match picker_bounds {
            Some([x, y, w, h]) => base
                .with_position(egui::pos2(x, y))
                .with_inner_size([w, h])
                .with_always_on_top(),
            None => base.with_fullscreen(true),
        }
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("shotr")
    };

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
