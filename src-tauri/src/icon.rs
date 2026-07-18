//! Dynamic tray-icon renderer: a ring gauge plus the numeric value, color-coded by usage
//! and themed (light/dark). Currently unused (the tray shows the fixed app icon); kept
//! for a possible "dynamic tray icon" option.

#![allow(dead_code)]

use std::f32::consts::{FRAC_PI_2, TAU};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tauri::image::Image;
use tiny_skia::{LineCap, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Stroke, Transform};

const FONT: &[u8] = include_bytes!("../assets/fonts/tray-digits.ttf");
const SIZE: u32 = 32;

type Rgb = (u8, u8, u8);

fn status_color(value: u8, remaining_mode: bool) -> Rgb {
    let danger = (0xE5, 0x3E, 0x3E);
    let warn = (0xF5, 0xA6, 0x23);
    let ok = (0x2E, 0xA0, 0x43);
    if remaining_mode {
        if value < 20 {
            danger
        } else if value < 50 {
            warn
        } else {
            ok
        }
    } else if value > 80 {
        danger
    } else if value > 50 {
        warn
    } else {
        ok
    }
}

fn stroke_ring(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, color: [u8; 4], width: f32) {
    let segs = 96;
    let mut pb = PathBuilder::new();
    for i in 0..=segs {
        let t = start + sweep * (i as f32 / segs as f32);
        let x = cx + r * t.cos();
        let y = cy + r * t.sin();
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Default::default()
        };
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn blend_pixel(pm: &mut Pixmap, x: i32, y: i32, color: Rgb, cov: f32) {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 {
        return;
    }
    let a = cov.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let idx = (y as u32 * pm.width() + x as u32) as usize;
    let px = pm.pixels_mut()[idx];
    let inv = 1.0 - a;
    let sr = color.0 as f32 * a;
    let sg = color.1 as f32 * a;
    let sb = color.2 as f32 * a;
    let dr = px.red() as f32 * inv + sr;
    let dg = px.green() as f32 * inv + sg;
    let db = px.blue() as f32 * inv + sb;
    let da = px.alpha() as f32 * inv + a * 255.0;
    if let Some(newpx) =
        PremultipliedColorU8::from_rgba(dr.round() as u8, dg.round() as u8, db.round() as u8, da.round() as u8)
    {
        pm.pixels_mut()[idx] = newpx;
    }
}

fn draw_run(pm: &mut Pixmap, font: &FontRef, text: &str, scale: PxScale, mut x: f32, baseline: f32, color: Rgb) {
    let sf = font.as_scaled(scale);
    for c in text.chars() {
        let gid = font.glyph_id(c);
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                blend_pixel(pm, px_x, px_y, color, cov);
            });
        }
        x += sf.h_advance(gid);
    }
}

fn draw_number(pm: &mut Pixmap, font: &FontRef, text: &str, cx: f32, cy: f32, fill: Rgb, outline: Rgb) {
    // Large digits so they stay legible when Windows downscales the tray icon to ~16px.
    let px = if text.len() >= 3 { 17.0 } else { 26.0 };
    let scale = PxScale::from(px);
    let sf = font.as_scaled(scale);
    let width: f32 = text.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum();
    let start_x = cx - width / 2.0;
    let baseline = cy + px * 0.34; // approx vertical centering for digits

    // Outline: draw the run offset in 8 directions, then the fill on top.
    let offsets = [
        (-1.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ];
    for (dx, dy) in offsets {
        draw_run(pm, font, text, scale, start_x + dx, baseline + dy, outline);
    }
    draw_run(pm, font, text, scale, start_x, baseline, fill);
}

fn build_gauge_pixmap(value: u8, remaining_mode: bool, dark: bool) -> Option<Pixmap> {
    let mut pm = Pixmap::new(SIZE, SIZE)?;
    let cx = SIZE as f32 / 2.0;
    let cy = cx;
    let radius = 14.0;
    let stroke_w = 2.6;

    let status = status_color(value, remaining_mode);
    let track: [u8; 4] = if dark { [210, 212, 222, 60] } else { [70, 72, 84, 55] };

    // Full faint track, then the progress arc clockwise from the top.
    stroke_ring(&mut pm, cx, cy, radius, -FRAC_PI_2, TAU, track, stroke_w);
    let frac = (value as f32 / 100.0).clamp(0.0, 1.0);
    if frac > 0.001 {
        stroke_ring(
            &mut pm,
            cx,
            cy,
            radius,
            -FRAC_PI_2,
            TAU * frac,
            [status.0, status.1, status.2, 255],
            stroke_w,
        );
    }

    if let Ok(font) = FontRef::try_from_slice(FONT) {
        let outline: Rgb = if dark { (18, 18, 26) } else { (250, 250, 252) };
        draw_number(&mut pm, &font, &value.to_string(), cx, cy, status, outline);
    }

    Some(pm)
}

/// Render the tray gauge for `value` (0..=100). `remaining_mode` chooses the color
/// bands; `dark` themes the neutral ring + number outline.
pub fn render_gauge(value: u8, remaining_mode: bool, dark: bool) -> Option<Image<'static>> {
    build_gauge_pixmap(value, remaining_mode, dark).map(pixmap_to_image)
}

/// Placeholder icon (no data / not signed in): the faint track ring only.
pub fn render_placeholder(dark: bool) -> Option<Image<'static>> {
    let mut pm = Pixmap::new(SIZE, SIZE)?;
    let cx = SIZE as f32 / 2.0;
    let cy = cx;
    let track: [u8; 4] = if dark { [210, 212, 222, 70] } else { [70, 72, 84, 65] };
    stroke_ring(&mut pm, cx, cy, 14.0, -FRAC_PI_2, TAU, track, 2.6);
    Some(pixmap_to_image(pm))
}

/// Convert premultiplied (tiny-skia) -> straight RGBA (tauri Image expects straight).
fn pixmap_to_image(pm: Pixmap) -> Image<'static> {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for p in pm.pixels() {
        let c = p.demultiply();
        rgba.push(c.red());
        rgba.push(c.green());
        rgba.push(c.blue());
        rgba.push(c.alpha());
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{FilterQuality, PixmapPaint, Transform};

    /// Render sample gauges (scaled 4x) + a ~16px tray emulation for visual inspection.
    #[test]
    fn dump_sample_icons() {
        let dir = std::env::temp_dir().join("sessionmeter_icons");
        std::fs::create_dir_all(&dir).unwrap();
        let samples = [
            (92u8, true, true),
            (34u8, true, true),
            (12u8, true, false),
            (100u8, true, true),
            (5u8, true, true),
        ];
        for (v, mode, dark) in samples {
            let pm = build_gauge_pixmap(v, mode, dark).unwrap();
            let mut big = Pixmap::new(SIZE * 4, SIZE * 4).unwrap();
            big.draw_pixmap(
                0,
                0,
                pm.as_ref(),
                &PixmapPaint::default(),
                Transform::from_scale(4.0, 4.0),
                None,
            );
            big.save_png(dir.join(format!("gauge_{v}_{}.png", if dark { "dark" } else { "light" })))
                .unwrap();
        }

        // Emulate the ~16px tray size: downscale 32 -> 16 (bilinear), then magnify 8x.
        let pm = build_gauge_pixmap(92, true, true).unwrap();
        let mut small = Pixmap::new(16, 16).unwrap();
        small.draw_pixmap(
            0,
            0,
            pm.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..Default::default()
            },
            Transform::from_scale(0.5, 0.5),
            None,
        );
        let mut mag = Pixmap::new(128, 128).unwrap();
        mag.draw_pixmap(
            0,
            0,
            small.as_ref(),
            &PixmapPaint::default(),
            Transform::from_scale(8.0, 8.0),
            None,
        );
        mag.save_png(dir.join("tray16_92.png")).unwrap();
        println!("wrote sample icons to {}", dir.display());
    }
}
