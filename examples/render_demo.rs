//! Headless render harness: exercises the compositing pipeline over a matrix of
//! styles and writes the results next to the input, plus a timing for a
//! full-resolution export.
//!
//!     cargo run --release --example render_demo -- <input.png> <outdir>

use shotr::annotate::{Layer, Tool};
use shotr::render::{Scene, background::BG_PRESETS, render, text};
use shotr::settings::{Background, Ratio, Style};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("usage: render_demo <input> <outdir>");
    let outdir = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&outdir).unwrap();

    let shot = image::open(&input).expect("open input").to_rgba8();
    let font = text::load_system_font().map(|(_, f)| f);
    println!("input: {} ({}x{})", input, shot.width(), shot.height());

    let cases: Vec<(&str, Style)> = vec![
        ("01-default", Style::default()),
        (
            "02-preset-love",
            Style {
                background: Background::Preset(4),
                ..Default::default()
            },
        ),
        (
            "03-preset-rain",
            Style {
                background: Background::Preset(5),
                ..Default::default()
            },
        ),
        (
            "04-inset",
            Style {
                inset: 14,
                radius: 28,
                background: Background::Preset(1),
                ..Default::default()
            },
        ),
        (
            "05-no-shadow",
            Style {
                shadow: 0,
                ..Default::default()
            },
        ),
        (
            "06-max-shadow",
            Style {
                shadow: 100,
                padding: 120,
                ..Default::default()
            },
        ),
        (
            "07-transparent",
            Style {
                background: Background::None,
                ..Default::default()
            },
        ),
        (
            "08-instagram",
            Style {
                ratio: Ratio::Size(1080, 1080),
                background: Background::Preset(2),
                ..Default::default()
            },
        ),
        (
            "09-16x9",
            Style {
                ratio: Ratio::Aspect(16.0 / 9.0),
                background: Background::Preset(3),
                ..Default::default()
            },
        ),
        (
            "10-watermark",
            Style {
                watermark: true,
                background: Background::Preset(6),
                ..Default::default()
            },
        ),
        (
            "11-balance",
            Style {
                balance: true,
                background: Background::Preset(0),
                ..Default::default()
            },
        ),
        (
            "15-aurora",
            Style {
                background: Background::Preset(7),
                padding: 110,
                radius: 22,
                shadow: 55,
                ..Default::default()
            },
        ),
        (
            "16-lagoon",
            Style {
                background: Background::Preset(12),
                padding: 110,
                radius: 22,
                inset: 10,
                ..Default::default()
            },
        ),
        (
            "14-auto-bg",
            Style {
                background: Background::Auto,
                ..Default::default()
            },
        ),
        (
            "12-radius0",
            Style {
                radius: 0,
                inset: 0,
                background: Background::Preset(5),
                ..Default::default()
            },
        ),
    ];

    for (name, settings) in &cases {
        let start = std::time::Instant::now();
        let out = render(&Scene {
            font: font.as_ref(),
            ..Scene::plain(&shot, settings, 1.0)
        });
        let elapsed = start.elapsed();
        let path = outdir.join(format!("{name}.png"));
        out.save(&path).unwrap();
        println!(
            "{name:<16} {:>5}x{:<5} {:>7.0?}  {}",
            out.width(),
            out.height(),
            elapsed,
            path.display()
        );
    }

    // Every annotation tool at once, on one image.
    {
        let (w, h) = (shot.width() as f32, shot.height() as f32);
        let mk = |kind, a: [f32; 2], b: [f32; 2], color: [u8; 4]| {
            let mut l = Layer::new(kind, a, color, 8.0, 64.0, 14.0);
            l.b = b;
            l
        };
        let mut label = mk(
            Tool::Text,
            [w * 0.06, h * 0.06],
            [0.0, 0.0],
            [255, 255, 255, 255],
        );
        label.text = "Chú thích tiếng Việt".into();

        let layers = vec![
            mk(
                Tool::Highlight,
                [w * 0.05, h * 0.28],
                [w * 0.42, h * 0.36],
                [255, 235, 0, 200],
            ),
            mk(
                Tool::Blur,
                [w * 0.05, h * 0.42],
                [w * 0.30, h * 0.56],
                [0, 0, 0, 255],
            ),
            mk(
                Tool::Rect,
                [w * 0.50, h * 0.25],
                [w * 0.90, h * 0.55],
                [255, 59, 48, 255],
            ),
            mk(
                Tool::Ellipse,
                [w * 0.55, h * 0.62],
                [w * 0.85, h * 0.85],
                [52, 199, 89, 255],
            ),
            mk(
                Tool::Arrow,
                [w * 0.20, h * 0.90],
                [w * 0.52, h * 0.60],
                [0, 122, 255, 255],
            ),
            label,
        ];
        let settings = Style {
            background: Background::Preset(3),
            ..Default::default()
        };
        let out = render(&Scene {
            font: font.as_ref(),
            layers: &layers,
            ..Scene::plain(&shot, &settings, 1.0)
        });
        let path = outdir.join("13-annotations.png");
        out.save(&path).unwrap();
        println!("13-annotations   {}", path.display());
    }

    // Every preset as a labelled grid, to eyeball the palette side by side.
    let (cell_w, cell_h, cols) = (300u32, 190u32, 5u32);
    let rows = BG_PRESETS.len().div_ceil(cols as usize) as u32;
    let mut sheet = image::RgbaImage::from_pixel(
        cell_w * cols,
        cell_h * rows,
        image::Rgba([255, 255, 255, 255]),
    );
    for (i, preset) in BG_PRESETS.iter().enumerate() {
        let (cx, cy) = (i as u32 % cols, i as u32 / cols);
        let mut tile = shotr::render::background::mesh(cell_w - 16, cell_h - 16, preset);
        if let Some(f) = font.as_ref() {
            text::draw(
                &mut tile,
                f,
                20.0,
                12.0,
                10.0,
                image::Rgba([30, 30, 40, 220]),
                preset.name,
            );
        }
        image::imageops::overlay(
            &mut sheet,
            &tile,
            (cx * cell_w + 8) as i64,
            (cy * cell_h + 8) as i64,
        );
    }
    let sheet_path = outdir.join("00-palette.png");
    sheet.save(&sheet_path).unwrap();
    println!(
        "palette          {} ({} preset)",
        sheet_path.display(),
        BG_PRESETS.len()
    );
}
