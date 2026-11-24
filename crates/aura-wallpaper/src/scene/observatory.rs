//! Star Ridge: a wide night sky over a dark ridge. The galaxy arcs across,
//! a small domed observatory glows on the crest, lanterns mark the path up
//! to it and a far town warms the horizon. Most of the picture is sky, left
//! open for falling stars.

use super::canvas::{lerp, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, noise};
use super::painter::{Clouds, LAMP, Painter, WARM};

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    // A faint glow along the horizon from the last of the light.
    let dusk = lerp(p.l.horizon, rgb(0xd88a7a), 0.3);
    p.cv.light_region(0.0, 0.45 * h, w, 0.85 * h, |_, y| {
        let t = ((y / h - 0.45) / 0.35).clamp(0.0, 1.0);
        Some((dusk, t * t * 0.18))
    });
    p.milky_way(
        (0.02, 0.92),
        (0.98, -0.08),
        0.13,
        [rgb(0x7e6ac8), rgb(0x6ac0c8)],
        1.0,
    );
    p.stars(3200, 0.85, 1.0);
    p.body(1.0);
    let cl = lerp(rgb(0x6a5a9a), p.l.horizon, 0.3);
    let cs = rgb(0x1e1a40);
    p.clouds(&Clouds {
        y0: 0.55,
        y1: 0.72,
        size: 0.06,
        cover: 0.3,
        stretch: 4.0,
        softness: 0.08,
        lit: cl,
        shade: cs,
        alpha: 0.55,
        seed: seed + 1,
    });

    // Far mountains with a town glowing at their feet.
    let far = p.skyline(0.79, 0.11, 2.2, seed + 2, 0.6);
    let far_c = lerp(rgb(0x24244e), p.l.horizon, 0.5);
    let far_low = lerp(rgb(0x14143a), p.l.horizon, 0.3);
    p.land(&far, 0.9, far_c, far_low, 0.1, 0.05, 0.3, seed + 3);
    let tx = 0.2 * w;
    let ty = far(tx) + 0.035 * h;
    p.cv.glow_ellipse(tx, ty, 0.2 * h, 0.07 * h, WARM, 0.22);
    for _ in 0..p.many(90) {
        let x = tx + (p.rng.next() + p.rng.next() - 1.0) * 0.12 * h;
        let y = ty + p.rng.range(-0.008, 0.02) * h;
        if y < far(x) + 2.0 {
            continue;
        }
        p.cv.disk(
            x,
            y,
            p.rng.range(0.6, 1.4) * u,
            p.rng.pick(&[LAMP, WARM, rgb(0xfff4e0)]),
            p.rng.range(0.5, 1.0),
        );
    }
    p.fog(
        0.8,
        0.02,
        0.25,
        lerp(p.l.horizon, rgb(0x5a5088), 0.5),
        seed + 4,
    );

    // The ridge, climbing to a knoll on the right.
    let ridge = move |x: f32| {
        let t = x / w;
        let knoll = (-((x - 0.72 * w) / (0.32 * h)).powi(2)).exp();
        let crag = fbm1(x / h * 9.0, seed + 9, 4) - 0.5;
        h * (0.86 - 0.17 * knoll - 0.03 * t
            + 0.025 * (fbm1(x / h * 4.0, seed + 5, 4) - 0.5)
            + 0.012 * crag * (0.3 + knoll))
    };
    let ridge_c = p.lit(rgb(0x262a48));
    let ridge_low = rgb(0x0c0e1e);
    p.land(&ridge, 0.96, ridge_c, ridge_low, 0.2, 0.12, 0.7, seed + 6);
    // Rocks and scrub on the slope.
    let rock = scale(ridge_c, 0.85);
    let rock_hi = lerp(ridge_c, p.rim(), 0.35);
    for _ in 0..p.many(70) {
        let x = p.rng.next() * w;
        let y = ridge(x) + p.rng.range(0.005, 0.12) * h;
        let r = p.rng.range(0.004, 0.012) * h;
        p.cv.ellipse(x, y, r * 1.4, r * 0.8, rock, 1.0);
        p.cv.ellipse(x - r * 0.3, y - r * 0.4, r * 0.7, r * 0.25, rock_hi, 0.35);
    }

    // The observatory on the knoll.
    let ox = 0.72 * w;
    observatory(p, ox, ridge(ox) + 0.008 * h, 0.07 * h);

    // A winding path with lanterns up to it.
    let lamps = 0.6 + 0.4 * p.lamps();
    let path_c = lerp(ridge_c, rgb(0x6a6488), 0.35);
    let mut prev: Option<[f32; 2]> = None;
    for i in 0..=40 {
        let t = i as f32 / 40.0;
        let x = w * (0.42 + 0.3 * t) + (t * 11.0).sin() * h * 0.05 * (1.0 - t);
        let y = h * (1.02 - 0.0 * t) + (ridge(x) + 0.012 * h - h * 1.02) * t.powf(0.8);
        if let Some(a) = prev {
            p.cv.line(a, [x, y], (5.0 * (1.0 - t) + 1.2) * u, path_c, 0.7);
        }
        prev = Some([x, y]);
        if i % 6 == 3 {
            let s = (0.02 * (1.0 - t) + 0.006) * h;
            p.cv.line(
                [x + s * 0.4, y],
                [x + s * 0.4, y - s],
                s * 0.06,
                rgb(0x0e1020),
                1.0,
            );
            p.cv.glow(x + s * 0.4, y - s, s * 1.4, WARM, 0.5 * lamps);
            p.cv.disk(x + s * 0.4, y - s, s * 0.08 + 0.6, LAMP, lamps);
            p.cv.glow_ellipse(x + s * 0.4, y, s * 1.2, s * 0.35, WARM, 0.2 * lamps);
        }
    }

    // Foreground: a dark grassy brow and a wind-bent pine.
    let brow = move |x: f32| {
        let t = x / w;
        h * (0.93 - 0.12 * (1.0 - t).powi(3) + 0.012 * (fbm1(x / h * 6.0, seed + 7, 3) - 0.5))
    };
    p.land(
        &brow,
        1.0,
        rgb(0x121428),
        rgb(0x06070f),
        0.1,
        0.1,
        0.6,
        seed + 8,
    );
    let blades = [rgb(0x0e1022), rgb(0x161a34), rgb(0x1c2040)];
    p.grass(&brow, 0.0, 1.0, 1400, 0.03, &blades, 0.06);
    pine(p, 0.085 * w, brow(0.085 * w) + 0.01 * h, 0.42 * h);
    let _ = (ramp3, smooth, fbm, noise);
}

fn observatory(p: &mut Painter, x: f32, ground: f32, s: f32) {
    let side = p.sun_side();
    let wall = p.lit(rgb(0xd8d4dc));
    let wall_dark = p.lit(rgb(0x8a86a0));
    let dome = p.lit(rgb(0xe4e8f0));
    let dome_dark = p.lit(rgb(0x7a7e98));
    let rim = p.rim();
    let lamps = 0.6 + 0.4 * p.lamps();
    let half = s * 0.75;
    // Drum.
    p.cv.region(
        x - half - 1.0,
        ground - s * 0.6,
        x + half + 1.0,
        ground,
        |px, _| {
            let t = (px - x) / half;
            if t.abs() > 1.0 {
                return None;
            }
            let li = (0.5 + 0.5 * t * side).clamp(0.0, 1.0);
            Some((lerp(wall_dark, wall, li * li), 1.0))
        },
    );
    // Dome with its open slit and the telescope.
    let (cx, cy, r) = (x, ground - s * 0.6, half * 1.02);
    let slit = p.lit(rgb(0x1a1a2a));
    p.cv.region(cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy, |px, py| {
        let (dx, dy) = ((px - cx) / r, (py - cy) / r);
        let d = (dx * dx + dy * dy).sqrt();
        let cover = ((1.0 - d) * r + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let nz = (1.0 - d * d).max(0.0).sqrt();
        let li = (0.35 + 0.55 * dx * side - 0.3 * dy * 0.5 + 0.2 * nz).clamp(0.0, 1.0);
        let mut c = lerp(dome_dark, dome, li);
        if d > 0.9 {
            c = lerp(
                c,
                rim,
                (d - 0.9) * 10.0 * 0.5 * (0.5 + 0.5 * dx * side).max(0.0),
            );
        }
        // The slit runs over the dome slightly off centre.
        let sx = px - (cx + r * 0.15);
        if sx.abs() < r * 0.12 && py < cy - r * 0.05 {
            let glow = lerp(slit, rgb(0xffcf80), lamps * 0.75);
            c = lerp(
                glow,
                rgb(0xffe8b0),
                (1.0 - sx.abs() / (r * 0.12)) * 0.3 * lamps,
            );
        }
        Some((c, cover))
    });
    let tube = p.lit(rgb(0x3a3a4a));
    let a = [cx + r * 0.15, cy - r * 0.45];
    let b = [cx + r * 0.15 + r * 0.32, cy - r * 1.05];
    p.cv.taper(a, b, r * 0.13, r * 0.11, tube, 1.0);
    p.cv.glow(cx + r * 0.15, cy - r * 0.5, r * 1.3, WARM, 0.3 * lamps);
    // Door and a lit window.
    p.cv.rect(
        x - s * 0.1,
        ground - s * 0.38,
        x + s * 0.1,
        ground,
        lerp(p.lit(rgb(0x3a3040)), rgb(0xffd090), lamps * 0.85),
        1.0,
    );
    p.cv.glow(x, ground - s * 0.2, s * 0.9, WARM, 0.35 * lamps);
    p.cv.glow_ellipse(x, ground + s * 0.05, s * 1.5, s * 0.25, WARM, 0.25 * lamps);
    // A little hut beside it.
    let hx = x - s * 1.25;
    p.cv.rect(
        hx - s * 0.32,
        ground - s * 0.36,
        hx + s * 0.32,
        ground + s * 0.02,
        wall_dark,
        1.0,
    );
    p.cv.poly(
        &[
            [hx - s * 0.4, ground - s * 0.36],
            [hx + s * 0.4, ground - s * 0.36],
            [hx, ground - s * 0.6],
        ],
        p.lit(rgb(0x4a3a4a)),
        1.0,
    );
    p.cv.rect(
        hx - s * 0.12,
        ground - s * 0.26,
        hx + s * 0.08,
        ground - s * 0.12,
        lerp(rgb(0x2a2a3a), WARM, lamps),
        1.0,
    );
}

/// A gnarled pine bent by the wind, in silhouette, with a lark on a bough.
fn pine(p: &mut Painter, x: f32, base: f32, hgt: f32) {
    let bark = rgb(0x080a14);
    let needles = [rgb(0x070912), rgb(0x0c1020), rgb(0x161c34)];
    let mut pts = vec![[x, base]];
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        pts.push([
            x + hgt * (0.18 * t * t + 0.03 * (t * 8.0).sin()),
            base - hgt * t * 0.9,
        ]);
    }
    for (i, pair) in pts.windows(2).enumerate() {
        let wd = hgt * 0.05 * (1.0 - i as f32 / 7.0);
        p.cv.taper(pair[0], pair[1], wd, wd * 0.85, bark, 1.0);
    }
    // Flat pads of needles reaching away from the wind.
    let pads = [
        (0.35, 0.28, 0.16),
        (0.55, 0.22, 0.2),
        (0.72, 0.18, 0.22),
        (0.88, 0.16, 0.18),
        (0.98, 0.12, 0.12),
    ];
    for (k, &(t, len, size)) in pads.iter().enumerate() {
        let idx = (t * 6.0) as usize;
        let at = pts[idx.min(6)];
        let tip = [at[0] + hgt * len, at[1] - hgt * 0.03];
        p.cv.taper(at, tip, hgt * 0.025, hgt * 0.008, bark, 1.0);
        p.crown(
            tip[0] - hgt * len * 0.3,
            tip[1] - hgt * 0.02,
            hgt * size * 0.75,
            hgt * size * 0.22,
            needles,
            p.seed + 60 + k as u32,
            1.0,
        );
        if k > 0 {
            let back = [at[0] - hgt * len * 0.45, at[1] + hgt * 0.01];
            p.cv.taper(at, back, hgt * 0.02, hgt * 0.006, bark, 1.0);
            p.crown(
                back[0],
                back[1] - hgt * 0.015,
                hgt * size * 0.35,
                hgt * size * 0.15,
                needles,
                p.seed + 80 + k as u32,
                1.0,
            );
        }
    }
    // A lark in silhouette on the lowest bough.
    let at = pts[2];
    let px = at[0] + hgt * 0.2;
    let py = at[1] - hgt * 0.035;
    let c = rgb(0x05060c);
    let s = hgt * 0.055;
    p.cv.ellipse(px, py - s * 0.3, s * 0.32, s * 0.2, c, 1.0);
    p.cv.disk(px + s * 0.22, py - s * 0.55, s * 0.14, c, 1.0);
    p.cv.poly(
        &[
            [px + s * 0.12, py - s * 0.62],
            [px + s * 0.02, py - s * 0.86],
            [px + s * 0.24, py - s * 0.66],
        ],
        c,
        1.0,
    );
    p.cv.poly(
        &[
            [px - s * 0.25, py - s * 0.3],
            [px - s * 0.6, py - s * 0.12],
            [px - s * 0.2, py - s * 0.2],
        ],
        c,
        1.0,
    );
    p.cv.poly(
        &[
            [px + s * 0.33, py - s * 0.58],
            [px + s * 0.46, py - s * 0.54],
            [px + s * 0.33, py - s * 0.5],
        ],
        c,
        1.0,
    );
}
