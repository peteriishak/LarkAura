//! Shared painting tools: sky, light, clouds, land, trees, water and fog.
//!
//! Positions are shares of the width and height, and sizes are shares of
//! the height, so any size gives the same picture. `u` is one pixel of a
//! 1440 pixel tall picture, for fine details.

use super::canvas::{C, Canvas, lerp, mul, ramp3, rgb, scale, smooth};
use super::noise::{fbm, fbm1, hash, noise, ridged};
use super::{Body, Light};

/// The edge height at `x` from precomputed column tops.
#[inline]
pub fn top_at(tops: &[f32], x: f32) -> f32 {
    tops[(x.max(0.0) as usize).min(tops.len() - 1)]
}

/// How steeply an edge climbs around `x`, from precomputed column tops.
#[inline]
pub fn slope_at(tops: &[f32], x: f32, e: f32) -> f32 {
    let last = tops.len() - 1;
    let a = ((x - e).max(0.0) as usize).min(last);
    let b = ((x + e).max(0.0) as usize).min(last);
    (tops[b] - tops[a]) / ((b as f32 - a as f32).max(1.0))
}

pub struct Rng(u32);

impl Rng {
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[((self.next() * items.len() as f32) as usize).min(items.len() - 1)]
    }
}

pub const WARM: C = rgb(0xffc56a);
pub const LAMP: C = rgb(0xffe2a8);
pub const SNOW: C = rgb(0xf5f8fd);

pub struct Painter {
    pub cv: Canvas,
    pub l: Light,
    pub w: f32,
    pub h: f32,
    pub u: f32,
    pub rng: Rng,
    pub seed: u32,
}

/// How a band of painted clouds looks.
pub struct Clouds {
    /// Top and bottom of the band, as shares of the height.
    pub y0: f32,
    pub y1: f32,
    /// Size of a cloud, as a share of the height.
    pub size: f32,
    /// How much of the band is cloud, 0 to 1.
    pub cover: f32,
    /// Horizontal stretch: above 1 gives long, flat clouds.
    pub stretch: f32,
    pub softness: f32,
    pub lit: C,
    pub shade: C,
    pub alpha: f32,
    pub seed: u32,
}

/// How a fir tree looks.
#[derive(Clone, Copy)]
pub struct Fir {
    pub dark: C,
    pub mid: C,
    pub light: C,
    pub trunk: C,
    /// 0 is bare, 1 is heavy with snow.
    pub snow: f32,
    pub snow_lit: C,
    pub snow_shade: C,
    /// Half width at the base, as a share of the height.
    pub width: f32,
    pub tiers: f32,
}

impl Painter {
    pub fn new(w: usize, h: usize, l: Light, seed: u32) -> Self {
        Self {
            cv: Canvas::new(w, h),
            l,
            w: w as f32,
            h: h as f32,
            u: h as f32 / 1440.0,
            rng: Rng(0x2f6e_2b1d ^ seed.wrapping_mul(0x9e37_79b9)),
            seed: seed.wrapping_mul(7919),
        }
    }

    // ----- Light -----

    /// Colour of a surface under this light, from daylight to moonlight.
    pub fn lit(&self, c: C) -> C {
        let day = mul(c, self.l.tint);
        let moon = mul(c, [0.17, 0.22, 0.4]);
        let out = lerp(day, moon, self.l.darkness * 0.9);
        out.map(|v| v.clamp(0.0, 1.0))
    }

    /// Fades a colour into the air, for things further away.
    pub fn air(&self, c: C, amount: f32) -> C {
        lerp(c, self.l.horizon, amount.clamp(0.0, 1.0))
    }

    pub fn dark(&self) -> f32 {
        self.l.darkness
    }

    /// How brightly windows and lamps shine: off by day, full at night.
    pub fn lamps(&self) -> f32 {
        smooth(0.16, 0.55, self.l.darkness)
    }

    pub fn stars_amount(&self) -> f32 {
        smooth(0.3, 0.85, self.l.darkness)
    }

    /// Which side the light comes from: -1 left, 1 right.
    pub fn sun_side(&self) -> f32 {
        if self.l.body_at.0 >= 0.5 { 1.0 } else { -1.0 }
    }

    pub fn body_pos(&self) -> (f32, f32) {
        (self.l.body_at.0 * self.w, self.l.body_at.1 * self.h)
    }

    pub fn rim(&self) -> C {
        lerp(self.l.rim, scale(self.l.rim, 0.55), self.l.darkness * 0.5)
    }

    /// Scales a count of small details to the picture's size, so previews
    /// are not crowded and large wallpapers are not sparse.
    pub fn many(&self, count: usize) -> usize {
        let s = (self.w * self.h / (2560.0 * 1440.0))
            .sqrt()
            .clamp(0.06, 1.6);
        ((count as f32 * s).ceil() as usize).max(1)
    }

    // ----- Sky -----

    pub fn sky_at(&self, x: f32, y: f32) -> C {
        let l = &self.l;
        let t = (y / (self.h * 0.72)).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let mut c = lerp(l.top, l.horizon, t.powf(1.15));
        if l.body == Body::Sun {
            // The sky warms and brightens around the sun and along the
            // horizon below it.
            let (sx, sy) = self.body_pos();
            let d = ((x - sx).powi(2) + (y - sy).powi(2)).sqrt() / self.h;
            let near = (-d * d * 5.0).exp() * 0.42 + (-d * 2.2).exp() * 0.12;
            let warm = lerp(l.horizon, l.body_color, 0.6);
            c = lerp(c, warm, (near * (1.0 - l.darkness * 0.4)).min(1.0));
            let beneath = (-((x - sx) / self.w).powi(2) * 7.0).exp() * t.powi(3) * 0.25;
            c = lerp(c, lerp(l.horizon, rgb(0xffffff), 0.35), beneath);
        } else if l.body == Body::Moon {
            let (sx, sy) = self.body_pos();
            let d = ((x - sx).powi(2) + (y - sy).powi(2)).sqrt() / self.h;
            let near = (-d * d * 9.0).exp() * 0.12;
            c = lerp(c, lerp(l.top, rgb(0x9aaee8), 0.5), near);
        }
        c
    }

    pub fn sky(&mut self) {
        let (w, h) = (self.cv.w, self.cv.h);
        for y in 0..h {
            for x in 0..w {
                self.cv.px[y * w + x] = self.sky_at(x as f32 + 0.5, y as f32 + 0.5);
            }
        }
    }

    /// The sun or moon. `alpha` lets fog or cloud veil it.
    pub fn body(&mut self, alpha: f32) {
        let l = self.l;
        let (x, y) = self.body_pos();
        let r = l.body_size * self.h;
        match l.body {
            Body::None => {}
            Body::Sun => {
                self.cv.glow(x, y, r * 12.0, l.body_color, 0.16 * alpha);
                self.cv.glow(x, y, r * 4.0, l.body_color, 0.32 * alpha);
                self.cv.region(
                    x - r - 2.0,
                    y - r - 2.0,
                    x + r + 2.0,
                    y + r + 2.0,
                    |px, py| {
                        let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                        let cover = (r - d + 0.8).clamp(0.0, 1.0);
                        let core = lerp(l.body_color, rgb(0xffffff), 0.6 * (1.0 - d / r).max(0.0));
                        (cover > 0.0).then_some((core, cover * alpha))
                    },
                );
            }
            Body::Moon => {
                self.cv.glow(x, y, r * 9.0, rgb(0xaab8f0), 0.1 * alpha);
                self.cv.glow(x, y, r * 2.6, rgb(0xe8e4ff), 0.12 * alpha);
                let top = l.top;
                let seed = self.seed;
                // A waxing crescent with the dark side faintly lit by the earth.
                let (ox, oy) = (x - r * 0.55, y - r * 0.12);
                self.cv.region(
                    x - r - 2.0,
                    y - r - 2.0,
                    x + r + 2.0,
                    y + r + 2.0,
                    |px, py| {
                        let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                        let inside = (r - d + 0.7).clamp(0.0, 1.0);
                        if inside <= 0.0 {
                            return None;
                        }
                        let shadow = ((px - ox).powi(2) + (py - oy).powi(2)).sqrt();
                        let lit = smooth(r * 0.86, r * 0.96, shadow);
                        let maria = fbm(px / (r * 0.45), py / (r * 0.45), seed, 3);
                        let face = scale(l.body_color, 0.86 + 0.18 * smooth(0.35, 0.65, maria));
                        let earthshine = lerp(top, rgb(0x5a6690), 0.35);
                        Some((lerp(earthshine, face, lit), inside * alpha))
                    },
                );
            }
        }
    }

    /// Stars, thinning towards the horizon. `reach` is how far down they go.
    pub fn stars(&mut self, count: usize, reach: f32, amount: f32) {
        let amount = amount * self.stars_amount();
        if amount <= 0.01 {
            return;
        }
        let u = self.u.max(0.45);
        let tints = [rgb(0xfff8ea), rgb(0xdfe8ff), rgb(0xffe6c8), rgb(0xffffff)];
        for _ in 0..self.many(count) {
            let x = self.rng.next() * self.w;
            let y = self.rng.next().powf(1.25) * self.h * reach;
            let size = self.rng.next().powi(4);
            let c = self.rng.pick(&tints);
            let fade = amount * (1.0 - y / (self.h * reach)).max(0.0).sqrt();
            let r = (0.55 + size * 1.9) * u;
            self.cv.disk(x, y, r, c, fade * (0.3 + size * 0.7));
            if size > 0.7 {
                self.cv.glow(x, y, r * 7.0, rgb(0xc8d6ff), fade * 0.22);
                let s = r * 6.0;
                self.cv
                    .taper([x - s, y], [x + s, y], r * 0.6, r * 0.6, c, fade * 0.35);
                self.cv
                    .taper([x, y - s], [x, y + s], r * 0.6, r * 0.6, c, fade * 0.35);
            }
        }
    }

    /// A glowing band of the galaxy with dark dust lanes, from `a` to `b`.
    pub fn milky_way(
        &mut self,
        a: (f32, f32),
        b: (f32, f32),
        width: f32,
        colors: [C; 2],
        amount: f32,
    ) {
        let amount = amount * self.stars_amount();
        if amount <= 0.01 {
            return;
        }
        let (ax, ay, bx, by) = (a.0 * self.w, a.1 * self.h, b.0 * self.w, b.1 * self.h);
        let (dx, dy) = (bx - ax, by - ay);
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = (-dy / len, dx / len);
        let band = width * self.h;
        let (seed, h) = (self.seed, self.h);
        self.cv.light_region(0.0, 0.0, self.w, self.h, |x, y| {
            let (px, py) = (x - ax, y - ay);
            let along = (px * dx + py * dy) / (len * len);
            let across = (px * nx + py * ny) / band;
            let wobble = (fbm1(along * 4.0, seed + 5, 3) - 0.5) * 0.9;
            let d = across - wobble;
            let core = (-d * d * 1.6).exp();
            if core < 0.01 {
                return None;
            }
            let cloud = fbm(x / (h * 0.09), y / (h * 0.09), seed + 9, 5);
            let dust = smooth(0.5, 0.72, fbm(x / (h * 0.05), y / (h * 0.05), seed + 13, 4))
                * (-d * d * 6.0).exp();
            let glow = core * (0.45 + cloud * 0.9) * (1.0 - dust * 0.85);
            let c = lerp(colors[0], colors[1], smooth(0.35, 0.7, cloud));
            Some((c, glow * 0.34 * amount))
        });
        let u = self.u.max(0.45);
        for _ in 0..self.many(2600) {
            let t = self.rng.next();
            let off = (self.rng.next() + self.rng.next() + self.rng.next() - 1.5) * band * 0.9;
            let wobble = (fbm1(t * 4.0, seed + 5, 3) - 0.5) * 0.9 * band;
            let (x, y) = (
                ax + dx * t + nx * (off + wobble),
                ay + dy * t + ny * (off + wobble),
            );
            let r = (0.4 + self.rng.next() * 0.8) * u;
            let a = amount * self.rng.range(0.2, 0.75);
            self.cv.disk(x, y, r, rgb(0xf4f0ff), a);
        }
    }

    /// A band of soft painted clouds, lit from the sun's side.
    pub fn clouds(&mut self, c: &Clouds) {
        let (y0, y1) = (c.y0 * self.h, c.y1 * self.h);
        let s = c.size * self.h;
        let (bx, by) = self.body_pos();
        let has_sun = self.l.body != Body::None;
        let thr = 1.0 - c.cover;
        let lining = lerp(self.l.body_color, rgb(0xffffff), 0.3);
        let density = move |nx: f32, ny: f32, env: f32| {
            let q = fbm(nx * 0.45, ny * 0.45, c.seed + 50, 3);
            fbm(nx + q * 1.6, ny + q * 0.7, c.seed, 4) * (0.55 + 0.45 * env) - (1.0 - env) * 0.25
        };
        self.cv.region(0.0, y0, self.w, y1, |x, y| {
            let v = (y - y0) / (y1 - y0);
            // Rounded tops and flatter bases.
            let env = smooth(0.0, 0.45, v) * smooth(1.0, 0.7, v);
            let (nx, ny) = (x / (s * c.stretch), y / s);
            let d = density(nx, ny, env);
            let a = smooth(thr, thr + c.softness, d);
            if a <= 0.0 {
                return None;
            }
            let (lx, ly) = if has_sun {
                let (vx, vy) = (bx - x, by - y);
                let l = (vx * vx + vy * vy).sqrt().max(1.0);
                (vx / l, vy / l)
            } else {
                (0.0, -1.0)
            };
            let o = 0.3;
            let (ox, oy) = ((lx * 0.6) / c.stretch, ly * 0.6 - 0.5);
            let d_light = density(nx + ox * o, ny + oy * o, env);
            let lit = ((d - d_light) * 5.5 + 0.5 - v * 0.2).clamp(0.0, 1.0);
            let lit = smooth(0.2, 0.8, lit);
            let mut col = lerp(c.shade, c.lit, lit);
            if has_sun {
                let near = (-(((x - bx).powi(2) + (y - by).powi(2)).sqrt() / s * 0.35)).exp();
                col = lerp(col, lining, (1.0 - a) * near * 0.8);
            }
            Some((col, a * c.alpha))
        });
    }

    /// Fair-weather cumulus: each cloud is a cluster of rounded lobes with a
    /// flat base, lit like soft balls from the sun's side and from above.
    pub fn cumulus(
        &mut self,
        y0: f32,
        y1: f32,
        count: usize,
        size: f32,
        lit: C,
        shade: C,
        alpha: f32,
        seed: u32,
    ) {
        let (w, h) = (self.w, self.h);
        let mut rng = Rng(seed.wrapping_mul(2_654_435_761) | 1);
        let (bx, by) = self.body_pos();
        let has_sun = self.l.body != Body::None;
        let lining = lerp(self.l.body_color, rgb(0xffffff), 0.35);
        let mut slots: Vec<f32> = (0..count)
            .map(|i| (i as f32 + rng.range(0.15, 0.85)) / count as f32)
            .collect();
        // Shuffle so sizes do not follow position.
        for i in (1..slots.len()).rev() {
            let j = (rng.next() * (i + 1) as f32) as usize % (i + 1);
            slots.swap(i, j);
        }
        for (k, &slot) in slots.iter().enumerate() {
            let cx = (slot * 1.1 - 0.05) * w;
            let base = rng.range(y0, y1) * h;
            let width = size
                * h
                * rng.range(1.4, 3.4)
                * (0.6 + (base / h - y0) / (y1 - y0).max(0.01) * 0.6);
            let height = width * rng.range(0.24, 0.4);
            let peak = rng.range(0.3, 0.7);
            let n = 8 + (rng.next() * 6.0) as usize;
            let mut lobes: Vec<[f32; 3]> = Vec::new();
            for i in 0..n {
                let t = ((i as f32 + rng.range(0.2, 0.8)) / n as f32).clamp(0.0, 1.0);
                let hump =
                    (1.0 - ((t - peak) / if t < peak { peak } else { 1.0 - peak }).abs()).max(0.0);
                let r = height * (0.28 + 0.5 * hump.powf(0.8)) * rng.range(0.75, 1.2);
                lobes.push([
                    cx + (t - 0.5) * width * 0.85,
                    base - r * rng.range(0.55, 0.8),
                    r,
                ]);
            }
            for _ in 0..(n / 3) {
                let t = peak + rng.range(-0.18, 0.18);
                let r = height * rng.range(0.28, 0.42);
                lobes.push([
                    cx + (t - 0.5) * width * 0.85,
                    base - height * rng.range(0.75, 1.0),
                    r,
                ]);
            }
            let reach = lobes.iter().fold(0.0f32, |m, l| m.max(l[2]));
            let cseed = seed + k as u32 * 101;
            self.cv.region(
                cx - width * 0.5 - reach,
                base - height * 1.5 - reach,
                cx + width * 0.5 + reach,
                base + 2.0,
                |x, y| {
                    if y > base + 1.0 {
                        return None;
                    }
                    let edge_n = fbm(x / (height * 0.18), y / (height * 0.18), cseed, 3) - 0.5;
                    let mut best = f32::MIN;
                    let mut owner = lobes[0];
                    for l in &lobes {
                        let d = ((x - l[0]).powi(2) + (y - l[1]).powi(2)).sqrt() / l[2];
                        let v = 1.0 - d + edge_n * 0.12;
                        if v > best {
                            best = v;
                            owner = *l;
                        }
                    }
                    let cover =
                        (best * owner[2] + 0.5).clamp(0.0, 1.0) * (base - y + 0.5).clamp(0.0, 1.0);
                    if cover <= 0.0 {
                        return None;
                    }
                    let (nx, ny) = ((x - owner[0]) / owner[2], (y - owner[1]) / owner[2]);
                    let nz = (1.0 - (nx * nx + ny * ny).min(1.0)).sqrt();
                    let (lx, ly) = if has_sun {
                        let (vx, vy) = (bx - x, by - y);
                        let l = (vx * vx + vy * vy).sqrt().max(1.0);
                        (vx / l * 0.7, vy / l * 0.5 - 0.5)
                    } else {
                        (0.0, -0.8)
                    };
                    let lam = (nx * lx + ny * ly + nz * 0.45).clamp(-1.0, 1.0);
                    let under = smooth(base - height * 0.35, base, y);
                    let texture = fbm(x / (height * 0.12), y / (height * 0.12), cseed + 7, 2) - 0.5;
                    let li =
                        (smooth(-0.3, 0.75, lam) - under * 0.45 + texture * 0.25).clamp(0.0, 1.0);
                    let mut col = lerp(shade, lit, li);
                    if has_sun {
                        let near =
                            (-((x - bx).powi(2) + (y - by).powi(2)).sqrt() / (h * 0.35)).exp();
                        let rim = smooth(0.06, 0.0, best) * near;
                        col = lerp(col, lining, rim * 0.7);
                    }
                    Some((col, cover * alpha))
                },
            );
        }
    }

    /// Long thin clouds catching low light, for dawn and dusk skies.
    pub fn streaks(&mut self, y: f32, thick: f32, color: C, alpha: f32, seed: u32) {
        let (cy, half) = (y * self.h, thick * self.h);
        let h = self.h;
        self.cv
            .region(0.0, cy - half * 2.0, self.w, cy + half * 2.0, |x, py| {
                let d = (py - cy) / half;
                let n = fbm(x / (h * 0.9), py / (h * 0.012), seed, 4);
                let a = smooth(0.5, 0.75, n) * (-d * d).exp() * alpha;
                (a > 0.0).then_some((color, a))
            });
    }

    /// Soft rays fanning out from the sun.
    pub fn rays(&mut self, strength: f32, color: C) {
        if self.l.body != Body::Sun || strength <= 0.0 {
            return;
        }
        let (sx, sy) = self.body_pos();
        let (seed, h) = (self.seed + 77, self.h);
        self.cv.light_region(0.0, 0.0, self.w, self.h, |x, y| {
            let (dx, dy) = (x - sx, y - sy);
            let d = (dx * dx + dy * dy).sqrt() / h;
            if d < 0.02 {
                return None;
            }
            let ang = dy.atan2(dx);
            let ray = smooth(0.52, 0.82, noise(ang * 9.0 + 40.0, 0.5, seed))
                * smooth(0.45, 0.7, noise(ang * 23.0 + 9.0, 1.5, seed));
            let fall = (-d * 1.6).exp() * smooth(0.0, 0.08, d);
            let a = ray * fall * strength;
            (a > 0.002).then_some((color, a))
        });
    }

    // ----- Land -----

    /// The edge of a layer at every column, so it is worked out only once.
    pub fn column_tops(&self, top: &dyn Fn(f32) -> f32) -> Vec<f32> {
        (0..self.cv.w).map(|x| top(x as f32 + 0.5)).collect()
    }

    /// A natural skyline: `base` and `amp` are shares of the height, `freq`
    /// is how many bumps per picture height, `sharp` mixes in craggy crests.
    pub fn skyline(
        &self,
        base: f32,
        amp: f32,
        freq: f32,
        seed: u32,
        sharp: f32,
    ) -> impl Fn(f32) -> f32 + use<> {
        let h = self.h;
        move |x: f32| {
            let s = x / h * freq;
            let soft = fbm1(s, seed, 4);
            let crag = if sharp > 0.0 {
                ridged(s * 0.6, 0.37, seed + 1, 4)
            } else {
                0.0
            };
            h * (base - amp * (soft * (1.0 - sharp) + crag * sharp))
        }
    }

    /// Fills the land below `top` with a painted texture and light along
    /// the edges that face the sun or moon.
    pub fn land(
        &mut self,
        top: &dyn Fn(f32) -> f32,
        bottom: f32,
        near: C,
        deep: C,
        depth: f32,
        texture: f32,
        rim: f32,
        seed: u32,
    ) {
        let tops = self.column_tops(top);
        let top = |x: f32| top_at(&tops, x);
        let bottom = bottom * self.h;
        let u = self.u.max(0.4);
        let depth = depth * self.h;
        let rim_c = self.rim();
        let side = self.sun_side();
        let rim_w = 9.0 * u;
        let h = self.h;
        self.cv.layer_to(
            top,
            bottom,
            |x, y, d| {
                let mut c = lerp(near, deep, (d / depth).clamp(0.0, 1.0));
                if texture > 0.0 {
                    let n = fbm(x / (h * 0.04), y / (h * 0.011), seed, 3);
                    c = scale(c, 1.0 + (n - 0.5) * texture);
                }
                if rim > 0.0 && d < rim_w {
                    let slope = slope_at(&tops, x, 3.0 * u);
                    let facing = (0.4 - slope * side * 1.5).clamp(0.0, 1.0);
                    let k = (1.0 - d / rim_w).powi(2) * facing * rim;
                    c = lerp(c, rim_c, k);
                }
                c
            },
            1.0,
        );
    }

    /// A craggy mountain range. Faces turned to the light are bright and
    /// the others fall into shadow; rock shows through the snow in gullies.
    pub fn mountains(
        &mut self,
        top: &dyn Fn(f32) -> f32,
        bottom: f32,
        rock: C,
        snow: C,
        haze: f32,
        snow_line: f32,
        seed: u32,
    ) {
        let tops = self.column_tops(top);
        let top = |x: f32| top_at(&tops, x);
        let bottom = bottom * self.h;
        let side = self.sun_side();
        let h = self.h;
        let lit_rock = self.air(self.lit(rock), haze);
        let dark_rock = self.air(self.lit(lerp(rock, rgb(0x141a2e), 0.55)), haze);
        let lit_snow = self.air(
            lerp(self.lit(snow), self.rim(), 0.18 * (1.0 - self.dark())),
            haze * 0.8,
        );
        let shade_snow = self.air(self.lit(lerp(snow, rgb(0x5d74ae), 0.45)), haze * 0.8);
        let rim_c = self.air(self.rim(), haze);
        let glint = 1.0 - self.dark() * 0.6;
        let u = self.u.max(0.4);
        let (mut min_top, mut max_top) = (f32::MAX, f32::MIN);
        for x in (0..self.cv.w).step_by(3) {
            let t = top(x as f32);
            min_top = min_top.min(t);
            max_top = max_top.max(t);
        }
        let span = (max_top - min_top + 0.04 * h).max(1.0);
        self.cv.layer_to(
            top,
            bottom,
            |x, y, d| {
                let e = 4.0 * u + d * 0.6;
                let slope = slope_at(&tops, x, e);
                let streak = fbm(
                    x / (h * 0.025) + y / (h * 0.03) * side * 0.5,
                    y / (h * 0.1),
                    seed,
                    3,
                );
                let face = (0.5 + slope * side * 1.5 + (streak - 0.5) * 0.35).clamp(0.0, 1.0);
                let li = smooth(0.4, 0.6, face);
                let height = 1.0 - ((y - min_top) / span).clamp(0.0, 1.0);
                let gully = ridged(
                    x / (h * 0.035) + y / (h * 0.05) * side * 0.6,
                    y / (h * 0.14),
                    seed + 3,
                    2,
                );
                let cover = height * snow_line + (streak - 0.5) * 0.5
                    - gully * 0.6 * slope.abs().min(1.0)
                    - 0.62;
                let snowy = smooth(0.0, 0.08, cover);
                let rock_c = lerp(dark_rock, lit_rock, li * 0.8);
                let snow_c = lerp(shade_snow, lit_snow, li);
                let mut c = lerp(rock_c, snow_c, snowy);
                if d < 4.0 * u {
                    c = lerp(c, rim_c, (1.0 - d / (4.0 * u)) * 0.55 * li * glint);
                }
                c
            },
            1.0,
        );
    }

    /// A long soft shadow cast across the ground, away from the light.
    pub fn cast_shadow(
        &mut self,
        x: f32,
        base: f32,
        length: f32,
        width: f32,
        color: C,
        alpha: f32,
    ) {
        let dir = -self.sun_side();
        let low = (self.l.body_at.1 * 2.2).clamp(0.3, 1.0);
        let len = length * low;
        self.cv.region(
            x - len * 1.2,
            base - width * 2.0,
            x + len * 1.2,
            base + width * 2.0,
            |px, py| {
                let t = (px - x) * dir / len;
                if !(-0.15..=1.0).contains(&t) {
                    return None;
                }
                let half = width * (1.0 - t * 0.6);
                let d = (py - base).abs() / half;
                let a = smooth(1.0, 0.3, d) * smooth(1.0, 0.2, t) * smooth(-0.15, 0.05, t) * alpha;
                (a > 0.0).then_some((color, a))
            },
        );
    }

    /// Soft drifting fog. `y` and `thick` are shares of the height.
    pub fn fog(&mut self, y: f32, thick: f32, alpha: f32, color: C, seed: u32) {
        let (cy, half) = (y * self.h, thick * self.h);
        let h = self.h;
        self.cv
            .region(0.0, cy - half * 2.5, self.w, cy + half * 2.5, |x, py| {
                let d = (py - cy) / half;
                let base = (-d * d).exp();
                let n = fbm(x / (h * 0.5), py / (h * 0.06), seed, 4);
                let a = base * alpha * smooth(0.2, 0.7, n * 0.7 + base * 0.45);
                (a > 0.003).then_some((color, a))
            });
    }

    // ----- Trees -----

    /// A fir with drooping, uneven tiers. `base` is where the trunk meets
    /// the ground.
    pub fn fir(&mut self, x: f32, base: f32, height: f32, f: &Fir, seed: u32) {
        let wid = height * f.width;
        let side = self.sun_side();
        let trunk_w = height * 0.025;
        let tiers = f.tiers;
        self.cv.region(
            x - wid * 1.25,
            base - height,
            x + wid * 1.25,
            base + 1.0,
            |px, py| {
                let t = (base - py) / height;
                if !(0.0..=1.0).contains(&t) {
                    return None;
                }
                let dx = px - x;
                let t0 = 0.06;
                if t < t0 + 0.04 {
                    let cover = (trunk_w - dx.abs() + 0.5).clamp(0.0, 1.0);
                    if t < t0 && cover > 0.0 {
                        return Some((f.trunk, cover));
                    }
                }
                let tt = ((t - t0) / (1.0 - t0)).max(0.0);
                let ax = (dx.abs() / wid).min(1.2);
                // Boughs droop at their tips.
                let ta = tt + ax * 0.55 / tiers;
                let k = (ta * tiers).floor();
                let fr = ta * tiers - k;
                let tier_w = 1.0 + (noise(k * 3.7, 0.5, seed) - 0.5) * 0.35;
                let taper = (1.0 - tt).powf(1.1);
                let s = if dx < 0.0 { 0.0 } else { 1.0 };
                let jag = noise(py / (height * 0.012) + s * 31.0, k + s * 7.0, seed + 1) - 0.5;
                let hw = wid * taper * tier_w * (0.5 + 0.5 * (1.0 - fr).powf(1.3))
                    + wid * 0.08 * jag * (0.4 + ax);
                let dist = hw - dx.abs();
                let cover = (dist + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 || tt <= 0.0 {
                    return None;
                }
                let sx = (dx / hw.max(0.5)).clamp(-1.0, 1.0);
                let clump = noise(px / (height * 0.03), py / (height * 0.03), seed + 3) - 0.5;
                let mut li = 0.4 + 0.35 * sx * side + clump * 0.45 - 0.35 * smooth(0.6, 1.0, fr);
                li += 0.12 * (1.0 - fr);
                let col = ramp3(f.dark, f.mid, f.light, li.clamp(0.0, 1.0));
                if f.snow > 0.0 {
                    let lump = noise(px / (height * 0.022), py / (height * 0.02), seed + 9);
                    let thr = 0.78 - 0.3 * f.snow + (lump - 0.5) * 0.5 + ax * 0.12;
                    if fr > thr && fr < 0.97 {
                        let under = smooth(thr, thr + 0.06, fr);
                        let sl = (0.55 + 0.4 * sx * side + clump * 0.5).clamp(0.0, 1.0);
                        let sc = lerp(f.snow_shade, f.snow_lit, sl * under);
                        return Some((sc, cover));
                    }
                }
                Some((col, cover))
            },
        );
    }

    /// Firs along a ridge, drawn back to front.
    pub fn firs(
        &mut self,
        top: &dyn Fn(f32) -> f32,
        from: f32,
        to: f32,
        count: usize,
        size: (f32, f32),
        look: &Fir,
        sink: f32,
    ) -> Vec<(f32, f32, f32)> {
        let mut trees: Vec<(f32, f32, f32, u32)> = (0..count)
            .map(|i| {
                let x = self.rng.range(from, to) * self.w;
                let hgt = self.rng.range(size.0, size.1) * self.h;
                let s = self.rng.range(0.0, sink) * self.h;
                (x, top(x) + s, hgt, self.seed.wrapping_add(i as u32 * 31))
            })
            .collect();
        trees.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(x, base, hgt, seed) in &trees {
            self.fir(x, base, hgt, look, seed);
        }
        trees.into_iter().map(|(x, b, h, _)| (x, b, h)).collect()
    }

    /// Fir positions along a ridge without drawing them, so shadows can go
    /// down first.
    pub fn plan_firs(
        &mut self,
        top: &dyn Fn(f32) -> f32,
        from: f32,
        to: f32,
        count: usize,
        size: (f32, f32),
    ) -> Vec<(f32, f32, f32, u32)> {
        let mut trees: Vec<(f32, f32, f32, u32)> = (0..count)
            .map(|i| {
                let x = self.rng.range(from, to) * self.w;
                let hgt = self.rng.range(size.0, size.1) * self.h;
                (
                    x,
                    top(x) + 0.004 * self.h,
                    hgt,
                    self.seed.wrapping_add(900 + i as u32 * 37),
                )
            })
            .collect();
        trees.sort_by(|a, b| a.1.total_cmp(&b.1));
        trees
    }

    /// A leafy crown: a lumpy shape shaded like a ball of foliage, with
    /// clumps of light and shadow. `tones` run from shadow to highlight.
    pub fn crown(
        &mut self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        tones: [C; 3],
        seed: u32,
        alpha: f32,
    ) {
        let side = self.sun_side();
        let clump = rx.min(ry) * 0.32;
        self.cv.region(
            cx - rx * 1.3,
            cy - ry * 1.3,
            cx + rx * 1.3,
            cy + ry * 1.3,
            |x, y| {
                let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
                let d = (dx * dx + dy * dy).sqrt();
                if d > 1.35 {
                    return None;
                }
                let edge = 1.0 + (fbm(x / clump, y / clump, seed, 3) - 0.5) * 0.55;
                let cover = ((edge - d) * rx.min(ry) + 0.5).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    return None;
                }
                let n = fbm(x / (clump * 0.8) + 9.0, y / (clump * 0.8), seed + 1, 3);
                let shape = 0.55 - dy * 0.42 + dx * side * 0.32;
                let li = smooth(0.25, 0.85, shape + (n - 0.5) * 0.9);
                // Three soft steps, like brush strokes.
                let stepped = (li * 3.0).floor() / 2.0;
                let li = li + (stepped.min(1.0) - li) * 0.3;
                Some((ramp3(tones[0], tones[1], tones[2], li), cover * alpha))
            },
        );
    }

    /// A branching limb. Tips are collected for leaves or blossom.
    pub fn branch(
        &mut self,
        x: f32,
        y: f32,
        angle: f32,
        len: f32,
        width: f32,
        depth: u32,
        color: C,
        spread: f32,
        tips: &mut Vec<[f32; 2]>,
    ) {
        let bend = self.rng.range(-0.18, 0.18);
        let mid_a = angle + bend;
        let (mx, my) = (x + mid_a.cos() * len * 0.5, y + mid_a.sin() * len * 0.5);
        let end_a = angle - bend * 0.5;
        let (ex, ey) = (mx + end_a.cos() * len * 0.5, my + end_a.sin() * len * 0.5);
        let w_end = width * 0.68;
        let w_mid = (width + w_end) * 0.5;
        self.cv.taper([x, y], [mx, my], width, w_mid, color, 1.0);
        self.cv.taper([mx, my], [ex, ey], w_mid, w_end, color, 1.0);
        if depth == 0 || len < 2.0 {
            tips.push([ex, ey]);
            return;
        }
        tips.push([mx, my]);
        let n = if self.rng.next() < 0.3 { 3 } else { 2 };
        for i in 0..n {
            let off = if n == 2 {
                if i == 0 { -1.0 } else { 1.0 }
            } else {
                i as f32 - 1.0
            };
            let a = end_a + off * spread * self.rng.range(0.6, 1.1) - 0.08 * end_a.cos();
            let l = len * self.rng.range(0.62, 0.8);
            self.branch(ex, ey, a, l, w_end, depth - 1, color, spread, tips);
        }
    }

    /// Blades of grass rising from `ground`.
    pub fn grass(
        &mut self,
        ground: &dyn Fn(f32) -> f32,
        from: f32,
        to: f32,
        count: usize,
        height: f32,
        colors: &[C],
        depth: f32,
    ) {
        let u = self.u.max(0.45);
        for _ in 0..self.many(count) {
            let x = self.rng.range(from, to) * self.w;
            let y = ground(x) + self.rng.range(0.0, depth) * self.h;
            if y > self.h + 6.0 {
                continue;
            }
            let near = ((y - ground(x)) / (depth * self.h).max(1.0)).clamp(0.0, 1.0);
            let hgt = self.rng.range(0.4, 1.0) * height * self.h * (0.7 + near * 0.6);
            let lean = self.rng.range(-0.4, 0.4) * hgt;
            let c = self.rng.pick(colors);
            let mid = [x + lean * 0.3, y - hgt * 0.55];
            let tip = [x + lean, y - hgt];
            self.cv.taper([x, y], mid, 2.6 * u, 1.6 * u, c, 1.0);
            self.cv.taper(mid, tip, 1.6 * u, 0.4 * u, c, 1.0);
        }
    }

    // ----- Water -----

    /// Mirrors what is above `axis` into the water between `y0` and `y1`
    /// (shares of the height), with ripples and a tint. `mask` gives how
    /// much of each pixel is water.
    pub fn reflect(
        &mut self,
        axis: f32,
        y0: f32,
        y1: f32,
        tint: C,
        amount: f32,
        ripple: f32,
        mask: &dyn Fn(f32, f32) -> f32,
    ) {
        let (axis, y0, y1) = (axis * self.h, y0 * self.h, y1 * self.h);
        let (w, h) = (self.cv.w, self.cv.h);
        let u = self.u.max(0.4);
        let seed = self.seed + 404;
        let ya = (y0.max(0.0) as usize).min(h);
        let yb = (y1.ceil().max(0.0) as usize).min(h);
        for y in ya..yb {
            let py = y as f32 + 0.5;
            let dy = py - axis;
            let depth = ((py - y0) / (y1 - y0).max(1.0)).clamp(0.0, 1.0);
            let src_y = (axis - dy * 1.06).max(0.0);
            for x in 0..w {
                let px = x as f32 + 0.5;
                let m = mask(px, py);
                if m <= 0.0 {
                    continue;
                }
                let n = noise(px / (70.0 * u), py / (2.2 * u + dy * 0.02), seed) - 0.5;
                let wob = n * ripple * u * (1.0 + dy / self.h * 6.0);
                let src = self.cv.sample(px + wob, src_y + n * ripple * u * 0.4);
                let c = lerp(src, tint, (amount + depth * 0.25).min(1.0));
                self.cv.blend(x, y, c, m);
            }
        }
    }

    /// Fine horizontal lines of light on water.
    pub fn ripples(
        &mut self,
        y0: f32,
        y1: f32,
        count: usize,
        color: C,
        alpha: f32,
        mask: &dyn Fn(f32, f32) -> f32,
    ) {
        let u = self.u.max(0.45);
        for _ in 0..self.many(count) {
            let y = self.rng.range(y0, y1) * self.h;
            let x = self.rng.next() * self.w;
            if mask(x, y) <= 0.5 {
                continue;
            }
            let near = (y / self.h - y0) / (y1 - y0);
            let len = self.rng.range(8.0, 40.0) * u * (1.0 + near * 3.0);
            let a = alpha * self.rng.range(0.3, 1.0);
            self.cv
                .taper([x - len, y], [x + len, y], 1.4 * u, 1.4 * u, color, a);
        }
    }

    /// A path of glinting light on water below a light source at `x`.
    pub fn glitter(
        &mut self,
        x: f32,
        y0: f32,
        y1: f32,
        spread: f32,
        color: C,
        alpha: f32,
        count: usize,
    ) {
        let u = self.u.max(0.45);
        for _ in 0..self.many(count) {
            let t = self.rng.next().powf(0.8);
            let y = (y0 + (y1 - y0) * t) * self.h;
            let off = (self.rng.next() + self.rng.next() - 1.0) * spread * self.h * (0.3 + t);
            let len = self.rng.range(3.0, 16.0) * u * (0.5 + t * 2.0);
            let a = alpha * self.rng.range(0.3, 1.0) * (1.0 - t * 0.5);
            self.cv.taper(
                [x + off - len, y],
                [x + off + len, y],
                1.6 * u * (0.6 + t),
                1.2 * u,
                color,
                a,
            );
        }
    }

    // ----- Small things -----

    /// A lark in flight, seen from below: `s` is its wingspan in pixels.
    pub fn lark_flying(&mut self, x: f32, y: f32, s: f32, color: C, flap: f32) {
        let lift = s * (0.12 + 0.2 * flap);
        let w = s * 0.07;
        self.cv
            .taper([x, y], [x - s * 0.25, y - lift], w, w * 0.7, color, 1.0);
        self.cv.taper(
            [x - s * 0.25, y - lift],
            [x - s * 0.5, y - lift * 0.4],
            w * 0.7,
            w * 0.2,
            color,
            1.0,
        );
        self.cv
            .taper([x, y], [x + s * 0.25, y - lift], w, w * 0.7, color, 1.0);
        self.cv.taper(
            [x + s * 0.25, y - lift],
            [x + s * 0.5, y - lift * 0.4],
            w * 0.7,
            w * 0.2,
            color,
            1.0,
        );
        self.cv
            .ellipse(x, y + s * 0.02, s * 0.06, s * 0.1, color, 1.0);
    }

    /// A small crested lark perched at (x, y), facing `dir` (-1 or 1).
    /// `s` is its height in pixels.
    pub fn lark_perched(&mut self, x: f32, y: f32, s: f32, dir: f32) {
        let back = self.lit(rgb(0x8a6a48));
        let belly = self.lit(rgb(0xe8d6b4));
        let dark = self.lit(rgb(0x4a3828));
        let beak = self.lit(rgb(0x3a2e24));
        // Tail, body, head and crest.
        self.cv.poly(
            &[
                [x - dir * s * 0.25, y - s * 0.3],
                [x - dir * s * 0.62, y - s * 0.12],
                [x - dir * s * 0.55, y - s * 0.02],
                [x - dir * s * 0.18, y - s * 0.2],
            ],
            dark,
            1.0,
        );
        self.cv
            .ellipse(x, y - s * 0.32, s * 0.3, s * 0.2, back, 1.0);
        self.cv.ellipse(
            x + dir * s * 0.06,
            y - s * 0.27,
            s * 0.2,
            s * 0.13,
            belly,
            1.0,
        );
        self.cv
            .disk(x + dir * s * 0.22, y - s * 0.55, s * 0.14, back, 1.0);
        self.cv.poly(
            &[
                [x + dir * s * 0.12, y - s * 0.62],
                [x + dir * s * 0.02, y - s * 0.86],
                [x + dir * s * 0.24, y - s * 0.66],
            ],
            back,
            1.0,
        );
        self.cv.poly(
            &[
                [x + dir * s * 0.33, y - s * 0.58],
                [x + dir * s * 0.46, y - s * 0.54],
                [x + dir * s * 0.33, y - s * 0.5],
            ],
            beak,
            1.0,
        );
        self.cv
            .disk(x + dir * s * 0.27, y - s * 0.58, s * 0.025, dark, 1.0);
        // Streaks on the back.
        for i in 0..3 {
            let t = i as f32 * 0.1;
            self.cv.taper(
                [x - dir * s * (0.12 - t), y - s * 0.4],
                [x - dir * s * (0.02 - t), y - s * 0.36],
                s * 0.03,
                s * 0.02,
                dark,
                0.7,
            );
        }
        self.cv.line(
            [x - s * 0.02, y - s * 0.14],
            [x - s * 0.04, y],
            s * 0.025,
            beak,
            1.0,
        );
        self.cv.line(
            [x + s * 0.06, y - s * 0.14],
            [x + s * 0.06, y],
            s * 0.025,
            beak,
            1.0,
        );
    }

    /// A warm lit window with a soft glow. Unlit, it reflects the sky.
    pub fn window(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, frame: C, lit: f32, bars: bool) {
        let (w, h) = (x1 - x0, y1 - y0);
        let f = w.min(h) * 0.12;
        self.cv.rect(x0 - f, y0 - f, x1 + f, y1 + f, frame, 1.0);
        let day = lerp(self.l.horizon, rgb(0x2b3550), 0.55);
        let inner_top = lerp(day, WARM, lit);
        let inner_bottom = lerp(scale(day, 0.8), rgb(0xffe1a0), lit);
        self.cv.region(x0, y0, x1, y1, |_, y| {
            let t = ((y - y0) / h).clamp(0.0, 1.0);
            Some((lerp(inner_top, inner_bottom, t), 1.0))
        });
        if bars {
            self.cv
                .line([x0 + w * 0.5, y0], [x0 + w * 0.5, y1], f * 0.8, frame, 1.0);
            self.cv
                .line([x0, y0 + h * 0.5], [x1, y0 + h * 0.5], f * 0.8, frame, 1.0);
        }
        if lit > 0.0 {
            self.cv.glow(
                (x0 + x1) * 0.5,
                (y0 + y1) * 0.5,
                w.max(h) * 2.2,
                WARM,
                0.22 * lit,
            );
        }
    }

    /// Smoke rising and drifting from a chimney.
    pub fn smoke(&mut self, x: f32, y: f32, size: f32, drift: f32) {
        let c = lerp(rgb(0xe9edf2), rgb(0x5a6380), self.dark());
        let seed = self.seed + 55;
        let (lean_l, lean_r) = ((drift * 6.0).min(0.0), (drift * 6.0).max(0.0));
        self.cv.region(
            x - size * (2.5 - lean_l),
            y - size * 7.0,
            x + size * (2.5 + lean_r),
            y + size * 0.2,
            |px, py| {
                let t = ((y - py) / (size * 7.0)).clamp(0.0, 1.0);
                let cx = x + drift * size * 6.0 * t * t + (t * 9.0).sin() * size * 0.3 * t;
                let r = size * (0.25 + t * 1.1);
                let n = fbm(px / (size * 0.7), py / (size * 0.7), seed, 3);
                let d = (px - cx).abs() / r;
                let a = smooth(1.0, 0.3, d + (n - 0.5) * 0.8) * (1.0 - t).powf(1.4) * 0.5;
                (a > 0.0).then_some((c, a))
            },
        );
    }

    // ----- Finish -----

    /// Adds a fine paper grain and a gentle vignette, then dithers to
    /// 8 bits so smooth skies show no banding.
    pub fn finish(self) -> Vec<u8> {
        let (w, h) = (self.cv.w, self.cv.h);
        let gu = self.u.max(0.5);
        let mut out = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            let fy = y as f32 + 0.5;
            let vy = fy / h as f32 - 0.5;
            for x in 0..w {
                let fx = x as f32 + 0.5;
                let p = self.cv.px[y * w + x];
                let grain = (noise(fx / (1.4 * gu), fy / (1.4 * gu), 991) - 0.5) * 0.04;
                let paper = (fbm(fx / (90.0 * gu), fy / (90.0 * gu), 773, 2) - 0.5) * 0.022;
                let vx = fx / w as f32 - 0.5;
                let vignette = 1.0 - 0.16 * (vx * vx * 1.1 + vy * vy * 1.8);
                let k = (1.0 + grain + paper) * vignette;
                let dither = (hash(x as i32, y as i32, 4242) - 0.5) * 1.6;
                for v in p {
                    // A soft shoulder keeps glows from clipping harshly.
                    let v = v * k;
                    let v = if v > 0.85 {
                        0.85 + (1.0 - (-(v - 0.85) * 6.6).exp()) * 0.15
                    } else {
                        v
                    };
                    out.push(
                        (v.clamp(0.0, 1.0) * 255.0 + dither)
                            .round()
                            .clamp(0.0, 255.0) as u8,
                    );
                }
            }
        }
        out
    }
}
