//! Lark Cliffs: a clifftop meadow above a calm sea. A chalk headland runs
//! out into the water with a lighthouse and keeper's cottage, larks rise
//! over the wildflowers and the sun lays a glittering path on the sea.

use super::canvas::{lerp, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, noise};
use super::painter::{Clouds, LAMP, Painter, WARM};

const HORIZON: f32 = 0.565;

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(900, 0.5, 0.8);
    p.body(1.0);
    let lit = p.air(lerp(p.lit(rgb(0xffffff)), p.l.body_color, 0.15), 0.1);
    let shade = p.air(p.lit(rgb(0xa9b7d2)), 0.35);
    p.cumulus(0.14, 0.44, 5, 0.12, lit, shade, 0.95, seed + 1);
    p.clouds(&Clouds {
        y0: 0.36,
        y1: 0.52,
        size: 0.06,
        cover: 0.3,
        stretch: 3.5,
        softness: 0.06,
        lit: p.air(lit, 0.3),
        shade: p.air(shade, 0.4),
        alpha: 0.7,
        seed: seed + 2,
    });
    let streak = lerp(p.l.horizon, p.l.rim, 0.5);
    p.streaks(0.47, 0.012, streak, 0.35 * (1.0 - p.dark()), seed + 3);

    // Larks rising.
    let bird = p.air(p.lit(rgb(0x3a3430)), 0.2);
    for (x, y, s) in [
        (0.3, 0.24, 0.022),
        (0.345, 0.205, 0.016),
        (0.4, 0.27, 0.012),
    ] {
        let flap = noise(x * 40.0, y * 30.0, seed);
        p.lark_flying(x * w, y * h, s * h, bird, flap);
    }

    // The sea.
    sea(p);

    // A far headland melting into the haze.
    let far_top = move |x: f32| {
        let t = smooth(0.0, 0.3, x / w);
        h * (HORIZON - 0.06 * (1.0 - t).powf(0.7) - 0.006 * fbm1(x / h * 12.0, seed + 4, 3))
    };
    let far_c = p.air(p.lit(rgb(0x7a9a8a)), 0.62);
    let far_face = p.air(p.lit(rgb(0xd8d2c0)), 0.55);
    p.cv.band(
        far_top,
        |_| HORIZON * h + 1.0,
        |x, y, d| {
            let cap = 0.012 * h * (1.0 - smooth(0.0, 0.3, x / w));
            if d < cap {
                far_c
            } else {
                lerp(
                    far_face,
                    far_c,
                    smooth(0.6, 1.0, (y / h - 0.5) * 10.0) * 0.3,
                )
            }
        },
        1.0,
    );

    headland(p);

    // Our own clifftop meadow, with a path out towards the light.
    let front = move |x: f32| {
        let t = x / w;
        h * (0.79 + 0.05 * t * t - 0.05 * (1.0 - t).powi(3)
            + 0.012 * (fbm1(x / h * 3.0, seed + 5, 3) - 0.5))
    };
    let g_far = p.lit(rgb(0x8fb35e));
    let g_near = p.lit(rgb(0x4c7d36));
    let g_dark = p.lit(rgb(0x2f5a28));
    let rim = p.rim();
    let side = p.sun_side();
    p.cv.layer(
        front,
        |x, y, d| {
            let n = fbm(x / (h * 0.05), y / (h * 0.014), seed + 6, 3);
            let near = (d / (h * 0.2)).clamp(0.0, 1.0);
            let mut c = ramp3(g_dark, g_near, g_far, (1.0 - near) * 0.6 + n * 0.5);
            if d < 8.0 * u {
                let slope = (front(x + 3.0 * u) - front(x - 3.0 * u)) / (6.0 * u);
                c = lerp(
                    c,
                    rim,
                    (1.0 - d / (8.0 * u)).powi(2)
                        * (0.4 - slope * side * 2.0).clamp(0.0, 1.0)
                        * 0.8,
                );
            }
            c
        },
        1.0,
    );
    let dirt = p.lit(rgb(0xc8b088));
    let dirt_dark = p.lit(rgb(0x9a8462));
    p.cv.region(0.0, 0.78 * h, w, h, |x, y| {
        if y < front(x) + 2.0 {
            return None;
        }
        let v = ((y - front(x)) / (h - front(x)).max(1.0)).clamp(0.0, 1.0);
        let cx = w * 0.56 + h * (0.25 * (1.0 - v).powf(1.5) - 0.08 * v);
        let hw = h * (0.008 + 0.05 * v * v);
        let n = noise(y / (3.0 * u), x / (30.0 * u), seed + 7);
        let edge = hw * (1.0 + (n - 0.5) * 0.5) - (x - cx).abs();
        let cover = (edge + 0.5).clamp(0.0, 1.0);
        (cover > 0.0).then(|| {
            (
                lerp(
                    dirt,
                    dirt_dark,
                    fbm(x / (h * 0.02), y / (h * 0.01), seed + 8, 2),
                ),
                cover * 0.85,
            )
        })
    });

    // Grass, sea thrift and daisies.
    let blades = [
        p.lit(rgb(0x3e6e2e)),
        p.lit(rgb(0x5a8a3a)),
        p.lit(rgb(0x6f9a44)),
        p.lit(rgb(0x2f5626)),
    ];
    p.grass(&front, 0.0, 1.0, 3200, 0.035, &blades, 0.22);
    let tall = move |x: f32| h * 0.94 + 0.0 * x;
    p.grass(&tall, 0.0, 0.12, 260, 0.09, &blades, 0.08);
    p.grass(&tall, 0.86, 1.0, 260, 0.08, &blades, 0.08);
    let pinks = [
        p.lit(rgb(0xf08aa8)),
        p.lit(rgb(0xf6b0c4)),
        p.lit(rgb(0xe06890)),
    ];
    let clumps = move |x: f32, y: f32, k: u32| fbm(x / (h * 0.12), y / (h * 0.04), seed + k, 3);
    for _ in 0..p.many(700) {
        let x = p.rng.next() * w;
        let y = front(x) + p.rng.range(0.01, 0.2) * h;
        if clumps(x, y, 30) < 0.56 {
            continue;
        }
        let s = h * 0.006 * (1.0 + (y / h - 0.8) * 6.0);
        let stem = p.lit(rgb(0x4a7a34));
        p.cv.line([x, y], [x, y - s * 2.2], s * 0.25, stem, 1.0);
        for k in 0..5 {
            let a = k as f32 * 1.26;
            p.cv.disk(
                x + a.cos() * s * 0.4,
                y - s * 2.4 + a.sin() * s * 0.35,
                s * 0.45,
                p.rng.pick(&pinks),
                1.0,
            );
        }
    }
    let white = p.lit(rgb(0xfbf8ee));
    let yellow = p.lit(rgb(0xf6c840));
    for _ in 0..p.many(900) {
        let x = p.rng.next() * w;
        let y = front(x) + p.rng.range(0.01, 0.22) * h;
        if clumps(x, y, 31) < 0.58 {
            continue;
        }
        let s = h * 0.003 * (1.0 + (y / h - 0.8) * 6.0);
        p.cv.disk(x, y, s * 1.2, white, 0.95);
        p.cv.disk(x, y, s * 0.5, yellow, 1.0);
    }

    // A weathered post with our lark.
    let px = 0.2 * w;
    let base = front(px) + 0.05 * h;
    let post = p.lit(rgb(0x6a5a4a));
    let post_dark = p.lit(rgb(0x4a3e34));
    let s = 0.11 * h;
    p.cv.poly(
        &[
            [px - s * 0.08, base],
            [px + s * 0.08, base],
            [px + s * 0.07, base - s],
            [px - s * 0.07, base - s * 1.03],
        ],
        post,
        1.0,
    );
    p.cv.poly(
        &[
            [px + s * 0.01, base],
            [px + s * 0.08, base],
            [px + s * 0.07, base - s],
            [px + s * 0.01, base - s],
        ],
        post_dark,
        0.7,
    );
    p.lark_perched(px, base - s * 1.01, s * 0.36, -1.0);
    let _ = (scale, LAMP, WARM);
}

fn sea(p: &mut Painter) {
    let (w, h) = (p.w, p.h);
    let hy = HORIZON * h;
    let far = p.air(p.lit(rgb(0x6f9cc0)), 0.55);
    let near = p.lit(rgb(0x1e5878));
    let deep = p.lit(rgb(0x173f5c));
    let crest = lerp(p.l.horizon, rgb(0xffffff), 0.35);
    let seed = p.seed + 20;
    let horizon = p.l.horizon;
    p.cv.region(0.0, hy, w, h, |x, y| {
        let v = ((y - hy) / (h - hy)).clamp(0.0, 1.0);
        let z = v.powf(0.7);
        let sx = h * (0.015 + z * 0.18);
        let sy = h * (0.0015 + z * 0.012);
        let n = fbm(x / sx, y / sy, seed, 3);
        let mut c = ramp3(far, near, deep, z.powf(0.8));
        c = lerp(c, horizon, (1.0 - v).powi(6) * 0.6);
        c = lerp(c, crest, smooth(0.62, 0.8, n) * (0.35 - z * 0.15));
        c = scale(c, 0.94 + n * 0.1);
        Some((c, 1.0))
    });
    if p.l.body != super::Body::None {
        let (bx, _) = p.body_pos();
        let glint = lerp(p.l.body_color, rgb(0xffffff), 0.3);
        let a = if p.l.body == super::Body::Sun {
            0.9
        } else {
            0.6
        };
        p.glitter(bx, HORIZON + 0.004, 0.98, 0.035, glint, a, 1500);
    }
}

fn headland(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed + 40;
    let tip = 0.43 * w;
    let grass_top = move |x: f32| {
        let t = smooth(tip, w * 1.05, x);
        h * (0.552 - 0.075 * t.powf(0.8) + 0.006 * (fbm1(x / h * 5.0, seed, 3) - 0.5))
    };
    let base = move |x: f32| {
        let t = smooth(tip, w * 1.05, x);
        h * (0.6 + 0.12 * t.powf(1.3))
    };
    let cap = move |x: f32| h * (0.006 + 0.04 * smooth(tip, w * 1.05, x).powf(1.2));
    let side = p.sun_side();
    let chalk_lit = p.lit(rgb(0xf0e8d6));
    let chalk_shade = p.lit(rgb(0xb4aeb6));
    let chalk_dark = p.lit(rgb(0x8a8494));
    let g_lit = p.lit(rgb(0x9cbf62));
    let g_dark = p.lit(rgb(0x5e8a3e));
    let rim = p.rim();
    let face_light = if side < 0.0 { 1.0 } else { 0.45 };
    // Edge of the tip slants back towards the land.
    let tip_x = move |y: f32| tip + (y - grass_top(tip)) * 0.35;
    p.cv.region(tip - 4.0, grass_top(w) - 4.0, w, base(w) + 4.0, |x, y| {
        let (t, b) = (grass_top(x), base(x));
        let left = x - tip_x(y);
        let cover = (y - t + 0.5).clamp(0.0, 1.0)
            * (b - y + 0.5).clamp(0.0, 1.0)
            * (left + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let d = y - t;
        let c = if d < cap(x) {
            let n = fbm(x / (h * 0.03), y / (h * 0.01), seed + 1, 3);
            let mut g = lerp(
                g_dark,
                g_lit,
                smooth(0.3, 0.7, n) * 0.7 + 0.3 * (1.0 - d / cap(x).max(1.0)),
            );
            if d < 4.0 * u {
                g = lerp(g, rim, (1.0 - d / (4.0 * u)) * 0.6);
            }
            g
        } else {
            // Chalk with vertical fissures and soft horizontal bedding.
            let fiss = fbm(x / (h * 0.014), y / (h * 0.06), seed + 2, 4);
            let bed = fbm(x / (h * 0.2), y / (h * 0.006), seed + 3, 2);
            let li =
                (face_light * (0.62 + (fiss - 0.5) * 0.7) + (bed - 0.5) * 0.35).clamp(0.0, 1.0);
            let under = smooth(0.0, h * 0.012, d - cap(x));
            let mut c = ramp3(chalk_dark, chalk_shade, chalk_lit, li);
            c = lerp(scale(c, 0.7), c, under);
            // The tip catches light on its own face.
            let tip_face = smooth(h * 0.02, 0.0, left);
            lerp(
                c,
                if side < 0.0 { chalk_lit } else { chalk_shade },
                tip_face * 0.7,
            )
        };
        Some((c, cover))
    });
    // Sea stacks off the tip.
    for (sx, top, bottom, wid) in [(0.385, 0.578, 0.6, 0.012), (0.36, 0.586, 0.598, 0.007)] {
        let (x, t, b, half) = (sx * w, top * h, bottom * h, wid * h);
        p.cv.region(
            x - half * 1.5,
            t - 2.0,
            x + half * 1.5,
            b + 2.0,
            |px, py| {
                let v = ((py - t) / (b - t)).clamp(0.0, 1.0);
                let hw = half
                    * (0.7 + 0.3 * v)
                    * (1.0 + (noise(py / (2.0 * u), 0.3, seed + 5) - 0.5) * 0.3);
                let cover = (hw - (px - x).abs() + 0.5).clamp(0.0, 1.0)
                    * (py - t + 0.5).clamp(0.0, 1.0)
                    * (b - py + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    return None;
                }
                let li = 0.5 + 0.4 * ((px - x) / hw) * -side;
                let c = if py - t < 2.5 * u {
                    g_dark
                } else {
                    ramp3(chalk_dark, chalk_shade, chalk_lit, li)
                };
                Some((c, cover))
            },
        );
    }
    // Foam where the sea meets the rock.
    let foam = lerp(p.lit(rgb(0xffffff)), p.l.horizon, 0.15);
    p.cv.region(tip - h * 0.08, 0.57 * h, w, base(w) + h * 0.03, |x, y| {
        let b = if x > tip_x(base(x)) { base(x) } else { 0.6 * h };
        let d = (y - b) / (h * 0.006);
        if !(-0.5..=3.0).contains(&d) {
            return None;
        }
        let n = fbm(x / (h * 0.01), y / (h * 0.004), seed + 6, 3);
        let near_rock = smooth(h * 0.06, 0.0, (tip_x(y) - x).max(0.0));
        let a = smooth(0.45, 0.7, n) * (1.0 - d / 3.0).max(0.0) * near_rock;
        (a > 0.0).then_some((foam, a * 0.85))
    });

    // Lighthouse and the keeper's cottage.
    let lx = 0.615 * w;
    let lg = grass_top(lx) + 0.006 * h;
    lighthouse(p, lx, lg, 0.15 * h);
    let kx = 0.67 * w;
    cottage(p, kx, grass_top(kx) + 0.008 * h, 0.03 * h);
}

fn lighthouse(p: &mut Painter, x: f32, ground: f32, hgt: f32) {
    let side = p.sun_side();
    let white = p.lit(rgb(0xf4f0e8));
    let shade = p.lit(rgb(0xb6b4c4));
    let red = p.lit(rgb(0xc8443a));
    let red_shade = p.lit(rgb(0x8a2e2e));
    let dark = p.lit(rgb(0x2e3038));
    let (bw, tw) = (hgt * 0.12, hgt * 0.075);
    let top = ground - hgt * 0.8;
    p.cv.region(x - bw - 2.0, top, x + bw + 2.0, ground, |px, py| {
        let v = (ground - py) / (ground - top);
        let half = bw + (tw - bw) * v;
        let cover = (half - (px - x).abs() + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let band = ((v * 4.0).floor() as i32) % 2 == 1;
        let li = (0.55 + 0.45 * ((px - x) / half) * side).clamp(0.0, 1.0);
        let c = if band {
            lerp(red_shade, red, li)
        } else {
            lerp(shade, white, li)
        };
        Some((c, cover))
    });
    // Gallery, lantern room and cap.
    p.cv.rect(
        x - tw * 1.45,
        top - hgt * 0.025,
        x + tw * 1.45,
        top,
        dark,
        1.0,
    );
    let lamps = p.lamps();
    let glass = lerp(
        lerp(p.l.horizon, rgb(0x384050), 0.4),
        LAMP,
        0.3 + 0.7 * lamps,
    );
    p.cv.rect(
        x - tw * 0.9,
        top - hgt * 0.11,
        x + tw * 0.9,
        top - hgt * 0.025,
        glass,
        1.0,
    );
    for k in [-0.45, 0.0, 0.45] {
        p.cv.line(
            [x + tw * k * 2.0, top - hgt * 0.11],
            [x + tw * k * 2.0, top - hgt * 0.025],
            hgt * 0.008,
            dark,
            1.0,
        );
    }
    p.cv.poly(
        &[
            [x - tw * 1.15, top - hgt * 0.11],
            [x + tw * 1.15, top - hgt * 0.11],
            [x, top - hgt * 0.19],
        ],
        red_shade,
        1.0,
    );
    p.cv.disk(x, top - hgt * 0.2, hgt * 0.012, dark, 1.0);
    let ly = top - hgt * 0.068;
    if lamps > 0.0 {
        // Two beams sweeping across the evening air.
        let (w, h) = (p.w, p.h);
        for dir in [-1.0f32, 1.0] {
            let len = if dir < 0.0 { 0.55 * w } else { 0.4 * w };
            p.cv.light_region(
                x.min(x + dir * len),
                ly - h * 0.06,
                x.max(x + dir * len),
                ly + h * 0.06,
                |px, py| {
                    let t = (px - x) * dir / len;
                    if t <= 0.0 {
                        return None;
                    }
                    let spread = h * (0.004 + 0.03 * t);
                    let d = (py - (ly - t * h * 0.012)).abs() / spread;
                    let a = smooth(1.0, 0.0, d) * (1.0 - t).powf(1.5) * 0.35 * lamps;
                    (a > 0.0).then_some((LAMP, a))
                },
            );
        }
        p.cv.glow(x, ly, hgt * 0.5, WARM, 0.6 * lamps);
    }
    p.cv.glow(x, ly, hgt * 0.12, LAMP, 0.5 * (0.2 + 0.8 * lamps));
}

fn cottage(p: &mut Painter, x: f32, ground: f32, s: f32) {
    let side = p.sun_side();
    let wall = p.lit(rgb(0xf2ece0));
    let wall_shade = p.lit(rgb(0xb8b4c0));
    let roof = p.lit(rgb(0x5a6a7a));
    let roof_dark = p.lit(rgb(0x3e4a58));
    let half = s * 1.3;
    p.cv.rect(x - half, ground - s, x + half, ground, wall, 1.0);
    let shaded = if side > 0.0 {
        [x - half, x - half * 0.2]
    } else {
        [x + half * 0.2, x + half]
    };
    p.cv.rect(shaded[0], ground - s, shaded[1], ground, wall_shade, 0.5);
    p.cv.poly(
        &[
            [x - half * 1.1, ground - s],
            [x + half * 1.1, ground - s],
            [x + half * 0.8, ground - s * 1.7],
            [x - half * 0.8, ground - s * 1.7],
        ],
        roof,
        1.0,
    );
    p.cv.poly(
        &[
            [x - half * 1.1, ground - s],
            [x + half * 1.1, ground - s],
            [x + half * 1.05, ground - s * 1.1],
            [x - half * 1.05, ground - s * 1.1],
        ],
        roof_dark,
        1.0,
    );
    p.cv.rect(
        x + half * 0.45,
        ground - s * 2.0,
        x + half * 0.65,
        ground - s * 1.5,
        wall_shade,
        1.0,
    );
    let frame = p.lit(rgb(0x3a4450));
    let lamps = p.lamps();
    for k in [-0.55, 0.0, 0.55] {
        let cx = x + half * k;
        if k == 0.0 {
            p.cv.rect(
                cx - s * 0.16,
                ground - s * 0.62,
                cx + s * 0.16,
                ground,
                p.lit(rgb(0x2e5a7a)),
                1.0,
            );
        } else {
            p.window(
                cx - s * 0.16,
                ground - s * 0.7,
                cx + s * 0.16,
                ground - s * 0.36,
                frame,
                lamps,
                false,
            );
        }
    }
}
