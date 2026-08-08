//! Write the app icon as PNGs for the desktop launcher.
//!
//!     cargo run --release --example gen_icon -- <outdir>

fn main() {
    let outdir =
        std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_string()));
    std::fs::create_dir_all(&outdir).expect("create outdir");

    for size in [32u32, 48, 64, 128, 256, 512] {
        let path = outdir.join(format!("shotr-{size}.png"));
        shotr::tray::icon_image(size)
            .save(&path)
            .expect("save icon");
        println!("{}", path.display());
    }
}
