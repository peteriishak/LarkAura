//! Blossom Valley: a spring valley seen from under an old cherry tree. Its
//! branches arch across the top of the view, a river winds past orchards
//! and whitewashed cottages to a stone bridge, and lavender peaks stand
//! beyond. Petals fall from the canopy.

use super::canvas::{lerp, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, noise};
use super::painter::{Painter, SNOW, WARM};
use std::f32::consts::PI;

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(800, 0.45, 0.8);
    p.body(1.0);
    let cl = lerp(p.lit(rgb(0xffffff)), rgb(0xffe8f0), 0.3);
    let cs = lerp(p.lit(rgb(0xc0b4d8)), p.l.top, 0.25);
    p.cumulus(0.2, 0.44, 5, 0.1, cl, cs, 0.92, seed + 1);

    // Lavender peaks with snow on their tops.
    let tent = move |x: f32, at: f32, half: f32| {
        (1.0 - ((x - at * w) / (half * h)).abs()).max(0.0).powf(1.3)
    };
    let peaks = move |x: f32| {
        let env = tent(x, 0.3, 0.35)
            .max(tent(x, 0.62, 0.42) * 1.1)
            .max(tent(x, 0.9, 0.3) * 0.7);
        h * (0.55
            - 0.13 * env
            - 0.025 * super::noise::ridged(x / h * 6.0, 0.2, seed + 2, 4) * (0.3 + env))
    };
    p.mountains(&peaks, 0.59, rgb(0x8a78b0), SNOW, 0.32, 1.05, seed + 3);
    p.fog(0.55, 0.02, 0.45, p.air(p.lit(rgb(0xfbeef4)), 0.4), seed + 4);

    // Rolling meadows stepping down.
    let m1 = p.skyline(0.58, 0.025, 1.6, seed + 5, 0.0);
    let m1c = p.air(p.lit(rgb(0x9cc07a)), 0.4);
    p.land(&m1, 0.64, m1c, p.air(m1c, 0.1), 0.05, 0.08, 0.5, seed + 6);
    orchard(p, &m1, 0.0, 1.0, 26, 0.012, 0.35);
    let m2 = p.skyline(0.63, 0.03, 1.3, seed + 7, 0.0);
    let m2c = p.air(p.lit(rgb(0x88b868)), 0.2);
    p.land(
        &m2,
        0.86,
        m2c,
        p.lit(rgb(0x6aa050)),
        0.1,
        0.1,
        0.6,
        seed + 8,
    );
    let rape = p.air(p.lit(rgb(0xf2d84a)), 0.12);
    let rape_dark = p.air(p.lit(rgb(0xc8b038)), 0.12);
    p.cv.region(0.0, 0.58 * h, w, 0.7 * h, |x, y| {
        let top = m2(x);
        if y < top + 1.0 {
            return None;
        }
        let n = fbm(x / (h * 0.3), y / (h * 0.018), seed + 30, 3);
        let a = smooth(0.56, 0.6, n) * smooth(top, top + 3.0, y);
        let rows = ((x * 0.25 + (y - top) * 4.0) / (h * 0.006)).sin() * 0.5 + 0.5;
        (a > 0.0).then(|| (lerp(rape_dark, rape, 0.6 + rows * 0.4), a))
    });
    orchard(p, &m2, 0.0, 0.35, 10, 0.02, 0.15);
    orchard(p, &m2, 0.7, 1.0, 9, 0.02, 0.15);
    cypresses(p, &m2);

    river(p);
    let bush = [
        p.lit(rgb(0x2e5a34)),
        p.lit(rgb(0x4a7e44)),
        lerp(p.lit(rgb(0x8ab866)), p.rim(), 0.2),
    ];
    for i in 0..p.many(40).max(12) {
        let x = p.rng.next() * w;
        let y = river_y(w, h, x) - h * p.rng.range(0.012, 0.022);
        let r = h * p.rng.range(0.008, 0.016) * (0.6 + (y / h - 0.62) * 6.0).clamp(0.5, 1.4);
        p.crown(x, y, r * 1.3, r, bush, seed + 900 + i as u32, 1.0);
    }
    let m3 = move |x: f32| river_y(w, h, x) + 0.02 * h;
    orchard(p, &m3, 0.05, 0.3, 4, 0.045, 0.05);
    orchard(p, &m3, 0.55, 0.8, 4, 0.04, 0.05);
    let bridge_x = 0.43 * w;
    bridge(p, bridge_x, river_y(w, h, bridge_x), 0.03 * h);
    cottages(p, 0.62, 0.035 * h);

    // The near slope, with a stone path stepping down.
    let front = move |x: f32| {
        let t = x / w;
        h * (0.8 + 0.06 * (t - 0.4).powi(2) + 0.012 * (fbm1(x / h * 3.0, seed + 9, 3) - 0.5))
    };
    let g_lit = lerp(p.lit(rgb(0xb0d080)), p.rim(), 0.15);
    let g = p.lit(rgb(0x6aa850));
    let g_dark = p.lit(rgb(0x3e7a3a));
    p.cv.layer(
        front,
        |x, y, d| {
            let n = fbm(x / (h * 0.05), y / (h * 0.014), seed + 10, 3);
            let near = (d / (h * 0.2)).clamp(0.0, 1.0);
            ramp3(g_dark, g, g_lit, (1.0 - near) * 0.55 + n * 0.5)
        },
        1.0,
    );
    path(p, &front);
    let blades = [
        p.lit(rgb(0x5a9a44)),
        p.lit(rgb(0x78b058)),
        p.lit(rgb(0x3e7a36)),
        p.lit(rgb(0x92c068)),
    ];
    p.grass(&front, 0.0, 1.0, 2400, 0.03, &blades, 0.2);
    let clump = move |x: f32, y: f32, k: u32| fbm(x / (h * 0.1), y / (h * 0.035), seed + k, 3);
    let blooms = [
        (rgb(0x8a8ae0), 40u32),
        (rgb(0xfff4f8), 41),
        (rgb(0xf8d050), 42),
    ];
    for (c, k) in blooms {
        let c = p.lit(c);
        for _ in 0..p.many(700) {
            let x = p.rng.next() * w;
            let y = front(x) + p.rng.range(0.01, 0.22) * h;
            if clump(x, y, k) < 0.57 {
                continue;
            }
            let s = h * 0.0032 * (1.0 + (y / h - 0.8) * 6.0);
            p.cv.disk(x, y, s, c, 1.0);
        }
    }
    // Petals lying on the grass.
    let petal = p.lit(rgb(0xf8c8d8));
    for _ in 0..p.many(900) {
        let x = p.rng.next() * w;
        let y = front(x) + p.rng.range(0.0, 0.22) * h;
        let s = h * 0.0028 * (1.0 + (y / h - 0.8) * 5.0);
        let a = p.rng.range(0.0, PI);
        p.cv.ellipse(x + a.cos(), y, s * 1.3, s * 0.7, petal, 0.9);
    }

    // The cherry tree arching over the top of the view.
    cherry(p);
    let _ = (u, scale, smooth, noise, WARM);
}

fn orchard(
    p: &mut Painter,
    top: &dyn Fn(f32) -> f32,
    from: f32,
    to: f32,
    count: usize,
    size: f32,
    haze: f32,
) {
    let pinks = [rgb(0xd88aa8), rgb(0xf4b8cc), rgb(0xffe4ee)].map(|c| p.air(p.lit(c), haze));
    let whites = [rgb(0xc8b8c8), rgb(0xf0e8ee), rgb(0xffffff)].map(|c| p.air(p.lit(c), haze));
    let bark = p.air(p.lit(rgb(0x5a4038)), haze);
    for i in 0..count {
        let x = (from + (to - from) * (i as f32 + p.rng.range(0.2, 0.8)) / count as f32) * p.w;
        let s = size * p.h * p.rng.range(0.8, 1.2);
        let base = top(x) + p.rng.range(0.004, 0.02) * p.h;
        p.cv.taper(
            [x, base],
            [x, base - s * 0.7],
            s * 0.12,
            s * 0.08,
            bark,
            1.0,
        );
        let pal = if i % 3 == 0 { whites } else { pinks };
        p.crown(
            x,
            base - s * 1.0,
            s * 0.6,
            s * 0.48,
            pal,
            p.seed + 300 + i as u32,
            1.0,
        );
    }
}

fn cypresses(p: &mut Painter, top: &dyn Fn(f32) -> f32) {
    let dark = p.air(p.lit(rgb(0x1e3e2a)), 0.15);
    let mid = p.air(p.lit(rgb(0x2e5a3a)), 0.15);
    let side = p.sun_side();
    for (fx, s) in [(0.69, 0.07), (0.705, 0.055), (0.18, 0.06)] {
        let x = fx * p.w;
        let hgt = s * p.h;
        let base = top(x) + 0.01 * p.h;
        let seed = p.seed + (fx * 1000.0) as u32;
        p.cv.region(
            x - hgt * 0.12,
            base - hgt,
            x + hgt * 0.12,
            base,
            |px, py| {
                let t = (base - py) / hgt;
                let half = hgt * 0.1 * (t * PI).sin().powf(0.7) * (1.0 - t * 0.3);
                let jag = (noise(py / (hgt * 0.03), 0.5, seed) - 0.5) * hgt * 0.02;
                let cover = (half + jag - (px - x).abs() + 0.5).clamp(0.0, 1.0);
                let li = 0.5 + 0.5 * ((px - x) / half.max(0.5)) * side;
                (cover > 0.0).then(|| (lerp(dark, mid, li.clamp(0.0, 1.0)), cover))
            },
        );
    }
}

fn river_y(w: f32, h: f32, x: f32) -> f32 {
    let t = x / w;
    h * (0.655 + 0.03 * (t * 4.2 + 0.8).sin() + 0.012 * (t * 11.0).sin() + 0.035 * t)
}

fn river(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed + 50;
    let bank = p.lit(rgb(0x5a9048));
    let water_shade = p.lit(rgb(0x4a7aa0));
    let top = p.l.top;
    let horizon = p.l.horizon;
    let shine = lerp(horizon, rgb(0xffffff), 0.4);
    p.cv.region(0.0, 0.6 * h, w, 0.78 * h, |x, y| {
        let cy = river_y(w, h, x);
        let hw = h
            * (0.004 + 0.016 * ((cy / h - 0.63) * 12.0).clamp(0.0, 1.0))
            * (1.0 + (noise(x / (h * 0.08), 0.0, seed) - 0.5) * 0.8);
        let d = (y - cy).abs();
        let cover = (hw + 2.0 * u - d + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        if d > hw {
            return Some((bank, cover));
        }
        let t = ((y - cy) / hw + 1.0) * 0.5;
        let sky = lerp(horizon, top, t * 0.5);
        let streak = fbm(x / (h * 0.06), y / (h * 0.003), seed + 1, 3);
        let mut c = lerp(sky, water_shade, 0.35 + t * 0.2);
        c = lerp(c, shine, smooth(0.6, 0.8, streak) * 0.5);
        Some((c, cover))
    });
}

fn bridge(p: &mut Painter, x: f32, y: f32, s: f32) {
    let stone = p.lit(rgb(0xd8ccb8));
    let stone_dark = p.lit(rgb(0x9a8c7c));
    let span = s * 2.4;
    p.cv.region(
        x - span - 2.0,
        y - s * 1.1,
        x + span + 2.0,
        y + s * 0.4,
        |px, py| {
            let t = (px - x) / span;
            if t.abs() > 1.0 {
                return None;
            }
            let deck = y - s * (0.75 + 0.25 * (1.0 - t * t));
            let arch = y + s * 0.3 - s * 0.85 * (1.0 - (t / 0.6).powi(2)).max(0.0).sqrt();
            let in_deck = (py - deck + 0.5).clamp(0.0, 1.0);
            let under_arch = (arch - py + 0.5).clamp(0.0, 1.0);
            let bottom = (y + s * 0.3 - py + 0.5).clamp(0.0, 1.0);
            let cover = in_deck * under_arch * bottom;
            if cover <= 0.0 {
                return None;
            }
            let blocks = ((px / (s * 0.3)).floor() + (py / (s * 0.18)).floor()) as i32 % 2 == 0;
            let c = if py - deck < s * 0.12 {
                stone_dark
            } else if blocks {
                stone
            } else {
                lerp(stone, stone_dark, 0.08)
            };
            Some((c, cover))
        },
    );
    // The arch's reflection in the water.
    p.cv.region(
        x - span * 0.6,
        y + s * 0.3,
        x + span * 0.6,
        y + s * 0.9,
        |px, py| {
            let t = (px - x) / (span * 0.6);
            let refl = y + s * 0.3 + s * 0.5 * (1.0 - t * t).max(0.0).sqrt();
            let a = (refl - py + 0.5).clamp(0.0, 1.0) * 0.35;
            (a > 0.0 && t.abs() < 1.0).then_some((stone_dark, a))
        },
    );
}

fn cottages(p: &mut Painter, at: f32, s: f32) {
    let (w, h) = (p.w, p.h);
    let side = p.sun_side();
    let wall = p.lit(rgb(0xf8f2e8));
    let shade = p.lit(rgb(0xc4bccc));
    let roofs = [rgb(0x5a7ab0), rgb(0xc86a4a), rgb(0x6a8a9a)];
    let lamps = p.lamps();
    for (k, dx) in [-1.6f32, 0.0, 1.4].iter().enumerate() {
        let x = at * w + dx * s * 1.4;
        let ground = river_y(w, h, x) - h * 0.03 + (k as f32 - 1.0).abs() * s * 0.2;
        let ss = s * (1.0 - (k as f32 - 1.0).abs() * 0.15);
        let half = ss * 0.75;
        p.cv.rect(x - half, ground - ss, x + half, ground, wall, 1.0);
        let shaded = if side > 0.0 {
            [x - half, x - half * 0.1]
        } else {
            [x + half * 0.1, x + half]
        };
        p.cv.rect(shaded[0], ground - ss, shaded[1], ground, shade, 0.55);
        let roof = p.lit(roofs[k % 3]);
        p.cv.poly(
            &[
                [x - half * 1.2, ground - ss],
                [x + half * 1.2, ground - ss],
                [x, ground - ss * 1.75],
            ],
            roof,
            1.0,
        );
        let glass = lerp(p.lit(rgb(0x4a5a70)), WARM, lamps);
        p.cv.rect(
            x - ss * 0.15,
            ground - ss * 0.65,
            x + ss * 0.15,
            ground - ss * 0.35,
            glass,
            1.0,
        );
    }
}

fn path(p: &mut Painter, front: &dyn Fn(f32) -> f32) {
    let (w, h) = (p.w, p.h);
    let stone = p.lit(rgb(0xc8beae));
    let stone_dark = p.lit(rgb(0x8a8070));
    let moss = p.lit(rgb(0x6a8a48));
    let steps = 14;
    for i in (0..steps).rev() {
        let t = i as f32 / steps as f32;
        let x = w * (0.34 + 0.12 * t) + (t * 6.0).sin() * h * 0.04 * t;
        let top = front(x);
        let y = top + (h * 1.02 - top) * t.powf(1.4);
        let s = h * (0.008 + 0.05 * t * t);
        if y < top + 2.0 {
            continue;
        }
        let seed = i as u32 * 7;
        p.cv.ellipse(x + s * 0.1, y + s * 0.18, s * 1.5, s * 0.5, stone_dark, 1.0);
        p.cv.region(
            x - s * 1.6,
            y - s * 0.6,
            x + s * 1.6,
            y + s * 0.6,
            |px, py| {
                let (dx, dy) = ((px - x) / (s * 1.45), (py - y) / (s * 0.45));
                let edge = 1.0 + (noise(px / (s * 0.4), py / (s * 0.4), seed) - 0.5) * 0.25;
                let d = (dx * dx + dy * dy).sqrt();
                let cover = ((edge - d) * s * 0.45 + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    return None;
                }
                let n = fbm(px / (s * 0.5), py / (s * 0.3), seed + 1, 3);
                let mut c = lerp(
                    stone_dark,
                    stone,
                    (0.75 - dy * 0.35 + (n - 0.5) * 0.4).clamp(0.0, 1.0),
                );
                c = lerp(c, moss, smooth(0.62, 0.75, n) * 0.6);
                Some((c, cover))
            },
        );
    }
}

/// Branches enter from the right and arch across the top, heavy with
/// blossom; a smaller bough reaches in from the top left.
fn cherry(p: &mut Painter) {
    let (w, h) = (p.w, p.h);
    let bark = p.lit(rgb(0x3a2a2a));
    let bark_lit = lerp(p.lit(rgb(0x6a5048)), p.rim(), 0.25);
    let seed = p.seed + 700;
    let base = [1.02 * w, 1.05 * h];
    let fork = [0.93 * w, 0.42 * h];
    let bend = [0.975 * w, 0.75 * h];
    p.cv.taper(base, bend, 0.12 * h, 0.09 * h, bark, 1.0);
    p.cv.taper(bend, fork, 0.09 * h, 0.065 * h, bark, 1.0);
    for k in 0..7 {
        let t = k as f32 / 7.0;
        let a = [
            base[0] + (bend[0] - base[0]) * t - 0.03 * h,
            base[1] + (bend[1] - base[1]) * t,
        ];
        p.cv.taper(
            a,
            [a[0] + 0.004 * h, a[1] - 0.06 * h],
            0.004 * h,
            0.002 * h,
            bark_lit,
            0.5,
        );
    }
    p.cv.taper(
        [bend[0] - 0.03 * h, bend[1]],
        [fork[0] - 0.02 * h, fork[1] + 0.02 * h],
        0.016 * h,
        0.01 * h,
        bark_lit,
        0.55,
    );
    let mut tips = Vec::new();
    // The long arching limb across the top.
    let mut prev = fork;
    let limb: Vec<[f32; 2]> = (1..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            [
                fork[0] - t * 0.62 * w,
                fork[1] - (t * PI * 0.55).sin() * 0.38 * h + t * t * 0.02 * h,
            ]
        })
        .collect();
    for (i, pt) in limb.iter().enumerate() {
        let t = i as f32 / 8.0;
        p.cv.taper(
            prev,
            *pt,
            (0.06 - t * 0.05) * h,
            (0.055 - t * 0.05) * h,
            bark,
            1.0,
        );
        if i % 2 == 1 {
            let down = if i % 4 == 1 { 1.3 } else { 2.0 };
            p.branch(
                pt[0],
                pt[1],
                down,
                0.1 * h * (1.0 - t * 0.4),
                0.018 * h,
                2,
                bark,
                0.6,
                &mut tips,
            );
        }
        tips.push(*pt);
        prev = *pt;
    }
    p.branch(
        fork[0],
        fork[1],
        -1.9,
        0.22 * h,
        0.04 * h,
        3,
        bark,
        0.5,
        &mut tips,
    );
    p.branch(
        fork[0],
        fork[1] + 0.06 * h,
        -0.9,
        0.2 * h,
        0.035 * h,
        3,
        bark,
        0.5,
        &mut tips,
    );
    p.branch(
        -0.02 * w,
        -0.02 * h,
        0.55,
        0.22 * h,
        0.03 * h,
        3,
        bark,
        0.55,
        &mut tips,
    );
    // Blossom clouds over every tip.
    let pals = [
        [rgb(0xc86a8a), rgb(0xf0a0bc), rgb(0xffd8e6)],
        [rgb(0xd88aa4), rgb(0xf8bcd0), rgb(0xfff0f4)],
        [rgb(0xb85a7a), rgb(0xe890ae), rgb(0xffc8da)],
    ];
    let mut order: Vec<usize> = (0..tips.len()).collect();
    order.sort_by_key(|&i| (super::noise::hash(i as i32, 0, seed) * 1000.0) as i32);
    for (k, &i) in order.iter().enumerate() {
        let t = tips[i];
        if t[1] > 0.62 * h {
            continue;
        }
        let pal = pals[k % 3].map(|c| p.lit(c));
        let r = h * (0.035 + super::noise::hash(i as i32, 1, seed) * 0.03);
        p.crown(t[0], t[1], r * 1.3, r, pal, seed + k as u32, 1.0);
    }
    // Loose blossoms and buds around the clusters.
    let petal_c = [rgb(0xffffff), rgb(0xffd0e0), rgb(0xf8a8c4)];
    for _ in 0..p.many(1600) {
        let t = tips[(p.rng.next() * tips.len() as f32) as usize % tips.len()];
        if t[1] > 0.62 * h {
            continue;
        }
        let a = p.rng.range(0.0, 2.0 * PI);
        let d = p.rng.next().sqrt() * 0.06 * h;
        let (x, y) = (t[0] + a.cos() * d * 1.3, t[1] + a.sin() * d);
        let pick = p.rng.pick(&petal_c);
        let c = p.lit(pick);
        let r = h * p.rng.range(0.0025, 0.005);
        for k in 0..5 {
            let ang = k as f32 * 1.2566;
            p.cv.disk(
                x + ang.cos() * r * 0.7,
                y + ang.sin() * r * 0.7,
                r * 0.6,
                c,
                0.95,
            );
        }
        p.cv.disk(x, y, r * 0.3, p.lit(rgb(0xd04a7a)), 1.0);
    }
    // The lark on the arching limb.
    let perch = limb[4];
    p.lark_perched(perch[0], perch[1] - 0.012 * h, 0.035 * h, -1.0);
}
