//! Canvas backgrounds.
//!
//! The preset swatches are *mesh* gradients, not linear ones: a colour at each
//! corner, bilinearly interpolated with a smoothstep so the transitions stay
//! soft, plus an optional centre bloom. That soft four-way blend is what makes
//! these backgrounds read as "designed" rather than as a plain ramp.

use image::{Rgba, RgbaImage};

use crate::settings::{Background, CustomKind, Rgba8, Style};

/// A soft colour spot laid over the four-corner blend. Real mesh gradients get
/// their organic look from overlapping radial spots rather than from a single
/// bilinear ramp, so this is what separates "designed" from "faded".
pub struct Blob {
    /// Position in 0..1 of the canvas, `[x, y]`.
    pub at: [f32; 2],
    pub color: Rgba8,
    /// Reach, also in 0..1 of the canvas.
    pub radius: f32,
    /// Peak strength at the centre of the spot, 0..1.
    pub strength: f32,
}

pub struct BgPreset {
    pub name: &'static str,
    /// Top-left, top-right, bottom-left, bottom-right.
    pub corners: [Rgba8; 4],
    pub center: Option<Rgba8>,
    pub blobs: &'static [Blob],
}

const OP: u8 = 0xff;

pub const BG_PRESETS: &[BgPreset] = &[
    BgPreset {
        name: "Cool",
        corners: [
            [0x6d, 0xd5, 0xfa, OP],
            [0x21, 0x96, 0xf3, OP],
            [0x19, 0x76, 0xd2, OP],
            [0x0d, 0x47, 0xa1, OP],
        ],
        center: None,
        blobs: &[],
    },
    BgPreset {
        name: "Nice",
        corners: [
            [0xff, 0x4d, 0x8d, OP],
            [0xe9, 0x1e, 0x63, OP],
            [0xc2, 0x18, 0x5b, OP],
            [0x7b, 0x1f, 0xa2, OP],
        ],
        center: None,
        blobs: &[],
    },
    BgPreset {
        name: "Morning",
        corners: [
            [0xff, 0xd0, 0x8a, OP],
            [0xff, 0x8a, 0x65, OP],
            [0xff, 0x70, 0x43, OP],
            [0xf4, 0x51, 0x1e, OP],
        ],
        center: None,
        blobs: &[],
    },
    BgPreset {
        name: "Bright",
        corners: [
            [0x8e, 0x7d, 0xff, OP],
            [0x53, 0x6d, 0xfe, OP],
            [0x65, 0x1f, 0xff, OP],
            [0x30, 0x3f, 0xd8, OP],
        ],
        center: None,
        blobs: &[],
    },
    BgPreset {
        name: "Love",
        corners: [
            [0xa6, 0x2a, 0xd8, OP],
            [0xe0, 0x40, 0xfb, OP],
            [0x6a, 0x1b, 0x9a, OP],
            [0xd5, 0x00, 0xf9, OP],
        ],
        center: None,
        blobs: &[],
    },
    BgPreset {
        name: "Rain",
        corners: [
            [0x4d, 0xd0, 0xe1, OP],
            [0xf0, 0x62, 0x92, OP],
            [0x79, 0x86, 0xcb, OP],
            [0xec, 0x40, 0x7a, OP],
        ],
        center: Some([0xb3, 0x9d, 0xdb, OP]),
        blobs: &[],
    },
    BgPreset {
        name: "Sky",
        corners: [
            [0xa8, 0xe0, 0xff, OP],
            [0xb3, 0x9d, 0xdb, OP],
            [0x4f, 0xc3, 0xf7, OP],
            [0x95, 0x75, 0xcd, OP],
        ],
        center: None,
        blobs: &[],
    },
    // --- Pastel mesh gradients: softer, multi-spot, the style people reach for
    // --- on landing pages. Appended so existing `Background::Preset(i)` in a
    // --- saved settings.json keeps pointing at the same colour.
    BgPreset {
        name: "Aurora",
        corners: [
            [0xbc, 0xc6, 0xf7, OP],
            [0xf3, 0xaa, 0xe1, OP],
            [0xbd, 0xe2, 0xf2, OP],
            [0xf8, 0xcc, 0xea, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.4, 0.02],
                color: [0xa7, 0x8b, 0xfa, OP],
                radius: 0.55,
                strength: 0.85,
            },
            Blob {
                at: [0.98, 0.3],
                color: [0xef, 0x9f, 0xd8, OP],
                radius: 0.5,
                strength: 0.8,
            },
            Blob {
                at: [0.45, 1.0],
                color: [0xcd, 0xf3, 0xf0, OP],
                radius: 0.55,
                strength: 0.75,
            },
        ],
    },
    BgPreset {
        name: "Cotton",
        corners: [
            [0xff, 0xd9, 0xec, OP],
            [0xff, 0xc2, 0xd1, OP],
            [0xe9, 0xd5, 0xff, OP],
            [0xfd, 0xe2, 0xe4, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.15, 0.2],
                color: [0xff, 0xb3, 0xd9, OP],
                radius: 0.5,
                strength: 0.7,
            },
            Blob {
                at: [0.85, 0.8],
                color: [0xd8, 0xb4, 0xfe, OP],
                radius: 0.5,
                strength: 0.65,
            },
        ],
    },
    BgPreset {
        name: "Mint",
        corners: [
            [0xc7, 0xf9, 0xe5, OP],
            [0xa7, 0xf3, 0xd0, OP],
            [0xba, 0xe6, 0xfd, OP],
            [0xd9, 0xf9, 0x9d, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.3, 0.75],
                color: [0x6e, 0xe7, 0xb7, OP],
                radius: 0.55,
                strength: 0.6,
            },
            Blob {
                at: [0.8, 0.15],
                color: [0x7d, 0xd3, 0xfc, OP],
                radius: 0.5,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Peach",
        corners: [
            [0xff, 0xe4, 0xd0, OP],
            [0xff, 0xc9, 0xb3, OP],
            [0xff, 0xd7, 0xc2, OP],
            [0xff, 0xb4, 0xa2, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.85, 0.7],
                color: [0xfc, 0xa5, 0xa5, OP],
                radius: 0.5,
                strength: 0.6,
            },
            Blob {
                at: [0.15, 0.2],
                color: [0xfe, 0xd7, 0xaa, OP],
                radius: 0.5,
                strength: 0.7,
            },
        ],
    },
    BgPreset {
        name: "Lilac",
        corners: [
            [0xde, 0xd4, 0xfb, OP],
            [0xf0, 0xd7, 0xfb, OP],
            [0xcf, 0xd8, 0xfb, OP],
            [0xf6, 0xdc, 0xf0, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.25, 0.15],
                color: [0xc4, 0xb5, 0xfd, OP],
                radius: 0.55,
                strength: 0.75,
            },
            Blob {
                at: [0.85, 0.85],
                color: [0xf0, 0xab, 0xfc, OP],
                radius: 0.5,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Lagoon",
        corners: [
            [0xcf, 0xfa, 0xfe, OP],
            [0xa5, 0xf3, 0xfc, OP],
            [0xbf, 0xdb, 0xfe, OP],
            [0x99, 0xf6, 0xe4, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.7, 0.25],
                color: [0x67, 0xe8, 0xf9, OP],
                radius: 0.55,
                strength: 0.65,
            },
            Blob {
                at: [0.2, 0.85],
                color: [0x93, 0xc5, 0xfd, OP],
                radius: 0.55,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Dusk",
        corners: [
            [0xfe, 0xd7, 0xaa, OP],
            [0xfb, 0xcf, 0xe8, OP],
            [0xfb, 0xcf, 0xe8, OP],
            [0xdd, 0xd6, 0xfe, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.5, 0.15],
                color: [0xfd, 0xa4, 0xaf, OP],
                radius: 0.55,
                strength: 0.6,
            },
            Blob {
                at: [0.9, 0.9],
                color: [0xc4, 0xb5, 0xfd, OP],
                radius: 0.5,
                strength: 0.65,
            },
        ],
    },
    BgPreset {
        name: "Lemon",
        corners: [
            [0xfe, 0xf9, 0xc3, OP],
            [0xfe, 0xf3, 0xc7, OP],
            [0xec, 0xfc, 0xcb, OP],
            [0xd9, 0xf9, 0x9d, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.2, 0.2],
                color: [0xfd, 0xe6, 0x8a, OP],
                radius: 0.55,
                strength: 0.7,
            },
            Blob {
                at: [0.85, 0.85],
                color: [0xbb, 0xf7, 0xd0, OP],
                radius: 0.5,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Berry",
        corners: [
            [0xfb, 0xcf, 0xe8, OP],
            [0xf9, 0xa8, 0xd4, OP],
            [0xe9, 0xd5, 0xff, OP],
            [0xf5, 0xd0, 0xfe, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.75, 0.3],
                color: [0xf4, 0x72, 0xb6, OP],
                radius: 0.5,
                strength: 0.55,
            },
            Blob {
                at: [0.2, 0.8],
                color: [0xc0, 0x84, 0xfc, OP],
                radius: 0.5,
                strength: 0.55,
            },
        ],
    },
    BgPreset {
        name: "Frost",
        corners: [
            [0xe0, 0xf2, 0xfe, OP],
            [0xdb, 0xea, 0xfe, OP],
            [0xed, 0xe9, 0xfe, OP],
            [0xcf, 0xfa, 0xfe, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.3, 0.3],
                color: [0xba, 0xe6, 0xfd, OP],
                radius: 0.6,
                strength: 0.7,
            },
            Blob {
                at: [0.8, 0.75],
                color: [0xdd, 0xd6, 0xfe, OP],
                radius: 0.55,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Sand",
        corners: [
            [0xfe, 0xf3, 0xc7, OP],
            [0xfd, 0xe4, 0xcf, OP],
            [0xfc, 0xe7, 0xd8, OP],
            [0xfb, 0xd5, 0xc0, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.25, 0.25],
                color: [0xfc, 0xd9, 0xb6, OP],
                radius: 0.6,
                strength: 0.7,
            },
            Blob {
                at: [0.8, 0.8],
                color: [0xf9, 0xc8, 0xb0, OP],
                radius: 0.55,
                strength: 0.6,
            },
        ],
    },
    BgPreset {
        name: "Iris",
        corners: [
            [0xc7, 0xd2, 0xfe, OP],
            [0xdd, 0xd6, 0xfe, OP],
            [0xbf, 0xdb, 0xfe, OP],
            [0xe9, 0xd5, 0xff, OP],
        ],
        center: None,
        blobs: &[
            Blob {
                at: [0.35, 0.2],
                color: [0xa5, 0xb4, 0xfc, OP],
                radius: 0.55,
                strength: 0.75,
            },
            Blob {
                at: [0.8, 0.85],
                color: [0xc4, 0xb5, 0xfd, OP],
                radius: 0.55,
                strength: 0.7,
            },
        ],
    },
];

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn lerp_rgba(a: Rgba8, b: Rgba8, t: f32) -> Rgba8 {
    [
        lerp8(a[0], b[0], t),
        lerp8(a[1], b[1], t),
        lerp8(a[2], b[2], t),
        lerp8(a[3], b[3], t),
    ]
}

/// Bilinear four-corner blend with an optional radial bloom in the middle.
pub fn mesh(w: u32, h: u32, preset: &BgPreset) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let (fw, fh) = ((w.max(2) - 1) as f32, (h.max(2) - 1) as f32);
    let [tl, tr, bl, br] = preset.corners;

    for y in 0..h {
        let ty = smoothstep(y as f32 / fh);
        for x in 0..w {
            let tx = smoothstep(x as f32 / fw);
            let top = lerp_rgba(tl, tr, tx);
            let bot = lerp_rgba(bl, br, tx);
            let mut c = lerp_rgba(top, bot, ty);

            for blob in preset.blobs {
                let bx = x as f32 / fw - blob.at[0];
                let by = y as f32 / fh - blob.at[1];
                let d = (bx * bx + by * by).sqrt() / blob.radius.max(0.01);
                if d < 1.0 {
                    // smoothstep falloff: no hard rim where the spot ends.
                    let w = smoothstep(1.0 - d) * blob.strength;
                    c = lerp_rgba(c, blob.color, w);
                }
            }

            if let Some(mid) = preset.center {
                // Normalised distance from the centre, squared falloff. The
                // corner sits at sqrt(0.5²+0.5²) = 1/√2, so that normalises d.
                let dx = x as f32 / fw - 0.5;
                let dy = y as f32 / fh - 0.5;
                let d =
                    ((dx * dx + dy * dy).sqrt() / std::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0);
                let wgt = (1.0 - d).powi(2) * 0.55;
                c = lerp_rgba(c, mid, wgt);
            }
            img.put_pixel(x, y, Rgba(c));
        }
    }
    img
}

pub fn solid(w: u32, h: u32, color: Rgba8) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba(color))
}

/// Linear gradient at an arbitrary angle. 0° runs left→right.
pub fn linear(w: u32, h: u32, a: Rgba8, b: Rgba8, angle_deg: f32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.cos(), rad.sin());
    let (fw, fh) = (w as f32, h as f32);

    // Project the four corners to find the value range along the gradient axis,
    // so the ramp always spans exactly the whole canvas whatever the angle.
    let projections = [0.0 * dx + 0.0 * dy, fw * dx, fh * dy, fw * dx + fh * dy];
    let lo = projections.iter().cloned().fold(f32::INFINITY, f32::min);
    let hi = projections
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let span = (hi - lo).abs().max(1.0);

    for y in 0..h {
        for x in 0..w {
            let t = ((x as f32 * dx + y as f32 * dy) - lo) / span;
            img.put_pixel(x, y, Rgba(lerp_rgba(a, b, t)));
        }
    }
    img
}

/// Cover-fit an image onto the canvas, cropping the overflow (like CSS `cover`).
pub fn image_cover(w: u32, h: u32, src: &RgbaImage) -> RgbaImage {
    if src.width() == 0 || src.height() == 0 {
        return RgbaImage::new(w, h);
    }
    let scale = (w as f32 / src.width() as f32).max(h as f32 / src.height() as f32);
    let nw = ((src.width() as f32 * scale).ceil() as u32).max(1);
    let nh = ((src.height() as f32 * scale).ceil() as u32).max(1);
    let resized = image::imageops::resize(src, nw, nh, image::imageops::FilterType::Triangle);
    let ox = (nw.saturating_sub(w)) / 2;
    let oy = (nh.saturating_sub(h)) / 2;
    image::imageops::crop_imm(&resized, ox, oy, w.min(nw), h.min(nh)).to_image()
}

/// Build a mesh gradient from the screenshot's own dominant colours.
///
/// Screenshots are mostly greys and near-whites, so picking the *most common*
/// colours would produce a beige smear. Weighting by saturation and then
/// pushing the winners toward a vivid, mid-lightness band is what makes the
/// result look chosen rather than averaged.
pub fn auto_preset(shot: &RgbaImage) -> BgPreset {
    const GRID: usize = 6;
    let small = image::imageops::resize(shot, 64, 64, image::imageops::FilterType::Triangle);

    // Coarse RGB histogram, each pixel weighted by how colourful it is.
    let mut bins = vec![(0.0f32, [0.0f32; 3]); GRID * GRID * GRID];
    for px in small.pixels() {
        if px.0[3] < 16 {
            continue;
        }
        let rgb = [
            px.0[0] as f32 / 255.0,
            px.0[1] as f32 / 255.0,
            px.0[2] as f32 / 255.0,
        ];
        let max = rgb[0].max(rgb[1]).max(rgb[2]);
        let min = rgb[0].min(rgb[1]).min(rgb[2]);
        // Ignore near-greys entirely; they carry no hue to build a gradient from.
        let sat = max - min;
        if sat < 0.08 {
            continue;
        }
        let idx = |c: f32| ((c * (GRID - 1) as f32).round() as usize).min(GRID - 1);
        let bin = idx(rgb[0]) * GRID * GRID + idx(rgb[1]) * GRID + idx(rgb[2]);
        let weight = sat * sat;
        bins[bin].0 += weight;
        for (acc, c) in bins[bin].1.iter_mut().zip(rgb) {
            *acc += c * weight;
        }
    }

    let mut ranked: Vec<(f32, [f32; 3])> = bins
        .into_iter()
        .filter(|(w, _)| *w > 0.0)
        .map(|(w, sum)| (w, [sum[0] / w, sum[1] / w, sum[2] / w]))
        .collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Greedily take colours that are not near-duplicates of one already taken.
    let mut picked: Vec<[f32; 3]> = Vec::new();
    for (_, c) in &ranked {
        if picked.iter().all(|p| dist2(*p, *c) > 0.03) {
            picked.push(*c);
        }
        if picked.len() == 4 {
            break;
        }
    }
    // A flat or greyscale screenshot leaves us nothing to work with.
    if picked.is_empty() {
        return BgPreset {
            name: "Auto",
            corners: BG_PRESETS[0].corners,
            center: None,
            blobs: &[],
        };
    }
    while picked.len() < 4 {
        let next = rotate_hue(picked[picked.len() % picked.len().max(1)], 0.12);
        picked.push(next);
    }

    let corners = [
        vivid(picked[0]),
        vivid(picked[1]),
        vivid(picked[2]),
        vivid(picked[3]),
    ];
    BgPreset {
        name: "Auto",
        corners,
        center: None,
        blobs: &[],
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// Push a colour into the saturated, mid-lightness band the presets live in.
fn vivid(rgb: [f32; 3]) -> Rgba8 {
    let (h, s, l) = rgb_to_hsl(rgb);
    let s = s.clamp(0.62, 0.95);
    let l = l.clamp(0.42, 0.62);
    let [r, g, b] = hsl_to_rgb(h, s, l);
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        255,
    ]
}

fn rotate_hue(rgb: [f32; 3], by: f32) -> [f32; 3] {
    let (h, s, l) = rgb_to_hsl(rgb);
    hsl_to_rgb((h + by).rem_euclid(1.0), s, l)
}

fn rgb_to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h.rem_euclid(1.0), s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s.abs() < f32::EPSILON {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0)]
}

/// Paint the canvas background. `bg_image` is the already-loaded wallpaper or
/// custom image; `shot` feeds the Auto palette. Loading is the caller's job so
/// rendering stays pure.
pub fn paint(
    w: u32,
    h: u32,
    settings: &Style,
    bg_image: Option<&RgbaImage>,
    shot: Option<&RgbaImage>,
) -> RgbaImage {
    if settings.background == Background::Auto {
        let preset = match shot {
            Some(s) => auto_preset(s),
            None => BgPreset {
                name: "Auto",
                corners: BG_PRESETS[0].corners,
                center: None,
                blobs: &[],
            },
        };
        return mesh(w, h, &preset);
    }
    match settings.background {
        Background::Auto => unreachable!("handled above"),
        Background::None => RgbaImage::new(w, h), // all zeroes = transparent
        Background::Preset(i) => {
            let preset = BG_PRESETS.get(i).unwrap_or(&BG_PRESETS[0]);
            mesh(w, h, preset)
        }
        Background::Desktop => match bg_image {
            Some(img) => image_cover(w, h, img),
            None => mesh(w, h, &BG_PRESETS[0]),
        },
        Background::Custom => {
            let c = &settings.custom_bg;
            match c.kind {
                CustomKind::Solid => solid(w, h, c.color_a),
                CustomKind::Linear => linear(w, h, c.color_a, c.color_b, c.angle),
                CustomKind::Image => match bg_image {
                    Some(img) => image_cover(w, h, img),
                    None => solid(w, h, c.color_a),
                },
            }
        }
    }
}
