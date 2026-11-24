//! Small, fast value noise. It gives the scenes organic edges, painted
//! texture and soft clouds without any artwork files.

#[inline]
pub fn hash(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343)
        ^ (iy as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

/// Smooth value noise in 0..1.
#[inline]
pub fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i32, fy as i32);
    let (tx, ty) = (x - fx, y - fy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Layered noise in roughly 0..1, with finer detail in each octave.
#[inline]
pub fn fbm(x: f32, y: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    let (mut x, mut y) = (x, y);
    for i in 0..octaves {
        sum += noise(x, y, seed.wrapping_add(i * 1013)) * amp;
        norm += amp;
        amp *= 0.5;
        // A slight turn each octave hides the grid.
        let (nx, ny) = (x * 1.6 + y * 1.2, -x * 1.2 + y * 1.6);
        x = nx + 17.3;
        y = ny + 5.9;
    }
    sum / norm
}

/// Noise along a line, for ridges and edges.
#[inline]
pub fn fbm1(x: f32, seed: u32, octaves: u32) -> f32 {
    fbm(x, 0.5, seed, octaves)
}

/// Sharp-crested noise in 0..1, for mountain ridges.
#[inline]
pub fn ridged(x: f32, y: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    let (mut x, mut y) = (x, y);
    let mut weight = 1.0;
    for i in 0..octaves {
        let n = 1.0 - (noise(x, y, seed.wrapping_add(i * 7919)) * 2.0 - 1.0).abs();
        let n = n * n * weight;
        weight = (n * 1.5).clamp(0.0, 1.0);
        sum += n * amp;
        norm += amp;
        amp *= 0.5;
        let (nx, ny) = (x * 1.7 + y * 1.1, -x * 1.1 + y * 1.7);
        x = nx + 3.1;
        y = ny + 11.7;
    }
    sum / norm
}
