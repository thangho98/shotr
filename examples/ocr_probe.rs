//! End-to-end OCR check, across both engines.
//!
//!     cargo run --release --example ocr_probe -- ảnh.png
//!
//! Runs whichever backends are available on this machine and prints what each
//! read, because they differ in a way no unit test can show: `ocrs` has a fixed
//! ASCII alphabet and turns every Vietnamese diacritic into `?`, while
//! Tesseract with `vie` reads it correctly. Seeing them side by side is the
//! fastest way to tell a bad *image* from a bad *engine*.

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("dùng: cargo run --example ocr_probe -- <ảnh>");
        std::process::exit(2);
    };
    let img = match image::open(&path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("không mở được {path}: {e}");
            std::process::exit(1);
        }
    };
    println!("ảnh: {}x{}", img.width(), img.height());

    let mut best: Vec<shotr::ocr::Word> = Vec::new();

    match shotr::ocr::tesseract::best_langs() {
        Some(langs) => {
            let t = std::time::Instant::now();
            match shotr::ocr::tesseract::read(&img, &langs) {
                Ok(words) => {
                    report(&format!("tesseract (-l {langs})"), &words, t.elapsed());
                    best = words;
                }
                Err(e) => println!("\ntesseract: lỗi — {e}"),
            }
        }
        None => println!("\ntesseract: không có gói ngôn ngữ nào dùng được"),
    }

    if shotr::ocr::models_present() {
        let t = std::time::Instant::now();
        match shotr::ocr::Engine::load().and_then(|e| e.read(&img)) {
            Ok(words) => {
                report("ocrs", &words, t.elapsed());
                if best.is_empty() {
                    best = words;
                }
            }
            Err(e) => println!("\nocrs: lỗi — {e}"),
        }
    } else {
        println!("\nocrs: chưa tải model");
    }

    if best.is_empty() {
        println!("\nKhông engine nào đọc được gì.");
        return;
    }

    let findings = shotr::ocr::detect::scan(&best);
    let phones = shotr::ocr::detect::scan_phones(&best);
    println!("\n=== thông tin nhạy cảm dò được ===");
    if findings.is_empty() && phones.is_empty() {
        println!("  (không có)");
    }
    for f in findings.iter().chain(phones.iter()) {
        println!("  {:?}", f);
    }
}

fn report(engine: &str, words: &[shotr::ocr::Word], took: std::time::Duration) {
    let line: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    println!("\n=== {engine} ===");
    println!("  {} từ trong {took:?}", words.len());
    println!("  {}", line.join(" "));
}
