//! The compositing pipeline.
//!
//! Order matters: annotate → balance → inset frame → shadow → background →
//! ratio fit → watermark. Everything is driven by `Style` plus a `scale`
//! factor so the on-screen preview and the full-resolution export run identical
//! code.

pub mod background;
pub mod frame;
pub mod icon;
pub mod text;
pub mod watermark;

use ab_glyph::FontArc;
use image::{Rgba, RgbaImage};

use crate::annotate::Layer;
use crate::settings::{Ratio, Style};
use frame::{Shadow, blend, rounded_coverage};

pub struct Scene<'a> {
    pub shot: &'a RgbaImage,
    pub style: &'a Style,
    /// 1.0 for export, <1.0 for the preview.
    pub scale: f32,
    pub bg_image: Option<&'a RgbaImage>,
    pub font: Option<&'a FontArc>,
    pub layers: &'a [Layer],
    /// Logo for the watermark, when one is chosen.
    pub logo: Option<&'a RgbaImage>,
}

impl<'a> Scene<'a> {
    /// A scene with no annotations — handy in tests and for one-off renders.
    pub fn plain(shot: &'a RgbaImage, style: &'a Style, scale: f32) -> Self {
        Self {
            shot,
            style,
            scale,
            bg_image: None,
            font: None,
            layers: &[],
            logo: None,
        }
    }
}

/// Where the screenshot ended up on the canvas. Enough for the UI to map
/// pointer positions back onto original screenshot pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Where the screenshot landed on the canvas: `(x, y, w, h)` in canvas px.
    pub content: (i64, i64, u32, u32),
    /// Offset introduced by the Balance crop, in `scene.shot` pixels.
    pub crop_offset: (u32, u32),
    /// Extra scaling applied to the shot to fit a fixed canvas size.
    pub shot_fit: f32,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            content: (0, 0, 0, 0),
            crop_offset: (0, 0),
            shot_fit: 1.0,
        }
    }
}

pub struct Rendered {
    pub image: RgbaImage,
    pub geom: Geometry,
}

impl Geometry {
    /// Map a canvas pixel back to a pixel in the *original, unscaled*
    /// screenshot — the coordinate space annotation layers live in.
    ///
    /// `scene_scale` is the `Scene::scale` that produced this render.
    pub fn canvas_to_shot(&self, cx: f32, cy: f32, scene_scale: f32) -> [f32; 2] {
        let (ox, oy, _, _) = self.content;
        let fit = self.shot_fit.max(f32::EPSILON);
        let scale = scene_scale.max(f32::EPSILON);
        [
            ((cx - ox as f32) / fit + self.crop_offset.0 as f32) / scale,
            ((cy - oy as f32) / fit + self.crop_offset.1 as f32) / scale,
        ]
    }

    /// The inverse of [`Self::canvas_to_shot`].
    pub fn shot_to_canvas(&self, sx: f32, sy: f32, scene_scale: f32) -> [f32; 2] {
        let (ox, oy, _, _) = self.content;
        [
            (sx * scene_scale - self.crop_offset.0 as f32) * self.shot_fit + ox as f32,
            (sy * scene_scale - self.crop_offset.1 as f32) * self.shot_fit + oy as f32,
        ]
    }
}

fn px(v: u32, scale: f32) -> u32 {
    (v as f32 * scale).round() as u32
}

pub fn render(scene: &Scene) -> RgbaImage {
    render_detailed(scene).image
}

/// Render at full size with every resource the style asks for, loaded fresh.
///
/// This is what a one-shot render needs — `shotr --capture --copy` opens no
/// window, so no editor is holding a decoded wallpaper, a system font and a
/// watermark logo. Reaching for [`Scene::plain`] instead looks equivalent and is
/// not: it leaves all three `None`, so a style with a wallpaper background or a
/// lettered watermark would copy differently from the way it exports.
pub fn beautify(shot: &RgbaImage, style: &Style) -> RgbaImage {
    let open = |p: &std::path::PathBuf| image::open(p).ok().map(|i| i.to_rgba8());
    let bg_image = style.background_image_path().as_ref().and_then(open);
    let logo = style.watermark_image.as_ref().and_then(open);
    let font = text::load_system_font().map(|(_, font)| font);

    render(&Scene {
        shot,
        style,
        scale: 1.0,
        bg_image: bg_image.as_ref(),
        font: font.as_ref(),
        layers: &[],
        logo: logo.as_ref(),
    })
}

pub fn render_detailed(scene: &Scene) -> Rendered {
    let s = scene.style;
    let scale = scene.scale.max(0.01);

    // 1. Annotations are baked into the screenshot before anything else, so
    //    Balance sees them and the frame clips them like real content.
    let annotated;
    let shot: &RgbaImage = if scene.layers.is_empty() {
        scene.shot
    } else {
        let mut copy = scene.shot.clone();
        crate::annotate::apply(&mut copy, scene.layers, scale, scene.font);
        annotated = copy;
        &annotated
    };

    // 2. Balance: trim uniform margins so the subject sits centred.
    let balanced;
    let mut crop_offset = (0, 0);
    let shot: &RgbaImage = if s.balance {
        let (x, y, w, h) = frame::content_box(shot, 6);
        if w != shot.width() || h != shot.height() {
            crop_offset = (x, y);
            balanced = image::imageops::crop_imm(shot, x, y, w, h).to_image();
            &balanced
        } else {
            shot
        }
    } else {
        shot
    };

    let pad = px(s.padding, scale);
    let radius = px(s.radius, scale);
    let mut inset = px(s.inset, scale);

    // 3. Canvas size and how much the screenshot must shrink to fit it.
    let (cw, ch, shot_fit) = layout(shot.width(), shot.height(), pad, inset, s.ratio, scale);

    let fitted;
    let shot: &RgbaImage = if shot_fit < 0.999 {
        let nw = ((shot.width() as f32 * shot_fit).round() as u32).max(1);
        let nh = ((shot.height() as f32 * shot_fit).round() as u32).max(1);
        fitted = image::imageops::resize(shot, nw, nh, image::imageops::FilterType::Lanczos3);
        &fitted
    } else {
        shot
    };

    let (sw, sh) = (shot.width(), shot.height());
    // Never let the frame eat more than the canvas can hold.
    inset = inset
        .min(cw.saturating_sub(sw) / 2)
        .min(ch.saturating_sub(sh) / 2);
    let (iw, ih) = (sw + inset * 2, sh + inset * 2);
    let ox = ((cw.saturating_sub(iw)) / 2) as i64;
    let oy = ((ch.saturating_sub(ih)) / 2) as i64;

    // 4. Background.
    let mut canvas = background::paint(cw, ch, s, scene.bg_image, Some(shot));

    // 5. Drop shadow, cast by the outer (inset) rectangle.
    if let Some(shadow) = Shadow::from_strength(s.shadow, scale) {
        let place = frame::Placement {
            origin: (ox, oy),
            size: (iw, ih),
            radius,
        };
        let layer = frame::shadow_layer((cw, ch), &place, &shadow);
        image::imageops::overlay(&mut canvas, &layer, 0, 0);
    }

    // 6. The inset frame, then the screenshot inside it. A frame of thickness
    //    `inset` around an outer radius R has an inner radius of R - inset.
    if inset > 0 {
        // The detected colour makes the frame read as part of the captured
        // window rather than a white band stuck around it.
        let c = if s.inset_auto_color {
            frame::border_color(shot, 8).unwrap_or(Rgba(s.inset_color))
        } else {
            Rgba(s.inset_color)
        };
        for y in 0..ih {
            for x in 0..iw {
                let cov = rounded_coverage(x, y, iw, ih, radius);
                if cov > 0.0 {
                    blend(
                        &mut canvas,
                        (ox + x as i64) as u32,
                        (oy + y as i64) as u32,
                        c,
                        cov,
                    );
                }
            }
        }
    }

    let inner_radius = radius.saturating_sub(inset);
    let sx = ox + inset as i64;
    let sy = oy + inset as i64;
    for y in 0..sh {
        for x in 0..sw {
            let cov = rounded_coverage(x, y, sw, sh, inner_radius);
            if cov > 0.0 {
                blend(
                    &mut canvas,
                    (sx + x as i64) as u32,
                    (sy + y as i64) as u32,
                    *shot.get_pixel(x, y),
                    cov,
                );
            }
        }
    }

    // 7. Watermark.
    watermark::draw(&mut canvas, s, scene.font, scene.logo);

    Rendered {
        image: canvas,
        geom: Geometry {
            content: (sx, sy, sw, sh),
            crop_offset,
            shot_fit,
        },
    }
}

/// Returns `(canvas_w, canvas_h, shot_scale)`. `shot_scale` is 1.0 unless a
/// fixed output size forces the screenshot to shrink.
fn layout(sw: u32, sh: u32, pad: u32, inset: u32, ratio: Ratio, scale: f32) -> (u32, u32, f32) {
    let base_w = sw + inset * 2 + pad * 2;
    let base_h = sh + inset * 2 + pad * 2;

    match ratio {
        Ratio::Auto => (base_w.max(1), base_h.max(1), 1.0),

        // Grow the short axis; never crop.
        Ratio::Aspect(r) => {
            let r = r.max(0.01);
            let current = base_w as f32 / base_h as f32;
            if current < r {
                (
                    ((base_h as f32 * r).round() as u32).max(1),
                    base_h.max(1),
                    1.0,
                )
            } else {
                (
                    base_w.max(1),
                    ((base_w as f32 / r).round() as u32).max(1),
                    1.0,
                )
            }
        }

        Ratio::Size(tw, th) => {
            let cw = px(tw, scale).max(1);
            let ch = px(th, scale).max(1);
            // Room left for the screenshot itself after padding and frame.
            let avail_w = cw.saturating_sub(pad * 2 + inset * 2) as f32;
            let avail_h = ch.saturating_sub(pad * 2 + inset * 2) as f32;
            if avail_w <= 1.0 || avail_h <= 1.0 {
                return (cw, ch, 0.01);
            }
            let fit = (avail_w / sw as f32).min(avail_h / sh as f32).min(1.0);
            (cw, ch, fit.max(0.01))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::{Layer, Tool};

    /// `Scene::plain` leaves the font `None`, and a lettered watermark drawn
    /// with no font draws nothing. If `beautify` ever produces what `plain`
    /// does, `--copy` has quietly stopped putting the watermark on an image the
    /// editor would have lettered.
    #[test]
    fn beautify_loads_the_font_a_lettered_watermark_needs() {
        // A machine carrying none of the candidate fonts cannot show a
        // difference either way, and this suite may not fetch one.
        if text::load_system_font().is_none() {
            return;
        }

        let shot = RgbaImage::from_pixel(80, 60, Rgba([90, 120, 200, 255]));
        let style = Style {
            watermark: true,
            watermark_text: "shotr".to_owned(),
            ..Style::default()
        };

        assert_ne!(
            beautify(&shot, &style).into_raw(),
            render(&Scene::plain(&shot, &style, 1.0)).into_raw(),
            "the watermark font was never loaded, so a copy loses lettering the \
             editor draws for the same style"
        );
    }

    #[test]
    fn auto_just_adds_padding_and_frame() {
        let (w, h, fit) = layout(800, 600, 40, 10, Ratio::Auto, 1.0);
        assert_eq!((w, h), (800 + 100, 600 + 100));
        assert_eq!(fit, 1.0);
    }

    #[test]
    fn aspect_grows_the_short_axis_and_never_shrinks() {
        // A tall image asked for 16:9 must gain width, not lose height.
        let (w, h, fit) = layout(400, 900, 0, 0, Ratio::Aspect(16.0 / 9.0), 1.0);
        assert_eq!(h, 900);
        assert!(w >= 400);
        assert!((w as f32 / h as f32 - 16.0 / 9.0).abs() < 0.01);
        assert_eq!(fit, 1.0);

        // ...and a wide one must gain height.
        let (w, h, _) = layout(1600, 200, 0, 0, Ratio::Aspect(1.0), 1.0);
        assert_eq!(w, 1600);
        assert_eq!(h, 1600);
    }

    #[test]
    fn fixed_size_pins_the_canvas_and_shrinks_the_shot_to_fit() {
        let (w, h, fit) = layout(3440, 1440, 64, 0, Ratio::Size(1080, 1080), 1.0);
        assert_eq!((w, h), (1080, 1080));
        assert!(fit < 1.0);
        assert!((3440.0 * fit).round() as u32 + 128 <= 1080);
        assert!((1440.0 * fit).round() as u32 + 128 <= 1080);
    }

    #[test]
    fn fixed_size_does_not_upscale_a_small_shot() {
        let (_, _, fit) = layout(100, 100, 20, 0, Ratio::Size(1920, 1080), 1.0);
        assert_eq!(fit, 1.0);
    }

    #[test]
    fn preview_scale_shrinks_a_fixed_canvas_proportionally() {
        let (fw, fh, _) = layout(800, 600, 40, 0, Ratio::Size(1200, 630), 1.0);
        let (hw, hh, _) = layout(800, 600, 40, 0, Ratio::Size(1200, 630), 0.5);
        assert_eq!((fw, fh), (1200, 630));
        assert_eq!((hw, hh), (600, 315));
    }

    #[test]
    fn absurd_padding_does_not_panic_or_produce_zero_size() {
        let (w, h, fit) = layout(800, 600, 5000, 200, Ratio::Size(320, 240), 1.0);
        assert!(w > 0 && h > 0);
        assert!(fit > 0.0);
    }

    /// The whole pipeline, end to end, on every ratio kind.
    #[test]
    fn render_produces_the_size_layout_promised() {
        let shot = RgbaImage::from_pixel(400, 300, Rgba([10, 120, 200, 255]));
        for ratio in [Ratio::Auto, Ratio::Aspect(1.0), Ratio::Size(600, 600)] {
            let settings = Style {
                ratio,
                ..Default::default()
            };
            let out = render(&Scene::plain(&shot, &settings, 1.0));
            let (w, h, _) = layout(400, 300, settings.padding, settings.inset, ratio, 1.0);
            assert_eq!((out.width(), out.height()), (w, h), "ratio {ratio:?}");
        }
    }

    #[test]
    fn transparent_background_keeps_alpha_at_the_corners() {
        let shot = RgbaImage::from_pixel(200, 150, Rgba([255, 0, 0, 255]));
        let settings = Style {
            background: crate::settings::Background::None,
            shadow: 0,
            ..Default::default()
        };
        let out = render(&Scene::plain(&shot, &settings, 1.0));
        assert_eq!(out.get_pixel(0, 0).0[3], 0, "corner must stay transparent");
        let (cx, cy) = (out.width() / 2, out.height() / 2);
        assert_eq!(
            out.get_pixel(cx, cy).0[3],
            255,
            "the shot itself must be opaque"
        );
    }

    #[test]
    fn content_rect_reports_where_the_shot_landed() {
        let shot = RgbaImage::from_pixel(400, 300, Rgba([0, 0, 0, 255]));
        let settings = Style {
            padding: 50,
            inset: 10,
            shadow: 0,
            ..Default::default()
        };
        let out = render_detailed(&Scene::plain(&shot, &settings, 1.0));
        assert_eq!(out.geom.content, (60, 60, 400, 300));
    }

    #[test]
    fn canvas_and_shot_coordinates_round_trip() {
        let shot = RgbaImage::from_pixel(400, 300, Rgba([0, 0, 0, 255]));
        let settings = Style {
            padding: 40,
            shadow: 0,
            ..Default::default()
        };
        for scale in [1.0, 0.5] {
            let out = render_detailed(&Scene::plain(&shot, &settings, scale));
            let canvas = out.geom.shot_to_canvas(123.0, 77.0, scale);
            let back = out.geom.canvas_to_shot(canvas[0], canvas[1], scale);
            assert!((back[0] - 123.0).abs() < 0.01, "scale {scale}: {back:?}");
            assert!((back[1] - 77.0).abs() < 0.01, "scale {scale}: {back:?}");
        }
    }

    /// With Balance on, the crop shifts the image — mapping must undo that or
    /// every annotation would land in the wrong place.
    #[test]
    fn coordinate_mapping_survives_the_balance_crop() {
        let mut shot = RgbaImage::from_pixel(300, 300, Rgba([255, 255, 255, 255]));
        for y in 40..200 {
            for x in 50..210 {
                shot.put_pixel(x, y, Rgba([10, 10, 10, 255]));
            }
        }
        let settings = Style {
            balance: true,
            padding: 20,
            shadow: 0,
            ..Default::default()
        };
        let out = render_detailed(&Scene::plain(&shot, &settings, 1.0));
        assert_ne!(
            out.geom.crop_offset,
            (0, 0),
            "expected Balance to trim something"
        );

        let canvas = out.geom.shot_to_canvas(120.0, 90.0, 1.0);
        let back = out.geom.canvas_to_shot(canvas[0], canvas[1], 1.0);
        assert!((back[0] - 120.0).abs() < 0.01);
        assert!((back[1] - 90.0).abs() < 0.01);
    }

    #[test]
    fn annotations_reach_the_output() {
        let shot = RgbaImage::from_pixel(200, 200, Rgba([255, 255, 255, 255]));
        let settings = Style {
            padding: 0,
            radius: 0,
            shadow: 0,
            ..Default::default()
        };
        let mut layer = Layer::new(Tool::Rect, [20.0, 20.0], [255, 0, 0, 255], 6.0, 20.0, 8.0);
        layer.b = [150.0, 150.0];
        let layers = vec![layer];

        let scene = Scene {
            layers: &layers,
            ..Scene::plain(&shot, &settings, 1.0)
        };
        let out = render(&scene);
        assert_ne!(
            out.get_pixel(20, 80).0,
            [255, 255, 255, 255],
            "the rect outline should be visible in the composed image"
        );
    }
}
