//! Write the app icon as PNGs for the desktop launcher and the macOS bundle.
//!
//!     cargo run --release --example gen_icon -- <outdir>

fn main() {
    let outdir =
        std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_string()));
    std::fs::create_dir_all(&outdir).expect("create outdir");

    // 32..512 are the hicolor sizes `install.sh` places; 16 and 1024 exist only
    // because a macOS `.iconset` demands them (16x16 and 512x512@2x). Rendering
    // them here rather than resampling keeps every icon on every platform the
    // output of one function.
    for size in [16u32, 32, 48, 64, 128, 256, 512, 1024] {
        let path = outdir.join(format!("shotr-{size}.png"));
        shotr::render::icon::icon_image(size)
            .save(&path)
            .expect("save icon");
        println!("{}", path.display());
    }
}
