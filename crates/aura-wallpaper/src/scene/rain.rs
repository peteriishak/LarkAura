//! Rainy Lane: a narrow street of tall townhouses on a wet evening. Shop
//! windows and lamps glow and shimmer in the puddles, a clock tower fades
//! into the rain mist at the end of the lane, and someone hurries home
//! under a red umbrella. Drawn in true perspective.

use super::canvas::{C, lerp, rgb, scale, smooth};
use super::noise::{fbm, hash, noise};
use super::painter::{Clouds, LAMP, Painter, WARM};

const EYE: f32 = 1.6;
const WALL: f32 = 5.2;
const CURB: f32 = 3.5;

struct Cam {
    vx: f32,
    vy: f32,
    f: f32,
}

impl Cam {
    fn at(&self, x: f32, y: f32, z: f32) -> [f32; 2] {
        [self.vx + x / z * self.f, self.vy + (EYE - y) / z * self.f]
    }
    /// Where a light at (x, y, z) shows in the wet ground.
    fn mirror(&self, x: f32, y: f32, z: f32) -> [f32; 2] {
        [self.vx + x / z * self.f, self.vy + (EYE + y) / z * self.f]
    }
}

struct House {
    side: f32,
    z0: f32,
    z1: f32,
    height: f32,
    color: C,
    gable: bool,
    shop: Option<C>,
    seed: u32,
}

pub fn paint(p: &mut Painter) {
    let (w, h) = (p.w, p.h);
    let seed = p.seed;
    let cam = Cam {
        vx: 0.45 * w,
        vy: 0.5 * h,
        f: 0.95 * h,
    };
    p.sky();
    let cl = p.lit(rgb(0xa8b0ba));
    let cs = p.lit(rgb(0x5e6874));
    p.clouds(&Clouds {
        y0: 0.0,
        y1: 0.5,
        size: 0.2,
        cover: 0.75,
        stretch: 2.0,
        softness: 0.3,
        lit: cl,
        shade: cs,
        alpha: 0.7,
        seed: seed + 1,
    });

    // Fog colour deepens with distance.
    let fog = lerp(p.l.horizon, p.lit(rgb(0x9aa4ae)), 0.3);
    let haze = |z: f32| 1.0 - (-z / 38.0).exp();

    // The clock tower at the end of the lane.
    tower(p, &cam, &fog);
    p.fog(0.5, 0.06, 0.5, fog, seed + 2);

    // Houses on both sides, far to near.
    let palette = [
        rgb(0xa98670),
        rgb(0xc4b49a),
        rgb(0x8796a4),
        rgb(0xa47062),
        rgb(0x98a08a),
        rgb(0xb8a07e),
        rgb(0x7e8a96),
    ];
    let mut houses = Vec::new();
    for side in [-1.0f32, 1.0] {
        let mut z = 2.5;
        let mut i = 0u32;
        while z < 90.0 {
            let len = p.rng.range(5.0, 8.5);
            let shop = (p.rng.next() < 0.55).then(|| {
                p.rng
                    .pick(&[rgb(0x3e5a4a), rgb(0x6a2e2e), rgb(0x2e3e5a), rgb(0x5a4a2e)])
            });
            houses.push(House {
                side,
                z0: z,
                z1: z + len,
                height: p.rng.range(8.0, 14.0),
                color: p.rng.pick(&palette),
                gable: p.rng.next() < 0.4,
                shop,
                seed: seed + i * 13 + if side > 0.0 { 1000 } else { 0 },
            });
            z += len;
            i += 1;
        }
    }
    houses.sort_by(|a, b| b.z0.total_cmp(&a.z0));

    ground(p, &cam, &fog);

    let mut lights: Vec<([f32; 3], f32, C)> = Vec::new();
    for house in &houses {
        draw_house(p, &cam, house, haze(house.z0), &fog, &mut lights);
    }

    // Street lamps on both kerbs.
    let lamps = 0.25 + 0.75 * p.lamps();
    let iron = p.lit(rgb(0x22262c));
    let mut posts: Vec<(f32, f32)> = Vec::new();
    for side in [-1.0f32, 1.0] {
        let mut z = if side < 0.0 { 9.0 } else { 6.5 };
        while z < 80.0 {
            posts.push((side, z));
            z += 11.0;
        }
    }
    posts.sort_by(|a, b| b.1.total_cmp(&a.1));
    for &(side, z) in &posts {
        let x = side * (CURB + 0.1);
        let c = lerp(iron, fog, haze(z));
        let a = cam.at(x, 0.0, z);
        let b = cam.at(x, 4.2, z);
        let width = (0.12 / z * cam.f).max(0.8);
        p.cv.taper(a, b, width * 1.3, width, c, 1.0);
        let arm = cam.at(x - side * 0.7, 4.1, z);
        p.cv.taper(b, arm, width * 0.6, width * 0.5, c, 1.0);
        let head = cam.at(x - side * 0.7, 3.85, z);
        let r = 0.22 / z * cam.f;
        p.cv.poly(
            &[
                [head[0] - r, head[1] - r * 0.6],
                [head[0] + r, head[1] - r * 0.6],
                [head[0] + r * 0.7, head[1] + r * 0.5],
                [head[0] - r * 0.7, head[1] + r * 0.5],
            ],
            lerp(c, LAMP, lamps),
            1.0,
        );
        let fade = 1.0 - haze(z) * 0.7;
        p.cv.glow(head[0], head[1], 2.2 / z * cam.f, WARM, 0.35 * lamps * fade);
        p.cv.glow(head[0], head[1], 0.6 / z * cam.f, LAMP, 0.5 * lamps * fade);
        lights.push(([x - side * 0.7, 3.85, z], 0.9 * lamps * fade, WARM));
    }

    // Reflections of every light in the wet street.
    let vy = cam.vy;
    for &(pos, strength, color) in &lights {
        let m = cam.mirror(pos[0], pos[1], pos[2]);
        if m[1] < vy + 2.0 || m[1] > h + 50.0 {
            continue;
        }
        let r = (0.5 / pos[2] * cam.f).max(1.5);
        p.cv.glow_ellipse(m[0], m[1], r * 1.2, r * 5.0, color, 0.35 * strength);
        p.cv.glow_ellipse(m[0], m[1], r * 0.4, r * 3.0, LAMP, 0.3 * strength);
    }

    // Someone hurrying home under a red umbrella, and a lark sheltering on
    // a lamp arm.
    walker(p, &cam, 3.9, 10.5);
    let (side, z) = (1.0f32, 6.5f32);
    let perch = cam.at(side * (CURB + 0.1) - side * 0.45, 4.12, z);
    p.lark_perched(perch[0], perch[1], 0.32 / z * cam.f, -1.0);

    // A veil of rain mist over everything far away.
    p.fog(0.48, 0.1, 0.35, fog, seed + 3);
}

fn ground(p: &mut Painter, cam: &Cam, fog: &C) {
    let (w, h, u) = (p.w, p.h, p.u.max(0.4));
    let seed = p.seed + 100;
    let stone = p.lit(rgb(0x4a4e56));
    let stone_hi = p.lit(rgb(0x6a6e78));
    let grout = p.lit(rgb(0x24262c));
    let slab = p.lit(rgb(0x707680));
    let kerb = p.lit(rgb(0x8a8e96));
    let fog = *fog;
    let sky_top = p.l.top;
    let horizon = p.l.horizon;
    let vy = cam.vy;
    let f = cam.f;
    // The far, foggy end of the street is mirrored in the wet stones.
    let mirror_row = |y: f32| (2.0 * vy - y).max(0.0);
    let mut out: Vec<(usize, usize, C)> = Vec::new();
    let (y0, y1) = ((vy + 1.0) as usize, p.cv.h);
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        let z = f * EYE / (py - vy);
        let px_per_m = f / z;
        for x in 0..p.cv.w {
            let fx = x as f32 + 0.5;
            let wx = (fx - cam.vx) / px_per_m;
            if wx.abs() > WALL + 0.5 {
                continue;
            }
            let haze = 1.0 - (-z / 38.0).exp();
            let puddle = smooth(0.56, 0.72, fbm(wx * 0.3, z * 0.25, seed, 3));
            let (base, line_w) = if wx.abs() < CURB {
                // Cobbles in staggered rows.
                let row = (z / 0.2).floor();
                let cx = wx / 0.26 + (row * 0.5).rem_euclid(1.0);
                let (fxc, fzc) = (cx.rem_euclid(1.0), (z / 0.2).rem_euclid(1.0));
                let edge = fxc.min(1.0 - fxc).min(fzc.min(1.0 - fzc) * 1.3);
                let px = 0.2 * px_per_m * (EYE / z);
                let g = smooth(0.0, (2.0 / px.max(0.01)).min(0.5), edge);
                let tone = hash(cx.floor() as i32, row as i32, seed);
                (lerp(grout, lerp(stone, stone_hi, tone * 0.7), g), px)
            } else if wx.abs() < CURB + 0.25 {
                (kerb, 1.0)
            } else {
                let row = (z / 1.2).floor();
                let fzc = (z / 1.2).fract();
                let fxc = ((wx.abs() - CURB) / 0.9).fract();
                let edge = fxc.min(1.0 - fxc).min(fzc.min(1.0 - fzc));
                let g = smooth(0.0, 0.06, edge);
                (
                    lerp(
                        grout,
                        scale(slab, 0.9 + hash(row as i32, 0, seed + 1) * 0.2),
                        g,
                    ),
                    1.0,
                )
            };
            let _ = line_w;
            let fres = 0.25 + 0.55 * (1.0 - (py - vy) / (h - vy)).powi(2);
            let wet = (fres + puddle * 0.45).min(0.92);
            let sx = fx
                + (noise(fx / (40.0 * u), py / (3.0 * u), seed + 2) - 0.5)
                    * 6.0
                    * u
                    * (1.0 - puddle * 0.8);
            let reflected = p.cv.sample(sx, mirror_row(py));
            let sky = lerp(horizon, sky_top, 0.3);
            let refl = lerp(sky, reflected, 0.75);
            let mut c = lerp(base, scale(refl, 0.85), wet * (0.5 + 0.35 * puddle));
            c = lerp(c, fog, haze * 0.85);
            out.push((x, y, c));
        }
    }
    for (x, y, c) in out {
        p.cv.px[y * p.cv.w + x] = c;
    }
    let _ = w;
}

fn draw_house(
    p: &mut Painter,
    cam: &Cam,
    house: &House,
    haze: f32,
    fog: &C,
    lights: &mut Vec<([f32; 3], f32, C)>,
) {
    let s = house.side;
    let x = s * WALL;
    let (z0, z1, hgt) = (house.z0, house.z1, house.height);
    let tint = if s < 0.0 { 0.92 } else { 1.0 };
    let wall = lerp(
        scale(p.lit(lerp(house.color, rgb(0x8a9098), 0.35)), tint),
        *fog,
        haze,
    );
    let dark = lerp(scale(wall, 0.72), *fog, haze * 0.5);
    let trim = lerp(p.lit(rgb(0xd8d4cc)), *fog, haze);
    let roof = lerp(p.lit(rgb(0x3a3e48)), *fog, haze);
    let lamps = p.lamps();
    let q = |y: f32, z: f32| cam.at(x, y, z);

    // Pitched roof behind the facade.
    if !house.gable {
        let xr = s * (WALL + 3.0);
        let ridge = |z: f32| cam.at(xr, hgt + 2.6, z);
        p.cv.poly(&[q(hgt, z0), q(hgt, z1), ridge(z1), ridge(z0)], roof, 1.0);
        let cz = z0 + (z1 - z0) * 0.7;
        let ch = [
            cam.at(s * (WALL + 2.0), hgt + 1.0, cz),
            cam.at(s * (WALL + 2.0), hgt + 3.4, cz),
            cam.at(s * (WALL + 2.0), hgt + 3.4, cz + 0.7),
            cam.at(s * (WALL + 2.0), hgt + 1.0, cz + 0.7),
        ];
        p.cv.poly(&ch, dark, 1.0);
    }
    // Facade.
    p.cv.poly(&[q(0.0, z0), q(hgt, z0), q(hgt, z1), q(0.0, z1)], wall, 1.0);
    if house.gable {
        let zm = (z0 + z1) * 0.5;
        let peak = hgt + (z1 - z0) * 0.45;
        p.cv.poly(&[q(hgt, z0), q(peak, zm), q(hgt, z1)], wall, 1.0);
        p.cv.taper(
            q(hgt, z0),
            q(peak, zm),
            (0.25 / z0 * cam.f).max(0.8),
            (0.25 / zm * cam.f).max(0.8),
            trim,
            1.0,
        );
        p.cv.taper(
            q(peak, zm),
            q(hgt, z1),
            (0.25 / zm * cam.f).max(0.8),
            (0.25 / z1 * cam.f).max(0.8),
            trim,
            1.0,
        );
        // A round window in the gable.
        let c = q(hgt + (z1 - z0) * 0.16, zm);
        let r = 0.45 / zm * cam.f;
        let lit = hash(house.seed as i32, 7, 3) < 0.5;
        let glass = if lit {
            lerp(rgb(0x3a3a44), WARM, lamps)
        } else {
            lerp(p.l.horizon, rgb(0x303844), 0.6)
        };
        p.cv.ellipse(c[0], c[1], r * 0.55, r, lerp(glass, *fog, haze), 1.0);
    }
    // Cornice and the line between houses.
    p.cv.poly(
        &[q(hgt - 0.45, z0), q(hgt, z0), q(hgt, z1), q(hgt - 0.45, z1)],
        trim,
        1.0,
    );
    p.cv.poly(
        &[q(3.2, z0), q(3.45, z0), q(3.45, z1), q(3.2, z1)],
        trim,
        0.8,
    );
    p.cv.taper(
        q(0.0, z0),
        q(hgt, z0),
        (0.3 / z0 * cam.f).max(0.8),
        (0.3 / z0 * cam.f).max(0.8),
        dark,
        1.0,
    );

    // Upper windows.
    let floors = ((hgt - 3.6) / 3.0).floor() as i32;
    let n = ((z1 - z0) / 2.1).floor().max(1.0) as i32;
    for fl in 0..floors {
        let y0 = 4.2 + fl as f32 * 3.0;
        for k in 0..n {
            let zc = z0 + (z1 - z0) * (k as f32 + 0.5) / n as f32;
            let (za, zb) = (zc - 0.5, zc + 0.5);
            let lit = hash(house.seed as i32, fl * 17 + k, 5);
            let on = if lit < 0.35 + 0.35 * lamps {
                lamps.max(0.18) * (0.6 + lit)
            } else {
                0.0
            };
            let frame = lerp(scale(wall, 0.55), *fog, haze * 0.5);
            p.cv.poly(
                &[
                    q(y0 - 0.12, za - 0.12),
                    q(y0 + 1.72, za - 0.12),
                    q(y0 + 1.72, zb + 0.12),
                    q(y0 - 0.12, zb + 0.12),
                ],
                frame,
                1.0,
            );
            let glass = if on > 0.0 {
                lerp(rgb(0x4a4040), WARM, on.min(1.0))
            } else {
                lerp(p.l.horizon, rgb(0x2e3540), 0.55)
            };
            p.cv.poly(
                &[q(y0, za), q(y0 + 1.6, za), q(y0 + 1.6, zb), q(y0, zb)],
                lerp(glass, *fog, haze),
                1.0,
            );
            p.cv.poly(
                &[
                    q(y0 - 0.2, za - 0.15),
                    q(y0 - 0.05, za - 0.15),
                    q(y0 - 0.05, zb + 0.15),
                    q(y0 - 0.2, zb + 0.15),
                ],
                trim,
                1.0,
            );
            if fl == 0 && hash(house.seed as i32, k, 21) < 0.45 {
                let leaf = lerp(p.lit(rgb(0x3e6a3a)), *fog, haze);
                let flower = p.rng.pick(&[rgb(0xd84a5a), rgb(0xf0a040), rgb(0xe878a0)]);
                let bloom = lerp(p.lit(flower), *fog, haze);
                p.cv.poly(
                    &[
                        q(y0 - 0.55, za - 0.1),
                        q(y0 - 0.2, za - 0.1),
                        q(y0 - 0.2, zb + 0.1),
                        q(y0 - 0.55, zb + 0.1),
                    ],
                    lerp(scale(wall, 0.5), *fog, haze * 0.5),
                    1.0,
                );
                for j in 0..6 {
                    let zj = za + (zb - za) * (j as f32 + 0.5) / 6.0;
                    let c = q(y0 - 0.1, zj);
                    let r = (0.13 / zj * cam.f).max(0.6);
                    p.cv.disk(c[0], c[1], r * 1.3, leaf, 1.0);
                    p.cv.disk(c[0], c[1] - r * 0.5, r * 0.8, bloom, 1.0);
                }
            }
            if on > 0.2 {
                let c = q(y0 + 0.8, zc);
                p.cv.glow(c[0], c[1], 1.6 / zc * cam.f, WARM, 0.18 * on * (1.0 - haze));
                lights.push(([x, y0 + 0.8, zc], 0.35 * on * (1.0 - haze), WARM));
            }
        }
    }

    // A shop on the ground floor with an awning, or a front door.
    if let Some(awning) = house.shop {
        let (za, zb) = (z0 + 0.6, z1 - 0.6);
        let glow = 0.45 + 0.55 * lamps;
        let inside_top = lerp(rgb(0x6a5848), rgb(0xffd890), glow);
        let inside_bottom = lerp(rgb(0x4a3e34), rgb(0xffb860), glow);
        let frame = lerp(p.lit(awning), *fog, haze);
        p.cv.poly(
            &[
                q(0.4, za - 0.15),
                q(2.85, za - 0.15),
                q(2.85, zb + 0.15),
                q(0.4, zb + 0.15),
            ],
            frame,
            1.0,
        );
        p.cv.poly(
            &[q(0.6, za), q(2.7, za), q(2.7, zb), q(0.6, zb)],
            lerp(inside_top, *fog, haze * 0.6),
            1.0,
        );
        p.cv.poly(
            &[q(0.6, za), q(1.3, za), q(1.3, zb), q(0.6, zb)],
            lerp(inside_bottom, *fog, haze * 0.6),
            1.0,
        );
        // Shelves and goods silhouetted against the light.
        let goods = lerp(scale(p.lit(awning), 0.6), *fog, haze * 0.6);
        for k in 0..((zb - za) / 0.9) as i32 {
            let zc = za + 0.45 + k as f32 * 0.9;
            let hgt = 0.35 + hash(house.seed as i32, k, 9) * 0.5;
            p.cv.poly(
                &[
                    q(1.3, zc - 0.3),
                    q(1.3 + hgt, zc - 0.3),
                    q(1.3 + hgt, zc + 0.3),
                    q(1.3, zc + 0.3),
                ],
                goods,
                0.7,
            );
        }
        p.cv.poly(
            &[q(1.95, za), q(2.02, za), q(2.02, zb), q(1.95, zb)],
            goods,
            0.8,
        );
        // Striped awning leaning out over the pavement.
        let stripes = ((zb - za) / 0.5).round().max(2.0) as i32;
        let cream = lerp(p.lit(rgb(0xe8e0d0)), *fog, haze);
        let colour = lerp(p.lit(lerp(awning, rgb(0xc04040), 0.5)), *fog, haze);
        for i in 0..stripes {
            let (sa, sb) = (
                za + (zb - za) * i as f32 / stripes as f32,
                za + (zb - za) * (i + 1) as f32 / stripes as f32,
            );
            let c = if i % 2 == 0 { cream } else { colour };
            let out = s * (WALL - 1.6);
            p.cv.poly(
                &[
                    q(3.3, sa),
                    q(3.3, sb),
                    cam.at(out, 2.6, sb),
                    cam.at(out, 2.6, sa),
                ],
                c,
                1.0,
            );
            p.cv.poly(
                &[
                    cam.at(out, 2.6, sa),
                    cam.at(out, 2.6, sb),
                    cam.at(out, 2.35, sb),
                    cam.at(out, 2.35, sa),
                ],
                scale(c, 0.85),
                1.0,
            );
        }
        // A hanging sign on a bracket.
        if hash(house.seed as i32, 3, 11) < 0.5 {
            let zs = z0 + 0.3;
            let arm = [q(3.9, zs), cam.at(s * (WALL - 0.9), 3.9, zs)];
            let wid = (0.05 / zs * cam.f).max(0.7);
            p.cv.taper(arm[0], arm[1], wid, wid, dark, 1.0);
            let a = cam.at(s * (WALL - 0.25), 3.85, zs);
            let b = cam.at(s * (WALL - 0.85), 3.0, zs);
            p.cv.rect(
                a[0].min(b[0]),
                a[1],
                a[0].max(b[0]),
                b[1],
                lerp(p.lit(awning), *fog, haze),
                1.0,
            );
            let m = cam.at(s * (WALL - 0.55), 3.42, zs);
            p.cv.disk(
                m[0],
                m[1],
                0.18 / zs * cam.f,
                lerp(p.lit(rgb(0xe0c070)), *fog, haze),
                1.0,
            );
        }
        let zc = (za + zb) * 0.5;
        let c = q(1.6, zc);
        p.cv.glow(
            c[0],
            c[1],
            3.0 / zc * cam.f,
            WARM,
            0.25 * glow * (1.0 - haze),
        );
        lights.push(([x, 1.6, zc], 0.8 * glow * (1.0 - haze), WARM));
    } else {
        let zc = (z0 + z1) * 0.5;
        let door = lerp(p.lit(rgb(0x3a2e2a)), *fog, haze);
        p.cv.poly(
            &[
                q(0.0, zc - 0.55),
                q(2.4, zc - 0.55),
                q(2.4, zc + 0.55),
                q(0.0, zc + 0.55),
            ],
            door,
            1.0,
        );
        let lamp = q(2.7, zc + 0.85);
        let on = 0.3 + 0.7 * lamps;
        p.cv.glow(
            lamp[0],
            lamp[1],
            0.8 / zc * cam.f,
            WARM,
            0.4 * on * (1.0 - haze),
        );
        p.cv.disk(
            lamp[0],
            lamp[1],
            0.08 / zc * cam.f + 0.5,
            lerp(rgb(0x8a8070), LAMP, on),
            1.0,
        );
        for k in [-1.0f32, 1.0] {
            let (za, zb) = (zc + k * 1.4 - 0.45, zc + k * 1.4 + 0.45);
            if za < z0 + 0.2 || zb > z1 - 0.2 {
                continue;
            }
            let glass = lerp(lerp(p.l.horizon, rgb(0x2e3540), 0.55), WARM, lamps * 0.8);
            p.cv.poly(
                &[q(0.9, za), q(2.5, za), q(2.5, zb), q(0.9, zb)],
                lerp(glass, *fog, haze),
                1.0,
            );
        }
    }
}

fn tower(p: &mut Painter, cam: &Cam, fog: &C) {
    let z = 120.0;
    let haze = 0.7;
    let stone = lerp(p.lit(rgb(0x6a6e78)), *fog, haze);
    let x0 = 1.2;
    let (wdt, hgt) = (6.0, 40.0);
    let a = cam.at(x0, 0.0, z);
    let b = cam.at(x0 + wdt, hgt, z);
    p.cv.rect(a[0], b[1], b[0], a[1], stone, 1.0);
    let apex = cam.at(x0 + wdt * 0.5, hgt + 12.0, z);
    p.cv.poly(&[[a[0] - 1.0, b[1]], [b[0] + 1.0, b[1]], apex], stone, 1.0);
    let c = cam.at(x0 + wdt * 0.5, hgt - 6.0, z);
    let r = 2.0 / z * cam.f;
    let lamps = p.lamps();
    let face = lerp(lerp(p.lit(rgb(0xe8e2d4)), *fog, 0.5), LAMP, lamps * 0.8);
    p.cv.disk(c[0], c[1], r, face, 1.0);
    p.cv.line(c, [c[0], c[1] - r * 0.7], r * 0.12, stone, 1.0);
    p.cv.line(c, [c[0] + r * 0.5, c[1]], r * 0.12, stone, 1.0);
    if lamps > 0.0 {
        p.cv.glow(c[0], c[1], r * 4.0, WARM, 0.25 * lamps);
    }
}

fn walker(p: &mut Painter, cam: &Cam, x: f32, z: f32) {
    let coat = p.lit(rgb(0x2a3442));
    let legs = p.lit(rgb(0x1c2028));
    let red = p.lit(rgb(0xc8323a));
    let red_dark = p.lit(rgb(0x8a1e28));
    let foot = cam.at(x, 0.0, z);
    let k = cam.f / z;
    let (fx, fy) = (foot[0], foot[1]);
    p.cv.taper(
        [fx - 0.1 * k, fy],
        [fx - 0.06 * k, fy - 0.8 * k],
        0.11 * k,
        0.12 * k,
        legs,
        1.0,
    );
    p.cv.taper(
        [fx + 0.12 * k, fy - 0.02 * k],
        [fx + 0.06 * k, fy - 0.8 * k],
        0.11 * k,
        0.12 * k,
        legs,
        1.0,
    );
    p.cv.poly(
        &[
            [fx - 0.26 * k, fy - 0.7 * k],
            [fx + 0.26 * k, fy - 0.7 * k],
            [fx + 0.18 * k, fy - 1.45 * k],
            [fx - 0.18 * k, fy - 1.45 * k],
        ],
        coat,
        1.0,
    );
    p.cv.disk(fx, fy - 1.55 * k, 0.12 * k, p.lit(rgb(0xd8b8a0)), 1.0);
    p.cv.line(
        [fx + 0.14 * k, fy - 1.3 * k],
        [fx + 0.05 * k, fy - 2.0 * k],
        0.03 * k,
        legs,
        1.0,
    );
    // Umbrella dome with ribs.
    let (ux, uy, ur) = (fx + 0.05 * k, fy - 1.95 * k, 0.6 * k);
    p.cv.region(
        ux - ur - 2.0,
        uy - ur * 0.6,
        ux + ur + 2.0,
        uy + 2.0,
        |px, py| {
            let dx = (px - ux) / ur;
            let dy = (uy - py) / (ur * 0.55);
            let d = dx * dx + dy * dy;
            let scallop = ((dx * 4.0 * std::f32::consts::PI).cos() * 0.5 + 0.5) * 0.06;
            let inside = (1.0 - d) * ur * 0.5 + 0.5;
            let above = (uy - py + scallop * ur) + 0.5;
            let cover = inside.clamp(0.0, 1.0) * above.clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            let panel = ((dx + 1.0) * 2.5).fract();
            let c = lerp(red_dark, red, smooth(0.0, 0.5, panel) * (1.0 - dy * 0.3));
            Some((c, cover))
        },
    );
    p.cv.disk(ux, uy - ur * 0.56, 0.03 * k, red_dark, 1.0);
    // Faint reflection on the wet pavement.
    p.cv.glow_ellipse(ux, fy + 1.9 * k, ur * 0.9, ur * 0.6, red, 0.12);
    let _ = (fbm, noise);
}
