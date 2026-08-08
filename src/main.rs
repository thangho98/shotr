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
#[cfg(target_os = "linux")]
use shotr::daemon;

const HELP: &str = "\
shotr — chụp và làm đẹp ảnh màn hình

    shotr                Chạy ở khay hệ thống (tray). Bấm icon để chụp.
    shotr --capture      Chụp mọi màn hình rồi cho kéo chọn vùng
    shotr --capture --full   Chụp hết, vào thẳng trình sửa
    shotr --capture --monitor N   Mở sẵn ở màn hình thứ N (đếm từ 0)
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

/// `--monitor N`, if given and a number.
fn monitor_arg(args: &[String]) -> Option<usize> {
    args.iter()
        .skip_while(|a| *a != "--monitor")
        .nth(1)
        .and_then(|v| v.parse().ok())
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    if has("--help") || has("-h") {
        println!("{HELP}");
        return Ok(());
    }

    // Plain `shotr` is the tray daemon — on Linux only, and for one reason: a
    // Wayland client cannot hide its own window, so the only way to stay out of
    // its own screenshot is for no window to exist when the shutter fires.
    // Windows and macOS can hide a window, so there `shotr` just captures.
    #[cfg(target_os = "linux")]
    if !has("--capture") && !has("--open") {
        std::process::exit(daemon::run());
    }

    let source = match monitor_arg(&args) {
        Some(i) => Source::Monitor(i),
        None => Source::All,
    };
    let mut views = Vec::new();

    let start = if has("--open") {
        // `--open path/to.png`, or no path to get a file dialog.
        match args.iter().skip_while(|a| *a != "--open").nth(1) {
            Some(path) => Start::OpenPath(path.into()),
            None => Start::OpenDialog,
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
    let viewport = if matches!(start, Start::Picker(_)) {
        egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_decorations(false)
            .with_title("shotr")
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
