//! Christmas Eve: a red clapboard house on a gentle rise is the heart of the
//! picture, every window glowing gold, a wreath on the door and coloured
//! lights along the eaves. A decorated tree stands lit beside it, a lantern
//! path winds down through the snow to a sled, and a little village of
//! lit windows and a steeple sits on the far hills under an evening sky.

use super::canvas::{lerp, rgb, scale, smooth};
use super::noise::{fbm, fbm1, noise};
use super::painter::{Fir, LAMP, Painter, SNOW, WARM};
use super::winter::snowfield;

/// Bulb colours for string lights: warm white, red, gold, green and blue.
const BULBS: [u32; 5] = [0xfff1c8, 0xff5a4a, 0xffc24a, 0x6fe08a, 0x6fb4ff];

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed;
    p.sky();
    p.stars(1100, 0.55, 0.9);
    p.body(1.0);
    let cloud_lit = p.air(p.lit(rgb(0xf4e6f0)), 0.3);
    let cloud_shade = p.air(p.lit(rgb(0x8f8fb8)), 0.4);
    p.cumulus(0.06, 0.22, 3, 0.06, cloud_lit, cloud_shade, 0.55, seed + 1);

    // Rolling hills, layer on layer, with no great peak: the village sits
    // on the far ones.
    let far = move |x: f32| {
        h * (0.5
            - 0.05 * (x / w * 3.4 + 0.6).sin() * (x / w * 1.3).cos()
            - 0.025 * fbm1(x / h * 2.5, seed + 2, 3))
    };
    snowfield(p, &far, 0.62, 0.55, seed + 3);
    let distant = Fir {
        width: 0.32,
        tiers: 5.0,
        ..firs_look(p, 0.6, 0.6)
    };
    p.firs(&far, 0.0, 0.42, 70, (0.016, 0.03), &distant, 0.003);
    village(p, &far);
    let mist = p.air(p.lit(rgb(0xe8ecf6)), 0.4);
    p.fog(0.535, 0.018, 0.5, mist, seed + 4);

    let middle = move |x: f32| {
        h * (0.585 + 0.03 * (x / w * 2.2 + 2.0).sin() - 0.018 * fbm1(x / h * 3.0, seed + 5, 3))
    };
    snowfield(p, &middle, 0.75, 0.3, seed + 6);
    let wood = Fir {
        width: 0.3,
        tiers: 6.0,
        ..firs_look(p, 0.3, 0.75)
    };
    p.firs(&middle, 0.55, 1.02, 90, (0.03, 0.06), &wood, 0.004);
    p.firs(&middle, -0.02, 0.18, 30, (0.03, 0.055), &wood, 0.004);

    // The rise the house stands on, rounding off toward the right.
    let hx = 0.36 * w;
    let rise = move |x: f32| {
        let d = (x - hx) / (0.42 * w);
        h * (0.7 - 0.045 * (1.0 - d * d).max(0.0) + 0.012 * (fbm1(x / h * 3.5, seed + 7, 3) - 0.5))
    };
    snowfield(p, &rise, 0.9, 0.0, seed + 8);
    let shadow = p.lit(rgb(0x8a98c8));
    let near = firs_look(p, 0.08, 0.85);
    let mut grove = p.plan_firs(&rise, 0.04, 0.16, 7, (0.08, 0.15));
    grove.extend(p.plan_firs(&rise, 0.62, 0.74, 6, (0.07, 0.12)));
    grove.sort_by(|a, b| a.1.total_cmp(&b.1));
    for &(x, b, hgt, _) in &grove {
        p.cast_shadow(x, b, hgt * 1.2, hgt * 0.05, shadow, 0.4);
    }
    for &(x, b, hgt, s) in &grove {
        p.fir(x, b, hgt, &near, s);
    }

    // The house and its tree.
    let s = 0.25 * h;
    let ground = rise(hx) + 0.02 * h;
    let lamps = p.lamps();
    p.cv.glow_ellipse(hx, ground + s * 0.12, s * 2.6, s * 0.5, WARM, 0.3 * lamps);
    p.cast_shadow(hx, ground, s * 1.1, s * 0.06, shadow, 0.35);
    house(p, hx, ground, s);
    let tx = hx + s * 1.45;
    christmas_tree(p, tx, rise(tx) + 0.02 * h, s * 1.1, seed + 20);

    // The lantern path down to the front of the picture.
    let front = move |x: f32| {
        let lift = (1.0 - (x / w - 0.85).abs() / 0.3).max(0.0) * 0.05;
        h * (0.885 - lift + 0.025 * (fbm1(x / h * 2.0, seed + 9, 3) - 0.5))
    };
    path(p, hx + s * 0.02, ground, &front);
    snowfield(p, &front, 1.0, 0.0, seed + 10);
    sled(p, 0.6 * w, front(0.6 * w) + 0.05 * h, 0.075 * h);
    street_lamp(p, 0.8 * w, front(0.8 * w) + 0.05 * h, 0.3 * h);

    // Snow-laden firs at the right edge frame the view.
    let framing = Fir {
        width: 0.26,
        tiers: 10.0,
        ..firs_look(p, 0.0, 0.95)
    };
    p.cast_shadow(
        0.95 * w,
        front(0.95 * w) + 0.03 * h,
        0.4 * h,
        0.03 * h,
        shadow,
        0.45,
    );
    p.fir(
        0.93 * w,
        front(0.93 * w) + 0.05 * h,
        0.62 * h,
        &framing,
        seed + 30,
    );
    p.fir(1.03 * w, front(w) + 0.07 * h, 0.95 * h, &framing, seed + 31);
    p.fir(
        -0.04 * w,
        front(0.0) + 0.08 * h,
        0.78 * h,
        &framing,
        seed + 32,
    );

    // Glints where lamplight catches the snow.
    let spark = lerp(rgb(0xffffff), LAMP, 0.4);
    for _ in 0..p.many(600) {
        let x = p.rng.next() * w;
        let y = front(x) + p.rng.range(0.0, 0.12) * h;
        let a = p.rng.range(0.2, 0.7);
        p.cv.disk(x, y, 1.0 * u, spark, a);
    }
}

fn firs_look(p: &Painter, haze: f32, snow: f32) -> Fir {
    Fir {
        dark: p.air(p.lit(rgb(0x0f2620)), haze),
        mid: p.air(p.lit(rgb(0x1d3f33)), haze),
        light: p.air(p.lit(rgb(0x3a664e)), haze),
        trunk: p.air(p.lit(rgb(0x3a2a22)), haze),
        snow,
        snow_lit: p.air(p.lit(SNOW), haze * 0.7),
        snow_shade: p.air(p.lit(rgb(0xa9b8dc)), haze * 0.7),
        width: 0.3,
        tiers: 6.0,
    }
}

/// A string of small coloured bulbs hanging between two points.
fn string_lights(
    p: &mut Painter,
    a: [f32; 2],
    b: [f32; 2],
    sag: f32,
    step: f32,
    size: f32,
    first: usize,
) {
    let wire = p.lit(rgb(0x1e2420));
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    let n = ((len / step) as usize).max(2);
    let at = |t: f32| {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t + sag * 4.0 * t * (1.0 - t),
        ]
    };
    for i in 0..n {
        let (p0, p1) = (at(i as f32 / n as f32), at((i + 1) as f32 / n as f32));
        p.cv.line(p0, p1, size * 0.25, wire, 0.8);
    }
    for i in 0..n {
        let q = at((i as f32 + 0.5) / n as f32);
        let c = rgb(BULBS[(first + i) % BULBS.len()]);
        p.cv.glow(q[0], q[1] + size, size * 5.0, c, 0.28);
        p.cv.disk(q[0], q[1] + size, size, lerp(c, rgb(0xffffff), 0.35), 1.0);
    }
}

fn wreath(p: &mut Painter, x: f32, y: f32, r: f32, seed: u32) {
    let green = p.lit(rgb(0x2c5a36));
    let light = p.lit(rgb(0x4f8a52));
    p.cv.region(
        x - r * 1.4,
        y - r * 1.4,
        x + r * 1.4,
        y + r * 1.4,
        |px, py| {
            let d = (px - x).hypot(py - y);
            let n = noise(px / (r * 0.18), py / (r * 0.18), seed);
            let ring = (r * 0.32 + (n - 0.5) * r * 0.2 - (d - r).abs() + 0.5).clamp(0.0, 1.0);
            let up = smooth(0.0, r, y - py) * 0.5 + n * 0.5;
            (ring > 0.0).then_some((lerp(green, light, up), ring))
        },
    );
    let berry = rgb(0xd8392e);
    for k in 0..6 {
        let a = k as f32 * 1.05 + 0.3;
        p.cv.disk(x + a.cos() * r, y + a.sin() * r, r * 0.11, berry, 1.0);
    }
    // A red bow at the bottom.
    let bow = rgb(0xc92f2a);
    p.cv.poly(
        &[
            [x, y + r],
            [x - r * 0.45, y + r * 0.75],
            [x - r * 0.45, y + r * 1.25],
        ],
        bow,
        1.0,
    );
    p.cv.poly(
        &[
            [x, y + r],
            [x + r * 0.45, y + r * 0.75],
            [x + r * 0.45, y + r * 1.25],
        ],
        bow,
        1.0,
    );
    p.cv.disk(x, y + r, r * 0.14, scale(bow, 0.8), 1.0);
}

/// Snow lying along a ledge from x0 to x1, with a rounded, lumpy top.
fn snow_ledge(p: &mut Painter, x0: f32, x1: f32, y: f32, depth: f32, seed: u32) {
    let lit = p.lit(SNOW);
    let shade = p.lit(rgb(0xb4c2e0));
    p.cv.region(
        x0 - depth,
        y - depth * 1.6,
        x1 + depth,
        y + depth * 0.6,
        |x, py| {
            let lump = noise(x / (depth * 2.0), 0.4, seed) * depth * 0.6;
            let top = y - depth - lump;
            let end = (x - (x0 - depth * 0.4)).min((x1 + depth * 0.4) - x);
            let cover = (py - top + 0.5).clamp(0.0, 1.0)
                * (y + depth * 0.3 - py + 0.5).clamp(0.0, 1.0)
                * (end + 0.5).clamp(0.0, 1.0);
            (cover > 0.0).then_some((
                lerp(lit, shade, smooth(top, y + depth * 0.3, py) * 0.6),
                cover,
            ))
        },
    );
}

/// A gable roof thick with snow, from eave to eave through the apex. A
/// gable that faces us shows only its edges, over the wall below; one seen
/// from the side shows the whole slope of the roof.
fn snowy_roof(
    p: &mut Painter,
    left: [f32; 2],
    apex: [f32; 2],
    right: [f32; 2],
    s: f32,
    facing_us: bool,
    seed: u32,
) {
    let roof = p.lit(rgb(0x3a3240));
    let roof_edge = p.lit(rgb(0x241e28));
    if facing_us {
        p.cv.line(left, apex, s * 0.07, roof, 1.0);
        p.cv.line(apex, right, s * 0.07, roof, 1.0);
    } else {
        p.cv.poly(&[apex, right, left], roof, 1.0);
    }
    p.cv.line(left, apex, s * 0.03, roof_edge, 1.0);
    p.cv.line(apex, right, s * 0.03, roof_edge, 1.0);
    let lit = p.lit(SNOW);
    let shade = p.lit(rgb(0xb0bfdf));
    let side = p.sun_side();
    let (ax, ay) = (apex[0], apex[1] - s * 0.1);
    let along = |x: f32| {
        let (ex, ey) = if x < ax {
            (left[0], left[1])
        } else {
            (right[0], right[1])
        };
        ay + (x - ax) / (ex - ax) * (ey - ay)
    };
    p.cv.region(
        left[0] - s * 0.08,
        ay - 2.0,
        right[0] + s * 0.08,
        left[1].max(right[1]) + s * 0.2,
        |x, y| {
            let top = along(x.clamp(left[0], right[0]));
            let lump = (noise(x / (s * 0.1), 0.5, seed) - 0.5) * s * 0.06;
            let bottom = top + s * 0.16 + lump;
            let ends = (x - (left[0] - s * 0.05)).min((right[0] + s * 0.05) - x);
            let cover = ((y - (top - s * 0.02)) + 0.5).clamp(0.0, 1.0)
                * ((bottom - y) + 0.5).clamp(0.0, 1.0)
                * (ends + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            let facing = if (x - ax) * side > 0.0 { 1.0 } else { 0.6 };
            let under = smooth(bottom - s * 0.06, bottom, y);
            Some((lerp(lerp(shade, lit, facing), shade, under * 0.7), cover))
        },
    );
}

fn house(p: &mut Painter, cx: f32, ground: f32, s: f32) {
    let seed = p.seed + 100;
    let lamps = p.lamps();
    let side = p.sun_side();
    let red = p.lit(rgb(0x9a3328));
    let red_light = p.lit(rgb(0xc2503c));
    let red_dark = p.lit(rgb(0x5e1f1a));
    let trim = p.lit(rgb(0xf2ece0));
    let stone = p.lit(rgb(0x6a6670));

    // A lower wing to the left, set back a little.
    let (wl, wr) = (cx - s * 1.25, cx - s * 0.55);
    let wing_top = ground - s * 0.55;
    p.cv.region(wl, wing_top, wr, ground, |x, y| {
        let board = ((ground - y) / (s * 0.07)).fract();
        let n = fbm(x / (s * 0.2), y / (s * 0.04), seed, 2);
        Some((
            lerp(red_dark, red, smooth(0.0, 0.25, board) * 0.6 + n * 0.3),
            1.0,
        ))
    });
    snowy_roof(
        p,
        [wl - s * 0.1, wing_top + s * 0.02],
        [(wl + wr) * 0.5, ground - s * 0.95],
        [wr + s * 0.08, wing_top + s * 0.02],
        s * 0.8,
        false,
        seed + 1,
    );
    p.window(
        wl + s * 0.18,
        ground - s * 0.42,
        wl + s * 0.46,
        ground - s * 0.16,
        trim,
        lamps,
        true,
    );
    snow_ledge(
        p,
        wl + s * 0.15,
        wl + s * 0.49,
        ground - s * 0.14,
        s * 0.025,
        seed + 2,
    );

    // The main house: clapboard walls under a steep gable.
    let half = s * 0.62;
    let (x0, x1) = (cx - half, cx + half);
    let eaves = ground - s * 0.95;
    p.cv.region(x0, eaves, x1, ground, |x, y| {
        let board = ((ground - y) / (s * 0.065)).fract();
        let n = fbm(x / (s * 0.25), y / (s * 0.03), seed + 3, 2);
        let lit = smooth(0.0, 0.3, board) * 0.55 + n * 0.25 + (x - cx) / half * side * 0.12;
        Some((
            lerp(red_dark, lerp(red, red_light, 0.35), lit.clamp(0.0, 1.0)),
            1.0,
        ))
    });
    // The gable wall above the eaves.
    let apex = [cx, ground - s * 1.62];
    p.cv.poly(&[[x0, eaves], [x1, eaves], apex], red, 1.0);
    p.cv.region(x0, apex[1], x1, eaves, |x, y| {
        let inside = (y - apex[1]) / (eaves - apex[1]) * half - (x - cx).abs();
        let board = ((ground - y) / (s * 0.065)).fract();
        (inside > 0.0).then_some((lerp(red_dark, red, smooth(0.0, 0.25, board) * 0.7), 1.0))
    });
    // Corner boards.
    for x in [x0, x1] {
        p.cv.rect(x - s * 0.03, eaves, x + s * 0.03, ground, trim, 1.0);
    }
    // Chimney and smoke.
    let (chx, chy) = (cx + half * 0.5, ground - s * 1.62);
    p.cv.rect(
        chx - s * 0.08,
        chy,
        chx + s * 0.08,
        ground - s * 1.2,
        stone,
        1.0,
    );
    p.cv.rect(
        chx - s * 0.1,
        chy - s * 0.04,
        chx + s * 0.1,
        chy,
        scale(stone, 0.8),
        1.0,
    );
    p.cv.ellipse(chx, chy - s * 0.05, s * 0.11, s * 0.035, p.lit(SNOW), 1.0);
    p.smoke(chx, chy - s * 0.08, s * 0.28, 0.5 * -side);
    let (left, right) = (
        [x0 - s * 0.14, eaves + s * 0.06],
        [x1 + s * 0.14, eaves + s * 0.06],
    );
    snowy_roof(
        p,
        left,
        [apex[0], apex[1] - s * 0.06],
        right,
        s,
        true,
        seed + 4,
    );
    // Lights along both edges of the gable and across the front.
    let drop = s * 0.17;
    string_lights(
        p,
        [left[0] + s * 0.05, left[1] + drop],
        [apex[0], apex[1] + drop * 0.8],
        s * 0.03,
        s * 0.09,
        s * 0.018,
        0,
    );
    string_lights(
        p,
        [apex[0], apex[1] + drop * 0.8],
        [right[0] - s * 0.05, right[1] + drop],
        s * 0.03,
        s * 0.09,
        s * 0.018,
        3,
    );
    string_lights(
        p,
        [x0, eaves + s * 0.2],
        [x1, eaves + s * 0.2],
        s * 0.05,
        s * 0.08,
        s * 0.016,
        1,
    );

    // Windows: a big front window with the family tree inside, two on the
    // other side of the door, and two in the gable with a round one above.
    let big = (
        cx - half * 0.88,
        ground - s * 0.62,
        cx - half * 0.28,
        ground - s * 0.2,
    );
    p.window(big.0, big.1, big.2, big.3, trim, lamps, false);
    indoor_tree(
        p,
        (big.0 + big.2) * 0.5 - s * 0.05,
        big.3,
        s * 0.36,
        seed + 6,
    );
    p.cv.line(
        [(big.0 + big.2) * 0.5, big.1],
        [(big.0 + big.2) * 0.5, big.3],
        s * 0.02,
        trim,
        1.0,
    );
    p.window(
        cx + half * 0.38,
        ground - s * 0.62,
        cx + half * 0.86,
        ground - s * 0.24,
        trim,
        lamps,
        true,
    );
    for (a, b) in [(-0.62, -0.18), (0.18, 0.62)] {
        p.window(
            cx + half * a,
            ground - s * 1.28,
            cx + half * b,
            ground - s * 1.0,
            trim,
            lamps,
            true,
        );
    }
    let round = [cx, ground - s * 1.43];
    p.cv.disk(round[0], round[1], s * 0.085, trim, 1.0);
    p.cv.disk(
        round[0],
        round[1],
        s * 0.065,
        lerp(WARM, LAMP, 0.4),
        0.4 + 0.6 * lamps,
    );
    p.cv.glow(round[0], round[1], s * 0.3, WARM, 0.25 * lamps);
    for (a, b, y) in [
        (-0.88, -0.28, 0.18),
        (0.38, 0.86, 0.22),
        (-0.62, -0.18, 0.98),
        (0.18, 0.62, 0.98),
    ] {
        snow_ledge(
            p,
            cx + half * a - s * 0.02,
            cx + half * b + s * 0.02,
            ground - s * y,
            s * 0.022,
            seed + 7,
        );
    }
    // Small wreaths in the upstairs windows.
    for a in [-0.4, 0.4] {
        wreath(p, cx + half * a, ground - s * 1.14, s * 0.055, seed + 8);
    }

    // The door under a little porch roof, with a wreath and lanterns.
    let door = p.lit(rgb(0x2f4a36));
    let (dx0, dx1) = (cx - half * 0.12 + s * 0.05, cx + half * 0.24 + s * 0.05);
    p.cv.rect(
        dx0 - s * 0.03,
        ground - s * 0.6,
        dx1 + s * 0.03,
        ground,
        trim,
        1.0,
    );
    p.cv.rect(dx0, ground - s * 0.56, dx1, ground, door, 1.0);
    p.cv.rect(
        dx0 + s * 0.03,
        ground - s * 0.5,
        dx1 - s * 0.03,
        ground - s * 0.3,
        lerp(door, rgb(0x1a2a1e), 0.4),
        0.7,
    );
    let dcx = (dx0 + dx1) * 0.5;
    wreath(p, dcx, ground - s * 0.4, s * 0.075, seed + 9);
    p.cv.disk(
        dx1 - s * 0.04,
        ground - s * 0.26,
        s * 0.012,
        p.lit(rgb(0xd8b04a)),
        1.0,
    );
    snowy_roof(
        p,
        [dx0 - s * 0.14, ground - s * 0.6],
        [dcx, ground - s * 0.76],
        [dx1 + s * 0.14, ground - s * 0.6],
        s * 0.45,
        true,
        seed + 10,
    );
    for x in [dx0 - s * 0.08, dx1 + s * 0.08] {
        p.cv.glow(
            x,
            ground - s * 0.46,
            s * 0.5,
            WARM,
            0.4 * (0.3 + 0.7 * lamps),
        );
        p.cv.rect(
            x - s * 0.025,
            ground - s * 0.5,
            x + s * 0.025,
            ground - s * 0.42,
            lerp(rgb(0x6a5a40), LAMP, 0.3 + 0.7 * lamps),
            1.0,
        );
    }
    // Steps, and snow banked against the walls.
    p.cv.rect(
        dx0 - s * 0.06,
        ground - s * 0.04,
        dx1 + s * 0.06,
        ground + s * 0.02,
        stone,
        1.0,
    );
    let lit = p.lit(SNOW);
    let shade = p.lit(rgb(0xb4c2e0));
    let (left_end, right_end) = (cx - s * 1.4, cx + half * 1.45);
    p.cv.region(
        left_end - s * 0.1,
        ground - s * 0.16,
        right_end + s * 0.1,
        ground + s * 0.08,
        |x, y| {
            if x > dx0 - s * 0.08 && x < dx1 + s * 0.08 {
                return None;
            }
            // Heaped against the walls, tapering off at either end and melting
            // into the snow below.
            let ends = smooth(left_end - s * 0.1, left_end + s * 0.15, x)
                * smooth(right_end + s * 0.1, right_end - s * 0.15, x);
            let mound =
                ground - (s * 0.04 + s * 0.07 * noise(x / (s * 0.25), 0.2, seed + 11)) * ends;
            let cover =
                (y - mound + 0.5).clamp(0.0, 1.0) * smooth(ground + s * 0.08, ground - s * 0.01, y);
            let c = lerp(lit, shade, smooth(mound, ground + s * 0.06, y) * 0.6);
            (cover > 0.0).then_some((c, cover))
        },
    );
}

/// The family's tree, seen through the front window.
fn indoor_tree(p: &mut Painter, x: f32, base: f32, height: f32, seed: u32) {
    let green = rgb(0x1f4a2c);
    let top = base - height;
    for k in 0..4 {
        let t0 = k as f32 / 4.0;
        let y0 = top + height * t0 * 0.9;
        let y1 = top + height * (t0 + 0.32).min(1.0);
        let wid = height * (0.12 + t0 * 0.16);
        p.cv.poly(&[[x, y0], [x + wid, y1], [x - wid, y1]], green, 0.9);
    }
    let mut r = seed;
    for i in 0..12 {
        r = r.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let t = 0.15 + 0.8 * ((r >> 8) % 1000) as f32 / 1000.0;
        let across = (((r >> 18) % 1000) as f32 / 1000.0 - 0.5) * 2.0 * height * (0.08 + t * 0.2);
        let c = rgb(BULBS[i % BULBS.len()]);
        p.cv.disk(x + across, top + height * t, height * 0.025, c, 1.0);
    }
    p.cv.disk(x, top, height * 0.04, rgb(0xffe08a), 1.0);
}

/// A tall fir by the house, hung with lights and baubles, a star on top.
fn christmas_tree(p: &mut Painter, x: f32, base: f32, height: f32, seed: u32) {
    let look = Fir {
        width: 0.34,
        tiers: 8.0,
        ..firs_look(p, 0.0, 0.7)
    };
    p.fir(x, base, height, &look, seed);
    let half = height * look.width * 0.5;
    let top = base - height;
    let mut r = seed.wrapping_mul(2_654_435_761);
    let mut next = || {
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        (r % 10_000) as f32 / 10_000.0
    };
    // Lights spiral down the tree.
    let turns = 4.5;
    let n = 70;
    for i in 0..n {
        let t = 0.08 + 0.86 * i as f32 / n as f32;
        let a = t * turns * std::f32::consts::TAU;
        let wid = half * t * 0.92;
        let bx = x + a.sin() * wid;
        let by = top + height * t + a.cos() * height * 0.012 - height * 0.02;
        let front = a.cos() > -0.3;
        if !front {
            continue;
        }
        let c = rgb(BULBS[i % BULBS.len()]);
        p.cv.glow(bx, by, height * 0.035, c, 0.35);
        p.cv.disk(bx, by, height * 0.0075, lerp(c, rgb(0xffffff), 0.4), 1.0);
    }
    // A few baubles catching the light.
    for i in 0..12 {
        let t = 0.25 + 0.65 * next();
        let bx = x + (next() - 0.5) * 2.0 * half * t * 0.75;
        let by = top + height * t;
        let c = p.lit(rgb([0xc8302a, 0xd8a83a, 0x3a6ac8][i % 3]));
        p.cv.disk(bx, by, height * 0.014, c, 1.0);
        p.cv.disk(
            bx - height * 0.004,
            by - height * 0.004,
            height * 0.004,
            rgb(0xffffff),
            0.7,
        );
    }
    // The star.
    let (sx, sy, r) = (x, top - height * 0.02, height * 0.045);
    p.cv.glow(sx, sy, r * 6.0, rgb(0xffd890), 0.45);
    let pts: Vec<[f32; 2]> = (0..10)
        .map(|k| {
            let a = k as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
            let rr = if k % 2 == 0 { r } else { r * 0.45 };
            [sx + a.cos() * rr, sy + a.sin() * rr]
        })
        .collect();
    p.cv.poly(&pts, rgb(0xffe08a), 1.0);
}

/// A trodden path from the door down to the front, lit by low lanterns.
fn path(p: &mut Painter, x: f32, door: f32, front: &dyn Fn(f32) -> f32) {
    let (w, h) = (p.w, p.h);
    let trodden = p.lit(rgb(0xb4c0dc));
    let rut = p.lit(rgb(0x98a6cc));
    let warm = lerp(trodden, WARM, 0.25 * p.lamps());
    let end = (0.7 * w, h * 1.02);
    let at = |t: f32| {
        let x = x + (end.0 - x) * t * t + (t * 3.2).sin() * h * 0.035 * (1.0 - t);
        let y = door + (end.1 - door) * t;
        (x, y)
    };
    let steps = 120;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let (px, py) = at(t);
        let half = h * (0.012 + 0.07 * t * t);
        if py > front(px) - h * 0.01 && py < front(px) + h * 0.02 {
            continue;
        }
        let c = lerp(warm, trodden, t);
        p.cv.ellipse(px, py, half, h * 0.006 + half * 0.08, c, 0.55);
        p.cv.ellipse(
            px - half * 0.35,
            py,
            half * 0.12,
            h * 0.002 + half * 0.03,
            rut,
            0.25,
        );
        p.cv.ellipse(
            px + half * 0.35,
            py,
            half * 0.12,
            h * 0.002 + half * 0.03,
            rut,
            0.25,
        );
    }
    let lamps = p.lamps();
    let post = p.lit(rgb(0x2a2220));
    for (i, t) in [0.22f32, 0.42, 0.62].into_iter().enumerate() {
        for side in [-1.0, 1.0] {
            if i == 2 && side < 0.0 {
                continue;
            }
            let (px, py) = at(t);
            let half = h * (0.012 + 0.07 * t * t);
            let lx = px + side * (half + h * 0.012);
            let s = h * (0.025 + 0.05 * t);
            p.cv.line([lx, py], [lx, py - s], s * 0.07, post, 1.0);
            let on = 0.3 + 0.7 * lamps;
            p.cv.glow(lx, py - s, s * 1.4, WARM, 0.5 * on);
            p.cv.glow_ellipse(lx, py, s * 1.2, s * 0.3, WARM, 0.25 * lamps);
            p.cv.rect(
                lx - s * 0.1,
                py - s * 1.12,
                lx + s * 0.1,
                py - s * 0.88,
                lerp(rgb(0x6a5a40), LAMP, on),
                1.0,
            );
            p.cv.ellipse(lx, py - s * 1.15, s * 0.13, s * 0.05, p.lit(SNOW), 1.0);
        }
    }
}

/// A wooden sled in the snow with a wrapped present on it.
fn sled(p: &mut Painter, x: f32, y: f32, s: f32) {
    let wood = p.lit(rgb(0x8a4a2c));
    let wood_dark = p.lit(rgb(0x5a2e1c));
    let runner = p.lit(rgb(0x2a2a30));
    let snow = p.lit(SNOW);
    // Runners curling up at the front.
    p.cv.line([x - s * 1.2, y], [x + s * 1.0, y], s * 0.08, runner, 1.0);
    for k in 0..8 {
        let a0 = k as f32 / 8.0 * 2.4;
        let a1 = (k + 1) as f32 / 8.0 * 2.4;
        let c = [x + s * 1.0, y - s * 0.2];
        p.cv.line(
            [c[0] + a0.sin() * s * 0.2, c[1] + a0.cos() * s * 0.2],
            [c[0] + a1.sin() * s * 0.2, c[1] + a1.cos() * s * 0.2],
            s * 0.08,
            runner,
            1.0,
        );
    }
    for k in [-0.8, 0.0, 0.7] {
        p.cv.line(
            [x + k * s, y],
            [x + k * s, y - s * 0.32],
            s * 0.08,
            wood_dark,
            1.0,
        );
    }
    for k in 0..3 {
        let yy = y - s * 0.36 - k as f32 * s * 0.001;
        p.cv.rect(
            x - s * 1.2,
            yy - s * 0.07,
            x + s * 0.95,
            yy + s * 0.05,
            if k == 1 { wood } else { wood_dark },
            1.0,
        );
    }
    // The present, then snow on top of everything.
    let paper = p.lit(rgb(0x2f6a48));
    let ribbon = p.lit(rgb(0xe0b44a));
    let (gx, gy, g) = (x - s * 0.2, y - s * 0.42, s * 0.55);
    p.cv.rect(gx - g * 0.5, gy - g, gx + g * 0.5, gy, paper, 1.0);
    p.cv.rect(gx - g * 0.07, gy - g, gx + g * 0.07, gy, ribbon, 1.0);
    p.cv.rect(
        gx - g * 0.5,
        gy - g * 0.57,
        gx + g * 0.5,
        gy - g * 0.43,
        ribbon,
        1.0,
    );
    p.cv.ellipse(gx - g * 0.15, gy - g * 1.05, g * 0.18, g * 0.1, ribbon, 1.0);
    p.cv.ellipse(gx + g * 0.15, gy - g * 1.05, g * 0.18, g * 0.1, ribbon, 1.0);
    p.cv.ellipse(gx, gy - g * 1.02, g * 0.42, g * 0.07, snow, 0.9);
    p.cv.ellipse(x + s * 0.5, y - s * 0.46, s * 0.4, s * 0.06, snow, 0.9);
    // Our lark, perched on the sled's front.
    p.lark_perched(x + s * 0.75, y - s * 0.44, s * 0.55, -1.0);
    p.cv.ellipse(
        x,
        y + s * 0.08,
        s * 1.4,
        s * 0.12,
        p.lit(rgb(0xa8b6d8)),
        0.5,
    );
}

/// An old iron street lamp wound with a garland and a red bow.
fn street_lamp(p: &mut Painter, x: f32, base: f32, height: f32) {
    let iron = p.lit(rgb(0x1e2024));
    let lamps = p.lamps();
    let on = 0.35 + 0.65 * lamps;
    let top = base - height;
    p.cv.glow_ellipse(x, base, height * 0.5, height * 0.08, WARM, 0.3 * lamps);
    p.cv.taper(
        [x, base],
        [x, top + height * 0.1],
        height * 0.022,
        height * 0.012,
        iron,
        1.0,
    );
    p.cv.rect(
        x - height * 0.04,
        base - height * 0.04,
        x + height * 0.04,
        base,
        iron,
        1.0,
    );
    // Garland spiralling up the post.
    let green = p.lit(rgb(0x2c5a36));
    for k in 0..40 {
        let t = k as f32 / 40.0;
        let a = t * 18.0;
        let y = base - height * 0.08 - t * height * 0.72;
        p.cv.disk(x + a.sin() * height * 0.016, y, height * 0.011, green, 0.95);
        if k % 5 == 0 {
            p.cv.disk(
                x + a.sin() * height * 0.016,
                y,
                height * 0.005,
                rgb(0xd8392e),
                1.0,
            );
        }
    }
    let bow = rgb(0xc92f2a);
    let by = top + height * 0.2;
    p.cv.poly(
        &[
            [x, by],
            [x - height * 0.05, by - height * 0.03],
            [x - height * 0.05, by + height * 0.03],
        ],
        bow,
        1.0,
    );
    p.cv.poly(
        &[
            [x, by],
            [x + height * 0.05, by - height * 0.03],
            [x + height * 0.05, by + height * 0.03],
        ],
        bow,
        1.0,
    );
    p.cv.taper(
        [x, by],
        [x - height * 0.03, by + height * 0.08],
        height * 0.012,
        height * 0.004,
        bow,
        1.0,
    );
    p.cv.taper(
        [x, by],
        [x + height * 0.035, by + height * 0.08],
        height * 0.012,
        height * 0.004,
        bow,
        1.0,
    );
    // The lantern.
    let ly = top + height * 0.06;
    p.cv.glow(x, ly, height * 0.45, WARM, 0.5 * on);
    p.cv.poly(
        &[
            [x - height * 0.05, ly - height * 0.06],
            [x + height * 0.05, ly - height * 0.06],
            [x + height * 0.035, ly + height * 0.06],
            [x - height * 0.035, ly + height * 0.06],
        ],
        lerp(rgb(0x6a5a40), LAMP, on),
        1.0,
    );
    p.cv.poly(
        &[
            [x - height * 0.07, ly - height * 0.06],
            [x + height * 0.07, ly - height * 0.06],
            [x, ly - height * 0.12],
        ],
        iron,
        1.0,
    );
    p.cv.ellipse(
        x,
        ly - height * 0.1,
        height * 0.05,
        height * 0.02,
        p.lit(SNOW),
        1.0,
    );
}

/// Little houses and a steeple on the far hills, their windows lit.
fn village(p: &mut Painter, far: &dyn Fn(f32) -> f32) {
    let (w, h) = (p.w, p.h);
    let haze = 0.42;
    let walls = [0x8a4a3a, 0xc8b89a, 0x6a5a6a, 0x9a6a4a];
    let roof = p.air(p.lit(rgb(0x3a3440)), haze);
    let snow = p.air(p.lit(SNOW), haze * 0.6);
    let lamps = p.lamps();
    let lit_window = lerp(rgb(0x5a5a6a), WARM, 0.3 + 0.7 * lamps);
    let mut houses: Vec<(f32, f32)> = Vec::new();
    let count = 16;
    for i in 0..count {
        let t = (i as f32 + p.rng.range(-0.3, 0.3)) / count as f32;
        let x = (0.5 + 0.48 * t) * w;
        let s = h * p.rng.range(0.018, 0.028);
        houses.push((x, s));
    }
    // The church sits among them.
    let church = 0.74 * w;
    houses.sort_by(|a, b| far(a.0).total_cmp(&far(b.0)));
    for (i, &(x, s)) in houses.iter().enumerate() {
        if (x - church).abs() < h * 0.04 {
            continue;
        }
        let base = far(x) + s * 0.4;
        let wall = p.air(p.lit(rgb(walls[i % walls.len()])), haze);
        let half = s * 0.6;
        p.cv.rect(x - half, base - s, x + half, base, wall, 1.0);
        p.cv.poly(
            &[
                [x - half * 1.2, base - s],
                [x + half * 1.2, base - s],
                [x, base - s * 1.7],
            ],
            roof,
            1.0,
        );
        p.cv.poly(
            &[
                [x - half * 1.15, base - s * 1.02],
                [x, base - s * 1.68],
                [x + half * 0.2, base - s * 1.55],
                [x - half * 0.9, base - s * 0.98],
            ],
            snow,
            0.95,
        );
        for k in [-0.5, 0.5] {
            let wx = x + k * half;
            p.cv.glow(wx, base - s * 0.55, s * 0.9, WARM, 0.18 * lamps);
            p.cv.rect(
                wx - s * 0.1,
                base - s * 0.7,
                wx + s * 0.1,
                base - s * 0.42,
                lit_window,
                1.0,
            );
        }
        if noise(x * 0.05, 0.1, p.seed + 40) > 0.6 {
            p.smoke(x + half * 0.5, base - s * 1.6, s * 0.18, 0.3);
        }
    }
    // The church: a nave with a tall steeple and a lit round window.
    let s = h * 0.035;
    let base = far(church) + s * 0.4;
    let stone = p.air(p.lit(rgb(0xd8d0c0)), haze);
    p.cv.rect(
        church - s * 0.9,
        base - s,
        church + s * 0.5,
        base,
        stone,
        1.0,
    );
    p.cv.poly(
        &[
            [church - s * 1.0, base - s],
            [church + s * 0.6, base - s],
            [church - s * 0.2, base - s * 1.55],
        ],
        roof,
        1.0,
    );
    p.cv.rect(
        church + s * 0.3,
        base - s * 2.1,
        church + s * 0.75,
        base,
        stone,
        1.0,
    );
    p.cv.poly(
        &[
            [church + s * 0.25, base - s * 2.1],
            [church + s * 0.8, base - s * 2.1],
            [church + s * 0.525, base - s * 3.3],
        ],
        roof,
        1.0,
    );
    p.cv.line(
        [church + s * 0.525, base - s * 3.3],
        [church + s * 0.525, base - s * 3.6],
        s * 0.05,
        roof,
        1.0,
    );
    p.cv.line(
        [church + s * 0.45, base - s * 3.5],
        [church + s * 0.6, base - s * 3.5],
        s * 0.05,
        roof,
        1.0,
    );
    p.cv.glow(
        church - s * 0.2,
        base - s * 0.55,
        s * 1.6,
        WARM,
        0.3 * lamps,
    );
    p.cv.disk(church - s * 0.2, base - s * 0.6, s * 0.18, lit_window, 1.0);
    p.cv.rect(
        church + s * 0.45,
        base - s * 1.7,
        church + s * 0.6,
        base - s * 1.4,
        lit_window,
        1.0,
    );
}
