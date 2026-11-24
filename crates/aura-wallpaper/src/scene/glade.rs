//! Firefly Pond: a still pond in a lingering evening. Rounded treelines
//! mirror in the water, a weeping willow leans in from the right, lily pads
//! float in the shallows, cattails stand in the foreground and a jar of
//! fireflies glows on an old stump.

use super::canvas::{C, lerp, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, hash, noise};
use super::painter::{Clouds, LAMP, Painter};

const SHORE: f32 = 0.6;
const FIREFLY: C = rgb(0xd8f27a);

/// A treeline of rounded crowns: scalloped bumps of varied height.
fn canopy(h: f32, base: f32, size: f32, seed: u32) -> impl Fn(f32) -> f32 {
    move |x: f32| {
        let mut best = 0.0f32;
        for (scale, amp, s) in [
            (size * 1.6, 1.25, seed + 11),
            (size, 1.0, seed),
            (size * 0.55, 0.55, seed + 7),
        ] {
            let k = x / (h * scale);
            let i = k.floor();
            for j in [i - 1.0, i, i + 1.0] {
                let centre = j + 0.5 + (hash(j as i32, 1, s) - 0.5) * 0.5;
                if scale > size * 1.2 && hash(j as i32, 4, s) < 0.55 {
                    continue;
                }
                let r = 0.6 + hash(j as i32, 2, s) * 0.5;
                let d = (k - centre) / r;
                if d.abs() < 1.0 {
                    let top = (1.0 - d * d).sqrt() * r * amp * (0.6 + hash(j as i32, 3, s) * 0.6);
                    best = best.max(top);
                }
            }
        }
        h * (base - best * size * 0.9 - 0.01 * fbm1(x / h * 30.0, seed + 3, 2))
    }
}

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(1200, 0.5, 0.9);
    p.body(1.0);
    let cl = lerp(p.l.horizon, rgb(0xf0c8b0), 0.35);
    let cs = lerp(p.l.top, rgb(0x2a3a58), 0.5);
    p.clouds(&Clouds {
        y0: 0.28,
        y1: 0.48,
        size: 0.07,
        cover: 0.3,
        stretch: 4.0,
        softness: 0.06,
        lit: cl,
        shade: cs,
        alpha: 0.6,
        seed: seed + 1,
    });

    // Two treelines, the far one fading into the dusk.
    let far = canopy(h, 0.545, 0.06, seed + 2);
    let near = canopy(h, SHORE + 0.004, 0.075, seed + 3);
    let far_c = lerp(p.lit(rgb(0x2e5a58)), p.l.horizon, 0.55);
    p.land(&far, SHORE + 0.01, far_c, far_c, 0.1, 0.05, 0.3, seed + 4);
    p.fog(
        0.565,
        0.018,
        0.35,
        lerp(p.l.horizon, rgb(0xb8d8d0), 0.3),
        seed + 5,
    );
    let tree_dark = p.lit(rgb(0x163a34));
    let tree_mid = p.lit(rgb(0x2a5a48));
    let tree_lit = lerp(p.lit(rgb(0x4a8060)), p.rim(), 0.25);
    let side = p.sun_side();
    p.cv.layer_to(
        &near,
        (SHORE + 0.01) * h,
        |x, y, d| {
            let n = fbm(x / (h * 0.02), y / (h * 0.02), seed + 6, 3);
            let up = (1.0 - d / (h * 0.06)).clamp(0.0, 1.0);
            let slope = (near(x + 4.0 * u) - near(x - 4.0 * u)) / (8.0 * u);
            let li = (0.3 + up * 0.35 + (n - 0.5) * 0.6 - slope * side * 0.4).clamp(0.0, 1.0);
            ramp3(tree_dark, tree_mid, tree_lit, li)
        },
        1.0,
    );

    // The pond mirrors the evening.
    let water_tint = p.lit(rgb(0x1c3e4a));
    let full = |_: f32, _: f32| 1.0;
    p.reflect(SHORE, SHORE, 1.0, water_tint, 0.3, 3.0, &full);
    p.fog(
        SHORE + 0.005,
        0.014,
        0.45,
        lerp(p.l.horizon, rgb(0xc8e0d8), 0.35),
        seed + 7,
    );
    if p.l.body != super::Body::None {
        let (bx, _) = p.body_pos();
        p.glitter(
            bx,
            SHORE + 0.01,
            0.9,
            0.012,
            lerp(p.l.body_color, rgb(0xffffff), 0.2),
            0.5,
            400,
        );
    }
    let ripple = lerp(p.l.horizon, rgb(0xffffff), 0.2);
    p.ripples(SHORE + 0.01, 0.95, 260, ripple, 0.18, &full);

    // Lily pads, smaller with distance.
    let pad = p.lit(rgb(0x3a6a3a));
    let pad_hi = lerp(p.lit(rgb(0x6a9a52)), p.rim(), 0.2);
    let bloom = p.lit(rgb(0xf6e8f0));
    let bloom_pink = p.lit(rgb(0xe8a0c0));
    for i in 0..p.many(160) {
        let v = p.rng.next().powf(0.8);
        let y = h * (SHORE + 0.03 + v * 0.28);
        let x = w * p.rng.range(0.15, 0.78);
        if fbm(x / (h * 0.25), y / (h * 0.06), seed + 8, 2) < 0.45 {
            continue;
        }
        let r = h * (0.008 + v * 0.036);
        let notch = p.rng.range(0.0, std::f32::consts::TAU);
        p.cv.region(
            x - r - 1.0,
            y - r * 0.4 - 1.0,
            x + r + 1.0,
            y + r * 0.4 + 1.0,
            |px, py| {
                let (dx, dy) = ((px - x) / r, (py - y) / (r * 0.36));
                let d = (dx * dx + dy * dy).sqrt();
                let ang = dy.atan2(dx);
                let cut = ((ang - notch).rem_euclid(std::f32::consts::TAU) < 0.35) as i32 as f32;
                let cover = ((1.0 - d) * r * 0.36 + 0.5).clamp(0.0, 1.0) * (1.0 - cut);
                (cover > 0.0).then(|| (lerp(pad, pad_hi, (0.5 - dy * 0.4).clamp(0.0, 1.0)), cover))
            },
        );
        if i % 4 == 0 {
            let c = if i % 8 == 0 { bloom } else { bloom_pink };
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                p.cv.ellipse(
                    x + a.cos() * r * 0.18,
                    y - r * 0.14 + a.sin() * r * 0.05,
                    r * 0.16,
                    r * 0.09,
                    c,
                    1.0,
                );
            }
            p.cv.disk(x, y - r * 0.18, r * 0.06, p.lit(rgb(0xf6d860)), 1.0);
        }
    }

    // The willow leaning in from the right.
    willow(p, 0.88 * w, 0.95 * h, 0.86 * h);

    // The near bank.
    let bank = move |x: f32| {
        let t = x / w;
        h * (0.8 + 0.12 * t * t * (3.0 - 2.0 * t) - 0.05 * (1.0 - t).powi(4)
            + 0.012 * (fbm1(x / h * 5.0, seed + 9, 3) - 0.5))
    };
    p.land(
        &bank,
        1.0,
        p.lit(rgb(0x24402c)),
        p.lit(rgb(0x0e1c14)),
        0.12,
        0.15,
        0.6,
        seed + 10,
    );
    let blades = [
        p.lit(rgb(0x1e3a26)),
        p.lit(rgb(0x2e5234)),
        p.lit(rgb(0x14281a)),
        p.lit(rgb(0x3a6040)),
    ];
    p.grass(&bank, 0.0, 1.0, 1800, 0.05, &blades, 0.15);
    cattails(p, &bank);

    // The stump with a jar of fireflies.
    let sx = 0.24 * w;
    stump(p, sx, bank(sx) + 0.075 * h, 0.085 * h);

    // Fireflies drifting over the water and the grass, each mirrored.
    let glow = 0.5 + 0.5 * smooth(0.5, 0.9, p.dark());
    for _ in 0..p.many(150) {
        let x = p.rng.next() * w;
        let y = h * p.rng.range(0.42, 0.95);
        let r = p.rng.range(1.4, 3.2) * u;
        let a = glow * p.rng.range(0.4, 1.0);
        p.cv.glow(x, y, r * 9.0, FIREFLY, 0.3 * a);
        p.cv.disk(x, y, r, rgb(0xf8ffd0), a);
        let my = 2.0 * SHORE * h - y;
        if y < SHORE * h && my < 0.8 * h {
            p.cv.glow(x, my, r * 6.0, FIREFLY, 0.15 * a);
            p.cv.disk(x, my, r * 0.8, FIREFLY, a * 0.5);
        }
    }
    let _ = (scale, noise, LAMP);
}

fn willow(p: &mut Painter, x: f32, base: f32, hgt: f32) {
    let bark = p.lit(rgb(0x2a241e));
    let leaf_dark = p.lit(rgb(0x183626));
    let leaf = p.lit(rgb(0x335f3c));
    let leaf_lit = lerp(p.lit(rgb(0x78a05e)), p.rim(), 0.35);
    let h = p.h;
    let seed = p.seed + 500;
    let side = p.sun_side();
    // A leaning trunk forking into the crown.
    let crown = [x - hgt * 0.08, base - hgt * 0.7];
    p.cv.taper(
        [x + hgt * 0.05, base],
        [x - hgt * 0.02, base - hgt * 0.4],
        hgt * 0.075,
        hgt * 0.055,
        bark,
        1.0,
    );
    p.cv.taper(
        [x - hgt * 0.02, base - hgt * 0.4],
        crown,
        hgt * 0.055,
        hgt * 0.035,
        bark,
        1.0,
    );
    p.cv.taper(
        [x - hgt * 0.02, base - hgt * 0.42],
        [x + hgt * 0.16, base - hgt * 0.66],
        hgt * 0.04,
        hgt * 0.02,
        bark,
        1.0,
    );
    p.cv.taper(
        [x - hgt * 0.04, base - hgt * 0.5],
        [x - hgt * 0.24, base - hgt * 0.7],
        hgt * 0.035,
        hgt * 0.018,
        bark,
        1.0,
    );
    // The canopy: a dome that spills down in curtains of hanging strands,
    // with a ragged fringe where the strands end.
    let (cx, cy) = (crown[0], crown[1] - hgt * 0.05);
    let rx = hgt * 0.42;
    let top = move |px: f32| {
        let d = ((px - cx) / rx).clamp(-1.0, 1.0);
        cy - (1.0 - d * d).max(0.0).sqrt() * hgt * 0.22
            - (fbm1(px / (hgt * 0.05), seed, 3) - 0.5) * hgt * 0.05
    };
    let hem = move |px: f32| {
        let d = ((px - cx) / rx).abs();
        let curtain = base - hgt * (0.18 + 0.35 * d * d);
        curtain - fbm1(px / (hgt * 0.06), seed + 1, 3) * hgt * 0.12
    };
    p.cv.region(
        cx - rx * 1.05,
        cy - hgt * 0.3,
        cx + rx * 1.05,
        base,
        |px, py| {
            let d = (px - cx) / rx;
            if d.abs() > 1.02 {
                return None;
            }
            let t = top(px);
            let b = hem(px);
            if py < t - 1.0 || py > b + hgt * 0.1 {
                return None;
            }
            // Strands: streaks running down, broken into separate tips below.
            let strand = noise(px / (h * 0.0045), py / (h * 0.25), seed + 2);
            let strand2 = noise(px / (h * 0.009) + 13.0, py / (h * 0.18), seed + 3);
            let s = strand * 0.6 + strand2 * 0.4;
            let tip = b + (s - 0.5) * hgt * 0.16;
            let cover_top = (py - t + 0.5).clamp(0.0, 1.0);
            let cover_tip = smooth(tip, tip - hgt * 0.02, py);
            let gaps = smooth(0.3, 0.42, s + (1.0 - (py - t) / (b - t).max(1.0)) * 0.25);
            let edge = smooth(1.02, 0.94, d.abs() + (s - 0.5) * 0.1);
            let cover = cover_top * cover_tip * gaps * edge;
            if cover <= 0.0 {
                return None;
            }
            let depth = ((py - t) / (b - t).max(1.0)).clamp(0.0, 1.0);
            let clump = fbm(px / (h * 0.04), py / (h * 0.06), seed + 4, 3);
            let li =
                (0.75 - depth * 0.45 + d * side * 0.25 + (s - 0.5) * 0.7 + (clump - 0.5) * 0.5)
                    .clamp(0.0, 1.0);
            Some((ramp3(leaf_dark, leaf, leaf_lit, li), cover))
        },
    );
}

fn cattails(p: &mut Painter, bank: &dyn Fn(f32) -> f32) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let stem = p.lit(rgb(0x2a4a2a));
    let head = p.lit(rgb(0x5a3a24));
    let blade = p.lit(rgb(0x30542e));
    for (from, to, count) in [(0.0, 0.14, 14), (0.62, 0.76, 9)] {
        for _ in 0..count {
            let x = p.rng.range(from, to) * w;
            let base = bank(x) + p.rng.range(0.01, 0.05) * h;
            let hgt = p.rng.range(0.16, 0.3) * h;
            let lean = p.rng.range(-0.05, 0.05) * hgt;
            let tip = [x + lean, base - hgt];
            p.cv.taper([x, base], tip, 3.0 * u, 1.6 * u, stem, 1.0);
            let hs = hgt * 0.14;
            let t0 = [x + lean * 0.85, base - hgt * 0.85];
            p.cv.taper(
                [t0[0], t0[1] + hs * 0.5],
                [t0[0] + lean * 0.05, t0[1] - hs * 0.5],
                hs * 0.24,
                hs * 0.22,
                head,
                1.0,
            );
            for s in [-1.0f32, 1.0] {
                let bend = [x + s * hgt * 0.12, base - hgt * 0.45];
                p.cv.taper([x, base], bend, 4.0 * u, 2.0 * u, blade, 1.0);
                p.cv.taper(
                    bend,
                    [x + s * hgt * 0.2, base - hgt * 0.3],
                    2.0 * u,
                    0.4 * u,
                    blade,
                    1.0,
                );
            }
        }
    }
}

fn stump(p: &mut Painter, x: f32, ground: f32, s: f32) {
    let bark = p.lit(rgb(0x4a3828));
    let bark_dark = p.lit(rgb(0x2a1e16));
    let top = p.lit(rgb(0x9a7a58));
    p.cv.poly(
        &[
            [x - s * 0.75, ground],
            [x + s * 0.75, ground],
            [x + s * 0.55, ground - s * 0.7],
            [x - s * 0.55, ground - s * 0.7],
        ],
        bark,
        1.0,
    );
    for k in 0..4 {
        let t = -0.4 + k as f32 * 0.27;
        p.cv.line(
            [x + s * t, ground - s * 0.68],
            [x + s * t * 1.3, ground],
            s * 0.04,
            bark_dark,
            0.7,
        );
    }
    p.cv.ellipse(x, ground - s * 0.7, s * 0.56, s * 0.13, top, 1.0);
    p.cv.ellipse(x, ground - s * 0.7, s * 0.3, s * 0.07, bark, 0.4);
    // The glass jar, lit from inside.
    let (jx, jy) = (x + s * 0.05, ground - s * 0.76);
    let jw = s * 0.22;
    let glass = lerp(p.lit(rgb(0x6a8a80)), FIREFLY, 0.5);
    p.cv.glow(jx, jy - jw * 1.1, s * 2.2, FIREFLY, 0.35);
    p.cv.rect(jx - jw, jy - jw * 2.4, jx + jw, jy, glass, 0.55);
    p.cv.rect(
        jx - jw * 0.85,
        jy - jw * 2.75,
        jx + jw * 0.85,
        jy - jw * 2.4,
        p.lit(rgb(0x8a7050)),
        1.0,
    );
    for k in 0..6 {
        let fx = jx + (hash(k, 0, 9) - 0.5) * jw * 1.4;
        let fy = jy - jw * (0.3 + hash(k, 1, 9) * 1.8);
        p.cv.glow(fx, fy, jw * 0.6, FIREFLY, 0.6);
        p.cv.disk(fx, fy, jw * 0.08, rgb(0xfaffd8), 1.0);
    }
    p.cv.line(
        [jx - jw * 0.6, jy - jw * 2.2],
        [jx - jw * 0.6, jy - jw * 0.3],
        jw * 0.12,
        rgb(0xffffff),
        0.35,
    );
    p.cv.glow_ellipse(jx, ground - s * 0.65, s * 1.4, s * 0.3, FIREFLY, 0.2);
    // A lark keeping the jar company.
    p.lark_perched(x - s * 0.36, ground - s * 0.74, s * 0.42, 1.0);
}
