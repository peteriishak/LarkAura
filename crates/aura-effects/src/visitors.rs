//! Small visitors that belong to each kind of weather. In the snow a
//! penguin, a fox or an owl comes by (see `snow_visitors`); a frog hops across window tops in the rain, butterflies
//! rest on windows among the petals, a whirlwind lifts fallen leaves, a comet
//! crosses the night, fireflies draw a heart, a lantern floats through the fog
//! and a paper plane glides through the morning light.
//!
//! The others are drawn from simple shapes. None of them receive clicks.

use crate::snow_visitors::{Fox, Gift, Owl, Penguin};
use crate::{
    Debris, DebrisKind, Painter, SHAPE_DISK, SHAPE_DOME, SHAPE_GLOW, SHAPE_PETAL, SHAPE_PUFF,
    SHAPE_RECT, SHAPE_SOFT, SHAPE_STREAK, SHAPE_TRIANGLE, Simulation, landing,
};
use aura_core::{Frequency, Rect};
use aura_format::EffectKind;
use std::f32::consts::{PI, TAU};

const INK: [f32; 3] = [0.13, 0.13, 0.17];
const WHITE: [f32; 3] = [0.97, 0.97, 0.99];
const FROG: [f32; 3] = [0.38, 0.72, 0.32];
const FROG_DARK: [f32; 3] = [0.22, 0.5, 0.22];
const FROG_BELLY: [f32; 3] = [0.85, 0.93, 0.6];
const PAPER: [f32; 3] = [0.97, 0.96, 0.92];
const PAPER_SHADE: [f32; 3] = [0.62, 0.66, 0.76];
const WARM: [f32; 3] = [1.0, 0.78, 0.4];
const WING_COLORS: [[f32; 3]; 3] = [[0.98, 0.62, 0.22], [0.55, 0.72, 0.98], [0.98, 0.86, 0.36]];

/// How long a heart of fireflies holds its shape, in seconds.
pub(crate) const HEART_SECONDS: f32 = 7.0;
const BUTTERFLY_STAY: f32 = 45.0;
const LANTERN_CLEAR_RADIUS: f32 = 240.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Butterfly {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    rest: f32,
    seed: f32,
}

#[derive(Clone, Debug)]
pub(crate) enum Critter {
    Penguin(Penguin),
    Fox(Fox),
    Owl(Owl),
    Frog {
        from: [f32; 2],
        to: [f32; 2],
        hop: f32,
        rest: f32,
        dir: f32,
        croak: f32,
    },
    Whirlwind {
        x: f32,
        dir: f32,
        age: f32,
    },
    Butterflies {
        list: Vec<Butterfly>,
        age: f32,
    },
    Comet {
        x: f32,
        y: f32,
        vx: f32,
        vy: f32,
    },
    Plane {
        x: f32,
        y: f32,
        base: f32,
        dir: f32,
        age: f32,
        loop_x: f32,
        looping: Option<(f32, f32, f32)>,
        heading: f32,
    },
    Heart {
        x: f32,
        y: f32,
        age: f32,
    },
    Lantern {
        x: f32,
        base: f32,
        dir: f32,
        age: f32,
    },
    Umbrella {
        x: f32,
        y: f32,
        vx: f32,
        spin: f32,
        age: f32,
    },
}

pub(crate) struct Visitors {
    pub critter: Option<Critter>,
    /// A present the owl left, which stays a while after it has gone.
    pub gift: Option<Gift>,
    /// Which snow visitor comes next; they take turns.
    pub turn: u32,
    next: f32,
}

impl Visitors {
    pub fn new() -> Self {
        Self {
            critter: None,
            gift: None,
            turn: 0,
            next: 75.0,
        }
    }

    pub fn clear(&mut self) {
        self.critter = None;
        self.gift = None;
    }

    /// The whirlwind's position, if one is passing.
    pub fn whirl(&self) -> Option<f32> {
        match self.critter {
            Some(Critter::Whirlwind { x, .. }) => Some(x),
            _ => None,
        }
    }

    /// Where the fireflies' heart is and how far through it they are.
    pub fn heart(&self) -> Option<(f32, f32, f32)> {
        match self.critter {
            Some(Critter::Heart { x, y, age }) if age < HEART_SECONDS => Some((x, y, age)),
            _ => None,
        }
    }

    pub fn lantern(&self) -> Option<[f32; 2]> {
        match self.critter {
            Some(Critter::Lantern { x, base, age, .. }) => {
                Some([x, base + (age * 0.9).sin() * 14.0])
            }
            _ => None,
        }
    }
}

/// A point on a heart shape, roughly 32 units across, for `t` in 0..TAU.
pub(crate) fn heart_point(t: f32) -> [f32; 2] {
    let s = t.sin();
    [
        16.0 * s * s * s,
        -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos()),
    ]
}

fn rotate(dx: f32, dy: f32, angle: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    [dx * c - dy * s, dx * s + dy * c]
}

fn wait_for(frequency: Frequency) -> Option<f32> {
    match frequency {
        Frequency::Off => None,
        Frequency::Sometimes => Some(480.0),
        Frequency::Often => Some(150.0),
    }
}

impl Simulation {
    /// The surface a small creature would stand on at column `x`: the highest
    /// uncovered window top, or the floor.
    fn standing_y(&self, surfaces: &[Rect], x: f32) -> f32 {
        let floor = self.height - 2.0 - self.piles.floor_height(x) * 0.6;
        landing(surfaces, x, 0.0, floor).map_or(floor, |(_, y)| y)
    }

    pub(crate) fn send_visitor(&mut self, surfaces: &[Rect]) {
        let (w, h) = (self.width, self.height);
        let dir = if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
        let edge = |dir: f32, margin: f32| if dir > 0.0 { -margin } else { w + margin };
        let critter = match self.aura.effect {
            EffectKind::Snow => self.snow_critter(surfaces),
            EffectKind::Rain if self.aura.lightning => Critter::Umbrella {
                x: edge(dir, 60.0),
                y: h * self.rng.range(0.25, 0.5),
                vx: dir * self.rng.range(170.0, 230.0),
                spin: 0.0,
                age: 0.0,
            },
            EffectKind::Rain => {
                let x = edge(dir, 30.0);
                let y = self.standing_y(surfaces, x.clamp(0.0, w - 1.0));
                Critter::Frog {
                    from: [x, y],
                    to: [x, y],
                    hop: 1.0,
                    rest: 0.3,
                    dir,
                    croak: 0.0,
                }
            }
            EffectKind::Leaves => Critter::Whirlwind {
                x: edge(dir, 60.0),
                dir,
                age: 0.0,
            },
            EffectKind::Blossom => {
                let list = (0..3)
                    .map(|i| Butterfly {
                        x: edge(dir, 30.0 + i as f32 * 50.0),
                        y: h * self.rng.range(0.3, 0.7),
                        vx: dir * 60.0,
                        vy: 0.0,
                        rest: 0.0,
                        seed: self.rng.range(0.0, TAU),
                    })
                    .collect();
                Critter::Butterflies { list, age: 0.0 }
            }
            EffectKind::Starfall => Critter::Comet {
                x: edge(dir, 120.0),
                y: h * self.rng.range(0.04, 0.22),
                vx: dir * self.rng.range(190.0, 250.0),
                vy: self.rng.range(25.0, 60.0),
            },
            EffectKind::Fireflies => {
                let (x, y) = match self.pointer.filter(|c| c.rest > 0.5) {
                    Some(c) => (c.x, c.y),
                    None => (w * self.rng.range(0.3, 0.7), h * self.rng.range(0.35, 0.6)),
                };
                Critter::Heart { x, y, age: 0.0 }
            }
            EffectKind::Fog => Critter::Lantern {
                x: edge(dir, 50.0),
                base: h * self.rng.range(0.6, 0.8),
                dir,
                age: 0.0,
            },
            EffectKind::Motes => {
                let base = h * self.rng.range(0.25, 0.5);
                Critter::Plane {
                    x: edge(dir, 50.0),
                    y: base,
                    base,
                    dir,
                    age: 0.0,
                    loop_x: w * self.rng.range(0.3, 0.7),
                    looping: None,
                    heading: if dir > 0.0 { 0.0 } else { PI },
                }
            }
        };
        self.visitors.critter = Some(critter);
    }

    pub(crate) fn step_visitor_schedule(&mut self, dt: f32, surfaces: &[Rect]) {
        // One visitor at a time, and none while the sleigh or plough is out.
        if self.visitors.critter.is_some() || self.scenery.busy() {
            return;
        }
        self.visitors.next -= dt;
        if self.visitors.next > 0.0 {
            return;
        }
        let wait = wait_for(self.scenery.decorations.visitors);
        if wait.is_some() && self.presence > 0.5 {
            self.send_visitor(surfaces);
        }
        self.visitors.next = wait.unwrap_or(300.0) * self.rng.range(0.8, 1.3);
    }

    pub(crate) fn step_visitors_motion(&mut self, dt: f32, surfaces: &[Rect]) {
        self.step_gift(dt, surfaces);
        let Some(mut critter) = self.visitors.critter.take() else {
            return;
        };
        let keep = match &mut critter {
            Critter::Penguin(p) => self.step_penguin(dt, p),
            Critter::Fox(f) => self.step_fox(dt, f),
            Critter::Owl(o) => self.step_owl(dt, surfaces, o),
            Critter::Frog {
                from,
                to,
                hop,
                rest,
                dir,
                croak,
            } => self.step_frog(dt, surfaces, from, to, hop, rest, *dir, croak),
            Critter::Whirlwind { x, dir, age } => {
                *age += dt;
                *x += *dir * (60.0 + (*age * 0.7).sin() * 25.0) * dt;
                if self.rng.next() < dt * 20.0 {
                    let gust = Debris::plain(
                        *x + self.rng.range(-20.0, 20.0),
                        self.height - self.rng.range(2.0, 10.0),
                        self.rng.range(-60.0, 60.0),
                        -self.rng.range(20.0, 60.0),
                        self.rng.range(0.4, 0.9),
                        self.rng.range(0.8, 1.6),
                        DebrisKind::Dust,
                    );
                    self.add_debris(gust);
                }
                *x > -120.0 && *x < self.width + 120.0
            }
            Critter::Butterflies { list, age } => {
                *age += dt;
                self.step_butterflies(dt, surfaces, list, *age)
            }
            Critter::Comet { x, y, vx, vy } => {
                let (x0, y0) = (*x, *y);
                *x += *vx * dt;
                *y += *vy * dt;
                for _ in 0..2 {
                    if self.rng.next() < dt * 40.0 {
                        let spark = Debris::plain(
                            x0 + self.rng.range(-3.0, 3.0),
                            y0 + self.rng.range(-3.0, 3.0),
                            -*vx * 0.08 + self.rng.range(-20.0, 20.0),
                            self.rng.range(-10.0, 25.0),
                            self.rng.range(0.6, 1.4),
                            self.rng.range(0.7, 1.4),
                            DebrisKind::Spark,
                        );
                        self.add_debris(spark);
                    }
                }
                *x > -400.0 && *x < self.width + 400.0 && *y < self.height + 100.0
            }
            Critter::Plane {
                x,
                y,
                base,
                dir,
                age,
                loop_x,
                looping,
                heading,
            } => {
                *age += dt;
                let (px, py) = (*x, *y);
                if let Some((cx, cy, t)) = looping {
                    // A loop-the-loop: once round a circle, then on its way.
                    *t += dt / 2.4;
                    let r = 62.0;
                    let a = *t * TAU;
                    *x = *cx + a.sin() * r * *dir;
                    *y = *cy - (1.0 - a.cos()) * r;
                    if *t >= 1.0 {
                        *looping = None;
                        *loop_x = f32::NAN;
                    }
                } else {
                    *x += *dir * 95.0 * dt;
                    *y = *base + (*age * 0.8).sin() * 36.0 + (*age * 0.31).sin() * 18.0;
                    let crossing = (*dir > 0.0 && px < *loop_x && *x >= *loop_x)
                        || (*dir < 0.0 && px > *loop_x && *x <= *loop_x);
                    if crossing {
                        *looping = Some((*x, *y, 0.0));
                    }
                }
                let (dx, dy) = (*x - px, *y - py);
                if dx * dx + dy * dy > 0.01 {
                    *heading = dy.atan2(dx);
                }
                *x > -120.0 && *x < self.width + 120.0
            }
            Critter::Heart { age, .. } => {
                *age += dt;
                *age < HEART_SECONDS + 2.0
            }
            Critter::Lantern { x, dir, age, .. } => {
                *age += dt;
                *x += *dir * 32.0 * dt;
                *x > -80.0 && *x < self.width + 80.0
            }
            Critter::Umbrella {
                x,
                y,
                vx,
                spin,
                age,
            } => {
                *age += dt;
                *x += *vx * dt;
                *y += ((*age * 1.7).sin() * 70.0 + (*age * 4.1).cos() * 25.0) * dt;
                *spin += dt * (2.5 + (*age * 1.3).sin() * 2.0) * vx.signum();
                *x > -140.0 && *x < self.width + 140.0
            }
        };
        if keep {
            self.visitors.critter = Some(critter);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn step_frog(
        &mut self,
        dt: f32,
        surfaces: &[Rect],
        from: &mut [f32; 2],
        to: &mut [f32; 2],
        hop: &mut f32,
        rest: &mut f32,
        dir: f32,
        croak: &mut f32,
    ) -> bool {
        *croak = (*croak - dt).max(0.0);
        if *hop < 1.0 {
            *hop += dt / 0.55;
            if *hop >= 1.0 {
                *hop = 1.0;
                *rest = self.rng.range(0.5, 1.6);
                for _ in 0..4 {
                    let drop = Debris::plain(
                        to[0] + self.rng.range(-6.0, 6.0),
                        to[1] - 1.0,
                        self.rng.range(-70.0, 70.0),
                        -self.rng.range(50.0, 120.0),
                        self.rng.range(0.25, 0.45),
                        self.rng.range(0.8, 1.3),
                        DebrisKind::Drop,
                    );
                    self.add_debris(drop);
                }
                if self.rng.next() < 0.35 {
                    *croak = 0.9;
                }
            }
            return true;
        }
        // Resting where it landed. If the window it sat on moved away, it
        // drops to whatever is below.
        let below = self.standing_y(surfaces, to[0].clamp(0.0, self.width - 1.0));
        if (below - to[1]).abs() > 2.0 && to[0] >= 0.0 && to[0] < self.width {
            *from = *to;
            to[1] = below;
            *hop = 0.4;
            return true;
        }
        *rest -= dt;
        if *rest <= 0.0 {
            *from = *to;
            let x = to[0] + dir * self.rng.range(70.0, 140.0);
            let y = if x >= 0.0 && x < self.width {
                self.standing_y(surfaces, x)
            } else {
                self.height - 2.0
            };
            *to = [x, y];
            *hop = 0.0;
        }
        to[0] > -60.0 && to[0] < self.width + 60.0
    }

    fn step_butterflies(
        &mut self,
        dt: f32,
        surfaces: &[Rect],
        list: &mut [Butterfly],
        age: f32,
    ) -> bool {
        let leaving = age > BUTTERFLY_STAY;
        let (w, h) = (self.width, self.height);
        let pointer = self.pointer;
        let mut inside = false;
        for b in list.iter_mut() {
            if b.rest > 0.0 {
                b.rest -= dt;
                let startled = pointer.is_some_and(|c| (c.x - b.x).hypot(c.y - b.y) < 90.0);
                let still_there = surfaces
                    .iter()
                    .any(|s| (s.y - b.y).abs() < 1.5 && b.x >= s.x && b.x < s.right());
                if startled || !still_there || leaving {
                    b.rest = 0.0;
                    b.vy = -80.0;
                }
                inside = true;
                continue;
            }
            let t = age * 1.3 + b.seed;
            // Wander towards the middle of the screen, then out again.
            let (goal_x, goal_y) = if leaving {
                // Out by the nearest side.
                (if b.x >= w * 0.5 { w + 300.0 } else { -300.0 }, h * 0.2)
            } else {
                (
                    w * (0.5 + 0.35 * (t * 0.23).sin()),
                    h * (0.45 + 0.2 * (t * 0.37).cos()),
                )
            };
            let pull = if leaving { 70.0 } else { 30.0 };
            b.vx += ((goal_x - b.x).signum() * pull + (t * 2.3).sin() * 60.0) * dt;
            b.vy += ((goal_y - b.y).signum() * 20.0 + (t * 3.1).cos() * 90.0) * dt;
            if let Some(c) = pointer {
                let (dx, dy) = (b.x - c.x, b.y - c.y);
                let d = dx.hypot(dy).max(1.0);
                if d < 120.0 {
                    b.vx += dx / d * 260.0 * dt;
                    b.vy += dy / d * 260.0 * dt;
                }
            }
            b.vx = b.vx.clamp(-90.0, 90.0);
            b.vy = b.vy.clamp(-70.0, 70.0);
            let y0 = b.y;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            if !leaving
                && b.vy > 0.0
                && self.rng.next() < 0.5
                && let Some((_, top)) = landing(surfaces, b.x, y0, b.y)
            {
                b.y = top;
                b.rest = self.rng.range(3.0, 7.0);
                (b.vx, b.vy) = (b.vx * 0.3, 0.0);
            }
            b.y = b.y.clamp(20.0, h - 10.0);
            inside |= b.x > -150.0 && b.x < w + 150.0;
        }
        inside || !leaving
    }

    pub(crate) fn draw_visitor(&self, paint: &mut Painter<'_>) {
        self.draw_gift(paint);
        let Some(critter) = &self.visitors.critter else {
            return;
        };
        let clock = self.clock;
        match critter {
            Critter::Penguin(p) => self.draw_penguin(paint, p),
            Critter::Fox(f) => self.draw_fox(paint, f),
            Critter::Owl(o) => self.draw_owl(paint, o),
            Critter::Frog {
                from,
                to,
                hop,
                dir,
                croak,
                ..
            } => {
                let t = *hop;
                let peak = from[1].min(to[1]) - 40.0 - (from[1] - to[1]).abs() * 0.15;
                let c = [(from[0] + to[0]) * 0.5, peak - 20.0];
                let u = 1.0 - t;
                let x = u * u * from[0] + 2.0 * u * t * c[0] + t * t * to[0];
                let y = u * u * from[1] + 2.0 * u * t * c[1] + t * t * to[1];
                // Sitting on a window, only windows above that one hide it.
                paint.layer = if t < 1.0 { 0.0 } else { self.layer_at(x, y) };
                draw_frog(paint, x, y, *dir, t < 1.0, *croak, clock);
                paint.layer = 0.0;
            }
            Critter::Whirlwind { x, age, .. } => {
                let fade = (age * 0.8).min(1.0);
                // A funnel of spinning dust: dark streaks over pale ones, so it
                // shows on light and dark wallpapers alike.
                for k in 0..9 {
                    let kf = k as f32;
                    let y = self.height - 8.0 - kf * 30.0;
                    let r = 10.0 + kf * 8.0;
                    let lean = (kf * 0.35 + age * 0.9).sin() * kf * 2.5;
                    let a = fade * (1.0 - kf / 11.0);
                    for (j, color, alpha) in [
                        (0.0, [0.45, 0.36, 0.28], 0.32),
                        (1.7, [0.96, 0.9, 0.78], 0.3),
                    ] {
                        let spin = (clock * 7.0 + kf * 0.8 + j).sin();
                        paint.put(
                            x + lean + spin * r * 0.5,
                            y,
                            r * 0.6,
                            3.6,
                            spin * 0.25,
                            SHAPE_PUFF,
                            color,
                            alpha * a * 1.4,
                        );
                    }
                }
            }
            Critter::Butterflies { list, .. } => {
                for (i, b) in list.iter().enumerate() {
                    let color = WING_COLORS[i % WING_COLORS.len()];
                    paint.layer = if b.rest > 0.0 {
                        self.layer_at(b.x, b.y)
                    } else {
                        0.0
                    };
                    draw_butterfly(paint, b, color, clock);
                    paint.layer = 0.0;
                }
            }
            Critter::Comet { x, y, vx, vy } => {
                let speed = vx.hypot(*vy);
                let (dx, dy) = (vx / speed, vy / speed);
                let length = 190.0;
                let rot = (-dx).atan2(dy);
                let color = [0.86, 0.92, 1.0];
                paint.put(
                    x - dx * length,
                    y - dy * length,
                    9.0,
                    length,
                    rot,
                    SHAPE_STREAK,
                    color,
                    0.35,
                );
                paint.put(
                    x - dx * length * 0.6,
                    y - dy * length * 0.6,
                    3.0,
                    length * 0.6,
                    rot,
                    SHAPE_STREAK,
                    [1.0, 1.0, 1.0],
                    0.8,
                );
                paint.put(*x, *y, 26.0, 26.0, 0.0, SHAPE_GLOW, color, 0.9);
                paint.put(
                    *x,
                    *y,
                    7.0,
                    7.0,
                    0.0,
                    crate::SHAPE_SPARKLE,
                    [1.0, 1.0, 1.0],
                    1.0,
                );
            }
            Critter::Plane { x, y, heading, .. } => draw_plane(paint, *x, *y, *heading),
            Critter::Heart { .. } => {}
            Critter::Lantern { .. } => {
                if let Some([lx, ly]) = self.visitors.lantern() {
                    let flicker = 0.85 + 0.15 * (clock * 9.0).sin() * (clock * 3.7).cos();
                    paint.put(lx, ly, 120.0, 120.0, 0.0, SHAPE_SOFT, WARM, 0.25 * flicker);
                    paint.put(lx, ly, 26.0, 26.0, 0.0, SHAPE_GLOW, WARM, 0.9 * flicker);
                    paint.put(lx, ly - 13.0, 7.0, 2.0, 0.0, SHAPE_RECT, INK, 0.9);
                    paint.put(lx, ly + 11.0, 8.0, 2.0, 0.0, SHAPE_RECT, INK, 0.9);
                    paint.put(lx, ly, 5.5, 9.0, 0.0, SHAPE_RECT, [1.0, 0.9, 0.62], 0.95);
                    paint.line(
                        [lx - 4.0, ly - 15.0],
                        [lx, ly - 21.0],
                        0.9,
                        SHAPE_RECT,
                        INK,
                        0.9,
                    );
                    paint.line(
                        [lx + 4.0, ly - 15.0],
                        [lx, ly - 21.0],
                        0.9,
                        SHAPE_RECT,
                        INK,
                        0.9,
                    );
                }
            }
            Critter::Umbrella { x, y, spin, .. } => draw_umbrella(paint, *x, *y, *spin),
        }
    }

    /// How much fog to keep near the lantern, from 0.25 next to it to 1.
    pub(crate) fn lantern_clearing(&self, x: f32, y: f32) -> f32 {
        self.visitors.lantern().map_or(1.0, |[lx, ly]| {
            let d = (x - lx).hypot((y - ly) * 1.6) / LANTERN_CLEAR_RADIUS;
            0.25 + 0.75 * d.clamp(0.0, 1.0)
        })
    }
}

fn draw_frog(
    paint: &mut Painter<'_>,
    x: f32,
    y: f32,
    dir: f32,
    flying: bool,
    croak: f32,
    clock: f32,
) {
    let a = 1.0;
    if flying {
        // Legs stretched out behind.
        paint.line(
            [x - dir * 6.0, y - 6.0],
            [x - dir * 20.0, y + 2.0],
            3.0,
            SHAPE_RECT,
            FROG_DARK,
            a,
        );
        paint.line(
            [x - dir * 4.0, y - 4.0],
            [x - dir * 18.0, y + 6.0],
            3.0,
            SHAPE_RECT,
            FROG_DARK,
            a,
        );
        paint.line(
            [x + dir * 6.0, y - 6.0],
            [x + dir * 12.0, y + 2.0],
            2.2,
            SHAPE_RECT,
            FROG_DARK,
            a,
        );
    } else {
        paint.put(
            x - dir * 8.0,
            y - 4.0,
            6.0,
            4.0,
            0.0,
            SHAPE_DISK,
            FROG_DARK,
            a,
        );
        paint.put(
            x + dir * 9.0,
            y - 2.0,
            3.5,
            2.2,
            0.0,
            SHAPE_DISK,
            FROG_DARK,
            a,
        );
    }
    let body_y = if flying { y - 9.0 } else { y - 7.0 };
    let squash = if flying { 1.12 } else { 1.0 };
    paint.put(
        x,
        body_y,
        12.0 * squash,
        7.5 / squash,
        0.0,
        SHAPE_DISK,
        FROG,
        a,
    );
    paint.put(
        x + dir * 2.0,
        body_y + 3.0,
        7.0,
        3.5,
        0.0,
        SHAPE_DISK,
        FROG_BELLY,
        a,
    );
    if croak > 0.0 {
        let puff = (croak * PI / 0.9).sin().max(0.0);
        paint.put(
            x + dir * 7.0,
            body_y + 5.0,
            4.0 + puff * 5.0,
            3.0 + puff * 4.0,
            0.0,
            SHAPE_DISK,
            [0.95, 0.9, 0.7],
            a,
        );
    }
    let blink = ((clock * 0.7).sin() > 0.985) as u8 as f32;
    for side in [-1.0_f32, 1.0] {
        let ex = x + dir * 5.0 + side * 4.5;
        let ey = body_y - 7.0;
        paint.put(ex, ey, 3.6, 3.6, 0.0, SHAPE_DISK, FROG, a);
        paint.put(
            ex,
            ey - 0.5,
            2.4,
            2.4 * (1.0 - blink * 0.85),
            0.0,
            SHAPE_DISK,
            WHITE,
            a,
        );
        paint.put(
            ex + dir * 0.6,
            ey - 0.3,
            1.2,
            1.2 * (1.0 - blink),
            0.0,
            SHAPE_DISK,
            INK,
            a,
        );
    }
    paint.line(
        [x + dir * 4.0, body_y + 0.5],
        [x + dir * 10.0, body_y - 1.0],
        0.6,
        SHAPE_RECT,
        FROG_DARK,
        a,
    );
}

fn draw_butterfly(paint: &mut Painter<'_>, b: &Butterfly, color: [f32; 3], clock: f32) {
    let resting = b.rest > 0.0;
    let speed = if resting { 2.0 } else { 16.0 };
    let flap = (clock * speed + b.seed).sin().abs();
    let open = if resting {
        0.25 + 0.4 * flap
    } else {
        0.15 + 0.85 * flap
    };
    let y = if resting { b.y - 7.0 } else { b.y };
    let dark = [color[0] * 0.55, color[1] * 0.55, color[2] * 0.55];
    for side in [-1.0_f32, 1.0] {
        let x = b.x + side * 5.5 * open;
        paint.put(
            x,
            y - 2.0,
            5.5 * open + 0.6,
            7.5,
            side * 0.5,
            SHAPE_PETAL,
            dark,
            1.0,
        );
        paint.put(
            x,
            y - 2.0,
            4.6 * open + 0.4,
            6.4,
            side * 0.5,
            SHAPE_PETAL,
            color,
            1.0,
        );
        paint.put(
            b.x + side * 3.5 * open,
            y + 4.0,
            3.0 * open + 0.4,
            3.8,
            side * 2.5,
            SHAPE_PETAL,
            color,
            1.0,
        );
    }
    paint.put(b.x, y, 1.1, 6.0, 0.0, SHAPE_RECT, INK, 1.0);
    paint.line(
        [b.x, y - 5.0],
        [b.x - 3.0, y - 10.0],
        0.4,
        SHAPE_RECT,
        INK,
        1.0,
    );
    paint.line(
        [b.x, y - 5.0],
        [b.x + 3.0, y - 10.0],
        0.4,
        SHAPE_RECT,
        INK,
        1.0,
    );
}

fn draw_plane(paint: &mut Painter<'_>, x: f32, y: f32, heading: f32) {
    // Triangles point up when unrotated, so turn them to face the heading.
    let rot = heading + PI * 0.5;
    paint.put(x, y, 10.5, 19.0, rot, SHAPE_TRIANGLE, PAPER_SHADE, 1.0);
    paint.put(x, y, 9.0, 17.0, rot, SHAPE_TRIANGLE, PAPER, 1.0);
    let [ox, oy] = rotate(0.0, 4.0, rot);
    paint.put(
        x + ox,
        y + oy,
        2.5,
        12.0,
        rot,
        SHAPE_TRIANGLE,
        PAPER_SHADE,
        1.0,
    );
}

fn draw_umbrella(paint: &mut Painter<'_>, x: f32, y: f32, spin: f32) {
    let canopy = [0.86, 0.24, 0.26];
    let stripe = [0.98, 0.88, 0.5];
    let [hx, hy] = rotate(0.0, 22.0, spin);
    paint.line([x, y], [x + hx, y + hy], 1.6, SHAPE_RECT, INK, 1.0);
    let [cx, cy] = rotate(-3.0, 24.0, spin);
    paint.put(
        x + hx + cx * 0.15,
        y + hy + cy * 0.15,
        3.0,
        3.0,
        0.0,
        SHAPE_DISK,
        INK,
        1.0,
    );
    paint.put(x, y, 28.0, 18.0, spin, SHAPE_DOME, canopy, 1.0);
    let [sx, sy] = rotate(0.0, -2.0, spin);
    paint.put(x + sx, y + sy, 9.0, 16.0, spin, SHAPE_DOME, stripe, 1.0);
    let [tx, ty] = rotate(0.0, -19.0, spin);
    paint.put(x + tx, y + ty, 1.5, 3.0, spin, SHAPE_RECT, INK, 1.0);
}
