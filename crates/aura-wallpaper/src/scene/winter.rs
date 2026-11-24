//! Winter Hollow: a snowy forest hollow under one great peak. A frozen
//! creek winds up to a small timber cabin with warm windows, and heavy
//! firs frame the view. Falling snow reads well against the deep sky.

use super::canvas::{C, lerp, ramp3, rgb, smooth};
use super::noise::{fbm, fbm1, noise, ridged};
use super::painter::{Fir, LAMP, Painter, SNOW, WARM, slope_at, top_at};

pub fn paint(p: &mut Painter) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    p.sky();
    p.stars(1400, 0.6, 1.0);
    p.body(1.0);
    let cloud_lit = p.air(p.lit(rgb(0xffffff)), 0.2);
    let cloud_shade = p.air(p.lit(rgb(0x9aa8c8)), 0.35);
    p.cumulus(0.1, 0.3, 4, 0.08, cloud_lit, cloud_shade, 0.8, p.seed + 1);

    // The far range and the great peak. Shapes are measured in picture
    // heights so they keep their proportions on any screen.
    let seed = p.seed;
    let tent = move |x: f32, at: f32, half: f32, pow: f32| {
        (1.0 - ((x - at * w) / (half * h)).abs()).max(0.0).powf(pow)
    };
    let far = move |x: f32| {
        let env = tent(x, 0.12, 0.45, 1.4).max(tent(x, 0.9, 0.4, 1.4) * 0.8);
        h * (0.545
            - 0.11 * env
            - 0.035 * ridged(x / h * 5.0, 0.2, seed + 3, 4)
            - 0.02 * fbm1(x / h * 2.0, seed + 2, 3))
    };
    p.mountains(&far, 0.61, rgb(0x8193bb), SNOW, 0.55, 1.6, seed + 4);
    let peak = move |x: f32| {
        let env = tent(x, 0.47, 0.42, 1.25)
            .max(tent(x, 0.62, 0.22, 1.3) * 0.55)
            .max(tent(x, 0.33, 0.2, 1.3) * 0.4);
        let crag = ridged(x / h * 6.0, 0.4, seed + 5, 5);
        h * (0.6
            - 0.3 * env
            - 0.055 * crag * (0.25 + env)
            - 0.012 * fbm1(x / h * 30.0, seed + 6, 2))
    };
    p.mountains(&peak, 0.62, rgb(0x6a7ba6), SNOW, 0.16, 1.5, seed + 6);
    let mist = p.air(p.lit(rgb(0xeef3fa)), 0.45);
    p.fog(0.58, 0.022, 0.6, mist, seed + 7);

    // Forested slopes stepping down into the hollow.
    let snow_lit = p.lit(SNOW);
    let snow_shade = p.lit(rgb(0xa6b9da));
    let pine = |p: &Painter, haze: f32, snow: f32| Fir {
        dark: p.air(p.lit(rgb(0x10271f)), haze),
        mid: p.air(p.lit(rgb(0x1f4134)), haze),
        light: p.air(p.lit(rgb(0x3d6a50)), haze),
        trunk: p.air(p.lit(rgb(0x3a2a22)), haze),
        snow,
        snow_lit: p.air(snow_lit, haze * 0.7),
        snow_shade: p.air(snow_shade, haze * 0.7),
        width: 0.3,
        tiers: 6.0,
    };
    let slope1 = p.skyline(0.612, 0.03, 2.0, seed + 8, 0.0);
    snowfield(p, &slope1, 0.69, 0.4, seed + 9);
    let f1 = pine(p, 0.45, 0.75);
    p.firs(&slope1, -0.02, 1.02, 260, (0.022, 0.045), &f1, 0.004);
    p.fog(0.625, 0.016, 0.45, mist, seed + 10);

    let rise = move |x: f32| {
        let bump = tent(x, 0.7, 0.42, 1.6);
        h * (0.675 - 0.045 * bump + 0.014 * (fbm1(x / h * 4.0, seed + 11, 3) - 0.5))
    };
    snowfield(p, &rise, 0.89, 0.0, seed + 12);
    let f2 = pine(p, 0.15, 0.85);
    let shadow = p.lit(rgb(0x8ea4cc));
    let mut groves = p.plan_firs(&rise, 0.52, 0.63, 8, (0.07, 0.13));
    groves.extend(p.plan_firs(&rise, 0.77, 0.94, 12, (0.08, 0.16)));
    groves.extend(p.plan_firs(&rise, 0.05, 0.3, 14, (0.05, 0.11)));
    groves.sort_by(|a, b| a.1.total_cmp(&b.1));
    for &(x, b, hgt, _) in &groves {
        p.cast_shadow(x, b, hgt * 1.4, hgt * 0.05, shadow, 0.45);
    }
    for &(x, b, hgt, s) in &groves {
        p.fir(x, b, hgt, &f2, s);
    }

    // The cabin on its rise.
    let cx = 0.69 * w;
    p.cast_shadow(cx, rise(cx) + 0.01 * h, 0.2 * h, 0.012 * h, shadow, 0.4);
    cabin(p, cx, rise(cx) + 0.012 * h, 0.08 * h);

    // The frozen creek and its footbridge.
    creek(p, &rise);
    bridge(p);

    // Foreground drifts.
    let front = move |x: f32| {
        let lift = tent(x, 0.0, 0.5, 1.5) * 0.08 + tent(x, 1.0, 0.45, 1.5) * 0.07;
        h * (0.87 + 0.03 * (fbm1(x / h * 2.2, seed + 13, 3) - 0.5) - lift)
    };
    snowfield(p, &front, 1.0, 0.0, seed + 14);
    creek_mouth(p, &front);
    footprints(p, &rise, &front);
    fence(p, &front);

    // Heavy firs frame the view.
    let f3 = Fir {
        width: 0.24,
        tiers: 11.0,
        ..pine(p, 0.0, 0.95)
    };
    p.cast_shadow(
        0.03 * w,
        front(0.03 * w) + 0.03 * h,
        0.5 * h,
        0.03 * h,
        shadow,
        0.5,
    );
    p.fir(-0.05 * w, front(0.0) + 0.05 * h, 1.08 * h, &f3, seed + 16);
    p.fir(
        0.045 * w,
        front(0.045 * w) + 0.03 * h,
        0.82 * h,
        &f3,
        seed + 15,
    );
    p.fir(
        0.955 * w,
        front(0.955 * w) + 0.04 * h,
        0.7 * h,
        &f3,
        seed + 17,
    );
    p.fir(1.04 * w, front(w) + 0.06 * h, 1.05 * h, &f3, seed + 18);

    // Sparkles where light catches the snow.
    let spark = lerp(rgb(0xffffff), p.l.rim, 0.3);
    for _ in 0..p.many(700) {
        let x = p.rng.next() * w;
        let y = front(x) + p.rng.range(0.0, 0.14) * h;
        let a = p.rng.range(0.2, 0.8) * (1.0 - p.dark() * 0.5);
        p.cv.disk(x, y, 1.1 * u, spark, a);
    }
}

/// Snow lying in soft drifts: hollows hold blue shadow and crests catch
/// the light. `haze` fades distant fields.
pub(super) fn snowfield(
    p: &mut Painter,
    top: &dyn Fn(f32) -> f32,
    bottom: f32,
    haze: f32,
    seed: u32,
) {
    let tops = p.column_tops(top);
    let top = |x: f32| top_at(&tops, x);
    let h = p.h;
    let u = p.u.max(0.4);
    let lit = p.air(p.lit(SNOW), haze);
    let warm = p.air(lerp(p.lit(SNOW), p.rim(), 0.35 * (1.0 - p.dark())), haze);
    let shade = p.air(p.lit(rgb(0xa2b5d8)), haze);
    let deep = p.air(p.lit(rgb(0x8799c4)), haze);
    let side = p.sun_side();
    p.cv.layer_to(
        top,
        bottom * p.h,
        |x, y, d| {
            let drift = fbm(x / (h * 0.5), y / (h * 0.025), seed, 4);
            let ripple = fbm(x / (h * 0.12), y / (h * 0.008), seed + 1, 3);
            let li = smooth(0.25, 0.75, drift * 0.75 + ripple * 0.3);
            let near = (d / (h * 0.25)).clamp(0.0, 1.0);
            let mut c = ramp3(deep, shade, lit, 0.45 + li * 0.5 - near * 0.15);
            c = lerp(c, warm, smooth(0.6, 0.78, drift) * 0.5);
            if d < 7.0 * u {
                let slope = slope_at(&tops, x, 3.0 * u);
                let facing = (0.5 - slope * side * 2.0).clamp(0.0, 1.0);
                c = lerp(c, warm, (1.0 - d / (7.0 * u)).powi(2) * facing);
            }
            c
        },
        1.0,
    );
}

fn bridge(p: &mut Painter) {
    let (w, h) = (p.w, p.h);
    let y = 0.8 * h;
    let Some((cx, hw, _)) = creek_at(w, h, y) else {
        return;
    };
    let span = hw * 1.5;
    let wood = p.lit(rgb(0x6a4a32));
    let dark = p.lit(rgb(0x3e2a1e));
    let snow = p.lit(SNOW);
    let s = h * 0.016;
    let arc = |t: f32| y - (t * std::f32::consts::PI).sin() * s * 1.4;
    let steps = 16;
    for i in 0..steps {
        let (t0, t1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
        let (x0, x1) = (cx - span + 2.0 * span * t0, cx - span + 2.0 * span * t1);
        p.cv.line(
            [x0, arc(t0) + s * 0.2],
            [x1, arc(t1) + s * 0.2],
            s * 1.1,
            dark,
            1.0,
        );
        p.cv.line([x0, arc(t0)], [x1, arc(t1)], s * 0.7, wood, 1.0);
        p.cv.line(
            [x0, arc(t0) - s * 0.45],
            [x1, arc(t1) - s * 0.45],
            s * 0.5,
            snow,
            1.0,
        );
        p.cv.line(
            [x0, arc(t0) - s * 2.2],
            [x1, arc(t1) - s * 2.2],
            s * 0.35,
            dark,
            1.0,
        );
    }
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let x = cx - span + 2.0 * span * t;
        p.cv.line([x, arc(t)], [x, arc(t) - s * 2.4], s * 0.35, dark, 1.0);
        p.cv.disk(x, arc(t) - s * 2.5, s * 0.3, snow, 1.0);
    }
}

fn cabin(p: &mut Painter, cx: f32, ground: f32, s: f32) {
    let u = p.u.max(0.4);
    let lamps = p.lamps();
    let log_dark = p.lit(rgb(0x4e3022));
    let log_light = p.lit(rgb(0x9a6a48));
    let half = s * 0.95;
    let side = p.sun_side();
    let seed = p.seed + 200;

    // Warm light spilling on the snow.
    if lamps > 0.0 {
        p.cv.glow_ellipse(cx, ground + s * 0.25, s * 3.2, s * 0.9, WARM, 0.32 * lamps);
    }
    // Side wall in perspective, then the gable front.
    p.cv.region(cx - half, ground - s, cx + half, ground, |x, y| {
        let row = ((ground - y) / (s / 7.0)).fract();
        let round = 1.0 - (row * 2.0 - 1.0).powi(2);
        let n = fbm(x / (s * 0.15), y / (s * 0.05), seed, 2);
        let li = (round * 0.75 + (n - 0.5) * 0.4 + (x - cx) / half * side * 0.1).clamp(0.0, 1.0);
        Some((lerp(log_dark, log_light, li), 1.0))
    });
    // Log ends at the corners.
    for i in 0..7 {
        let y = ground - s * (i as f32 + 0.5) / 7.0;
        for x in [cx - half, cx + half] {
            p.cv.disk(x, y, s * 0.07, log_light, 1.0);
            p.cv.disk(x, y, s * 0.035, log_dark, 0.6);
        }
    }
    // Chimney and smoke.
    let stone = p.lit(rgb(0x6c6a70));
    let (chx, chy) = (cx + half * 0.55, ground - s * 1.75);
    p.cv.rect(
        chx - s * 0.11,
        chy,
        chx + s * 0.11,
        ground - s * 1.1,
        stone,
        1.0,
    );
    p.smoke(chx, chy - s * 0.08, s * 0.35, 0.6 * -side);
    // Roof with a deep, lumpy blanket of snow.
    let apex = [cx, ground - s * 1.95];
    let eave = s * 0.35;
    let (left, right) = (
        [cx - half - eave, ground - s * 0.9],
        [cx + half + eave, ground - s * 0.9],
    );
    let roof = p.lit(rgb(0x3a2a24));
    p.cv.poly(&[apex, right, left], roof, 1.0);
    let snow_lit = p.lit(SNOW);
    let snow_shade = p.lit(rgb(0xb2c3df));
    let (ax, ay) = (apex[0], apex[1] - s * 0.16);
    p.cv.region(
        left[0] - s * 0.1,
        ay - 2.0,
        right[0] + s * 0.1,
        right[1] + s * 0.2,
        |x, y| {
            let slope_y = ay + (x - ax).abs() / (right[0] - ax) * (right[1] - ay + s * 0.04);
            let lump = (noise(x / (s * 0.12), 0.5, seed + 5) - 0.5) * s * 0.07;
            let bottom = slope_y + s * 0.2 + lump;
            let edge_l = (x - (left[0] - s * 0.06)).min((right[0] + s * 0.06) - x);
            let cover = ((y - (slope_y - s * 0.02)) + 0.5).clamp(0.0, 1.0)
                * ((bottom - y) + 0.5).clamp(0.0, 1.0)
                * (edge_l + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            let lit_side = if (x - ax) * side > 0.0 { 1.0 } else { 0.55 };
            let under = smooth(bottom - s * 0.08, bottom, y);
            Some((
                lerp(
                    lerp(snow_shade, snow_lit, lit_side),
                    snow_shade,
                    under * 0.7,
                ),
                cover,
            ))
        },
    );
    // Icicles.
    let ice = lerp(snow_lit, p.l.horizon, 0.3);
    let mut x = left[0] + s * 0.15;
    let mut k = 0u32;
    while x < right[0] - s * 0.1 {
        let slope_y = ay + (x - ax).abs() / (right[0] - ax) * (right[1] - ay + s * 0.04) + s * 0.19;
        let len = s * (0.06 + 0.1 * noise(k as f32 * 1.7, 0.2, seed + 9));
        p.cv.taper(
            [x, slope_y],
            [x, slope_y + len],
            s * 0.025,
            0.4 * u,
            ice,
            0.85,
        );
        x += s * 0.11;
        k += 1;
    }
    // Windows, door and porch lamp.
    let frame = p.lit(rgb(0x3a2418));
    p.window(
        cx - half * 0.62,
        ground - s * 0.66,
        cx - half * 0.22,
        ground - s * 0.3,
        frame,
        lamps,
        true,
    );
    p.window(
        cx + half * 0.3,
        ground - s * 0.66,
        cx + half * 0.7,
        ground - s * 0.3,
        frame,
        lamps,
        true,
    );
    p.window(
        cx - s * 0.12,
        ground - s * 1.45,
        cx + s * 0.12,
        ground - s * 1.15,
        frame,
        lamps,
        true,
    );
    let door = p.lit(rgb(0x4a2c1c));
    p.cv.rect(
        cx - s * 0.14,
        ground - s * 0.62,
        cx + s * 0.14,
        ground,
        door,
        1.0,
    );
    p.cv.rect(
        cx - s * 0.1,
        ground - s * 0.56,
        cx + s * 0.1,
        ground - s * 0.04,
        lerp(door, log_dark, 0.5),
        0.6,
    );
    let lamp_on = 0.25 + 0.75 * lamps;
    p.cv.glow(
        cx + s * 0.24,
        ground - s * 0.7,
        s * 0.9,
        WARM,
        0.45 * lamp_on,
    );
    p.cv.disk(
        cx + s * 0.24,
        ground - s * 0.7,
        s * 0.035,
        lerp(rgb(0x8a7a5a), LAMP, lamp_on),
        1.0,
    );
    // Wreath on the door.
    let green = p.lit(rgb(0x2f5a3a));
    p.cv.region(
        cx - s * 0.1,
        ground - s * 0.52,
        cx + s * 0.1,
        ground - s * 0.32,
        |x, y| {
            let d = ((x - cx).powi(2) + (y - (ground - s * 0.42)).powi(2)).sqrt();
            let ring = (s * 0.025 - (d - s * 0.06).abs() + 0.5).clamp(0.0, 1.0);
            (ring > 0.0).then_some((green, ring))
        },
    );
    p.cv.disk(cx, ground - s * 0.36, s * 0.018, p.lit(rgb(0xc8382e)), 1.0);
    // Firewood stacked by the side wall, and snow banked at the base.
    let wood = p.lit(rgb(0x8a5a38));
    for row in 0..3 {
        for i in 0..4 {
            let x = cx - half - s * 0.12 - i as f32 * s * 0.1 - row as f32 * s * 0.05;
            let y = ground - s * 0.06 - row as f32 * s * 0.1;
            p.cv.disk(x, y, s * 0.05, wood, 1.0);
            p.cv.disk(x, y, s * 0.025, log_light, 0.8);
        }
    }
    p.cv.region(
        cx - half * 1.6,
        ground - s * 0.15,
        cx + half * 1.9,
        ground + s * 0.12,
        |x, y| {
            let mound = ground - s * 0.06 - s * 0.06 * noise(x / (s * 0.3), 0.3, seed + 11);
            let cover = (y - mound + 0.5).clamp(0.0, 1.0);
            (cover > 0.0).then_some((lerp(snow_lit, snow_shade, 0.25), cover))
        },
    );
}

/// Centre, half width and how far away the creek is, at a screen row.
fn creek_at(w: f32, h: f32, y: f32) -> Option<(f32, f32, f32)> {
    let (near, far) = (1.02 * h, 0.668 * h);
    if y < far || y > near + 2.0 {
        return None;
    }
    let v = ((near - y) / (near - far)).clamp(0.0, 1.0);
    let cx = w * 0.5 + h * (-0.2 + 0.2 * v + 0.12 * (v * 6.5 + 0.9).sin() * (1.0 - v * 0.75));
    let hw = h * (0.1 * (1.0 - v).powf(1.8) + 0.004);
    Some((cx, hw, v))
}

fn creek(p: &mut Painter, rise: &dyn Fn(f32) -> f32) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let ice_far = p.air(p.lit(rgb(0x9fb8d0)), 0.2);
    let ice_near = p.lit(rgb(0x5f8199));
    let shine = lerp(p.l.horizon, rgb(0xffffff), 0.3);
    let bank_snow = p.lit(SNOW);
    let seed = p.seed + 300;
    let (bx, _) = p.body_pos();
    let tops = p.column_tops(rise);
    for row in (0.66 * h) as usize..p.cv.h {
        let yc = row as f32 + 0.5;
        let Some((cx0, hw0, _)) = creek_at(w, h, yc) else {
            continue;
        };
        let reach = hw0 * 1.2 + 2.0;
        p.cv.region(
            cx0 - reach,
            row as f32,
            cx0 + reach,
            row as f32 + 0.5,
            |x, y| {
                if y < top_at(&tops, x) - 1.0 {
                    return None;
                }
                let (cx, hw, v) = creek_at(w, h, y)?;
                let n = noise(y / (3.0 * u), x / (40.0 * u), seed);
                let edge = hw * (1.0 + (n - 0.5) * 0.25) - (x - cx).abs();
                let cover = (edge + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    return None;
                }
                let across = (x - cx) / hw;
                let streak = fbm(x / (h * 0.08), y / (h * 0.004), seed + 1, 3);
                let mut c = lerp(ice_near, ice_far, v.powf(0.6));
                c = lerp(c, shine, smooth(0.55, 0.8, streak) * 0.45);
                let path = (-((x - bx) / (h * 0.03)).powi(2)).exp() * 0.5;
                c = lerp(c, p_shine(shine), path * (1.0 - v * 0.5));
                let crust = smooth(0.8, 0.98, across.abs() + (n - 0.5) * 0.25);
                c = lerp(c, bank_snow, crust);
                Some((c, cover))
            },
        );
    }
    // Snow-capped stones along the banks.
    let stone = p.lit(rgb(0x4a4c58));
    for i in 0..p.many(60) {
        let y = p.rng.range(0.7, 1.0) * h;
        let Some((cx, hw, v)) = creek_at(w, h, y) else {
            continue;
        };
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        let x = cx + side * hw * p.rng.range(0.85, 1.1);
        let r = h * 0.012 * (1.0 - v).powf(1.3) * p.rng.range(0.6, 1.3) + 0.8 * u;
        p.cv.ellipse(x, y, r * 1.3, r * 0.8, stone, 1.0);
        p.cv.ellipse(x, y - r * 0.35, r * 1.2, r * 0.5, bank_snow, 1.0);
    }
}

fn p_shine(c: C) -> C {
    lerp(c, rgb(0xffffff), 0.4)
}

/// The creek runs on under the foreground drifts as an open channel.
fn creek_mouth(p: &mut Painter, front: &dyn Fn(f32) -> f32) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let water = p.lit(rgb(0x3d5f78));
    let water_hi = lerp(p.lit(rgb(0x8fb0c8)), p.l.horizon, 0.2);
    let snow_lit = p.lit(SNOW);
    let shade = p.lit(rgb(0xa9bcd9));
    let seed = p.seed + 320;
    let tops = p.column_tops(front);
    for row in (0.78 * h) as usize..p.cv.h {
        let yc = row as f32 + 0.5;
        let Some((cx0, hw0, _)) = creek_at(w, h, yc) else {
            continue;
        };
        let reach = hw0 + 2.0;
        p.cv.region(
            cx0 - reach,
            row as f32,
            cx0 + reach,
            row as f32 + 0.5,
            |x, y| {
                let top = top_at(&tops, x);
                if y < top - 1.0 {
                    return None;
                }
                let (cx, hw, _) = creek_at(w, h, y)?;
                let bank = hw * 0.85;
                let edge = bank - (x - cx).abs();
                let cover = (edge + 0.5).clamp(0.0, 1.0) * (y - top + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    return None;
                }
                let n = fbm(x / (h * 0.05), y / (h * 0.006), seed, 3);
                let mut c = lerp(water, water_hi, smooth(0.45, 0.75, n));
                let lip = smooth(bank * 0.75, bank, (x - cx).abs());
                c = lerp(c, shade, lip * 0.8);
                let crest = (1.0 - ((y - top) / (6.0 * u)).clamp(0.0, 1.0)) * 0.9;
                c = lerp(c, snow_lit, crest);
                Some((c, cover))
            },
        );
    }
}

fn footprints(p: &mut Painter, rise: &dyn Fn(f32) -> f32, front: &dyn Fn(f32) -> f32) {
    let (w, h) = (p.w, p.h);
    let dent = p.lit(rgb(0x9fb2d4));
    let start = (0.84 * w, 1.0 * h);
    let end = (0.69 * w, rise(0.69 * w) + 0.012 * h);
    let steps = 26;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let tt = t.powf(0.7);
        let x = start.0 + (end.0 - start.0) * tt + (t * 9.0).sin() * h * 0.012 * (1.0 - t);
        let y = start.1 + (end.1 - start.1) * tt;
        if y < front(x) && y > rise(x) + h * 0.02 && t < 0.5 {
            continue;
        }
        let s = h * 0.009 * (1.0 - t * 0.85) + 0.6;
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        p.cv.ellipse(x + side * s * 1.4, y, s * 0.9, s * 0.45, dent, 0.75);
    }
    // Lanterns on posts along the path.
    let lamps = p.lamps();
    let post = p.lit(rgb(0x2e2420));
    for (t, hgt) in [(0.25f32, 0.07f32), (0.62, 0.035)] {
        let tt = t.powf(0.7);
        let x = start.0 + (end.0 - start.0) * tt + h * 0.035;
        let y = start.1 + (end.1 - start.1) * tt;
        let s = hgt * h;
        p.cv.line([x, y], [x, y - s], s * 0.06, post, 1.0);
        let on = 0.2 + 0.8 * lamps;
        p.cv.glow(x, y - s, s * 0.9, WARM, 0.5 * on);
        p.cv.glow_ellipse(x, y, s * 0.9, s * 0.25, WARM, 0.25 * lamps);
        p.cv.rect(
            x - s * 0.07,
            y - s * 1.08,
            x + s * 0.07,
            y - s * 0.92,
            lerp(rgb(0x6a5a40), LAMP, on),
            1.0,
        );
        p.cv.ellipse(x, y - s * 1.1, s * 0.1, s * 0.04, p.lit(SNOW), 1.0);
    }
}

fn fence(p: &mut Painter, front: &dyn Fn(f32) -> f32) {
    let (w, h) = (p.w, p.h);
    let wood = p.lit(rgb(0x5e4430));
    let wood_dark = p.lit(rgb(0x3e2c20));
    let snow = p.lit(SNOW);
    let s = 0.07 * h;
    let xs = [0.13, 0.2, 0.27, 0.34];
    let posts: Vec<[f32; 2]> = xs
        .iter()
        .map(|&u| [u * w, front(u * w) + 0.035 * h])
        .collect();
    for pair in posts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        for k in [0.4, 0.75] {
            p.cv.line(
                [a[0], a[1] - s * k],
                [b[0], b[1] - s * k],
                s * 0.08,
                wood,
                1.0,
            );
            p.cv.line(
                [a[0], a[1] - s * k - s * 0.05],
                [b[0], b[1] - s * k - s * 0.05],
                s * 0.045,
                snow,
                0.95,
            );
        }
    }
    for (i, q) in posts.iter().enumerate() {
        let tilt = (i as f32 - 1.5) * s * 0.03;
        p.cv.poly(
            &[
                [q[0] - s * 0.05, q[1]],
                [q[0] + s * 0.05, q[1]],
                [q[0] + s * 0.05 + tilt, q[1] - s],
                [q[0] - s * 0.05 + tilt, q[1] - s],
            ],
            wood_dark,
            1.0,
        );
        p.cv.ellipse(
            q[0] + tilt,
            q[1] - s - s * 0.02,
            s * 0.08,
            s * 0.045,
            snow,
            1.0,
        );
    }
    // A lark on the last post, our little signature.
    let q = posts[2];
    p.lark_perched(
        q[0] + (2.0 - 1.5) * s * 0.03,
        q[1] - s - s * 0.05,
        s * 0.32,
        1.0,
    );
}
