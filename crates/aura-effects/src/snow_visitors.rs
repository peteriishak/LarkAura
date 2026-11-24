//! The visitors that come with snow, one at a time and only now and then: a
//! penguin in a knitted scarf, a red fox that hears something under the
//! snow and dives in after it, and a snowy owl that brings a tiny present
//! and leaves it on a window. They are Blender sprites (see
//! `art/visitors/build.py`) and they leave footprints, dents, grooves and
//! puffs in the snow they cross.

use crate::{
    Debris, DebrisKind, Painter, SHAPE_DOME, SHAPE_SNOWBANK, SHAPE_SOFT, Simulation, landing,
    sprites::{self, anchor},
};
use aura_core::Rect;
use std::f32::consts::PI;

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const SHADOW: [f32; 3] = [0.13, 0.13, 0.2];

const PENGUIN_SPEED: f32 = 42.0;
const PENGUIN_SLIDE_SPEED: f32 = 150.0;
/// Pixels walked per full waddle, two steps.
const PENGUIN_STRIDE: f32 = 19.0;

const FOX_SPEED: f32 = 78.0;
/// Pixels trotted per full gait cycle.
const FOX_STRIDE: f32 = 46.0;
const FOX_LISTEN: f32 = 2.4;
const FOX_CROUCH: f32 = 0.85;
const FOX_LEAP: f32 = 0.8;
const FOX_LEAP_LENGTH: f32 = 84.0;
const FOX_BURIED: f32 = 1.8;
const FOX_OUT: f32 = 0.55;
/// Half the width of the mound of snow over a diving fox.
const MOUND: f32 = 30.0;

const OWL_GLIDE_SPEED: f32 = 165.0;
const OWL_STARTLE_RADIUS: f32 = 110.0;
const GIFT_STAY: f32 = 40.0;
const GIFT_SCALE: f32 = 0.75;

#[derive(Clone, Debug)]
pub(crate) struct Penguin {
    pub x: f32,
    dir: f32,
    slide: f32,
    walked: f32,
    print: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FoxDo {
    Trot,
    Listen,
    Crouch,
    Leap,
    Buried,
    Out,
}

#[derive(Clone, Debug)]
pub(crate) struct Fox {
    x: f32,
    dir: f32,
    act: FoxDo,
    t: f32,
    /// Where it stops to listen.
    stop_x: f32,
    /// It has dived once and is on its way out.
    done: bool,
    gait: f32,
    print: f32,
    from: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OwlDo {
    Arrive,
    Perch,
    Leave,
}

#[derive(Clone, Debug)]
pub(crate) struct Owl {
    x: f32,
    y: f32,
    dir: f32,
    act: OwlDo,
    t: f32,
    from: [f32; 2],
    to: [f32; 2],
    duration: f32,
    /// The window it perches on, or `None` for the snow on the ground.
    window: Option<usize>,
    /// The top of that window when it landed, to notice it moving.
    top: f32,
    stay: f32,
    flap: f32,
    vx: f32,
    vy: f32,
    seed: f32,
}

/// The present the owl leaves behind.
#[derive(Clone, Debug)]
pub(crate) struct Gift {
    x: f32,
    y: f32,
    window: Option<usize>,
    top: f32,
    vy: f32,
    age: f32,
}

#[cfg(test)]
impl Fox {
    pub fn x(&self) -> f32 {
        self.x
    }

    pub fn buried(&self) -> bool {
        self.act == FoxDo::Buried
    }
}

#[cfg(test)]
impl Owl {
    pub fn at(&self) -> [f32; 2] {
        [self.x, self.y]
    }

    pub fn perched(&self) -> bool {
        self.act == OwlDo::Perch
    }
}

#[cfg(test)]
impl Gift {
    pub fn window(&self) -> Option<usize> {
        self.window
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Simulation {
    /// Where a small creature's feet are at column `x` on the ground: a
    /// little way into the snow, as feet sink in.
    fn ground_at(&self, x: f32) -> f32 {
        self.height - 2.0 - self.piles.floor_height(x) * 0.6
    }

    /// A burst of snow kicked up at (x, y).
    fn snow_burst(&mut self, x: f32, y: f32, flakes: usize, puffs: usize, up: f32) {
        for _ in 0..flakes {
            let flake = Debris::plain(
                x + self.rng.range(-10.0, 10.0),
                y - self.rng.range(0.0, 6.0),
                self.rng.range(-80.0, 80.0),
                -self.rng.range(up * 0.4, up),
                self.rng.range(0.5, 1.0),
                self.rng.range(1.0, 2.2),
                DebrisKind::Flake,
            );
            self.add_debris(flake);
        }
        for _ in 0..puffs {
            let puff = Debris::plain(
                x + self.rng.range(-14.0, 14.0),
                y - self.rng.range(2.0, 12.0),
                self.rng.range(-30.0, 30.0),
                -self.rng.range(10.0, 35.0),
                self.rng.range(0.7, 1.2),
                self.rng.range(6.0, 11.0),
                DebrisKind::Powder,
            );
            self.add_debris(puff);
        }
    }

    /// Picks the next snow visitor, taking turns.
    pub(crate) fn snow_critter(&mut self, surfaces: &[Rect]) -> crate::visitors::Critter {
        use crate::visitors::Critter;
        let w = self.width;
        let dir = if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
        let edge = |margin: f32| if dir > 0.0 { -margin } else { w + margin };
        let turn = self.visitors.turn;
        self.visitors.turn = turn.wrapping_add(1);
        match turn % 3 {
            0 => Critter::Penguin(Penguin {
                x: edge(40.0),
                dir,
                slide: 0.0,
                walked: 0.0,
                print: 0.0,
            }),
            1 => Critter::Fox(Fox {
                x: edge(70.0),
                dir,
                act: FoxDo::Trot,
                t: 0.0,
                stop_x: w * self.rng.range(0.3, 0.7),
                done: false,
                gait: 0.0,
                print: 0.0,
                from: 0.0,
            }),
            _ => Critter::Owl(self.new_owl(surfaces, dir, edge(80.0))),
        }
    }

    /// An owl on its way in, aiming for the top of a small window, or for
    /// the snow on the ground when there is none.
    fn new_owl(&mut self, surfaces: &[Rect], dir: f32, start_x: f32) -> Owl {
        let (w, h) = (self.width, self.height);
        let mut perches = Vec::new();
        for (i, r) in surfaces.iter().enumerate() {
            let small = r.w > 140.0 && r.w < w * 0.7 && !r.is_large(w, h);
            let placed = r.y > 150.0 && r.y < h - 120.0 && r.x > 0.0 && r.right() < w;
            if !small || !placed || !self.ledges.get(i).copied().unwrap_or(true) {
                continue;
            }
            let x = self.rng.range(r.x + 40.0, r.right() - 40.0);
            if landing(surfaces, x, 0.0, h).is_some_and(|(found, _)| found == i) {
                perches.push((i, x, r.y));
            }
        }
        let (window, x, top) = if perches.is_empty() {
            let x = w * self.rng.range(0.25, 0.75);
            (None, x, self.ground_at(x))
        } else {
            let pick = perches[(self.rng.next() * perches.len() as f32) as usize % perches.len()];
            (Some(pick.0), pick.1, pick.2)
        };
        let from = [start_x, (top - 220.0).clamp(60.0, h * 0.4)];
        let to = [x, top];
        let dist = (to[0] - from[0]).hypot(to[1] - from[1]);
        Owl {
            x: from[0],
            y: from[1],
            dir,
            act: OwlDo::Arrive,
            t: 0.0,
            from,
            to,
            duration: (dist / OWL_GLIDE_SPEED).clamp(2.5, 7.0),
            window,
            top,
            stay: self.rng.range(12.0, 18.0),
            flap: 0.0,
            vx: 0.0,
            vy: 0.0,
            seed: self.rng.range(0.0, 10.0),
        }
    }

    pub(crate) fn step_penguin(&mut self, dt: f32, p: &mut Penguin) -> bool {
        if p.slide > 0.0 {
            p.slide -= dt;
            p.x += p.dir * PENGUIN_SLIDE_SPEED * dt;
            // Its belly presses a groove into the snow and throws a little up.
            let moved = self.piles.pack_floor_near(p.x, 7.0, 0.8, f32::INFINITY);
            if moved > 0.2 || self.rng.next() < dt * 10.0 {
                let flake = Debris::plain(
                    p.x - p.dir * 14.0,
                    self.ground_at(p.x) - 4.0,
                    -p.dir * self.rng.range(30.0, 90.0),
                    -self.rng.range(40.0, 110.0),
                    self.rng.range(0.4, 0.8),
                    self.rng.range(1.2, 2.4),
                    DebrisKind::Flake,
                );
                self.add_debris(flake);
            }
            if p.slide <= 0.0 {
                // Back on its feet with a shake.
                self.snow_burst(p.x, self.ground_at(p.x) - 14.0, 5, 1, 70.0);
            }
        } else {
            let step = PENGUIN_SPEED * dt;
            p.x += p.dir * step;
            p.walked += step;
            p.print += step;
            if p.print > PENGUIN_STRIDE * 0.5 {
                // A footprint, left and right in turn.
                p.print = 0.0;
                let side = if (p.walked / PENGUIN_STRIDE).fract() < 0.5 {
                    3.0
                } else {
                    -3.0
                };
                self.piles.dent_floor(p.x + side, 1.6);
            }
            let deep = self.piles.floor_height(p.x) > 6.0;
            let chance = if deep { 0.25 } else { 0.06 };
            if self.rng.next() < dt * chance {
                p.slide = self.rng.range(1.2, 2.2);
                self.snow_burst(p.x + p.dir * 10.0, self.ground_at(p.x), 6, 1, 90.0);
            }
        }
        p.x > -60.0 && p.x < self.width + 60.0
    }

    pub(crate) fn step_fox(&mut self, dt: f32, f: &mut Fox) -> bool {
        f.t += dt;
        match f.act {
            FoxDo::Trot => {
                let step = FOX_SPEED * dt;
                let before = f.x;
                f.x += f.dir * step;
                f.gait += step;
                f.print += step;
                if f.print > FOX_STRIDE * 0.25 {
                    // Foxes step into their own prints, in a neat line.
                    f.print = 0.0;
                    self.piles.dent_floor(f.x, 1.2);
                }
                let crossed = (before - f.stop_x) * (f.x - f.stop_x) <= 0.0;
                if !f.done && crossed {
                    f.act = FoxDo::Listen;
                    f.t = 0.0;
                }
            }
            FoxDo::Listen if f.t > FOX_LISTEN => {
                f.act = FoxDo::Crouch;
                f.t = 0.0;
            }
            FoxDo::Crouch if f.t > FOX_CROUCH => {
                f.act = FoxDo::Leap;
                f.t = 0.0;
                f.from = f.x;
                self.snow_burst(f.x - f.dir * 20.0, self.ground_at(f.x), 6, 1, 60.0);
            }
            FoxDo::Leap => {
                let u = (f.t / FOX_LEAP).min(1.0);
                f.x = f.from + f.dir * FOX_LEAP_LENGTH * u;
                if u >= 1.0 {
                    f.act = FoxDo::Buried;
                    f.t = 0.0;
                    let surface = self.height - self.piles.floor_height(f.x);
                    self.snow_burst(f.x, surface, 18, 4, 170.0);
                    for k in -3..=3 {
                        self.piles.dent_floor(f.x + k as f32 * 4.0, 4.0);
                    }
                }
            }
            FoxDo::Buried => {
                if self.rng.next() < dt * 3.0 {
                    let surface = self.height - self.piles.floor_height(f.x);
                    self.snow_burst(f.x + f.dir * 8.0, surface - 10.0, 2, 0, 50.0);
                }
                if f.t > FOX_BURIED {
                    f.act = FoxDo::Out;
                    f.t = 0.0;
                    f.done = true;
                    let surface = self.height - self.piles.floor_height(f.x);
                    self.snow_burst(f.x, surface, 14, 3, 140.0);
                }
            }
            FoxDo::Out => {
                f.x += f.dir * 60.0 * dt;
                if f.t > FOX_OUT {
                    f.act = FoxDo::Trot;
                    f.t = 0.0;
                    self.snow_burst(f.x, self.ground_at(f.x), 5, 1, 60.0);
                }
            }
            FoxDo::Listen | FoxDo::Crouch => {}
        }
        f.x > -90.0 && f.x < self.width + 90.0
    }

    pub(crate) fn step_owl(&mut self, dt: f32, surfaces: &[Rect], o: &mut Owl) -> bool {
        o.t += dt;
        match o.act {
            OwlDo::Arrive => {
                let u = (o.t / o.duration).min(1.0);
                // Fast at first, then slowing to land.
                let e = 1.0 - (1.0 - u) * (1.0 - u);
                let c = [
                    lerp(o.from[0], o.to[0], 0.55),
                    o.from[1].min(o.to[1]) - 30.0,
                ];
                let v = 1.0 - e;
                o.x = v * v * o.from[0] + 2.0 * v * e * c[0] + e * e * o.to[0];
                o.y = v * v * o.from[1] + 2.0 * v * e * c[1] + e * e * o.to[1];
                // Glide in the middle of the approach, flap hard to brake.
                let rate = if u > 0.78 {
                    3.4
                } else if (0.3..0.62).contains(&u) {
                    0.0
                } else {
                    2.2
                };
                o.flap += dt * rate;
                if u >= 1.0 {
                    self.owl_lands(surfaces, o);
                }
            }
            OwlDo::Perch => {
                let (top, ground) = match o.window {
                    Some(i) => match surfaces.get(i) {
                        Some(r) if o.x >= r.x && o.x < r.right() => (r.y, false),
                        _ => (f32::NAN, false),
                    },
                    None => (self.ground_at(o.x), true),
                };
                // A window that went away leaves `top` NaN, so it never
                // counts as still there.
                let still_there = (top - o.top).abs() < 1.5;
                let moved = !ground && !still_there;
                let startled = self
                    .pointer
                    .is_some_and(|c| (c.x - o.x).hypot(c.y - (o.y - 30.0)) < OWL_STARTLE_RADIUS);
                if !moved {
                    o.y = match o.window {
                        Some(i) => top - self.piles.depth_on(i, o.x) * 0.7,
                        None => top,
                    };
                }
                if moved || startled || o.t > o.stay {
                    o.act = OwlDo::Leave;
                    o.t = 0.0;
                    o.vx = o.dir * 40.0;
                    o.vy = -120.0;
                    if let Some(i) = o.window.filter(|_| !moved) {
                        let shed = self.piles.knock_off(i, o.x, 16.0, 0.4);
                        self.throw_snow(&shed);
                    }
                    self.snow_burst(o.x, o.y, 6, 1, 60.0);
                }
            }
            OwlDo::Leave => {
                o.vx = (o.vx + o.dir * 220.0 * dt).clamp(-210.0, 210.0);
                o.vy = (o.vy + 50.0 * dt).min(-30.0);
                o.x += o.vx * dt;
                o.y += o.vy * dt;
                o.flap += dt * 3.0;
                return o.x > -150.0 && o.x < self.width + 150.0 && o.y > -160.0;
            }
        }
        true
    }

    fn owl_lands(&mut self, surfaces: &[Rect], o: &mut Owl) {
        o.act = OwlDo::Perch;
        o.t = 0.0;
        // Landing knocks snow off the window around its feet.
        match o.window {
            Some(i) if surfaces.get(i).is_some_and(|r| (r.y - o.top).abs() < 1.5) => {
                let shed = self.piles.knock_off(i, o.x, 22.0, 0.7);
                self.throw_snow(&shed);
            }
            Some(_) => {
                // The window moved while it flew in: it lands on the ground.
                o.window = None;
            }
            None => {
                for k in -2..=2 {
                    self.piles.dent_floor(o.x + k as f32 * 4.0, 2.5);
                }
            }
        }
        if o.window.is_none() {
            o.top = self.ground_at(o.x);
        }
        o.y = o.top;
        self.snow_burst(o.x, o.y, 8, 2, 70.0);
        // It sets its present down beside it.
        let gx = o.x + o.dir * 24.0;
        let on_window = o
            .window
            .and_then(|i| surfaces.get(i))
            .is_some_and(|r| gx > r.x + 6.0 && gx < r.right() - 6.0);
        self.visitors.gift = Some(Gift {
            x: gx,
            y: o.top,
            window: if on_window { o.window } else { None },
            top: if on_window { o.top } else { self.ground_at(gx) },
            vy: 0.0,
            age: 0.0,
        });
    }

    pub(crate) fn step_gift(&mut self, dt: f32, surfaces: &[Rect]) {
        let Some(mut g) = self.visitors.gift.take() else {
            return;
        };
        g.age += dt;
        if let Some(i) = g.window {
            let still = surfaces
                .get(i)
                .is_some_and(|r| (r.y - g.top).abs() < 1.5 && g.x >= r.x && g.x < r.right());
            if still {
                g.y = g.top - self.piles.depth_on(i, g.x) * 0.6;
            } else {
                // The window moved away, so the present tumbles down.
                g.window = None;
                g.vy = 0.0;
            }
        }
        if g.window.is_none() {
            let floor = self.ground_at(g.x);
            if g.y < floor - 0.5 {
                g.vy += 900.0 * dt;
                g.y = (g.y + g.vy * dt).min(floor);
                if g.y >= floor {
                    self.snow_burst(g.x, floor, 6, 1, 60.0);
                }
            } else {
                g.y = floor;
            }
        }
        if g.age < GIFT_STAY + 2.0 {
            self.visitors.gift = Some(g);
        }
    }

    pub(crate) fn draw_penguin(&self, paint: &mut Painter<'_>, p: &Penguin) {
        let ground = self.ground_at(p.x);
        paint.put(p.x, ground, 18.0, 3.5, 0.0, SHAPE_SOFT, SHADOW, 0.22);
        let frame = if p.slide > 0.0 {
            sprites::PENGUIN + sprites::PENGUIN_FRAMES - 1
        } else {
            let cycle = (p.walked / PENGUIN_STRIDE).fract();
            sprites::PENGUIN + (cycle * (sprites::PENGUIN_FRAMES - 1) as f32) as usize
        };
        paint.sprite_facing(frame, p.x, ground, 1.0, 0.0, p.dir, WHITE, 1.0);
    }

    pub(crate) fn draw_fox(&self, paint: &mut Painter<'_>, f: &Fox) {
        let ground = self.ground_at(f.x);
        let dir = f.dir;
        let shadow = |paint: &mut Painter<'_>, x: f32, k: f32| {
            paint.put(
                x,
                ground,
                34.0 * k,
                4.5 * k,
                0.0,
                SHAPE_SOFT,
                SHADOW,
                0.22 * k,
            );
        };
        match f.act {
            FoxDo::Trot => {
                shadow(paint, f.x, 1.0);
                let cycle = (f.gait / FOX_STRIDE).fract();
                let frame = sprites::FOX + (cycle * 6.0) as usize % 6;
                paint.sprite_facing(frame, f.x, ground, 1.0, 0.0, dir, WHITE, 1.0);
            }
            FoxDo::Listen => {
                shadow(paint, f.x, 1.0);
                paint.sprite_facing(sprites::FOX + 6, f.x, ground, 1.0, 0.0, dir, WHITE, 1.0);
            }
            FoxDo::Crouch => {
                shadow(paint, f.x, 1.0);
                // The little wiggle before the pounce.
                let wiggle = (f.t * 28.0).sin() * 0.8 * (f.t / FOX_CROUCH);
                let x = f.x + wiggle;
                paint.sprite_facing(sprites::FOX + 7, x, ground, 1.0, 0.0, dir, WHITE, 1.0);
            }
            FoxDo::Leap => {
                let u = (f.t / FOX_LEAP).min(1.0);
                let lift = 4.0 * u * (1.0 - u) * 72.0;
                shadow(paint, f.x, 1.0 - u * (1.0 - u) * 2.0);
                let frame = sprites::FOX_AIR + usize::from(u > 0.5);
                let rot = dir * (u - 0.5) * 0.35;
                paint.sprite_facing(frame, f.x, ground - lift, 1.0, rot, dir, WHITE, 1.0);
            }
            FoxDo::Buried => {
                let surface = self.height - self.piles.floor_height(f.x);
                // Only its back legs and tail stick out, and the tail waves.
                let wag = (self.clock * 9.0).sin() * 0.05 * dir;
                paint.sprite_facing(
                    sprites::FOX_AIR + 1,
                    f.x,
                    surface + 4.0,
                    1.0,
                    wag,
                    dir,
                    WHITE,
                    1.0,
                );
                self.draw_mound(paint, f.x, 1.0);
            }
            FoxDo::Out => {
                let u = (f.t / FOX_OUT).min(1.0);
                let lift = (u * PI).sin() * 26.0;
                shadow(paint, f.x, 0.8);
                let rot = -dir * (1.0 - u) * 0.3;
                paint.sprite_facing(
                    sprites::FOX_AIR,
                    f.x,
                    ground - lift,
                    1.0,
                    rot,
                    dir,
                    WHITE,
                    1.0,
                );
                self.draw_mound(paint, f.x - dir * FOX_OUT * 60.0 * u, 1.0 - u);
            }
        }
    }

    /// Snow heaped over a fox that has dived in. Below the surface it is
    /// drawn as the ground's own snowbank, flat-topped so it shades the
    /// same, which hides the fox's head; above, a soft heap rises.
    fn draw_mound(&self, paint: &mut Painter<'_>, x: f32, k: f32) {
        if k <= 0.0 {
            return;
        }
        const COLUMNS: usize = 6;
        let width = MOUND * 2.0 / COLUMNS as f32;
        for c in 0..COLUMNS {
            let cx = x - MOUND + (c as f32 + 0.5) * width;
            let h = self.piles.floor_height(cx);
            let top = h + 1.5;
            paint.put_extra(
                cx,
                self.height - top * 0.5,
                width * 0.5 + 0.5,
                top * 0.5,
                SHAPE_SNOWBANK,
                [h / top, h / top],
                self.aura.color,
                1.0,
            );
        }
        let surface = self.height - self.piles.floor_height(x);
        let color = self.aura.color.map(|c| c * 0.97);
        for (dx, r) in [(-0.5, 0.55), (0.0, 0.75), (0.5, 0.5)] {
            let r = r * 20.0 * k;
            paint.put(
                x + dx * MOUND,
                surface + 1.0,
                r * 1.5,
                r,
                0.0,
                SHAPE_DOME,
                color,
                1.0,
            );
        }
    }

    pub(crate) fn draw_owl(&self, paint: &mut Painter<'_>, o: &Owl) {
        let perch_layer = self.masks.layer_for(o.window);
        match o.act {
            OwlDo::Perch => {
                paint.layer = perch_layer;
                paint.put(o.x, o.y, 14.0, 2.5, 0.0, SHAPE_SOFT, SHADOW, 0.2);
                // It blinks every few seconds and now and then turns its head
                // right round to look at you.
                let blink = (self.clock * 0.31 + o.seed).fract() < 0.05;
                let look = (o.t + o.seed) % 7.0 > 4.2 && (o.t + o.seed) % 7.0 < 6.0;
                let frame = sprites::OWL + if look { 2 } else { usize::from(blink) };
                paint.sprite_facing(frame, o.x, o.y, 1.1, 0.0, o.dir, WHITE, 1.0);
            }
            OwlDo::Arrive | OwlDo::Leave => {
                // Over the last stretch it is already above its perch.
                let landing = o.act == OwlDo::Arrive && o.t > o.duration * 0.85;
                paint.layer = if landing { perch_layer } else { 0.0 };
                let f =
                    (o.flap * sprites::OWL_FLY_FRAMES as f32) as usize % sprites::OWL_FLY_FRAMES;
                let gliding = o.act == OwlDo::Arrive && {
                    let u = o.t / o.duration;
                    (0.3..0.62).contains(&u)
                };
                let f = if gliding { 1 } else { f };
                let [tx, ty] = anchor::OWL_FLY_TALONS[f];
                // The talons follow the path, so it lands on its feet.
                let (x, y) = (o.x - tx * o.dir, o.y - ty);
                let tilt = if o.act == OwlDo::Leave {
                    -o.dir * 0.12
                } else {
                    0.0
                };
                paint.sprite_facing(sprites::OWL_FLY + f, x, y, 1.0, tilt, o.dir, WHITE, 1.0);
                if o.act == OwlDo::Arrive {
                    paint.sprite(
                        sprites::GIFT,
                        o.x,
                        o.y + 8.0,
                        GIFT_SCALE * 0.9,
                        0.0,
                        WHITE,
                        1.0,
                    );
                }
            }
        }
        paint.layer = 0.0;
    }

    pub(crate) fn draw_gift(&self, paint: &mut Painter<'_>) {
        let Some(g) = &self.visitors.gift else {
            return;
        };
        let fade = (GIFT_STAY + 2.0 - g.age).clamp(0.0, 2.0) * 0.5;
        paint.layer = self.masks.layer_for(g.window);
        paint.put(g.x, g.y, 10.0, 2.0, 0.0, SHAPE_SOFT, SHADOW, 0.2 * fade);
        paint.sprite(sprites::GIFT, g.x, g.y, GIFT_SCALE, 0.0, WHITE, fade);
        paint.layer = 0.0;
    }
}
