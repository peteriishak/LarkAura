//! A floating point canvas with antialiased shapes.

pub type C = [f32; 3];

pub const fn rgb(hex: u32) -> C {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

#[inline]
pub fn lerp(a: C, b: C, t: f32) -> C {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
pub fn mul(a: C, b: C) -> C {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

#[inline]
pub fn scale(a: C, s: f32) -> C {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A colour ramp through three tones, `t` in 0..1.
#[inline]
pub fn ramp3(a: C, b: C, c: C, t: f32) -> C {
    if t < 0.5 {
        lerp(a, b, t * 2.0)
    } else {
        lerp(b, c, (t - 0.5) * 2.0)
    }
}

pub struct Canvas {
    pub px: Vec<C>,
    pub w: usize,
    pub h: usize,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            px: vec![[0.0; 3]; w * h],
            w,
            h,
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> C {
        self.px[y * self.w + x]
    }

    /// The colour under a point, clamped to the picture.
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> C {
        let xi = (x.max(0.0) as usize).min(self.w - 1);
        let yi = (y.max(0.0) as usize).min(self.h - 1);
        self.get(xi, yi)
    }

    #[inline]
    pub fn blend(&mut self, x: usize, y: usize, c: C, a: f32) {
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[y * self.w + x];
        *p = lerp(*p, c, a.min(1.0));
    }

    /// Screen blending: brightens like light, without blowing out.
    #[inline]
    pub fn screen(&mut self, x: usize, y: usize, c: C, a: f32) {
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[y * self.w + x];
        for i in 0..3 {
            let v = p[i].clamp(0.0, 1.0);
            p[i] = v + (1.0 - v) * c[i] * a;
        }
    }

    pub fn bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (usize, usize, usize, usize) {
        let cx = |v: f32| (v.max(0.0) as usize).min(self.w);
        let cy = |v: f32| (v.max(0.0) as usize).min(self.h);
        (
            cx(x0.floor()),
            cy(y0.floor()),
            cx(x1.ceil() + 1.0),
            cy(y1.ceil() + 1.0),
        )
    }

    /// Runs `f` on every pixel centre in a box. `f` returns a colour and its
    /// coverage, or `None` to leave the pixel alone.
    pub fn region(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        mut f: impl FnMut(f32, f32) -> Option<(C, f32)>,
    ) {
        let (a, b, c, d) = self.bounds(x0, y0, x1, y1);
        for y in b..d {
            let py = y as f32 + 0.5;
            for x in a..c {
                if let Some((col, al)) = f(x as f32 + 0.5, py) {
                    self.blend(x, y, col, al);
                }
            }
        }
    }

    /// Like `region`, but the colour is added as light.
    pub fn light_region(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        mut f: impl FnMut(f32, f32) -> Option<(C, f32)>,
    ) {
        let (a, b, c, d) = self.bounds(x0, y0, x1, y1);
        for y in b..d {
            let py = y as f32 + 0.5;
            for x in a..c {
                if let Some((col, al)) = f(x as f32 + 0.5, py) {
                    self.screen(x, y, col, al);
                }
            }
        }
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: C, a: f32) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let r = rx.min(ry);
        self.region(
            cx - rx - 1.0,
            cy - ry - 1.0,
            cx + rx + 1.0,
            cy + ry + 1.0,
            |x, y| {
                let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
                let d = (dx * dx + dy * dy).sqrt();
                let cover = ((1.0 - d) * r + 0.5).clamp(0.0, 1.0);
                (cover > 0.0).then_some((c, cover * a))
            },
        );
    }

    pub fn disk(&mut self, cx: f32, cy: f32, r: f32, c: C, a: f32) {
        self.ellipse(cx, cy, r, r, c, a);
    }

    /// Soft light with a smooth falloff, added with screen blending.
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, c: C, a: f32) {
        self.glow_ellipse(cx, cy, r, r, c, a);
    }

    pub fn glow_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: C, a: f32) {
        if rx <= 0.0 || ry <= 0.0 || a <= 0.0 {
            return;
        }
        self.light_region(cx - rx, cy - ry, cx + rx, cy + ry, |x, y| {
            let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
            let d2 = dx * dx + dy * dy;
            if d2 >= 1.0 {
                return None;
            }
            let f = (1.0 - d2).powi(3);
            Some((c, f * a))
        });
    }

    /// A filled convex polygon with antialiased edges.
    pub fn poly(&mut self, pts: &[[f32; 2]], c: C, a: f32) {
        if pts.len() < 3 {
            return;
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pts {
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        if maxx < 0.0 || maxy < 0.0 || minx > self.w as f32 || miny > self.h as f32 {
            return;
        }
        let mut area = 0.0;
        for i in 0..pts.len() {
            let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
            area += p[0] * q[1] - q[0] * p[1];
        }
        let sign = if area >= 0.0 { 1.0 } else { -1.0 };
        let edges: Vec<[f32; 4]> = (0..pts.len())
            .filter_map(|i| {
                let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len = (dx * dx + dy * dy).sqrt();
                (len > 1e-4).then(|| [p[0], p[1], dx / len * sign, dy / len * sign])
            })
            .collect();
        self.region(minx - 1.0, miny - 1.0, maxx + 1.0, maxy + 1.0, |px, py| {
            let mut d = f32::MAX;
            for e in &edges {
                d = d.min((py - e[1]) * e[2] - (px - e[0]) * e[3]);
                if d < -1.0 {
                    return None;
                }
            }
            let cover = (d + 0.5).clamp(0.0, 1.0);
            (cover > 0.0).then_some((c, cover * a))
        });
    }

    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: C, a: f32) {
        self.poly(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]], c, a);
    }

    /// A stroke that narrows from `wa` to `wb`, with round ends.
    pub fn taper(&mut self, a: [f32; 2], b: [f32; 2], wa: f32, wb: f32, c: C, alpha: f32) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        let (ra, rb) = (wa * 0.5, wb * 0.5);
        let r = ra.max(rb);
        self.region(
            a[0].min(b[0]) - r - 1.0,
            a[1].min(b[1]) - r - 1.0,
            a[0].max(b[0]) + r + 1.0,
            a[1].max(b[1]) + r + 1.0,
            |x, y| {
                let t = (((x - a[0]) * dx + (y - a[1]) * dy) / len2).clamp(0.0, 1.0);
                let (qx, qy) = (a[0] + dx * t - x, a[1] + dy * t - y);
                let d = (qx * qx + qy * qy).sqrt();
                let rr = ra + (rb - ra) * t;
                let cover = (rr - d + 0.5).clamp(0.0, 1.0);
                (cover > 0.0).then_some((c, cover * alpha))
            },
        );
    }

    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, c: C, alpha: f32) {
        self.taper(a, b, width, width, c, alpha);
    }

    /// Fills everything below `top(x)` with `color(x, y, depth)`, where
    /// depth is the distance below the edge. Antialiases the edge.
    pub fn layer(&mut self, top: impl Fn(f32) -> f32, color: impl Fn(f32, f32, f32) -> C, a: f32) {
        self.layer_to(top, f32::MAX, color, a);
    }

    /// Like `layer`, but stops at `bottom` (in pixels): whatever is drawn
    /// in front later covers the rest, so there is no need to paint it.
    pub fn layer_to(
        &mut self,
        top: impl Fn(f32) -> f32,
        bottom: f32,
        color: impl Fn(f32, f32, f32) -> C,
        a: f32,
    ) {
        let end = (bottom.ceil().max(0.0) as usize).min(self.h);
        for x in 0..self.w {
            let px = x as f32 + 0.5;
            let t = top(px);
            let start = (t.floor().max(0.0) as usize).min(end);
            for y in start..end {
                let py = y as f32 + 0.5;
                let cover = (py - t + 0.5).clamp(0.0, 1.0);
                self.blend(x, y, color(px, py, py - t), cover * a);
            }
        }
    }

    /// Fills the band between `top(x)` and `bottom(x)`.
    pub fn band(
        &mut self,
        top: impl Fn(f32) -> f32,
        bottom: impl Fn(f32) -> f32,
        color: impl Fn(f32, f32, f32) -> C,
        a: f32,
    ) {
        for x in 0..self.w {
            let px = x as f32 + 0.5;
            let (t, b) = (top(px), bottom(px));
            if b <= t {
                continue;
            }
            let y0 = (t.floor().max(0.0) as usize).min(self.h);
            let y1 = (b.ceil().max(0.0) as usize).min(self.h);
            for y in y0..y1 {
                let py = y as f32 + 0.5;
                let cover = (py - t + 0.5).clamp(0.0, 1.0) * (b - py + 0.5).clamp(0.0, 1.0);
                self.blend(x, y, color(px, py, py - t), cover * a);
            }
        }
    }
}
