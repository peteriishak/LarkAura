//! Harvest Valley: from a bench under a great red maple, a lane winds down
//! into a valley of patchwork fields and hedgerows. A red barn and silo
//! stand among round bales, a village spire rises on the far hills, and
//! the maple's leaves drift down across the view.

use super::canvas::{C, lerp, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, hash, noise};
use super::painter::{Painter, WARM, slope_at, top_at};
use std::f32::consts::PI;

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(800, 0.45, 0.8);
    p.body(1.0);
    let cl = lerp(p.lit(rgb(0xfff4e4)), p.l.body_color, 0.25);
    let cs = lerp(p.lit(rgb(0xc8a8a8)), p.l.top, 0.3);
    p.cumulus(0.14, 0.42, 5, 0.11, cl, cs, 0.95, seed + 1);
    p.rays(0.12 * (1.0 - p.dark()), lerp(p.l.body_color, WARM, 0.4));

    // Far hills melting into the warm haze, with a village on the last ridge.
    let far = p.skyline(0.5, 0.05, 1.6, seed + 2, 0.0);
    let far_c = p.air(p.lit(rgb(0x8a7fa8)), 0.6);
    p.land(&far, 0.6, far_c, far_c, 0.1, 0.03, 0.4, seed + 3);
    let far2 = p.skyline(0.54, 0.045, 2.2, seed + 4, 0.0);
    let far2_c = p.air(p.lit(rgb(0x8a8a78)), 0.45);
    p.land(
        &far2,
        0.62,
        far2_c,
        p.air(far2_c, 0.1),
        0.1,
        0.05,
        0.5,
        seed + 5,
    );
    village(p, &far2, 0.74, 0.86);
    p.fog(
        0.55,
        0.012,
        0.35,
        p.air(p.lit(rgb(0xffe8c8)), 0.5),
        seed + 6,
    );

    // The valley: three rolling layers of patchwork fields.
    let layers = [
        (0.585, 0.025, 1.8, 0.32),
        (0.64, 0.035, 1.4, 0.18),
        (0.72, 0.04, 1.1, 0.05),
    ];
    let mut tops: Vec<Box<dyn Fn(f32) -> f32>> = Vec::new();
    for (i, &(base, amp, freq, haze)) in layers.iter().enumerate() {
        let top = p.skyline(base, amp, freq, seed + 10 + i as u32 * 3, 0.0);
        fields(
            p,
            &top,
            [0.65, 0.73, 0.9][i],
            haze,
            seed + 20 + i as u32 * 7,
            0.03 + i as f32 * 0.05,
        );
        let trees = 6 + i * 4;
        hedgerow_trees(
            p,
            &top,
            trees,
            0.022 + i as f32 * 0.02,
            haze,
            seed + 40 + i as u32,
        );
        tops.push(Box::new(top));
    }
    let mid = &tops[1];
    let bx = 0.6 * w;
    farm(p, bx, mid(bx) + 0.035 * h, 0.042 * h);
    bales(p, &*tops[2], 0.45, 0.9, 0.022 * h);

    // Our knoll, carpeted with fallen leaves.
    let knoll = move |x: f32| {
        let t = x / w;
        h * (0.8 + 0.08 * t * t - 0.07 * (1.0 - t).powi(3)
            + 0.01 * (fbm1(x / h * 4.0, seed + 7, 3) - 0.5))
    };
    let g_lit = lerp(p.lit(rgb(0xa8a050)), p.rim(), 0.15);
    let g = p.lit(rgb(0x6a7a34));
    let g_dark = p.lit(rgb(0x3e4a22));
    p.cv.layer(
        knoll,
        |x, y, d| {
            let n = fbm(x / (h * 0.05), y / (h * 0.014), seed + 8, 3);
            let near = (d / (h * 0.2)).clamp(0.0, 1.0);
            ramp3(g_dark, g, g_lit, (1.0 - near) * 0.5 + n * 0.55)
        },
        1.0,
    );
    let blades = [
        p.lit(rgb(0x7a8a3a)),
        p.lit(rgb(0x9a9a4a)),
        p.lit(rgb(0x5a6a2a)),
        p.lit(rgb(0xb8a858)),
    ];
    p.grass(&knoll, 0.0, 1.0, 2200, 0.03, &blades, 0.2);
    // The lane winding down from our knoll towards the farm.
    lane(p, bx);
    let leaf_colors = [
        rgb(0xd8502a),
        rgb(0xe8822a),
        rgb(0xf0b03a),
        rgb(0xb8342a),
        rgb(0xc86a2a),
    ];
    for _ in 0..p.many(1800) {
        let x = p.rng.range(0.0, 0.55).powf(1.2) * w;
        let y = knoll(x) + p.rng.range(0.0, 0.22) * h;
        let s = h * p.rng.range(0.003, 0.007) * (1.0 + (y / h - 0.8) * 4.0);
        let pick = p.rng.pick(&leaf_colors);
        let c = p.lit(pick);
        let a = p.rng.range(0.0, PI);
        fallen_leaf(p, x, y, s, a, c);
    }

    // The maple and the bench beneath it.
    maple(p, 0.12 * w, knoll(0.12 * w) + 0.06 * h, 0.92 * h);
    let benchx = 0.3 * w;
    bench(p, benchx, knoll(benchx) + 0.06 * h, 0.06 * h);
    let _ = (scale, noise, smooth, u);
}

/// Fields as uneven strips across the slope, each with its own crop and
/// rows, divided by dark hedges.
fn fields(
    p: &mut Painter,
    top: &dyn Fn(f32) -> f32,
    bottom: f32,
    haze: f32,
    seed: u32,
    depth: f32,
) {
    let tops = p.column_tops(top);
    let top = |x: f32| top_at(&tops, x);
    let h = p.h;
    let u = p.u.max(0.4);
    let crops: Vec<C> = [
        rgb(0xe8b84a),
        rgb(0xd09040),
        rgb(0x9a6a3a),
        rgb(0x8a9a48),
        rgb(0xf0d080),
        rgb(0xb87a3a),
        rgb(0x6a8040),
    ]
    .iter()
    .map(|c| p.air(p.lit(*c), haze))
    .collect();
    let hedge = p.air(p.lit(rgb(0x3e4a26)), haze);
    let rim = p.air(p.rim(), haze);
    let side = p.sun_side();
    let size = h * (0.06 + depth * 1.5);
    p.cv.layer_to(
        top,
        bottom * h,
        |x, y, d| {
            let warp = (fbm(x / (size * 2.0), y / (size * 0.6), seed, 3) - 0.5) * 1.4;
            let a = x / size * 0.9 + d / size * 3.5 * (0.3 + warp * 0.3) + warp;
            let id = a.floor();
            let f = a - id;
            let crop =
                crops[(hash(id as i32, 1, seed) * crops.len() as f32) as usize % crops.len()];
            let row_dir = hash(id as i32, 2, seed) * 2.0 - 1.0;
            let rows =
                ((x * row_dir * 0.3 + d * 3.0) / (h * (0.004 + depth * 0.05))).sin() * 0.5 + 0.5;
            let mut c = scale(crop, 0.93 + rows * 0.1);
            let n = fbm(x / (h * 0.03), y / (h * 0.01), seed + 1, 2);
            c = scale(c, 0.92 + n * 0.16);
            let edge_w = (2.5 * u) / size;
            let line = smooth(edge_w, 0.0, f.min(1.0 - f));
            c = lerp(c, hedge, line * 0.85);
            if d < 5.0 * u {
                let slope = slope_at(&tops, x, 3.0 * u);
                c = lerp(
                    c,
                    rim,
                    (1.0 - d / (5.0 * u)) * (0.4 - slope * side * 2.0).clamp(0.0, 1.0) * 0.6,
                );
            }
            c
        },
        1.0,
    );
}

fn hedgerow_trees(
    p: &mut Painter,
    top: &dyn Fn(f32) -> f32,
    count: usize,
    size: f32,
    haze: f32,
    seed: u32,
) {
    let palettes = [
        [rgb(0x8a2e1e), rgb(0xc8482a), rgb(0xf08a4a)],
        [rgb(0x9a5a1e), rgb(0xe0922e), rgb(0xf8c860)],
        [rgb(0x6a4a1e), rgb(0xb8862e), rgb(0xe8c050)],
        [rgb(0x3e4a26), rgb(0x6a7a36), rgb(0xa8a858)],
    ];
    let bark = p.air(p.lit(rgb(0x3a2a20)), haze);
    for i in 0..count {
        let x = p.rng.next() * p.w;
        let s = size * p.h * p.rng.range(0.7, 1.3);
        let base = top(x) + p.rng.range(0.005, 0.03) * p.h;
        let pal =
            palettes[(hash(i as i32, 0, seed) * 4.0) as usize % 4].map(|c| p.air(p.lit(c), haze));
        p.cv.taper([x, base], [x, base - s * 0.8], s * 0.1, s * 0.07, bark, 1.0);
        p.cast_shadow(
            x,
            base,
            s * 1.6,
            s * 0.1,
            p.air(p.lit(rgb(0x5a4a2a)), haze),
            0.35,
        );
        p.crown(
            x,
            base - s * 1.05,
            s * 0.55,
            s * 0.5,
            pal,
            seed + i as u32 * 5,
            1.0,
        );
    }
}

fn village(p: &mut Painter, top: &dyn Fn(f32) -> f32, from: f32, to: f32) {
    let (w, h) = (p.w, p.h);
    let haze = 0.42;
    let wall = p.air(p.lit(rgb(0xe8dcc8)), haze);
    let roof = p.air(p.lit(rgb(0xa85a3e)), haze);
    let lamps = p.lamps();
    let n = 9;
    for i in 0..n {
        let x = w * (from + (to - from) * (i as f32 + 0.5) / n as f32);
        let s = h * p.rng.range(0.008, 0.012);
        let b = top(x) + s * 0.6;
        p.cv.rect(x - s, b - s, x + s, b, wall, 1.0);
        p.cv.poly(
            &[[x - s * 1.2, b - s], [x + s * 1.2, b - s], [x, b - s * 1.8]],
            roof,
            1.0,
        );
        if lamps > 0.0 {
            p.cv.disk(x, b - s * 0.5, s * 0.25, WARM, lamps);
        }
    }
    // The church with its spire.
    let x = w * (from + to) * 0.5;
    let s = h * 0.014;
    let b = top(x) + s * 0.4;
    p.cv.rect(x - s * 0.5, b - s * 2.6, x + s * 0.5, b, wall, 1.0);
    p.cv.poly(
        &[
            [x - s * 0.6, b - s * 2.6],
            [x + s * 0.6, b - s * 2.6],
            [x, b - s * 5.0],
        ],
        p.air(p.lit(rgb(0x5a5a6a)), haze),
        1.0,
    );
    p.cv.rect(x + s * 0.5, b - s * 1.3, x + s * 2.2, b, wall, 1.0);
    p.cv.poly(
        &[
            [x + s * 0.4, b - s * 1.3],
            [x + s * 2.3, b - s * 1.3],
            [x + s * 2.3, b - s * 1.9],
            [x + s * 0.4, b - s * 1.9],
        ],
        roof,
        1.0,
    );
}

fn farm(p: &mut Painter, x: f32, ground: f32, s: f32) {
    let side = p.sun_side();
    let red = p.lit(rgb(0xb0362a));
    let red_dark = p.lit(rgb(0x7a2420));
    let white = p.lit(rgb(0xf0e8dc));
    let roof = p.lit(rgb(0x4a3a36));
    let lamps = p.lamps();
    // Silo.
    let sx = x + s * 1.5;
    p.cv.region(
        sx - s * 0.32,
        ground - s * 2.1,
        sx + s * 0.32,
        ground,
        |px, _| {
            let t = (px - sx) / (s * 0.32);
            if t.abs() > 1.0 {
                return None;
            }
            Some((
                lerp(
                    p_shade(white),
                    white,
                    (0.5 + 0.5 * t * side).clamp(0.0, 1.0),
                ),
                1.0,
            ))
        },
    );
    p.cv.ellipse(sx, ground - s * 2.1, s * 0.32, s * 0.2, roof, 1.0);
    // Barn: a gambrel roof over red boards with white trim.
    let half = s * 0.85;
    p.cv.rect(x - half, ground - s, x + half, ground, red, 1.0);
    let shaded = if side > 0.0 {
        [x - half, x - half * 0.3]
    } else {
        [x + half * 0.3, x + half]
    };
    p.cv.rect(shaded[0], ground - s, shaded[1], ground, red_dark, 0.5);
    p.cv.poly(
        &[
            [x - half * 1.08, ground - s],
            [x + half * 1.08, ground - s],
            [x + half * 0.7, ground - s * 1.5],
            [x - half * 0.7, ground - s * 1.5],
        ],
        roof,
        1.0,
    );
    p.cv.poly(
        &[
            [x - half * 0.7, ground - s * 1.5],
            [x + half * 0.7, ground - s * 1.5],
            [x, ground - s * 1.85],
        ],
        roof,
        1.0,
    );
    p.cv.rect(
        x - s * 0.3,
        ground - s * 0.7,
        x + s * 0.3,
        ground,
        red_dark,
        1.0,
    );
    p.cv.line(
        [x - s * 0.3, ground - s * 0.7],
        [x + s * 0.3, ground],
        s * 0.05,
        white,
        1.0,
    );
    p.cv.line(
        [x + s * 0.3, ground - s * 0.7],
        [x - s * 0.3, ground],
        s * 0.05,
        white,
        1.0,
    );
    p.cv.rect(
        x - s * 0.3,
        ground - s * 0.73,
        x + s * 0.3,
        ground - s * 0.68,
        white,
        1.0,
    );
    p.cv.rect(
        x - s * 0.12,
        ground - s * 1.35,
        x + s * 0.12,
        ground - s * 1.12,
        lerp(white, WARM, lamps),
        1.0,
    );
    // Farmhouse beside it.
    let fx = x - s * 1.9;
    let wall = p.lit(rgb(0xe8d8bc));
    p.cv.rect(
        fx - s * 0.6,
        ground - s * 0.8,
        fx + s * 0.6,
        ground + s * 0.05,
        wall,
        1.0,
    );
    p.cv.poly(
        &[
            [fx - s * 0.72, ground - s * 0.8],
            [fx + s * 0.72, ground - s * 0.8],
            [fx, ground - s * 1.4],
        ],
        p.lit(rgb(0x6a3a2e)),
        1.0,
    );
    p.cv.rect(
        fx + s * 0.25,
        ground - s * 1.4,
        fx + s * 0.4,
        ground - s * 1.05,
        p.lit(rgb(0x7a5a4a)),
        1.0,
    );
    for k in [-0.3f32, 0.3] {
        let wx = fx + s * k;
        p.cv.rect(
            wx - s * 0.1,
            ground - s * 0.55,
            wx + s * 0.1,
            ground - s * 0.3,
            lerp(p.lit(rgb(0x3a4050)), WARM, lamps),
            1.0,
        );
        if lamps > 0.0 {
            p.cv.glow(wx, ground - s * 0.42, s * 0.5, WARM, 0.35 * lamps);
        }
    }
    p.smoke(fx + s * 0.32, ground - s * 1.45, s * 0.25, 0.8 * -side);
}

fn p_shade(c: C) -> C {
    lerp(c, rgb(0x8a7a8a), 0.45)
}

fn bales(p: &mut Painter, top: &dyn Fn(f32) -> f32, from: f32, to: f32, s: f32) {
    let gold = p.lit(rgb(0xe0b050));
    let dark = p.lit(rgb(0x9a6a2a));
    let face = p.lit(rgb(0xf0c868));
    let side = p.sun_side();
    let shadow = p.lit(rgb(0x6a5a2a));
    for _ in 0..7 {
        let x = p.rng.range(from, to) * p.w;
        let y = top(x) + p.rng.range(0.02, 0.07) * p.h;
        let r = s * (0.6 + (y / p.h - 0.72) * 6.0).clamp(0.6, 1.4);
        p.cast_shadow(x, y + r * 0.9, r * 4.0, r * 0.3, shadow, 0.4);
        p.cv.ellipse(x, y, r * 1.2, r, dark, 1.0);
        p.cv.ellipse(
            x + side * r * 0.1,
            y - r * 0.08,
            r * 1.08,
            r * 0.88,
            gold,
            1.0,
        );
        p.cv.ellipse(x - side * r * 0.75, y, r * 0.42, r * 0.82, face, 1.0);
        p.cv.ellipse(x - side * r * 0.75, y, r * 0.2, r * 0.4, dark, 0.35);
    }
}

fn lane(p: &mut Painter, to_x: f32) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let dirt = p.lit(rgb(0xd0b080));
    let dirt_dark = p.lit(rgb(0xa08058));
    let verge = p.lit(rgb(0x6a6a32));
    let seed = p.seed + 70;
    let far_y = 0.68 * h;
    p.cv.region(0.0, far_y, w, h, |x, y| {
        let v = ((h - y) / (h - far_y)).clamp(0.0, 1.0);
        let cx = to_x
            + (0.74 * w - to_x) * (1.0 - v).powf(0.6)
            + h * 0.12 * (v * 4.2 + 0.4).sin() * (1.0 - v).powf(0.8);
        let hw = h * (0.005 + 0.11 * (1.0 - v).powf(2.2));
        let n = noise(y / (3.0 * u), x / (30.0 * u), seed);
        let edge = hw * (1.0 + (n - 0.5) * 0.3) - (x - cx).abs();
        let cover = (edge + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let ruts = (((x - cx) / hw).abs() - 0.5).abs();
        let mut c = lerp(dirt_dark, dirt, smooth(0.0, 0.25, ruts));
        c = lerp(c, verge, smooth(0.75, 1.0, ((x - cx) / hw).abs()) * 0.6);
        let speck = fbm(x / (h * 0.01), y / (h * 0.005), seed + 1, 2);
        Some((scale(c, 0.9 + speck * 0.2), cover * 0.95))
    });
}

fn fallen_leaf(p: &mut Painter, x: f32, y: f32, s: f32, angle: f32, c: C) {
    let (dx, dy) = (angle.cos() * s, angle.sin() * s * 0.45);
    p.cv.poly(
        &[
            [x - dx, y - dy],
            [x - dy * 0.8, y + dx * 0.35],
            [x + dx, y + dy],
            [x + dy * 0.8, y - dx * 0.35],
        ],
        c,
        0.95,
    );
}

fn maple(p: &mut Painter, x: f32, base: f32, hgt: f32) {
    let bark = p.lit(rgb(0x3a2a22));
    let bark_lit = lerp(p.lit(rgb(0x6a4a36)), p.rim(), 0.2);
    let side = p.sun_side();
    // A thick trunk with a flared root and a gentle bend, lit down one side.
    let mid = [x + hgt * 0.015, base - hgt * 0.22];
    let top = [x + hgt * 0.035, base - hgt * 0.42];
    p.cast_shadow(x, base, hgt * 0.35, hgt * 0.03, p.lit(rgb(0x3a4a1e)), 0.5);
    p.cv.taper([x, base], mid, hgt * 0.11, hgt * 0.075, bark, 1.0);
    p.cv.taper(mid, top, hgt * 0.075, hgt * 0.055, bark, 1.0);
    let off = side * hgt * 0.022;
    p.cv.taper(
        [x + off, base - hgt * 0.02],
        [mid[0] + off * 0.8, mid[1]],
        hgt * 0.025,
        hgt * 0.018,
        bark_lit,
        0.7,
    );
    p.cv.taper(
        [mid[0] + off * 0.8, mid[1]],
        [top[0] + off * 0.6, top[1]],
        hgt * 0.018,
        hgt * 0.012,
        bark_lit,
        0.7,
    );
    let mut tips = Vec::new();
    p.branch(
        top[0],
        top[1],
        -1.15,
        hgt * 0.22,
        hgt * 0.05,
        3,
        bark,
        0.5,
        &mut tips,
    );
    p.branch(
        top[0],
        top[1],
        -2.1,
        hgt * 0.19,
        hgt * 0.045,
        3,
        bark,
        0.5,
        &mut tips,
    );
    p.branch(
        top[0],
        top[1] + hgt * 0.05,
        -0.45,
        hgt * 0.24,
        hgt * 0.04,
        3,
        bark,
        0.45,
        &mut tips,
    );
    p.branch(
        top[0],
        top[1] - hgt * 0.02,
        -1.6,
        hgt * 0.24,
        hgt * 0.04,
        2,
        bark,
        0.5,
        &mut tips,
    );
    // Clumps of red, orange and gold over a deep backing mass, so the crown
    // reads as one full tree with light on its outer edges.
    let pals = [
        [rgb(0x6a1a14), rgb(0xb8321e), rgb(0xf06a3a)],
        [rgb(0x7a2a12), rgb(0xd0522a), rgb(0xf89a48)],
        [rgb(0x8a4a14), rgb(0xe0882a), rgb(0xfac45a)],
    ];
    let seed = p.seed + 600;
    let (mut minx, mut maxx, mut miny, mut maxy) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for t in &tips {
        minx = minx.min(t[0]);
        maxx = maxx.max(t[0]);
        miny = miny.min(t[1]);
        maxy = maxy.max(t[1]);
    }
    let back = [
        p.lit(rgb(0x4a1210)),
        p.lit(rgb(0x7a2216)),
        p.lit(rgb(0xa8381e)),
    ];
    let (ccx, ccy) = ((minx + maxx) * 0.5, (miny + maxy) * 0.5);
    p.crown(
        ccx,
        ccy,
        (maxx - minx) * 0.48,
        (maxy - miny) * 0.5,
        back,
        seed + 99,
        1.0,
    );
    let mut clumps: Vec<[f32; 3]> = tips
        .iter()
        .enumerate()
        .map(|(i, t)| [t[0], t[1], hash(i as i32, 5, seed)])
        .collect();
    clumps.sort_by(|a, b| a[2].total_cmp(&b[2]));
    for (k, c) in clumps.iter().enumerate() {
        let pal = pals[k % 3].map(|v| p.lit(v));
        let r = hgt * (0.06 + c[2] * 0.05);
        p.crown(c[0], c[1], r * 1.25, r, pal, seed + k as u32, 1.0);
    }
}

fn bench(p: &mut Painter, x: f32, ground: f32, s: f32) {
    let wood = p.lit(rgb(0x7a5236));
    let dark = p.lit(rgb(0x4a3222));
    let iron = p.lit(rgb(0x2a2420));
    let half = s * 1.6;
    for k in [-0.85f32, 0.85] {
        p.cv.line(
            [x + half * k, ground],
            [x + half * k, ground - s * 0.5],
            s * 0.07,
            iron,
            1.0,
        );
        p.cv.line(
            [x + half * k * 1.02, ground - s * 0.5],
            [x + half * k * 1.05, ground - s * 1.05],
            s * 0.06,
            iron,
            1.0,
        );
    }
    p.cv.rect(
        x - half,
        ground - s * 0.55,
        x + half,
        ground - s * 0.45,
        wood,
        1.0,
    );
    p.cv.rect(
        x - half,
        ground - s * 0.46,
        x + half,
        ground - s * 0.42,
        dark,
        1.0,
    );
    for k in 0..3 {
        let y = ground - s * (0.7 + k as f32 * 0.14);
        p.cv.rect(
            x - half * 1.02,
            y - s * 0.05,
            x + half * 1.02,
            y + s * 0.04,
            wood,
            1.0,
        );
    }
    // A few leaves on the seat, and the lark on the backrest.
    let colors = [rgb(0xd8502a), rgb(0xf0b03a)];
    for k in 0..4 {
        let c = p.lit(colors[k % 2]);
        fallen_leaf(
            p,
            x - half * 0.5 + k as f32 * half * 0.35,
            ground - s * 0.55,
            s * 0.09,
            k as f32,
            c,
        );
    }
    p.lark_perched(x + half * 0.55, ground - s * 1.03, s * 0.42, -1.0);
}
