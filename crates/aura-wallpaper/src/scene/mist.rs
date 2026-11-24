//! Misty Lake: an ink-wash morning of forested ridges dissolving into fog,
//! one behind another, above a still lake. A pale sun shows through, a
//! lone fisher sits in a rowboat, a pier with a lantern reaches out from
//! the firs and a heron waits in the reeds.

use super::canvas::{C, lerp, rgb, scale, smooth};
use super::noise::{fbm, fbm1, noise};
use super::painter::{Fir, LAMP, Painter, WARM};

const WATER: f32 = 0.665;

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(500, 0.4, 0.6);
    p.body(0.65);
    let fog = lerp(p.l.horizon, p.lit(rgb(0xeef2ee)), 0.35);

    // Ridges from far to near, each fainter than the one in front.
    let ink = p.lit(rgb(0x1e3a36));
    let layers = [
        (0.5, 0.36, 1.2, 0.86, 0.8),
        (0.54, 0.28, 1.6, 0.74, 0.75),
        (0.58, 0.22, 2.0, 0.6, 0.7),
        (0.615, 0.15, 2.4, 0.45, 0.6),
        (0.645, 0.09, 2.8, 0.3, 0.5),
        (0.668, 0.045, 3.4, 0.15, 0.3),
    ];
    for (i, &(base, amp, freq, haze, sharp)) in layers.iter().enumerate() {
        let top = p.skyline(base, amp, freq, seed + i as u32 * 11, sharp);
        let c = lerp(ink, fog, haze);
        let lower = lerp(c, fog, 0.35);
        p.cv.layer_to(
            &top,
            (WATER + 0.005) * h,
            |x, y, d| {
                let n = fbm(x / (h * 0.03), y / (h * 0.05), seed + 50 + i as u32, 3);
                let fade = smooth(0.0, h * 0.09, d);
                scale(lerp(c, lower, fade), 0.96 + n * 0.08)
            },
            1.0,
        );
        // Firs bristling along each crest, smaller further away.
        let tree = Fir {
            dark: c,
            mid: c,
            light: lerp(c, fog, 0.12),
            trunk: c,
            snow: 0.0,
            snow_lit: c,
            snow_shade: c,
            width: 0.24,
            tiers: 5.0,
        };
        let size = 0.01 + i as f32 * 0.006;
        let count = (w / (size * h) * 4.0) as usize;
        p.firs(
            &top,
            -0.02,
            1.02,
            count.min(1400),
            (size * 0.5, size * 1.3),
            &tree,
            size * 0.6,
        );
        // Fog pooling in the valley in front of the ridge.
        p.fog(
            base + 0.01,
            0.018 + i as f32 * 0.002,
            0.75 - i as f32 * 0.06,
            fog,
            seed + 80 + i as u32,
        );
    }

    // The lake mirrors everything.
    let tint = lerp(fog, p.lit(rgb(0x4a6a68)), 0.35);
    let full = |_: f32, _: f32| 1.0;
    p.reflect(WATER, WATER, 1.0, tint, 0.28, 2.0, &full);
    let ripple = lerp(fog, rgb(0xffffff), 0.3);
    p.ripples(WATER + 0.01, 1.0, 320, ripple, 0.22, &full);
    p.fog(WATER + 0.006, 0.012, 0.55, fog, seed + 90);
    if p.l.body != super::Body::None {
        let (bx, _) = p.body_pos();
        p.glitter(
            bx,
            WATER + 0.01,
            0.95,
            0.02,
            lerp(p.l.body_color, rgb(0xffffff), 0.3),
            0.35,
            500,
        );
    }

    // The boat and its fisher, mirrored.
    rowboat(p, 0.6 * w, 0.745 * h, 0.032 * h, ink);

    // Firs on the near shore, the pier and the reeds.
    let shore = move |x: f32| {
        h * (0.7
            + 0.02 * (fbm1(x / h * 6.0, seed + 3, 3) - 0.5)
            + 0.25 * smooth(0.16 * w, 0.0, x) * 0.0)
    };
    let near_tree = Fir {
        dark: scale(ink, 0.7),
        mid: ink,
        light: lerp(ink, fog, 0.15),
        trunk: scale(ink, 0.6),
        snow: 0.0,
        snow_lit: ink,
        snow_shade: ink,
        width: 0.18,
        tiers: 9.0,
    };
    p.cv.region(0.0, 0.66 * h, 0.2 * w, 0.75 * h, |x, y| {
        let top = shore(x) - h * 0.012 * smooth(0.2 * w, 0.05 * w, x);
        let edge = 0.17 * w + (noise(y / (h * 0.01), 0.3, seed + 4) - 0.5) * h * 0.02;
        let cover = (y - top + 0.5).clamp(0.0, 1.0)
            * (edge - x + 0.5).clamp(0.0, 1.0)
            * (0.715 * h - y + 0.5).clamp(0.0, 1.0);
        (cover > 0.0).then_some((scale(ink, 0.8), cover))
    });
    let mut stand: Vec<(f32, f32)> = (0..9)
        .map(|i| {
            (
                p.rng.range(-0.04, 0.15) * w,
                p.rng.range(0.4, 0.8) + i as f32 * 0.0,
            )
        })
        .collect();
    stand.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (k, &(x, s)) in stand.iter().enumerate() {
        let hgt = s * h;
        let base = shore(x) + 0.008 * h;
        // Their reflections first, then the trees.
        let refl = lerp(scale(ink, 0.8), tint, 0.45);
        let r = Fir {
            dark: refl,
            mid: refl,
            light: refl,
            trunk: refl,
            ..near_tree
        };
        mirror_fir(p, x, base, hgt * 0.5, &r, seed + 200 + k as u32);
        p.fir(x, base, hgt, &near_tree, seed + 100 + k as u32);
    }
    pier(p, ink, fog);
    reeds(p, ink, fog);
    heron(p, 0.84 * w, 0.88 * h, 0.13 * h, ink);
    let _ = u;
}

/// A fir's reflection: upside down and squashed into the water.
fn mirror_fir(p: &mut Painter, x: f32, base: f32, hgt: f32, f: &Fir, seed: u32) {
    let wid = hgt * f.width * 2.0;
    p.cv.region(x - wid, base, x + wid, base + hgt, |px, py| {
        let t = (py - base) / hgt;
        let half = wid * (1.0 - t) * (0.5 + 0.5 * (1.0 - ((t * 8.0).fract())).powf(1.5));
        let wob = (noise(py / 3.0, 0.5, seed) - 0.5) * 3.0;
        let cover = (half - (px - x + wob).abs() + 0.5).clamp(0.0, 1.0) * (1.0 - t).powf(0.5);
        (cover > 0.0).then_some((f.mid, cover * 0.75))
    });
}

fn rowboat(p: &mut Painter, x: f32, y: f32, s: f32, ink: C) {
    let hull = lerp(p.lit(rgb(0x5a3a2a)), ink, 0.3);
    let hull_dark = scale(hull, 0.65);
    let fog = p.l.horizon;
    // Reflection.
    let refl = lerp(hull_dark, fog, 0.4);
    p.cv.region(x - s * 2.2, y, x + s * 2.2, y + s * 1.6, |px, py| {
        let t = (py - y) / (s * 1.4);
        let half = s * 2.0 * (1.0 - t * t).max(0.0);
        let wob = (noise(py / 2.5, 0.0, 7) - 0.5) * s * 0.15;
        let cover = (half - (px - x + wob).abs() + 0.5).clamp(0.0, 1.0) * (1.0 - t).max(0.0);
        (cover > 0.0).then_some((refl, cover * 0.6))
    });
    // Hull, gunwale and the fisher.
    p.cv.poly(
        &[
            [x - s * 2.1, y - s * 0.55],
            [x + s * 2.1, y - s * 0.6],
            [x + s * 1.5, y],
            [x - s * 1.6, y],
        ],
        hull,
        1.0,
    );
    p.cv.line(
        [x - s * 2.1, y - s * 0.55],
        [x + s * 2.1, y - s * 0.6],
        s * 0.1,
        hull_dark,
        1.0,
    );
    let body = lerp(ink, fog, 0.15);
    p.cv.poly(
        &[
            [x - s * 0.5, y - s * 0.55],
            [x + s * 0.1, y - s * 0.55],
            [x - s * 0.05, y - s * 1.45],
            [x - s * 0.45, y - s * 1.45],
        ],
        body,
        1.0,
    );
    p.cv.disk(x - s * 0.25, y - s * 1.7, s * 0.24, body, 1.0);
    p.cv.poly(
        &[
            [x - s * 0.7, y - s * 1.82],
            [x + s * 0.2, y - s * 1.82],
            [x - s * 0.25, y - s * 2.1],
        ],
        body,
        1.0,
    );
    // The rod and line.
    let tip = [x + s * 3.4, y - s * 2.6];
    p.cv.taper(
        [x - s * 0.1, y - s * 1.1],
        tip,
        s * 0.06,
        s * 0.02,
        body,
        1.0,
    );
    p.cv.line(tip, [tip[0] + s * 0.1, y + s * 0.1], s * 0.015, body, 0.6);
    // Rings spreading from the line.
    for k in 1..4 {
        let r = s * 0.3 * k as f32;
        let c = lerp(fog, rgb(0xffffff), 0.3);
        p.cv.region(
            tip[0] - r * 2.0,
            y - r * 0.4,
            tip[0] + r * 2.0,
            y + r * 0.6,
            |px, py| {
                let (dx, dy) = (
                    (px - tip[0] - s * 0.1) / (r * 1.8),
                    (py - y - s * 0.1) / (r * 0.35),
                );
                let d = (dx * dx + dy * dy).sqrt();
                let a = smooth(0.12, 0.0, (d - 1.0).abs()) * 0.4 / k as f32;
                (a > 0.0).then_some((c, a))
            },
        );
    }
}

fn pier(p: &mut Painter, ink: C, fog: C) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let wood = lerp(p.lit(rgb(0x6a5442)), fog, 0.15);
    let dark = lerp(scale(ink, 0.9), fog, 0.1);
    // Planks running out from the shore, narrowing with distance.
    let near = [0.04 * w, 0.82 * h];
    let far = [0.34 * w, 0.755 * h];
    let (wn, wf) = (0.03 * h, 0.012 * h);
    let posts = 7;
    for i in 0..=posts {
        let t = i as f32 / posts as f32;
        let x = near[0] + (far[0] - near[0]) * t;
        let y = near[1] + (far[1] - near[1]) * t;
        let wd = wn + (wf - wn) * t;
        let len = wd * 1.8;
        for side in [-0.5f32, 0.5] {
            p.cv.taper(
                [x, y + side * wd * 0.4],
                [x, y + side * wd * 0.4 + len],
                wd * 0.18,
                wd * 0.16,
                dark,
                1.0,
            );
            let refl = lerp(dark, fog, 0.45);
            p.cv.taper(
                [x, y + side * wd * 0.4 + len],
                [x, y + side * wd * 0.4 + len * 2.2],
                wd * 0.16,
                wd * 0.08,
                refl,
                0.6,
            );
        }
    }
    p.cv.region(
        near[0] - wn,
        far[1] - wf,
        far[0] + wf,
        near[1] + wn,
        |px, py| {
            let t = ((px - near[0]) / (far[0] - near[0])).clamp(0.0, 1.0);
            let cy = near[1] + (far[1] - near[1]) * t;
            let wd = wn + (wf - wn) * t;
            if px > far[0] + 1.0 {
                return None;
            }
            let cover = (wd * 0.5 - (py - cy).abs() + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            let plank = ((px - near[0]) / (wd * 0.35)).fract();
            let c = lerp(wood, scale(wood, 0.7), smooth(0.85, 1.0, plank));
            Some((if py > cy + wd * 0.3 { dark } else { c }, cover))
        },
    );
    // A lantern on the last post, and the lark on the next one in.
    let lamps = p.lamps();
    let lx = far[0];
    let ly = far[1] - wf * 2.2;
    p.cv.line([lx, far[1]], [lx, ly], 1.6 * u, dark, 1.0);
    let on = 0.25 + 0.75 * lamps;
    p.cv.glow(lx, ly, 0.05 * h, WARM, 0.45 * on);
    p.cv.rect(
        lx - 0.004 * h,
        ly - 0.007 * h,
        lx + 0.004 * h,
        ly + 0.002 * h,
        lerp(dark, LAMP, on),
        1.0,
    );
    p.cv.glow_ellipse(lx, ly + 0.06 * h, 0.01 * h, 0.04 * h, WARM, 0.25 * on);
    let t = 0.55;
    let px = near[0] + (far[0] - near[0]) * t;
    let py = near[1] + (far[1] - near[1]) * t;
    let wd = wn + (wf - wn) * t;
    p.lark_perched(px, py - wd * 0.45, wd * 1.1, 1.0);
}

fn reeds(p: &mut Painter, ink: C, fog: C) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let c = lerp(ink, fog, 0.1);
    let tip = lerp(p.lit(rgb(0x8a7a5a)), fog, 0.2);
    for _ in 0..p.many(160) {
        let x = p.rng.range(0.72, 1.02) * w;
        let base = h * p.rng.range(0.86, 1.0);
        let hgt = h * p.rng.range(0.06, 0.18) * (0.5 + (base / h - 0.86) * 4.0);
        let lean = p.rng.range(-0.15, 0.25) * hgt;
        p.cv.taper([x, base], [x + lean, base - hgt], 2.6 * u, 0.6 * u, c, 1.0);
        if p.rng.next() < 0.25 {
            p.cv.taper(
                [x + lean * 0.9, base - hgt * 0.9],
                [x + lean * 1.05, base - hgt * 1.05],
                4.0 * u,
                2.0 * u,
                tip,
                1.0,
            );
        }
    }
    p.fog(0.98, 0.04, 0.4, fog, p.seed + 95);
}

fn heron(p: &mut Painter, x: f32, y: f32, s: f32, ink: C) {
    let c = lerp(p.lit(rgb(0x6a7478)), ink, 0.35);
    let light = lerp(p.lit(rgb(0xc8ccd0)), c, 0.3);
    // Legs, body, neck folded in an S, head and bill.
    p.cv.line(
        [x - s * 0.03, y],
        [x - s * 0.02, y - s * 0.42],
        s * 0.018,
        c,
        1.0,
    );
    p.cv.line(
        [x + s * 0.04, y],
        [x + s * 0.02, y - s * 0.42],
        s * 0.018,
        c,
        1.0,
    );
    p.cv.poly(
        &[
            [x - s * 0.28, y - s * 0.5],
            [x + s * 0.18, y - s * 0.68],
            [x + s * 0.12, y - s * 0.44],
            [x - s * 0.08, y - s * 0.4],
        ],
        c,
        1.0,
    );
    p.cv.ellipse(x - s * 0.02, y - s * 0.55, s * 0.18, s * 0.12, c, 1.0);
    p.cv.ellipse(x + s * 0.02, y - s * 0.5, s * 0.12, s * 0.06, light, 0.7);
    p.cv.taper(
        [x + s * 0.12, y - s * 0.62],
        [x + s * 0.06, y - s * 0.78],
        s * 0.06,
        s * 0.045,
        c,
        1.0,
    );
    p.cv.taper(
        [x + s * 0.06, y - s * 0.78],
        [x + s * 0.14, y - s * 0.9],
        s * 0.045,
        s * 0.04,
        c,
        1.0,
    );
    p.cv.disk(x + s * 0.15, y - s * 0.92, s * 0.045, c, 1.0);
    p.cv.poly(
        &[
            [x + s * 0.18, y - s * 0.94],
            [x + s * 0.34, y - s * 0.9],
            [x + s * 0.18, y - s * 0.9],
        ],
        lerp(c, rgb(0xc8a040), 0.4),
        1.0,
    );
    p.cv.taper(
        [x + s * 0.12, y - s * 0.93],
        [x - s * 0.02, y - s * 0.97],
        s * 0.012,
        s * 0.004,
        ink,
        1.0,
    );
    // Reflection in the shallows.
    let refl = lerp(c, p.l.horizon, 0.5);
    p.cv.line([x, y], [x, y + s * 0.3], s * 0.02, refl, 0.5);
    p.cv.ellipse(x, y + s * 0.4, s * 0.14, s * 0.05, refl, 0.35);
    let _ = fbm;
}
