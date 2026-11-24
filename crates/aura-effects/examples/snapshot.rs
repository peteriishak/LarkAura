//! Draws simulation frames on the CPU over a generated wallpaper and saves
//! them as PNG files, so effects and visitors can be checked by eye without a
//! desktop. The shapes follow `aura-render/src/shader.wgsl`.
//!
//! Usage: snapshot OUT AURA [SECONDS...]
//! Env: THEME (wallpaper id), MOMENT, VISITOR_AT (seconds), WINDOW=x,y,w,h
//!      (more windows: WINDOW2, WINDOW3, stacked in that order),
//!      PLACEMENT=desktop|everywhere|behind_windows,
//!      SNOWMAN=x (a built snowman), PLOUGH_AT and SLEIGH_AT (seconds),
//!      CURSOR=x,y, TREE=x (a decorated tree), INTENSITY=0-1, CROP=x,y,w,h

use aura_core::{DesktopModel, Trigger, Window};
use aura_effects::{
    FrameInput, Instance, Mask, Simulation,
    sprites::{AtlasLevel, FRAMES, SHAPE_SPRITE, atlas_levels},
};
use aura_format::built_in_by_id;
use aura_wallpaper::{Moment, Theme, render_scene};
use std::{fs::File, io::BufWriter, path::PathBuf};

const W: u32 = 1280;
const H: u32 = 720;

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn shape_alpha(shape: u32, uv: [f32; 2], px: f32, extra: [f32; 2]) -> f32 {
    let (x, y) = (uv[0], uv[1]);
    let r2 = x * x + y * y;
    let soft = clamp01(1.0 - r2);
    let edge = |d: f32| clamp01(d / px + 0.5);
    match shape {
        0 => soft * soft,
        1 => {
            let across = clamp01(1.0 - x.abs());
            let along = clamp01((y + 1.0) * 0.5);
            across * along * (0.45 + 0.55 * along)
        }
        2 => soft * soft * soft,
        3 => {
            let core = clamp01(1.0 - r2 * 18.0);
            soft * soft * soft * 0.42 + core
        }
        4 => {
            let half_width = 1.0 - y * y;
            let vein = 1.0 - 0.28 * clamp01(1.0 - x.abs() * 12.0);
            edge(half_width - x.abs()) * vein
        }
        5 => {
            let t = clamp01((y + 1.0) * 0.5);
            let half_width = clamp01(1.0 - y * y).sqrt() * (1.0 - 0.55 * t);
            let notch = 0.16 - (x * x + (y + 1.02) * (y + 1.02)).sqrt() * 0.8;
            edge((half_width - x.abs()).min(-notch))
        }
        6 => edge(1.0 - r2.sqrt()),
        7 => edge(1.0 - x.abs().max(y.abs())),
        8 => edge(((y + 1.0) * 0.5 - x.abs()).min(1.0 - y)),
        9 => edge(1.0 - x.abs().sqrt() - y.abs().sqrt()),
        10 => {
            if y > 0.0 {
                0.0
            } else {
                edge(1.0 - r2.sqrt())
            }
        }
        12 => {
            let across = clamp01(1.0 - x.abs());
            let along = clamp01((y + 1.0) * 0.5);
            across.sqrt() * along * (0.6 + 0.4 * along)
        }
        13 => {
            let blur = clamp01(extra[0]);
            let r = r2.sqrt();
            let radius = 0.48 + (0.80 - 0.48) * blur;
            let width = (px * 1.3).max(0.14 + (0.46 - 0.14) * blur);
            let disk = clamp01((radius - r) / width + 0.5) * (1.0 - 0.35 * blur);
            let halo = clamp01(1.0 - r).powf(2.1) * 0.42 * (1.0 - blur * 0.7);
            disk.max(halo)
        }
        14 => {
            let u = x * 0.5 + 0.5;
            let up = 0.5 - y * 0.5;
            let top = extra[0] + (extra[1] - extra[0]) * u;
            clamp01((top - up) / (px * 0.5) + 0.5)
        }
        15 => {
            let a = y.atan2(x);
            let lump =
                1.0 + 0.13 * (a * 3.0 + extra[0]).sin() + 0.08 * (a * 5.0 - extra[0] * 1.7).cos();
            let r = r2.sqrt() / lump;
            clamp01((0.86 - r) / (px * 1.4).max(0.12) + 0.5)
        }
        _ => {
            let across = clamp01(1.0 - x.abs());
            across * across
        }
    }
}

/// Follows `snowbank_tone` in the shader.
fn snowbank_tone(uv: [f32; 2], extra: [f32; 2], hw: f32, hh: f32) -> [f32; 3] {
    let u = uv[0] * 0.5 + 0.5;
    let up = 0.5 - uv[1] * 0.5;
    let below = (extra[0] + (extra[1] - extra[0]) * u - up) * hh * 2.0;
    let slope = ((extra[1] - extra[0]) * hh / hw.max(0.5)).clamp(-1.5, 1.5);
    let rim = clamp01(1.0 - below / 3.0);
    let lit = clamp01(1.0 - below / 15.0);
    let mid = clamp01(below / 42.0);
    let deep = clamp01(below / 115.0);
    let crest = clamp01(1.0 - below / 28.0);
    let facing = slope * 0.08 * crest;
    let (top_c, mid_c, far_c) = ([0.94, 0.96, 1.0], [0.84, 0.89, 0.98], [0.69, 0.76, 0.93]);
    let k = lit * 0.72 + rim * 0.28;
    [0, 1, 2].map(|i| {
        let upper = top_c[i] + (mid_c[i] - top_c[i]) * mid;
        let body = (upper + (far_c[i] - upper) * deep) * (1.0 + facing);
        clamp01(body + (1.0 - body) * k)
    })
}

/// Bilinear sample of premultiplied RGBA at normalized coordinates.
fn sample(level: &AtlasLevel, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (level.width as f32, level.height as f32);
    let (fx, fy) = (u * w - 0.5, v * h - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let at = |x: f32, y: f32| {
        let x = x.clamp(0.0, w - 1.0) as u32;
        let y = y.clamp(0.0, h - 1.0) as u32;
        let i = ((y * level.width + x) * 4) as usize;
        [0, 1, 2, 3].map(|c| level.pixels[i + c] as f32 / 255.0)
    };
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1.0, y0),
        at(x0, y0 + 1.0),
        at(x0 + 1.0, y0 + 1.0),
    );
    [0, 1, 2, 3].map(|k| {
        let top = a[k] + (b[k] - a[k]) * tx;
        let bottom = c[k] + (d[k] - c[k]) * tx;
        top + (bottom - top) * ty
    })
}

/// Follows `unmasked` in the shader.
fn unmasked(masks: &[Mask], x: f32, y: f32, layer: f32) -> f32 {
    let mut keep = 1.0;
    for m in masks.iter().skip((layer.max(0.0) + 0.5) as usize) {
        let [rx, ry, rw, rh] = m.rect;
        let d = (x - rx).min(rx + rw - x).min(y - ry).min(ry + rh - y);
        keep *= 1.0 - clamp01(d + 0.5) * m.alpha;
    }
    keep
}

fn composite(base: &[u8], items: &[Instance], masks: &[Mask], atlas: &[AtlasLevel]) -> Vec<u8> {
    let mut px: Vec<[f32; 3]> = base
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| {
            [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
            ]
        })
        .collect();
    for it in items {
        let [cx, cy] = it.center;
        let [hw, hh] = it.half_size;
        let (s, c) = it.rotation.sin_cos();
        let reach = hw.abs().hypot(hh.abs()) + 1.0;
        let x0 = (cx - reach).floor().max(0.0) as u32;
        let x1 = ((cx + reach).ceil().max(0.0) as u32).min(W);
        let y0 = (cy - reach).floor().max(0.0) as u32;
        let y1 = ((cy + reach).ceil().max(0.0) as u32).min(H);
        let shape = (it.shape + 0.5) as u32;
        let pixel_uv = 1.0 / hw.abs().min(hh.abs()).max(0.5);
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                // Inverse of the vertex shader's rotation.
                let lx = dx * c + dy * s;
                let ly = -dx * s + dy * c;
                let uv = [lx / hw, ly / hh];
                if uv[0].abs() > 1.0 || uv[1].abs() > 1.0 {
                    continue;
                }
                let (mut col, a) = if it.shape >= SHAPE_SPRITE - 0.5 {
                    let f = &FRAMES[(it.shape - SHAPE_SPRITE + 0.5) as usize];
                    let level = (f.rect[2] as f32 / (2.0 * hw.abs()))
                        .log2()
                        .floor()
                        .clamp(0.0, atlas.len() as f32 - 1.0)
                        as usize;
                    let lv = &atlas[level];
                    let scale = lv.width as f32 / atlas[0].width as f32;
                    let u = (f.rect[0] as f32 + (uv[0] * 0.5 + 0.5) * f.rect[2] as f32) * scale;
                    let v = (f.rect[1] as f32 + (uv[1] * 0.5 + 0.5) * f.rect[3] as f32) * scale;
                    let t = sample(lv, u / lv.width as f32, v / lv.height as f32);
                    (
                        [t[0] * it.color[0], t[1] * it.color[1], t[2] * it.color[2]],
                        t[3] * it.color[3],
                    )
                } else {
                    let k = clamp01(shape_alpha(shape, uv, pixel_uv, it.extra));
                    let mut col = [it.color[0] * k, it.color[1] * k, it.color[2] * k];
                    if shape == 14 {
                        for (v, t) in
                            col.iter_mut()
                                .zip(snowbank_tone(uv, it.extra, hw.abs(), hh.abs()))
                        {
                            *v *= t;
                        }
                    }
                    if shape == 15 {
                        let lit = clamp01(0.68 - 0.36 * (uv[0] * 0.6 + uv[1] * 0.8));
                        let dark = [0.74, 0.80, 0.95];
                        for (i, v) in col.iter_mut().enumerate() {
                            *v *= dark[i] + (1.0 - dark[i]) * lit;
                        }
                    }
                    (col, it.color[3] * k)
                };
                if shape == 12 {
                    let core = clamp01(1.0 - uv[0].abs() * 2.2) * 0.7;
                    for v in &mut col {
                        *v += (a - *v) * core;
                    }
                }
                let keep = unmasked(masks, x as f32 + 0.5, y as f32 + 0.5, it.layer);
                let a = a * keep;
                for v in &mut col {
                    *v *= keep;
                }
                let p = &mut px[(y * W + x) as usize];
                for i in 0..3 {
                    p[i] = col[i].min(a) + p[i] * (1.0 - a);
                }
            }
        }
    }
    px.iter()
        .flat_map(|p| p.map(|v| (clamp01(v) * 255.0).round() as u8))
        .collect()
}

fn env_list(name: &str) -> Option<Vec<f32>> {
    std::env::var(name)
        .ok()
        .map(|v| v.split(',').map(|n| n.trim().parse().unwrap()).collect())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output folder"));
    let aura_id = args.next().expect("aura id");
    let times: Vec<f32> = args.map(|a| a.parse().unwrap()).collect();
    std::fs::create_dir_all(&out).unwrap();
    let mut aura = built_in_by_id(&aura_id).expect("known aura");
    aura.intensity = 0.8;
    let theme = std::env::var("THEME")
        .ok()
        .and_then(|t| Theme::from_id(&t))
        .unwrap_or_else(|| Theme::for_aura(&aura_id));
    let moment = std::env::var("MOMENT")
        .ok()
        .and_then(|m| Moment::from_id(&m))
        .unwrap_or(Moment::Dusk);
    let base = render_scene(theme, moment, W, H);
    let atlas = atlas_levels().expect("sprite atlas");
    if let Some(i) = env_list("INTENSITY") {
        aura.intensity = i[0];
    }
    let mut desktop = DesktopModel::default();
    for (i, name) in ["WINDOW", "WINDOW2", "WINDOW3"].into_iter().enumerate() {
        if let Some(r) = env_list(name) {
            desktop.windows.push(Window {
                id: i as u64 + 1,
                x: r[0],
                y: r[1],
                w: r[2],
                h: r[3],
            });
        }
    }
    if let Some(d) = env_list("DOCK") {
        desktop.docks.push(Window {
            id: 0xD0CC_0001,
            x: d[0],
            y: d[1],
            w: d[2],
            h: d[3],
        });
    }
    if let Some(c) = env_list("CURSOR") {
        desktop.cursor = Some([c[0], c[1]]);
    }
    let mut sim = Simulation::new(aura, 11, W as f32, H as f32);
    if let Ok(p) = std::env::var("PLACEMENT") {
        sim.set_tuning(aura_core::Tuning {
            placement: serde_json::from_str(&format!("\"{p}\"")).expect("a placement"),
            ..Default::default()
        });
    }
    if let Some(x) = env_list("TREE") {
        sim.set_decorations(aura_core::Decorations {
            trees: vec![aura_core::TreeDecoration {
                x: x[0],
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    let shake_at = env_list("SHAKE_AT").map(|v| v[0]);
    let base_win_x = desktop.windows.first().map(|w| w.x).unwrap_or(0.0);
    let sleigh_at = env_list("SLEIGH_AT").map(|v| v[0]);
    // Each time listed sends the next visitor; snow visitors take turns.
    let visitor_at = env_list("VISITOR_AT").unwrap_or_default();
    let plough_at = env_list("PLOUGH_AT").map(|v| v[0]);
    // Times at which the button is held for 0.6 s at CURSOR, packing a snowball.
    let holds = env_list("HOLDS").unwrap_or_default();
    if let Some(x) = env_list("SNOWMAN") {
        sim.debug_build_snowman(x[0], H as f32 - 1.0);
    }
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut out_items = Vec::new();
    for &target in &times {
        while t < target {
            if visitor_at.iter().any(|&v| t <= v && v < t + dt) {
                sim.trigger(Trigger::Visitor);
            }
            if plough_at.is_some_and(|v| t <= v && v < t + dt) {
                sim.trigger(Trigger::Plough);
            }
            if holds.iter().any(|&v| t <= v && v < t + dt) {
                desktop.presses += 1;
                desktop.pointer_down = true;
            }
            if holds.iter().any(|&v| t <= v + 0.6 && v + 0.6 < t + dt) {
                desktop.pointer_down = false;
            }
            if sleigh_at.is_some_and(|v| t <= v && v < t + dt) {
                sim.trigger(Trigger::Sleigh);
            }
            if let Some(st) = shake_at
                && let Some(w) = desktop.windows.first_mut()
                && t >= st
                && t < st + 0.5
            {
                let phase = (t - st) / 0.5;
                w.x = base_win_x + (phase * std::f32::consts::TAU * 2.5).sin() * 70.0;
            }
            sim.step(FrameInput {
                dt,
                presence: 1.0,
                quality_scale: 1.0,
                desktop: &desktop,
            });
            t += dt;
        }
        sim.write_instances(&mut out_items);
        // Fill in windows and dock under the effects, as the overlay sits above
        // both on a real desktop.
        let mut under = base.clone();
        for w in &desktop.windows {
            for x in (w.x as u32)..((w.x + w.w) as u32).min(W) {
                for y in (w.y as u32)..((w.y + w.h) as u32).min(H) {
                    let i = ((y * W + x) * 3) as usize;
                    under[i..i + 3].copy_from_slice(&[38, 40, 48]);
                }
            }
        }
        for d in &desktop.docks {
            for x in (d.x as u32)..((d.x + d.w) as u32).min(W) {
                for y in (d.y as u32)..((d.y + d.h) as u32).min(H) {
                    let i = ((y * W + x) * 3) as usize;
                    under[i..i + 3].copy_from_slice(&[28, 30, 38]);
                }
            }
        }
        let mut rgb = composite(&under, &out_items, sim.masks(), &atlas);
        let file = out.join(format!("{aura_id}-{target:05.1}.png"));
        let (mut cw, mut ch) = (W, H);
        if let Some(c) = env_list("CROP") {
            let [cx, cy] = [c[0] as u32, c[1] as u32];
            (cw, ch) = (c[2] as u32, c[3] as u32);
            rgb = (cy..cy + ch)
                .flat_map(|y| {
                    let i = ((y * W + cx) * 3) as usize;
                    rgb[i..i + (cw * 3) as usize].to_vec()
                })
                .collect();
        }
        let mut enc = png::Encoder::new(BufWriter::new(File::create(&file).unwrap()), cw, ch);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        eprintln!("{} ({} items)", file.display(), out_items.len());
    }
}
