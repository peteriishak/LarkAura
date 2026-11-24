//! Things that stay or pass by: decorations, snowballs and small visitors.
//! All of it is drawn from simple shapes and never receives clicks.

use crate::{
    Debris, DebrisKind, Painter, SHAPE_DISK, SHAPE_DOME, SHAPE_GLOW, SHAPE_PUFF, SHAPE_RECT,
    SHAPE_SOFT, SHAPE_SPARKLE, SHAPE_TRIANGLE, Simulation, landing, place,
    sprites::{self, anchor},
};
use aura_core::{
    Decorations, Frequency, MAX_SNOWMEN, MAX_TREES, Rect, SnowmanDecoration, TreeDecoration,
};
use aura_format::EffectKind;

const INK: [f32; 3] = [0.15, 0.14, 0.17];
const SNOW: [f32; 3] = [0.96, 0.97, 1.0];
const ORANGE: [f32; 3] = [0.93, 0.42, 0.17];
const SAND: [f32; 3] = [0.95, 0.84, 0.77];
const GOLD: [f32; 3] = [1.0, 0.8, 0.42];
const BERRY: [f32; 3] = [0.72, 0.16, 0.2];
const PINE: [[f32; 3]; 3] = [[0.13, 0.3, 0.24], [0.16, 0.36, 0.28], [0.2, 0.43, 0.32]];
const BARK: [f32; 3] = [0.36, 0.23, 0.17];
const REIN: [f32; 3] = [0.34, 0.19, 0.13];
const AMBER: [f32; 3] = [1.0, 0.62, 0.18];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

const MAX_BALLS: usize = 6;
const MAX_BALL_RADIUS: f32 = 24.0;
/// A ball packed from snow lying under the pointer.
const SCOOP_RADIUS: f32 = 24.0;
/// A ball packed from falling snow, on the bare desktop.
const AIR_BALL_RADIUS: f32 = 7.0;
/// How long the button must be held still to pack a snowball, so quick
/// everyday clicks never make one.
pub(crate) const HOLD_SECONDS: f32 = 0.45;
/// How far the pointer may wander while packing before it gives up.
const HOLD_SLOP: f32 = 18.0;
/// Dots in the ring that fills while a snowball is packed.
const HOLD_DOTS: usize = 12;
const SCOOP_DEPTH: f32 = 18.0;

fn ball_radius(snow: f32) -> f32 {
    (AIR_BALL_RADIUS * AIR_BALL_RADIUS + snow * 0.5)
        .sqrt()
        .min(MAX_BALL_RADIUS * 0.6)
}

/// A snowball being packed: the button is held where it went down.
#[derive(Clone, Copy, Debug)]
struct Packing {
    x: f32,
    y: f32,
    held: f32,
    /// The size the finished ball will have.
    radius: f32,
}
const STACK_REACH: f32 = 72.0;
const BALL_SPLAT_SPEED: f32 = 620.0;
const SLEIGH_SPEED: f32 = 150.0;
/// The sleigh's origin starts this far left of the display, so the reindeer
/// in front start out of sight too.
const SLEIGH_START: f32 = -330.0;
/// Reindeer origins ahead of the sleigh's origin.
const DEER_AHEAD: [f32; 2] = [124.0, 222.0];
/// Strides per second of the reindeer's gallop.
const GALLOP_RATE: f32 = 1.45;
const PLOUGH_SPEED: f32 = 118.0;
const PLOUGH_START: f32 = -120.0;
/// From the plough's origin to the bottom edge of its blade.
const PLOUGH_WIDTH: f32 = anchor::PLOUGH_BLADE[0];
/// Wheel radii in drawn pixels. The wheel sprite is the rear wheel.
const REAR_WHEEL_RADIUS: f32 = 24.5;
const FRONT_WHEEL_SCALE: f32 = 0.36 / 0.46;
/// Snow the blade can carry before it spills over.
const MAX_BERM: f32 = 2400.0;
/// The plough comes out by itself once the ground is this share of full.
const PLOUGH_DEPTH_SHARE: f32 = 0.6;

#[derive(Clone, Copy, Debug)]
struct Ball {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
}

#[derive(Clone, Copy, Debug)]
struct Stack {
    x: f32,
    y: f32,
    radius: f32,
    count: u8,
    left: f32,
    /// It sits on a window top, so it falls apart when the window moves.
    on_window: bool,
}

#[derive(Clone, Copy, Debug)]
struct BuiltSnowman {
    x: f32,
    y: f32,
    size: f32,
    /// Sway left after a hit, which dies away.
    wobble: f32,
    /// A fast snowball knocked the head off.
    headless: bool,
    /// How far the snowplough has pushed it.
    pushed: f32,
    /// It stands on a window top, so it falls apart when the window moves.
    on_window: bool,
}

/// Something big passing through the snow.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mover {
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
    pub vx: f32,
}

/// A snowball this fast knocks a snowman's head clean off.
const HEAD_OFF_SPEED: f32 = 380.0;
/// The plough pushes a snowman this far before it falls apart.
const PUSH_BEFORE_TOPPLE: f32 = 150.0;

pub(crate) struct Scenery {
    pub decorations: Decorations,
    tree_fade: [f32; MAX_TREES],
    /// The ball held in the pointer while the button is down.
    rolling: Option<Ball>,
    packing: Option<Packing>,
    flying: Vec<Ball>,
    stacks: Vec<Stack>,
    built: Vec<BuiltSnowman>,
    sleigh: Option<f32>,
    plough: Option<f32>,
    next_sleigh: f32,
    next_plough: f32,
    /// Snow heaped against the plough's blade.
    berm: f32,
    exhaust_in: f32,
    /// How much snow has settled on trees and snowmen, from 0 to 1.
    pub snow_cover: f32,
}

fn wait_for(frequency: Frequency, sometimes: f32, often: f32) -> Option<f32> {
    match frequency {
        Frequency::Off => None,
        Frequency::Sometimes => Some(sometimes),
        Frequency::Often => Some(often),
    }
}

fn scatter(index: usize, step: f32) -> f32 {
    (index as f32 * step + 0.13).fract()
}

impl Scenery {
    /// The sleigh or the plough is out.
    pub fn busy(&self) -> bool {
        self.sleigh.is_some() || self.plough.is_some()
    }

    pub fn new() -> Self {
        Self {
            decorations: Decorations::default(),
            tree_fade: [0.0; MAX_TREES],
            rolling: None,
            packing: None,
            flying: Vec::new(),
            stacks: Vec::new(),
            built: Vec::new(),
            sleigh: None,
            plough: None,
            next_sleigh: 240.0,
            next_plough: 90.0,
            berm: 0.0,
            exhaust_in: 0.0,
            snow_cover: 0.0,
        }
    }

    pub fn clear_snow_play(&mut self) {
        self.rolling = None;
        self.packing = None;
        self.flying.clear();
        self.stacks.clear();
        self.built.clear();
        self.plough = None;
        self.berm = 0.0;
        self.snow_cover = 0.0;
    }

    pub fn send_sleigh(&mut self) {
        self.sleigh.get_or_insert(SLEIGH_START);
    }

    pub fn send_plough(&mut self) {
        if self.plough.is_none() {
            self.plough = Some(PLOUGH_START);
            self.berm = 0.0;
        }
    }

    /// Where the sleigh and plough are, for snow to swirl around them:
    /// centre, half extent and speed.
    pub fn movers(&self, height: f32, path: impl Fn(f32) -> f32) -> [Option<Mover>; 2] {
        let sleigh = self.sleigh.map(|x| Mover {
            x: x + 70.0,
            y: path(x + 70.0) - 45.0,
            rx: 230.0,
            ry: 70.0,
            vx: SLEIGH_SPEED,
        });
        let plough = self.plough.map(|x| Mover {
            x: x + 20.0,
            y: height - 55.0,
            rx: 115.0,
            ry: 70.0,
            vx: PLOUGH_SPEED,
        });
        [sleigh, plough]
    }

    #[cfg(test)]
    pub fn has_visitors(&self) -> bool {
        self.sleigh.is_some() || self.plough.is_some()
    }

    #[cfg(test)]
    pub fn sleigh_x(&self) -> Option<f32> {
        self.sleigh
    }

    #[cfg(test)]
    pub fn built_count(&self) -> usize {
        self.built.len()
    }

    pub fn place_snowman(&mut self, x: f32, y: f32) {
        if self.built.len() >= MAX_SNOWMEN {
            self.built.remove(0);
        }
        self.built.push(BuiltSnowman {
            x,
            y,
            size: 1.0,
            wobble: 0.0,
            headless: false,
            pushed: 0.0,
            on_window: false,
        });
    }

    #[cfg(test)]
    pub fn build_snowman(&mut self, x: f32, y: f32) {
        self.built.push(BuiltSnowman {
            x,
            y,
            size: 1.0,
            wobble: 0.0,
            headless: false,
            pushed: 0.0,
            on_window: false,
        });
    }

    #[cfg(test)]
    pub fn throw(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        self.flying.push(Ball {
            x,
            y,
            vx,
            vy,
            radius: 9.0,
        });
    }

    #[cfg(test)]
    pub fn headless_count(&self) -> usize {
        self.built.iter().filter(|m| m.headless).count()
    }

    #[cfg(test)]
    pub fn snowman_x(&self, index: usize) -> Option<f32> {
        self.built.get(index).map(|m| m.x)
    }

    #[cfg(test)]
    pub fn packing(&self) -> bool {
        self.packing.is_some()
    }

    /// The radius of the ball held in the pointer, if any.
    #[cfg(test)]
    pub fn holding(&self) -> Option<f32> {
        self.rolling.map(|b| b.radius)
    }

    #[cfg(test)]
    pub fn stack_count(&self) -> usize {
        self.stacks.len()
    }
}

impl Simulation {
    fn covered(&self, x: f32, y: f32) -> bool {
        self.windows.iter().any(|w| w.contains(x, y))
    }

    pub(crate) fn step_scenery(&mut self, dt: f32, surfaces: &[Rect], windows: &[Rect]) {
        self.step_decoration_fades(dt, windows);
        let snow = self.aura.effect == EffectKind::Snow;
        if snow && self.tuning.piles {
            self.step_snowballs(dt, surfaces, windows);
        }
        if snow {
            // Snow gathers on the trees and snowmen over a couple of minutes.
            let cover = &mut self.scenery.snow_cover;
            *cover = (*cover + dt * self.intensity * 0.008).min(1.0);
        }
        self.step_visitors(dt, snow);
    }

    /// Decorations belong to the desktop, so they step aside for windows.
    fn step_decoration_fades(&mut self, dt: f32, windows: &[Rect]) {
        let height = self.height;
        let rate = dt * 3.0;
        let target = |sim: &Self, x: f32, tall: f32| {
            let px = x * sim.width;
            let covered = |y: f32| windows.iter().any(|w| w.contains(px, y));
            f32::from(!covered(height - 6.0) && !covered(height - tall))
        };
        for i in 0..MAX_TREES {
            let goal = self
                .scenery
                .decorations
                .trees
                .get(i)
                .map_or(0.0, |t| target(self, t.x, 120.0 * t.size));
            let f = &mut self.scenery.tree_fade[i];
            *f += (goal - *f).clamp(-rate, rate);
        }
    }

    /// Snow play with the pointer. Holding the button still on lying snow
    /// packs a snowball, and on the bare desktop packs a small one from the
    /// falling snow; a quick click does nothing. Dragging the ball through
    /// snow rolls it bigger, and letting go throws it with the pointer's
    /// speed, or sets it down if the pointer is still, so three in one spot
    /// build a snowman. Moving the pointer without pressing only brushes snow
    /// off, in a spray of powder.
    fn step_snowballs(&mut self, dt: f32, surfaces: &[Rect], windows: &[Rect]) {
        if let Some(c) = self.pointer {
            if c.pressed && self.scenery.rolling.is_none() {
                self.scenery.packing = self.ball_size_at(c.x, c.y, windows).map(|radius| Packing {
                    x: c.x,
                    y: c.y,
                    held: 0.0,
                    radius,
                });
            }
            if let Some(mut pack) = self.scenery.packing.take() {
                let wandered = (c.x - pack.x).hypot(c.y - pack.y) > HOLD_SLOP;
                if c.down && !wandered {
                    pack.held += dt;
                    if pack.held >= HOLD_SECONDS {
                        self.pick_up_ball(pack.x, pack.y, windows);
                    } else {
                        self.scenery.packing = Some(pack);
                    }
                }
            }
            if let Some(mut ball) = self.scenery.rolling.take() {
                // Rolling it through snow packs more on; it grows slowly by
                // itself while snow falls on it.
                let taken = if c.speed() > 40.0 {
                    self.piles
                        .carve(ball.x, ball.y + ball.radius * 0.5, 20.0, 90.0 * dt + 1.0)
                } else {
                    0.0
                };
                let packed = taken * 0.5 + dt * 6.0 * self.intensity;
                ball.radius = (ball.radius * ball.radius + packed)
                    .sqrt()
                    .min(MAX_BALL_RADIUS);
                // Held just under the pointer's tip, where the arrow does not hide it.
                let k = (dt * 18.0).min(1.0);
                ball.x += (c.x - ball.x) * k;
                ball.y += (c.y + ball.radius * 0.4 - ball.y) * k;
                (ball.vx, ball.vy) = (c.vx, c.vy);
                if c.down {
                    self.scenery.rolling = Some(ball);
                } else {
                    self.release_ball(ball, c.y, surfaces);
                }
            } else if !c.down {
                self.brush_snow(c.x, c.y, c.vx, c.vy, dt);
            }
        } else {
            self.scenery.packing = None;
            if let Some(ball) = self.scenery.rolling.take() {
                self.scenery.flying.push(ball);
            }
        }
        self.topple_unsupported(surfaces);

        let mut flying = std::mem::take(&mut self.scenery.flying);
        let mut heads = Vec::new();
        flying.retain_mut(|ball| {
            let y0 = ball.y;
            ball.vy += 980.0 * dt;
            ball.vx *= 1.0 - (0.3 * dt).min(1.0);
            ball.x += ball.vx * dt;
            ball.y += ball.vy * dt;
            if ball.x < -60.0 || ball.x > self.width + 60.0 {
                return false;
            }
            if let Some(head) = self.hit_snowman(ball) {
                heads.extend(head);
                return false;
            }
            let bottom = ball.y + ball.radius;
            let hit = if ball.vy > 0.0 {
                landing(surfaces, ball.x, y0 + ball.radius, bottom).map(|(_, top)| top)
            } else {
                None
            };
            let ground = self.height - self.piles.floor_height(ball.x) * 0.6;
            let Some(surface) = hit.or((bottom >= ground).then_some(ground)) else {
                return true;
            };
            let speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
            if speed > BALL_SPLAT_SPEED {
                self.splat(ball.x, surface, ball.radius);
            } else {
                self.settle_ball(ball.x, surface, ball.radius, hit.is_some());
            }
            false
        });
        flying.extend(heads);
        flying.truncate(MAX_BALLS + 2);
        self.scenery.flying = flying;
        for man in &mut self.scenery.built {
            man.wobble *= (1.0 - dt * 1.4).max(0.0);
        }

        for stack in &mut self.scenery.stacks {
            stack.left -= dt;
        }
        self.scenery.stacks.retain(|s| s.left > 0.0);
    }

    /// A thrown ball that meets a snowman makes it wobble. A fast one knocks
    /// its head off, and the head flies on as a new snowball.
    fn hit_snowman(&mut self, ball: &Ball) -> Option<Option<Ball>> {
        let speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
        let index = self.scenery.built.iter().position(|m| {
            let tall = 82.0 * m.size;
            (ball.x - m.x).abs() < 22.0 * m.size + ball.radius * 0.5
                && ball.y > m.y - tall
                && ball.y < m.y - 4.0
        })?;
        let man = &mut self.scenery.built[index];
        man.wobble = (man.wobble + 0.35 + speed / 1400.0).min(1.0);
        let head_height = man.y - 64.0 * man.size;
        let mut head = None;
        if speed > HEAD_OFF_SPEED && !man.headless && ball.y < man.y - 40.0 * man.size {
            man.headless = true;
            // It starts just clear of the body, so it cannot hit it again.
            let side = if ball.vx < 0.0 { -1.0 } else { 1.0 };
            let radius = 11.5 * man.size;
            head = Some(Ball {
                x: man.x + side * (22.0 * man.size + radius * 0.5 + 3.0),
                y: head_height,
                vx: side * (ball.vx.abs() * 0.55).max(140.0),
                vy: -220.0,
                radius,
            });
        }
        let (x, y, r) = (ball.x, ball.y, ball.radius);
        self.splat(x, y, r * 0.7);
        Some(head)
    }

    fn splat(&mut self, x: f32, y: f32, radius: f32) {
        for _ in 0..(radius as usize + 8) {
            let angle = self.rng.range(-3.0, -0.14);
            let speed = self.rng.range(60.0, 240.0);
            let flake = Debris::plain(
                x,
                y,
                angle.cos() * speed,
                angle.sin() * speed,
                self.rng.range(0.5, 1.1),
                self.rng.range(1.4, 3.0),
                DebrisKind::Flake,
            );
            self.add_debris(flake);
        }
    }

    /// How big a ball packed at this spot would be: from lying snow under
    /// the pointer, or from the falling snow over the bare desktop. None
    /// inside a window, where pressing does nothing.
    fn ball_size_at(&self, x: f32, y: f32, windows: &[Rect]) -> Option<f32> {
        if self.scenery.flying.len() >= MAX_BALLS {
            return None;
        }
        let snow = self.piles.snow_under(x, y, SCOOP_RADIUS, SCOOP_DEPTH);
        if snow > 1.0 {
            Some(ball_radius(snow))
        } else if !windows.iter().any(|w| w.contains(x, y)) {
            Some(AIR_BALL_RADIUS)
        } else {
            None
        }
    }

    /// Packs the ball once the button has been held long enough.
    fn pick_up_ball(&mut self, x: f32, y: f32, windows: &[Rect]) {
        let Some(_) = self.ball_size_at(x, y, windows) else {
            return;
        };
        let taken = self.piles.scoop(x, y, SCOOP_RADIUS, SCOOP_DEPTH);
        let radius = if taken > 1.0 {
            ball_radius(taken)
        } else {
            AIR_BALL_RADIUS
        };
        self.scenery.rolling = Some(Ball {
            x,
            y: y + radius * 0.4,
            vx: 0.0,
            vy: 0.0,
            radius,
        });
        // A little powder puffs out of the hand.
        for _ in 0..8 {
            let angle = self.rng.range(-std::f32::consts::PI, 0.0);
            let speed = self.rng.range(30.0, 110.0);
            let flake = Debris::plain(
                x + angle.cos() * radius,
                y + radius * 0.4,
                angle.cos() * speed,
                angle.sin() * speed,
                self.rng.range(0.4, 0.9),
                self.rng.range(1.0, 2.2),
                DebrisKind::Flake,
            );
            self.add_debris(flake);
        }
    }

    /// Lets go of the held ball. A slow one is set down on the snow under
    /// the pointer, so clicks in one spot stack up.
    fn release_ball(&mut self, mut ball: Ball, pointer_y: f32, surfaces: &[Rect]) {
        ball.vx = ball.vx.clamp(-1100.0, 1100.0);
        ball.vy = ball.vy.clamp(-1100.0, 1100.0);
        let speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
        if speed < 160.0 {
            (ball.vx, ball.vy) = (ball.vx * 0.3, 0.0);
            if let Some((_, top)) = landing(surfaces, ball.x, pointer_y - 30.0, pointer_y + 40.0) {
                ball.y = ball.y.min(top - ball.radius - 0.5);
            }
        }
        if self.scenery.flying.len() < MAX_BALLS {
            self.scenery.flying.push(ball);
        }
    }

    /// The pointer passing over lying snow sweeps it off, in a spray that
    /// follows the stroke. Faster strokes clear more.
    fn brush_snow(&mut self, x: f32, y: f32, vx: f32, vy: f32, dt: f32) {
        let speed = (vx * vx + vy * vy).sqrt();
        if speed < 90.0 {
            return;
        }
        let depth = (speed.min(1800.0) * 0.7 * dt).min(24.0);
        let taken = self.piles.scoop(x, y, 26.0, depth);
        if taken < 0.3 {
            return;
        }
        let k = 1.0 / speed;
        let (dx, dy) = (vx * k, vy * k);
        let count = ((taken * 0.7) as usize).clamp(1, 12);
        for _ in 0..count {
            let push = self.rng.range(0.25, 0.7);
            let flake = Debris::plain(
                x + self.rng.range(-8.0, 8.0),
                y + self.rng.range(-4.0, 3.0),
                vx * push + self.rng.range(-35.0, 35.0),
                vy * push * 0.4 - self.rng.range(50.0, 170.0),
                self.rng.range(0.8, 1.7),
                self.rng.range(1.0, 2.8),
                DebrisKind::Flake,
            );
            self.add_debris(flake);
        }
        if taken > 3.0 && self.rng.next() < 0.5 {
            let puff = Debris::plain(
                x - dx * 6.0,
                y - dy * 6.0 - 3.0,
                vx * 0.25,
                -self.rng.range(15.0, 45.0),
                self.rng.range(0.5, 0.9),
                self.rng.range(3.0, 6.0),
                DebrisKind::Powder,
            );
            self.add_debris(puff);
        }
    }

    /// Stacks and snowmen built on a window top fall apart when the window
    /// moves or closes, like the rest of the snow on it.
    fn topple_unsupported(&mut self, surfaces: &[Rect]) {
        let held = |x: f32, y: f32| {
            surfaces
                .iter()
                .any(|w| (w.y - y).abs() < 1.5 && x >= w.x && x < w.right())
        };
        let mut fallen = Vec::new();
        self.scenery.built.retain(|m| {
            let keep = !m.on_window || held(m.x, m.y);
            if !keep {
                fallen.push((m.x, m.y - 30.0 * m.size, 20.0 * m.size));
            }
            keep
        });
        self.scenery.stacks.retain(|st| {
            let keep = !st.on_window || held(st.x, st.y);
            if !keep {
                fallen.push((st.x, st.y - st.radius, st.radius));
            }
            keep
        });
        for (x, y, r) in fallen {
            self.splat(x, y, r);
        }
    }

    /// Balls landing close together stack, and three make a snowman.
    fn settle_ball(&mut self, x: f32, y: f32, radius: f32, on_window: bool) {
        let scenery = &mut self.scenery;
        let near = scenery.stacks.iter().position(|s| {
            // On the ground the snow under a stack changes height as balls
            // are scooped from it, so only window tops need the same level.
            let level = (s.y - y).abs() < 8.0 || (!on_window && !s.on_window);
            (s.x - x).abs() < STACK_REACH && level && s.on_window == on_window
        });
        if let Some(i) = near {
            let stack = &mut scenery.stacks[i];
            stack.count += 1;
            stack.left = 90.0;
            stack.radius = stack.radius.max(radius);
            if stack.count >= 3 {
                let made = BuiltSnowman {
                    x: stack.x,
                    y: stack.y,
                    size: (stack.radius / 18.0).clamp(0.6, 1.2),
                    wobble: 0.6,
                    headless: false,
                    pushed: 0.0,
                    on_window: stack.on_window,
                };
                scenery.stacks.swap_remove(i);
                if scenery.built.len() >= MAX_SNOWMEN {
                    scenery.built.remove(0);
                }
                scenery.built.push(made);
            }
        } else {
            if scenery.stacks.len() >= MAX_BALLS {
                scenery.stacks.remove(0);
            }
            scenery.stacks.push(Stack {
                x,
                y,
                radius,
                count: 1,
                left: 90.0,
                on_window,
            });
        }
    }

    fn step_visitors(&mut self, dt: f32, snow: bool) {
        let (sleigh_rule, plough_rule) = (
            self.scenery.decorations.sleigh,
            self.scenery.decorations.plough,
        );
        if snow && self.scenery.sleigh.is_none() {
            self.scenery.next_sleigh -= dt;
            if self.scenery.next_sleigh <= 0.0 {
                let wait = wait_for(sleigh_rule, 2700.0, 720.0);
                if wait.is_some() {
                    self.scenery.send_sleigh();
                }
                self.scenery.next_sleigh = wait.unwrap_or(600.0) * self.rng.range(0.8, 1.3);
            }
        }
        if let Some(x) = &mut self.scenery.sleigh {
            *x += SLEIGH_SPEED * dt;
            let px = *x;
            if px > self.width + 120.0 {
                self.scenery.sleigh = None;
            } else {
                self.sleigh_trail(px, dt);
            }
        }

        if snow && self.tuning.piles && self.scenery.plough.is_none() {
            self.scenery.next_plough -= dt;
            let deep = self.piles.floor_mean() > self.max_depth() * PLOUGH_DEPTH_SHARE;
            if self.scenery.next_plough <= 0.0 && deep {
                let wait = wait_for(plough_rule, 900.0, 240.0);
                if wait.is_some() {
                    self.scenery.send_plough();
                }
                self.scenery.next_plough = wait.unwrap_or(300.0);
            }
        }
        if let Some(x) = &mut self.scenery.plough {
            *x += PLOUGH_SPEED * dt;
            let px = *x;
            if px > self.width + 140.0 {
                self.scenery.plough = None;
            }
            let front = px + PLOUGH_WIDTH;
            self.plough_push(front, PLOUGH_SPEED * dt);
            // Smooth curved scooping pushes a rolling snow wave ahead of the
            // blade and scatters the rest upward in clumps and powder.
            let cleared = self.piles.plough_floor(front, self.max_depth());
            self.plough_spray(px, cleared, dt);
            self.plough_exhaust(px, dt);
        }
    }

    /// Stardust from the runners: warm sparks and twinkling glints that
    /// drift down behind the sleigh.
    fn sleigh_trail(&mut self, x: f32, dt: f32) {
        let (y, tilt) = self.sleigh_pose(x);
        let [tx, ty] = place([x, y], 1.0, tilt, anchor::SLEIGH_TAIL);
        if self.rng.next() < dt * 26.0 {
            let glint = Debris::plain(
                tx + self.rng.range(-6.0, 10.0),
                ty + self.rng.range(-4.0, 4.0),
                self.rng.range(-50.0, -15.0),
                self.rng.range(4.0, 24.0),
                self.rng.range(1.0, 2.2),
                self.rng.range(0.8, 1.6),
                DebrisKind::Glint,
            );
            self.add_debris(glint);
        }
        if self.rng.next() < dt * 14.0 {
            let spark = Debris::plain(
                tx + self.rng.range(0.0, 30.0),
                ty + self.rng.range(-2.0, 6.0),
                -30.0,
                10.0,
                self.rng.range(0.5, 1.1),
                self.rng.range(0.7, 1.2),
                DebrisKind::Spark,
            );
            self.add_debris(spark);
        }
    }

    /// The blade builds a rolling berm of snow ahead of it and flings cohesive
    /// arcs of tumbling clumps, scattered flakes and billowing powder forward.
    fn plough_spray(&mut self, x: f32, cleared: f32, dt: f32) {
        let berm = (self.scenery.berm + cleared * 1.15).min(MAX_BERM);
        let spill = berm * (dt * 1.05).min(1.0);
        self.scenery.berm = berm - spill;
        let ground = self.height;
        let blade = x + PLOUGH_WIDTH;
        let heap = self.berm_radius();
        let clumps = ((spill * 0.05 + cleared * 0.04) as usize).min(5);
        for _ in 0..clumps {
            let clump = Debris {
                x: blade + self.rng.range(-2.0, heap * 0.85),
                y: ground - self.rng.range(heap * 0.55, heap * 1.45 + 10.0),
                vx: PLOUGH_SPEED * self.rng.range(1.25, 2.55),
                vy: -self.rng.range(130.0, 290.0),
                life: self.rng.range(0.7, 1.5),
                size: self.rng.range(3.2, 7.6),
                kind: DebrisKind::Clump,
                window_id: 0,
                angle: self.rng.range(0.0, std::f32::consts::TAU),
                spin: self.rng.range(-7.0, 7.0),
                // Disperse cleanly once thrown clear of the blade.
                mass: 0.0,
                seed: self.rng.range(0.0, std::f32::consts::TAU),
            };
            self.add_debris(clump);
        }
        let chunks = ((spill * 0.11 + cleared * 0.07) as usize).min(12);
        for _ in 0..chunks {
            let flake = Debris::plain(
                blade + self.rng.range(-4.0, heap * 1.1),
                ground - self.rng.range(heap * 0.45, heap * 1.7 + 12.0),
                PLOUGH_SPEED * self.rng.range(1.15, 2.4) + self.rng.range(-15.0, 40.0),
                -self.rng.range(90.0, 260.0),
                self.rng.range(0.6, 1.3),
                self.rng.range(1.5, 3.8),
                DebrisKind::Flake,
            );
            self.add_debris(flake);
        }
        if (spill > 0.4 || cleared > 1.0) && self.rng.next() < dt * (8.0 + spill * 0.5) {
            let puff = Debris::plain(
                blade + heap * 0.45 + self.rng.range(-8.0, 14.0),
                ground - heap * 0.85 - self.rng.range(4.0, 18.0),
                self.rng.range(35.0, 125.0),
                -self.rng.range(20.0, 65.0),
                self.rng.range(0.75, 1.35),
                self.rng.range(7.0, 14.0),
                DebrisKind::Powder,
            );
            self.add_debris(puff);
        }
    }

    fn plough_exhaust(&mut self, x: f32, dt: f32) {
        self.scenery.exhaust_in -= dt;
        if self.scenery.exhaust_in > 0.0 {
            return;
        }
        self.scenery.exhaust_in = self.rng.range(0.22, 0.4);
        let [ex, ey] = place([x, self.plough_ground()], 1.0, 0.0, anchor::PLOUGH_EXHAUST);
        let smoke = Debris::plain(
            ex,
            ey,
            self.rng.range(-40.0, -15.0),
            -self.rng.range(22.0, 40.0),
            self.rng.range(1.2, 1.8),
            self.rng.range(3.5, 5.0),
            DebrisKind::Smoke,
        );
        self.add_debris(smoke);
    }

    fn berm_radius(&self) -> f32 {
        (self.scenery.berm.sqrt() * 1.35).min(46.0)
    }

    /// The plough's origin height: its near wheels touch the bottom edge.
    fn plough_ground(&self) -> f32 {
        self.height - (anchor::PLOUGH_REAR_WHEEL[1] + REAR_WHEEL_RADIUS)
    }

    /// The plough's blade shoves snowmen and half-built stacks along until
    /// they fall apart in a burst of snow.
    fn plough_push(&mut self, front: f32, step: f32) {
        let floor = self.height - 90.0;
        let mut toppled = Vec::new();
        for (i, man) in self.scenery.built.iter_mut().enumerate() {
            let reach = 24.0 * man.size;
            if man.y > floor && man.x >= front - 6.0 && man.x <= front + reach {
                man.x = front + reach;
                man.pushed += step;
                man.wobble = man.wobble.max(0.35);
                if man.pushed > PUSH_BEFORE_TOPPLE {
                    toppled.push(i);
                }
            }
        }
        for i in toppled.into_iter().rev() {
            let man = self.scenery.built.remove(i);
            for (dy, r) in [(18.0, 22.0), (52.0, 16.0), (80.0, 11.0)] {
                self.splat(man.x, man.y - dy * man.size, r * man.size);
            }
            if self.scenery.flying.len() < MAX_BALLS {
                self.scenery.flying.push(Ball {
                    x: man.x,
                    y: man.y - 70.0 * man.size,
                    vx: 260.0,
                    vy: -330.0,
                    radius: 11.0 * man.size,
                });
            }
        }
        let mut crumbled = Vec::new();
        for (i, stack) in self.scenery.stacks.iter_mut().enumerate() {
            if stack.y > floor && stack.x >= front - 6.0 && stack.x <= front + stack.radius {
                stack.x = front + stack.radius;
                stack.left -= step * 0.6;
                if stack.left < 88.0 - 50.0 * 0.6 {
                    crumbled.push(i);
                }
            }
        }
        for i in crumbled.into_iter().rev() {
            let stack = self.scenery.stacks.remove(i);
            self.splat(stack.x, stack.y - stack.radius, stack.radius);
        }
    }

    /// The flight path: a long, gentle swell high across the display.
    pub(crate) fn sleigh_path(&self, x: f32) -> f32 {
        self.height * 0.17 + (x * 0.0045 + 0.6).sin() * 22.0
    }

    /// Height and lean along the path, plus a small bob.
    fn sleigh_pose(&self, x: f32) -> (f32, f32) {
        let slope = 22.0 * 0.0045 * (x * 0.0045 + 0.6).cos();
        let bob = (self.clock * 2.3).sin() * 2.5;
        (self.sleigh_path(x) + bob, slope.atan() * 0.8)
    }

    /// Snow lying on decorations, from 0 to 1, only while it snows.
    fn snow_on_decorations(&self) -> f32 {
        if self.aura.effect == EffectKind::Snow {
            self.scenery.snow_cover
        } else {
            0.0
        }
    }

    pub(crate) fn draw_decorations(&self, paint: &mut Painter<'_>) {
        // Decorations stay solid while you work, because fading them whenever
        // the mouse moves would draw the eye instead of calming the desktop.
        let base = self.height;
        let presence = 1.0;
        for (i, tree) in self.scenery.decorations.trees.iter().enumerate() {
            let alpha = self.scenery.tree_fade[i] * presence;
            if alpha > 0.01 {
                self.draw_tree(paint, tree, base, alpha);
            }
        }
        let plain = SnowmanDecoration::default();
        for man in &self.scenery.built {
            if !self.covered(man.x, man.y - 8.0) {
                let look = Look {
                    tilt: (self.clock * 7.0 + man.x).sin() * man.wobble * 0.5,
                    headless: man.headless,
                    snow: self.snow_on_decorations() * 0.6,
                    ..Look::still(self.clock, man.x)
                };
                // On a window, only windows stacked above it hide it.
                paint.layer = self.layer_on(man.x, man.y, self.max_depth() + 4.0);
                draw_snowman(paint, man.x, man.y, man.size, &plain, presence, look);
            }
        }
        for stack in &self.scenery.stacks {
            let r = stack.radius;
            paint.layer = self.layer_on(stack.x, stack.y, self.max_depth() + 4.0);
            draw_snowball(paint, stack.x, stack.y - r, r, presence);
            if stack.count > 1 {
                let top = r * 0.72;
                draw_snowball(paint, stack.x, stack.y - r * 2.0 - top * 0.7, top, presence);
            }
        }
        paint.layer = 0.0;
        for ball in self.scenery.flying.iter().chain(&self.scenery.rolling) {
            draw_snowball(paint, ball.x, ball.y, ball.radius, presence);
        }
        if let Some(pack) = self.scenery.packing {
            // The ball grows in the hand while a ring of dots fills up.
            let done = (pack.held / HOLD_SECONDS).clamp(0.0, 1.0);
            let r = pack.radius * (0.3 + 0.7 * done);
            draw_snowball(paint, pack.x, pack.y + r * 0.4, r, done.sqrt());
            let ring = pack.radius + 9.0;
            for i in 0..HOLD_DOTS {
                let share = i as f32 / HOLD_DOTS as f32;
                let angle = share * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let (x, y) = (
                    pack.x + angle.cos() * ring,
                    pack.y + r * 0.4 + angle.sin() * ring,
                );
                let lit = share < done;
                let (size, alpha) = if lit { (2.2, 0.95) } else { (1.6, 0.35) };
                // A dark rim keeps the dots readable over white snow.
                let rim = size + 1.3;
                paint.put(
                    x,
                    y,
                    rim,
                    rim,
                    0.0,
                    SHAPE_DISK,
                    INK,
                    0.45 * alpha * presence,
                );
                paint.put(x, y, size, size, 0.0, SHAPE_DISK, SNOW, alpha * presence);
            }
        }
    }

    fn draw_tree(&self, paint: &mut Painter<'_>, tree: &TreeDecoration, base: f32, alpha: f32) {
        let s = tree.size;
        let cx = tree.x * self.width;
        paint.put(
            cx,
            base - 9.0 * s,
            7.0 * s,
            9.0 * s,
            0.0,
            SHAPE_RECT,
            BARK,
            alpha,
        );
        let bottom = base - 16.0 * s;
        let mut tier_base = bottom;
        for (i, color) in PINE.iter().enumerate() {
            let hh = (42.0 - i as f32 * 5.0) * s;
            let hw = (58.0 - i as f32 * 14.0) * s;
            paint.put(
                cx,
                tier_base - hh,
                hw,
                hh,
                0.0,
                SHAPE_TRIANGLE,
                *color,
                alpha,
            );
            tier_base -= hh * 1.15;
        }
        let cover = self.snow_on_decorations();
        if cover > 0.02 {
            // Snow settles on each tier: a cap at the top and lumps along
            // the lower edge, both growing as it keeps snowing.
            let mut tier_base = bottom;
            for i in 0..PINE.len() {
                let hh = (42.0 - i as f32 * 5.0) * s;
                let hw = (58.0 - i as f32 * 14.0) * s;
                let apex = tier_base - hh * 2.0;
                let f = 0.24 * cover;
                paint.put(
                    cx,
                    apex + hh * f,
                    hw * f,
                    hh * f,
                    0.0,
                    SHAPE_TRIANGLE,
                    SNOW,
                    alpha,
                );
                for k in [-0.72_f32, -0.25, 0.3, 0.75] {
                    let r = (3.2 + 1.8 * (k * 7.0).sin().abs()) * s * cover;
                    let x = cx + k * hw * 0.95;
                    let y = tier_base - hh * 0.04 - (1.0 - k.abs()) * 1.5 * s;
                    paint.put(x, y, r * 1.5, r, 0.0, SHAPE_DOME, SNOW, alpha);
                }
                tier_base -= hh * 1.15;
            }
        }
        let tip = bottom - 150.0 * s * 0.86;
        let height = bottom - tip;
        let spot = |i: usize, a: f32, b: f32| {
            let down = 0.16 + 0.8 * scatter(i, a);
            let side = (scatter(i, b) * 2.0 - 1.0) * 0.82;
            [cx + side * 56.0 * s * down, tip + height * down]
        };
        if tree.lights {
            for i in 0..14 {
                let [x, y] = spot(i, 0.6180, 0.2720);
                let twinkle = 0.45 + 0.55 * (self.clock * 1.6 + i as f32 * 1.9).sin().abs();
                let r = 5.5 * s;
                paint.put(x, y, r, r, 0.0, SHAPE_GLOW, GOLD, alpha * twinkle);
            }
        }
        if tree.baubles {
            let colors = [ORANGE, SAND, BERRY];
            for i in 0..9 {
                let [x, y] = spot(i, 0.7549, 0.5698);
                let r = 5.2 * s;
                paint.put(x, y, r, r, 0.0, SHAPE_DISK, colors[i % 3], alpha);
            }
        }
        if tree.candy {
            for i in 0..4 {
                let [x, y] = spot(i + 3, 0.4142, 0.8660);
                let tilt = 0.45 - i as f32 * 0.3;
                paint.put(x, y, 1.5 * s, 6.5 * s, tilt, SHAPE_RECT, SNOW, alpha);
                paint.put(x, y, 1.6 * s, 2.0 * s, tilt, SHAPE_RECT, BERRY, alpha);
            }
        }
        if tree.star {
            let y = tip - 4.0 * s;
            paint.put(
                cx,
                y,
                24.0 * s,
                24.0 * s,
                0.0,
                SHAPE_SOFT,
                GOLD,
                alpha * 0.35,
            );
            paint.put(cx, y, 14.0 * s, 14.0 * s, 0.0, SHAPE_SPARKLE, GOLD, alpha);
        }
    }

    pub(crate) fn draw_passers(&self, paint: &mut Painter<'_>) {
        if let Some(x) = self.scenery.plough {
            self.draw_plough(paint, x);
        }
        if let Some(x) = self.scenery.sleigh {
            self.draw_sleigh(paint, x);
        }
    }

    fn draw_sleigh(&self, paint: &mut Painter<'_>, x: f32) {
        let (y, tilt) = self.sleigh_pose(x);
        // The driver waves as he crosses the middle of the display, and
        // blinks every few seconds.
        let middle = (x - self.width * 0.42).abs() < self.width * 0.08;
        let frame = if middle {
            sprites::SLEIGH + 2 + ((self.clock * 4.0) as usize % 2)
        } else if (self.clock * 0.31).fract() < 0.04 {
            sprites::SLEIGH + 1
        } else {
            sprites::SLEIGH
        };
        let hitch = place([x, y], 1.0, tilt, anchor::SLEIGH_HITCH);
        let hand = place([x, y], 1.0, tilt, anchor::SLEIGH_HAND);
        let mut harness = [[0.0; 2]; 2];
        let mut deer = [(0usize, 0.0, 0.0, 0.0); 2];
        for (i, ahead) in DEER_AHEAD.into_iter().enumerate() {
            let dx = x + ahead;
            let (dy, dtilt) = self.sleigh_pose(dx);
            let dy = dy - 8.0 - i as f32 * 3.0;
            let step = (self.clock * GALLOP_RATE + i as f32 * 0.37).fract();
            let f = (step * sprites::DEER_FRAMES as f32) as usize % sprites::DEER_FRAMES;
            deer[i] = (sprites::DEER + f, dx, dy, dtilt);
            harness[i] = place([dx, dy], 1.0, dtilt, anchor::DEER_HARNESS[f]);
        }
        // Gold traces join the sleigh to the team, and the reins run from
        // the driver's hands. Both sag a little between their ends.
        let sag = |a: [f32; 2], b: [f32; 2], depth: f32| {
            let m = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5 + depth];
            [(a, m), (m, b)]
        };
        for (a, b) in sag(hitch, harness[0], 5.0)
            .into_iter()
            .chain(sag(harness[0], harness[1], 4.0))
        {
            paint.line(a, b, 1.2, SHAPE_RECT, GOLD, 0.95);
        }
        let lantern = place([x, y], 1.0, tilt, anchor::SLEIGH_LANTERN);
        let flicker = 0.85 + 0.15 * (self.clock * 9.0).sin() * (self.clock * 5.3).cos();
        paint.put(
            lantern[0],
            lantern[1],
            46.0,
            46.0,
            0.0,
            SHAPE_SOFT,
            GOLD,
            0.16 * flicker,
        );
        paint.sprite(frame, x, y, 1.0, tilt, WHITE, 1.0);
        paint.put(
            lantern[0],
            lantern[1],
            9.0,
            9.0,
            0.0,
            SHAPE_GLOW,
            GOLD,
            0.75 * flicker,
        );
        for (a, b) in sag(hand, harness[0], 7.0)
            .into_iter()
            .chain(sag(hand, harness[1], 9.0))
        {
            paint.line(a, b, 0.7, SHAPE_RECT, REIN, 0.9);
        }
        for &(f, dx, dy, dtilt) in deer.iter().rev() {
            paint.sprite(f, dx, dy, 1.0, dtilt, WHITE, 1.0);
        }
    }

    fn draw_plough(&self, paint: &mut Painter<'_>, x: f32) {
        let ground = self.height;
        // The engine's judder, and a soft shadow on the snow.
        let y = self.plough_ground() + (self.clock * 13.0).sin() * 0.6;
        paint.put(
            x + 12.0,
            ground - 2.0,
            104.0,
            9.0,
            0.0,
            SHAPE_SOFT,
            INK,
            0.3,
        );
        let headlight = place([x, y], 1.0, 0.0, anchor::PLOUGH_HEADLIGHT);
        paint.put(
            headlight[0] + 60.0,
            headlight[1] + 16.0,
            62.0,
            18.0,
            0.12,
            SHAPE_PUFF,
            GOLD,
            0.16,
        );
        // The driver waves while crossing the middle of the screen, and
        // otherwise blinks now and then.
        let middle = (x + PLOUGH_WIDTH * 0.5 - self.width * 0.5).abs() < self.width * 0.14;
        let frame = if middle {
            sprites::PLOUGH + 2 + ((self.clock * 3.2) as usize % 2)
        } else {
            let blink = (self.clock * 0.27 + 0.4).fract() < 0.04;
            sprites::PLOUGH + usize::from(blink)
        };
        paint.sprite(frame, x, y, 1.0, 0.0, WHITE, 1.0);
        for (anchor, scale) in [
            (anchor::PLOUGH_REAR_WHEEL, 1.0),
            (anchor::PLOUGH_FRONT_WHEEL, FRONT_WHEEL_SCALE),
        ] {
            let [wx, wy] = place([x, y], 1.0, 0.0, anchor);
            let turn = x / (REAR_WHEEL_RADIUS * scale);
            paint.sprite(sprites::WHEEL, wx, wy, scale, turn, WHITE, 1.0);
        }
        paint.put(
            headlight[0],
            headlight[1],
            8.0,
            8.0,
            0.0,
            SHAPE_GLOW,
            GOLD,
            0.8,
        );
        // The beacon turns, so it flashes twice a second.
        let [bx, by] = place([x, y], 1.0, 0.0, anchor::PLOUGH_BEACON);
        let flash = ((self.clock * 12.6).sin() * 0.5 + 0.5).powi(3);
        paint.put(bx, by, 44.0, 44.0, 0.0, SHAPE_SOFT, AMBER, 0.22 * flash);
        paint.put(
            bx,
            by,
            10.0,
            10.0,
            0.0,
            SHAPE_GLOW,
            AMBER,
            0.35 + 0.65 * flash,
        );
        // Snow heaped and curling against the blade, shading into the pushed wave.
        let heap = self.berm_radius();
        if heap > 2.0 {
            let front = x + PLOUGH_WIDTH;
            let shade = [0.82, 0.87, 0.97];
            for (dx, k, col) in [
                (-3.0, 1.05, shade),
                (-1.0, 0.92, SNOW),
                (heap * 0.65, 0.78, SNOW),
                (heap * 1.25, 0.52, [0.92, 0.95, 1.0]),
            ] {
                let r = heap * k;
                paint.put(front + dx, ground, r * 1.32, r, 0.0, SHAPE_DOME, col, 1.0);
            }
        }
    }
}

/// How a snowman looks right now.
#[derive(Clone, Copy, Debug)]
struct Look {
    /// Lean to the right, in radians. The head leans most.
    tilt: f32,
    blink: bool,
    headless: bool,
    /// Snow settled on the hat or head, from 0 to 1.
    snow: f32,
}

impl Look {
    /// Upright, blinking every few seconds at its own moment.
    fn still(clock: f32, x: f32) -> Self {
        Self {
            tilt: 0.0,
            blink: (clock * 0.23 + x * 0.0137).fract() < 0.035,
            headless: false,
            snow: 0.0,
        }
    }
}

/// A packed snowball, lit from the upper left: cool shade below, a bright
/// face and a soft highlight.
fn draw_snowball(paint: &mut Painter<'_>, x: f32, y: f32, r: f32, alpha: f32) {
    let shade = [0.74, 0.8, 0.94];
    paint.put(x, y, r, r, 0.0, SHAPE_DISK, shade, alpha);
    let lit = r * 0.84;
    paint.put(
        x - r * 0.12,
        y - r * 0.12,
        lit,
        lit,
        0.0,
        SHAPE_DISK,
        SNOW,
        alpha,
    );
    let glow = r * 0.42;
    paint.put(
        x - r * 0.34,
        y - r * 0.36,
        glow,
        glow,
        0.0,
        SHAPE_SOFT,
        WHITE,
        0.8 * alpha,
    );
}

fn draw_snowman(
    paint: &mut Painter<'_>,
    x: f32,
    base: f32,
    s: f32,
    man: &SnowmanDecoration,
    alpha: f32,
    look: Look,
) {
    let (low, mid, top) = (23.0 * s, 16.5 * s, 11.5 * s);
    let (low_y, mid_y) = (base - low * 0.92, base - low * 1.75 - mid * 0.7);
    let top_y = mid_y - mid * 0.82 - top * 0.8;
    // Leaning shifts each ball sideways by its height above the ground.
    let lean = |y: f32| x + (base - y) * look.tilt.sin();
    let (mid_x, top_x) = (lean(mid_y), lean(top_y));
    paint.put(x, low_y, low, low, 0.0, SHAPE_DISK, SNOW, alpha);
    paint.put(mid_x, mid_y, mid, mid, 0.0, SHAPE_DISK, SNOW, alpha);
    for i in 0..3 {
        let y = mid_y - 6.0 * s + i as f32 * 6.5 * s;
        paint.put(lean(y), y, 1.6 * s, 1.6 * s, 0.0, SHAPE_DISK, INK, alpha);
    }
    if look.headless {
        // A neat flat stump where the head was.
        paint.put(
            mid_x,
            mid_y - mid * 0.86,
            mid * 0.55,
            1.6 * s,
            look.tilt,
            SHAPE_RECT,
            SAND,
            alpha,
        );
        if man.scarf {
            let y = mid_y - mid * 0.7;
            paint.put(
                lean(y),
                y,
                12.5 * s,
                2.6 * s,
                look.tilt,
                SHAPE_RECT,
                ORANGE,
                alpha,
            );
        }
        return;
    }
    paint.put(top_x, top_y, top, top, 0.0, SHAPE_DISK, SNOW, alpha);
    let eye = if look.blink { 0.3 } else { 1.5 };
    for dx in [-4.0, 4.0] {
        paint.put(
            top_x + dx * s,
            top_y - 2.0 * s,
            1.5 * s,
            eye * s,
            look.tilt,
            SHAPE_DISK,
            INK,
            alpha,
        );
    }
    paint.put(
        top_x + 7.0 * s,
        top_y + 1.5 * s,
        2.0 * s,
        6.5 * s,
        std::f32::consts::FRAC_PI_2 + look.tilt,
        SHAPE_TRIANGLE,
        ORANGE,
        alpha,
    );
    if man.scarf {
        let y = top_y + top * 0.92;
        let sx = lean(y);
        paint.put(
            sx,
            y,
            12.5 * s,
            2.6 * s,
            look.tilt,
            SHAPE_RECT,
            ORANGE,
            alpha,
        );
        paint.put(
            sx + 7.0 * s,
            y + 7.0 * s,
            2.6 * s,
            6.5 * s,
            -0.12 + look.tilt,
            SHAPE_RECT,
            ORANGE,
            alpha,
        );
    }
    if man.hat {
        let y = top_y - top * 0.82;
        let hx = lean(y);
        paint.put(hx, y, 14.0 * s, 1.8 * s, look.tilt, SHAPE_RECT, INK, alpha);
        paint.put(
            lean(y - 8.5 * s),
            y - 8.5 * s,
            8.5 * s,
            7.5 * s,
            look.tilt,
            SHAPE_RECT,
            INK,
            alpha,
        );
        paint.put(
            lean(y - 3.2 * s),
            y - 3.2 * s,
            8.5 * s,
            1.5 * s,
            look.tilt,
            SHAPE_RECT,
            ORANGE,
            alpha,
        );
    }
    if look.snow > 0.02 {
        // A little snow on top of the hat, or on the head without one.
        let (y, w) = if man.hat {
            (top_y - top * 0.82 - 16.0 * s, 8.8 * s)
        } else {
            (top_y - top * 0.86, 8.0 * s)
        };
        paint.put(
            lean(y),
            y + 0.5 * s,
            w,
            3.4 * s * look.snow,
            look.tilt,
            SHAPE_DOME,
            SNOW,
            alpha,
        );
    }
}
