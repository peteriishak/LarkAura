//! CPU particle simulation. It is deterministic for a given seed and frame
//! sequence and knows nothing about graphics APIs or desktops beyond
//! `DesktopModel`.

mod masks;
mod piles;
mod scenery;
mod snow_visitors;
pub mod sprites;
mod visitors;

use aura_core::{Decorations, DesktopModel, Rect, Trigger, Tuning};
use aura_format::{Aura, EffectKind};
use bytemuck::{Pod, Zeroable};
use masks::Masks;
pub use masks::{MAX_MASKS, Mask};
use piles::{Piles, Shed};
use scenery::Scenery;
use visitors::{HEART_SECONDS, Visitors, heart_point};

pub const SHAPE_SOFT: f32 = 0.0;
pub const SHAPE_STREAK: f32 = 1.0;
pub const SHAPE_PUFF: f32 = 2.0;
pub const SHAPE_GLOW: f32 = 3.0;
pub const SHAPE_LEAF: f32 = 4.0;
pub const SHAPE_PETAL: f32 = 5.0;
pub const SHAPE_DISK: f32 = 6.0;
pub const SHAPE_RECT: f32 = 7.0;
pub const SHAPE_TRIANGLE: f32 = 8.0;
pub const SHAPE_SPARKLE: f32 = 9.0;
pub const SHAPE_DOME: f32 = 10.0;
pub const SHAPE_BEAM: f32 = 11.0;
/// A raindrop: a darker body with a pale core, visible on any wallpaper.
pub const SHAPE_RAIN: f32 = 12.0;
/// A soft fluffy snowflake, shaded by depth blur in extra.x.
pub const SHAPE_FLAKE: f32 = 13.0;
/// A column of piled snow whose top slopes between two heights.
pub const SHAPE_SNOWBANK: f32 = 14.0;
/// A tumbling irregular snow clump with upper-left shading.
pub const SHAPE_CLUMP: f32 = 15.0;

const MAX_DT: f32 = 0.1;
const REFERENCE_AREA: f32 = 1920.0 * 1080.0;
pub const MAX_PARTICLES: usize = 960;
/// Room for particles plus piles, flying debris, decorations and passers-by.
pub const MAX_INSTANCES: usize = 2800;
const MAX_DEBRIS: usize = 480;
const MAX_PILE_INSTANCES: usize = 960;
/// Snow keeps a share of its flakes in reserve for the opening flurry.
const SNOW_HEADROOM: f32 = 1.35;
/// Seconds the opening flurry takes to ease off.
const FLURRY_SECONDS: f32 = 45.0;
/// Flakes nearer than this land and pile up; farther ones fall past.
const LANDING_DEPTH: f32 = 0.22;
const CURSOR_RADIUS: f32 = 130.0;
const MAX_PILE_DEPTH: f32 = 260.0;
/// Seconds of motion a falling flake is smeared over, like a short exposure.
const MOTION_SMEAR: f32 = 1.0 / 110.0;
/// Seconds a flake stays visible on piled snow after landing.
const SETTLE_SECONDS: f32 = 0.55;
/// At most this share of leaves or petals may lie on surfaces at once.
const MAX_RESTING_SHARE: f32 = 0.65;
/// The cool, dark tint of shadows that snow casts.
const SNOW_SHADOW: [f32; 3] = [0.04, 0.06, 0.14];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct Instance {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    pub rotation: f32,
    pub shape: f32,
    /// Shape parameters; the snowbank keeps its two edge heights here.
    pub extra: [f32; 2],
    /// Premultiplied display color, matching how compositors blend surfaces.
    pub color: [f32; 4],
    /// Which window masks hide it: those from this index up. Falling
    /// effects use 0, so every window hides them; snow on a window's top is
    /// hidden only by windows stacked above that window.
    pub layer: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameInput<'a> {
    pub dt: f32,
    pub presence: f32,
    pub quality_scale: f32,
    pub desktop: &'a DesktopModel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Moving,
    Resting(f32),
    /// A flake that has just landed on piled snow, sinking into it.
    Settling(f32),
    Splash(f32),
    Waiting(f32),
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    seed: f32,
    /// 0 far away, 1 right in front of the viewer. Only snow uses it.
    depth: f32,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DebrisKind {
    Flake,
    /// A solid tumbling snow clump shaken from a window, spilled over a corner,
    /// or thrown forward by the snowplough.
    Clump,
    Spark,
    /// A tiny water droplet thrown up by a raindrop or a frog.
    Drop,
    /// Dust and bits of leaf lifted by a whirlwind.
    Dust,
    /// Stardust that twinkles behind the sleigh.
    Glint,
    /// A puff from the plough's exhaust.
    Smoke,
    /// Powder snow thrown up by the plough's blade or snow fracture.
    Powder,
}

#[derive(Clone, Copy, Debug)]
struct Debris {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    size: f32,
    kind: DebrisKind,
    /// Stable window id this debris fell from (0 for desktop/dock), keeping it
    /// visible over its source window while still hidden by windows above it.
    window_id: u64,
    angle: f32,
    spin: f32,
    /// Snow depth mass carried back to the floor drift when landing.
    mass: f32,
    seed: f32,
}

impl Debris {
    fn plain(x: f32, y: f32, vx: f32, vy: f32, life: f32, size: f32, kind: DebrisKind) -> Self {
        Self {
            x,
            y,
            vx,
            vy,
            life,
            size,
            kind,
            window_id: 0,
            angle: 0.0,
            spin: 0.0,
            mass: 0.0,
            seed: (x * 0.13 + y * 0.07).fract().abs() * std::f32::consts::TAU,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Pointer {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// Seconds since the pointer last moved noticeably.
    rest: f32,
    /// The primary button is held down.
    down: bool,
    /// The button went down since the last frame, even if it is up again.
    pressed: bool,
}

impl Pointer {
    fn speed(&self) -> f32 {
        (self.vx * self.vx + self.vy * self.vy).sqrt()
    }
}

const BOLT_POINTS: usize = 12;
const STORM_FLASH_SECONDS: f32 = 0.42;

/// Lightning. The sky flash is faint and flashes are at least nine seconds
/// apart, which keeps them far below the rate and contrast that can trouble
/// photosensitive people. Only the thin bolt is drawn strongly.
struct Storm {
    next: f32,
    age: Option<f32>,
    bolt: [[f32; 2]; BOLT_POINTS],
}

#[derive(Clone, Debug)]
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

pub struct Simulation {
    aura: Aura,
    intensity: f32,
    tuning: Tuning,
    rng: Rng,
    particles: Vec<Particle>,
    active: usize,
    resting: usize,
    width: f32,
    height: f32,
    /// Effect time, which follows the speed option.
    time: f32,
    /// Real time, for schedules that should not change with effect speed.
    clock: f32,
    presence: f32,
    pointer: Option<Pointer>,
    /// The desktop's press count at the last frame.
    presses_seen: Option<u32>,
    focused: Option<Rect>,
    windows: Vec<Rect>,
    window_ids: Vec<u64>,
    docks: Vec<Rect>,
    reacts_to_windows: bool,
    piles: Piles,
    spill_buf: Vec<Shed>,
    debris: Vec<Debris>,
    debris_next: usize,
    storm: Storm,
    scenery: Scenery,
    visitors: Visitors,
    /// The opening flurry, from 1 when snow starts down to 0.
    flurry: f32,
    /// The sleigh and plough this frame, for snow to swirl around.
    movers: [Option<scenery::Mover>; 2],
    /// Window outlines the renderer keeps clear.
    masks: Masks,
    /// For each window, whether snow may settle on its top. Fully opened
    /// windows are not ledges on the desktop.
    ledges: Vec<bool>,
    /// Fades everything back in after the effects were paused, from 0 to 1.
    wake: f32,
}

fn base_count(kind: EffectKind) -> f32 {
    match kind {
        EffectKind::Motes => 170.0,
        EffectKind::Snow => 440.0,
        EffectKind::Rain => 340.0,
        EffectKind::Fireflies => 64.0,
        EffectKind::Fog => 44.0,
        EffectKind::Leaves => 150.0,
        EffectKind::Blossom => 230.0,
        EffectKind::Starfall => 36.0,
    }
}

/// How fast a flake at `depth` falls, with a little variety from its seed.
fn snow_fall_speed(depth: f32, seed: f32) -> f32 {
    (46.0 + depth * 124.0) * (0.88 + 0.24 * (seed * 2.31).fract().abs())
}

/// The highest window top crossed between `y0` and `y1` at column `x` that is
/// not hidden by a window above it. Windows are ordered bottom to top.
fn landing(windows: &[Rect], x: f32, y0: f32, y1: f32) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (i, w) in windows.iter().enumerate() {
        if x < w.x || x >= w.right() || w.y < y0 || w.y > y1 {
            continue;
        }
        let covered = windows[i + 1..].iter().any(|o| o.contains(x, w.y - 0.5));
        if !covered && best.is_none_or(|(_, b)| w.y < b) {
            best = Some((i, w.y));
        }
    }
    best
}

/// Now and then the wind picks up for a few seconds, then settles again.
/// Returns 0 when calm and up to 1 at the height of a gust.
fn gust_at(t: f32) -> f32 {
    let wave = (t * 0.083).sin() * (t * 0.211 + 1.1).sin();
    (wave.max(0.0) * 1.6).min(1.0).powi(2)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A slowly changing field of eddies in the air, as a velocity of about
/// unit size. It is the curl of a smooth function, so it has no sources or
/// sinks: neighbouring flakes curl round together instead of bunching up.
fn swirl(x: f32, y: f32, t: f32) -> (f32, f32) {
    const A: f32 = 0.0045;
    const C: f32 = 0.0095;
    let (sx, cx) = (x * A + t * 0.21).sin_cos();
    let (sy, cy) = (y * A - t * 0.17).sin_cos();
    let cz = (x * C - y * C * 0.8 + t * 0.33).cos();
    // Stream function sin(X)cos(Y) + 0.5 sin(Z); velocity is (d/dy, -d/dx).
    let dx = A * cx * cy + 0.5 * C * cz;
    let dy = -A * sx * sy - 0.4 * C * cz;
    (dy / A, -dx / A)
}

fn has_surface_at(windows: &[Rect], x: f32, y: f32) -> bool {
    windows.iter().enumerate().any(|(i, w)| {
        (w.y - y).abs() < 1.5
            && x >= w.x
            && x < w.right()
            && !windows[i + 1..].iter().any(|o| o.contains(x, w.y - 0.5))
    })
}

/// Where a point `offset` from a sprite's origin lands once the sprite is
/// drawn at `origin`, scaled and turned the way the shader turns it.
pub(crate) fn place(origin: [f32; 2], scale: f32, rotation: f32, offset: [f32; 2]) -> [f32; 2] {
    let (s, c) = rotation.sin_cos();
    let [ox, oy] = [offset[0] * scale, offset[1] * scale];
    [origin[0] + ox * c - oy * s, origin[1] + ox * s + oy * c]
}

/// Writes instances, dropping anything off screen or beyond the buffer.
pub(crate) struct Painter<'a> {
    out: &'a mut Vec<Instance>,
    width: f32,
    height: f32,
    /// The mask layer for what is drawn next.
    pub(crate) layer: f32,
}

impl Painter<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put(
        &mut self,
        x: f32,
        y: f32,
        hw: f32,
        hh: f32,
        rotation: f32,
        shape: f32,
        [r, g, b]: [f32; 3],
        a: f32,
    ) {
        let reach = hw.abs().max(hh.abs()) + 300.0;
        if a > 0.003
            && self.out.len() < MAX_INSTANCES
            && x > -reach
            && x < self.width + reach
            && y > -reach
            && y < self.height + reach
        {
            self.out.push(Instance {
                center: [x, y],
                half_size: [hw, hh],
                rotation,
                shape,
                extra: [0.0; 2],
                color: [r * a, g * a, b * a, a],
                layer: self.layer,
            });
        }
    }

    /// Like `put`, with shape parameters.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put_extra(
        &mut self,
        x: f32,
        y: f32,
        hw: f32,
        hh: f32,
        shape: f32,
        extra: [f32; 2],
        color: [f32; 3],
        a: f32,
    ) {
        let before = self.out.len();
        self.put(x, y, hw, hh, 0.0, shape, color, a);
        if let Some(item) = self.out.get_mut(before) {
            item.extra = extra;
        }
    }

    /// Like `put_extra`, turned by `rotation`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put_turned(
        &mut self,
        x: f32,
        y: f32,
        hw: f32,
        hh: f32,
        rotation: f32,
        shape: f32,
        extra: [f32; 2],
        color: [f32; 3],
        a: f32,
    ) {
        let before = self.out.len();
        self.put(x, y, hw, hh, rotation, shape, color, a);
        if let Some(item) = self.out.get_mut(before) {
            item.extra = extra;
        }
    }

    /// Draws sprite `frame` with the model's origin at (x, y).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sprite(
        &mut self,
        frame: usize,
        x: f32,
        y: f32,
        scale: f32,
        rotation: f32,
        tint: [f32; 3],
        alpha: f32,
    ) {
        self.sprite_facing(frame, x, y, scale, rotation, 1.0, tint, alpha);
    }

    /// Like `sprite`, mirrored left to right when `dir` is negative, for
    /// visitors that can travel either way.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sprite_facing(
        &mut self,
        frame: usize,
        x: f32,
        y: f32,
        scale: f32,
        rotation: f32,
        dir: f32,
        tint: [f32; 3],
        alpha: f32,
    ) {
        let f = &sprites::FRAMES[frame];
        let [w, h] = f.size();
        let flip = if dir < 0.0 { -1.0 } else { 1.0 };
        let offset = [
            flip * (f.rect[2] as f32 * 0.5 - f.pivot[0]) / sprites::SCALE,
            (f.rect[3] as f32 * 0.5 - f.pivot[1]) / sprites::SCALE,
        ];
        let [cx, cy] = place([x, y], scale, rotation, offset);
        let shape = sprites::SHAPE_SPRITE + frame as f32;
        // A negative half width mirrors the quad, and the atlas with it.
        self.put(
            cx,
            cy,
            flip * w * 0.5 * scale,
            h * 0.5 * scale,
            rotation,
            shape,
            tint,
            alpha,
        );
    }

    /// A line between two points.
    pub(crate) fn line(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        width: f32,
        shape: f32,
        color: [f32; 3],
        alpha: f32,
    ) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt().max(0.01);
        self.put(
            (a[0] + b[0]) * 0.5,
            (a[1] + b[1]) * 0.5,
            width,
            length * 0.5,
            (-dx).atan2(dy),
            shape,
            color,
            alpha,
        );
    }
}

impl Simulation {
    pub fn new(aura: Aura, seed: u32, width: f32, height: f32) -> Self {
        let intensity = aura.intensity;
        let (width, height) = (width.max(1.0), height.max(1.0));
        let mut sim = Self {
            aura,
            intensity,
            tuning: Tuning::default(),
            rng: Rng(seed.max(1)),
            particles: Vec::new(),
            active: 0,
            resting: 0,
            width,
            height,
            time: 0.0,
            clock: 0.0,
            presence: 1.0,
            pointer: None,
            presses_seen: None,
            focused: None,
            windows: Vec::new(),
            window_ids: Vec::new(),
            docks: Vec::new(),
            reacts_to_windows: true,
            piles: Piles::new(width, height),
            spill_buf: Vec::new(),
            debris: Vec::with_capacity(MAX_DEBRIS),
            debris_next: 0,
            storm: Storm {
                next: 6.0,
                age: None,
                bolt: [[0.0; 2]; BOLT_POINTS],
            },
            scenery: Scenery::new(),
            visitors: Visitors::new(),
            flurry: 0.0,
            movers: [None; 2],
            masks: Masks::default(),
            ledges: Vec::new(),
            wake: 1.0,
        };
        sim.reseed();
        sim.lay_blanket();
        sim
    }

    pub fn aura(&self) -> &Aura {
        &self.aura
    }

    pub fn active_particles(&self) -> usize {
        self.active
    }

    /// The window outlines the renderer keeps clear this frame, ordered
    /// like the instances' layers.
    pub fn masks(&self) -> &[Mask] {
        self.masks.masks()
    }

    /// Effects stay off windows, so windows covering the whole desktop
    /// leave nothing to draw.
    pub fn keeps_windows_clear(&self) -> bool {
        self.tuning.placement.keeps_windows_clear()
    }

    /// Eases everything back in, for when effects return after a pause,
    /// such as after a full-screen window is restored.
    pub fn fade_in(&mut self) {
        self.wake = 0.0;
    }

    pub fn set_aura(&mut self, aura: Aura) {
        let keep_snow = aura.effect == EffectKind::Snow && self.aura.effect == EffectKind::Snow;
        self.intensity = aura.intensity;
        self.aura = aura;
        if !keep_snow {
            self.piles.clear();
            self.scenery.clear_snow_play();
        }
        self.debris.clear();
        self.storm.age = None;
        self.visitors.clear();
        self.reseed();
        if !keep_snow {
            self.lay_blanket();
        }
    }

    pub fn set_intensity(&mut self, value: f32) {
        self.intensity = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            self.intensity
        };
    }

    pub fn set_tuning(&mut self, tuning: Tuning) {
        self.tuning = tuning.sanitized();
        if !self.tuning.piles {
            self.piles.clear();
        } else {
            self.piles.limit(self.max_depth());
        }
    }

    pub fn set_decorations(&mut self, decorations: Decorations) {
        self.scenery.decorations = decorations.sanitized();
    }

    /// Places a finished snowman, for screenshots and checks by eye.
    #[doc(hidden)]
    pub fn debug_build_snowman(&mut self, x: f32, y: f32) {
        self.scenery.place_snowman(x, y);
    }

    pub fn trigger(&mut self, event: Trigger) {
        match event {
            Trigger::Sleigh => self.scenery.send_sleigh(),
            Trigger::Plough => self.scenery.send_plough(),
            Trigger::Visitor => {
                let surfaces = if self.reacts_to_windows {
                    self.windows.clone()
                } else {
                    Vec::new()
                };
                self.send_visitor(&surfaces);
            }
            Trigger::ShakeOff => {
                let shed = self.piles.shake_all();
                self.throw_snow(&shed);
                for p in &mut self.particles {
                    if matches!(p.phase, Phase::Resting(_)) {
                        p.phase = Phase::Moving;
                    }
                }
            }
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        let (width, height) = (width.max(1.0), height.max(1.0));
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            self.piles = Piles::new(width, height);
            self.scenery.clear_snow_play();
            self.visitors.clear();
            self.debris.clear();
            self.masks.clear();
            self.reseed();
            self.lay_blanket();
        }
    }

    /// Snow starts out already lying: a blanket along the bottom with
    /// drifts in the corners, caps on the windows that are open, and some on
    /// the decorations. It begins with a thick flurry that eases off.
    fn lay_blanket(&mut self) {
        if self.aura.effect != EffectKind::Snow {
            return;
        }
        self.flurry = 1.0;
        self.scenery.snow_cover = 0.4 + 0.5 * self.intensity;
        if !self.piles_on() {
            return;
        }
        // Starts already nicely drifted, below the plough's trigger share so
        // mounds keep building organically before the plough comes out.
        let depth = self.max_depth() * (0.32 + 0.16 * self.intensity);
        let phase = self.rng.range(0.0, std::f32::consts::TAU);
        self.piles
            .lay_blanket(depth, self.max_depth(), self.clock + 4.0, phase);
    }

    fn headroom(&self) -> f32 {
        if self.aura.effect == EffectKind::Snow {
            SNOW_HEADROOM
        } else {
            1.0
        }
    }

    fn capacity(&self) -> usize {
        let area = (self.width * self.height / REFERENCE_AREA).clamp(0.5, 2.0);
        ((base_count(self.aura.effect) * area * self.headroom()) as usize).clamp(1, MAX_PARTICLES)
    }

    fn wind(&self) -> f32 {
        self.tuning.wind.unwrap_or(self.aura.wind)
    }

    /// Now and then the wind picks up for a few seconds, then settles again.
    /// Returns 0 when calm and up to 1 at the height of a gust.
    fn gust(&self) -> f32 {
        gust_at(self.time)
    }

    /// The wind at one spot. Gusts arrive from upwind and sweep across the
    /// display, and slow eddies keep neighbouring flakes from moving as one.
    fn wind_at(&self, x: f32, y: f32) -> f32 {
        let wind = self.wind() + self.veer();
        let direction = if wind < 0.0 { -1.0 } else { 1.0 };
        let upwind = if direction > 0.0 { x } else { self.width - x };
        let gust = gust_at(self.time - upwind / 520.0);
        let t = self.time;
        let eddy = (y * 0.0071 + t * 0.43).sin() * (x * 0.0053 - t * 0.29).cos();
        wind + direction * gust * 0.75 + eddy * 0.22
    }

    /// The wind slowly backs and veers over a minute or two, so snow never
    /// drifts the same way for long. Strong aura winds keep their direction.
    fn veer(&self) -> f32 {
        let t = self.time;
        let swing = 0.16 * (t * 0.019 + 1.3).sin() + 0.08 * (t * 0.047).sin();
        swing * (1.0 - self.wind().abs()).clamp(0.3, 1.0)
    }

    /// How much a gust lifts the air at one spot, from 0 to 1.
    fn updraft_at(&self, x: f32) -> f32 {
        let direction = if self.wind() < 0.0 { -1.0 } else { 1.0 };
        let upwind = if direction > 0.0 { x } else { self.width - x };
        gust_at(self.time - upwind / 520.0 - 0.6)
    }

    /// The wind including gusts, which blow the way the wind already blows.
    fn breeze(&self) -> f32 {
        let wind = self.wind();
        let direction = if wind < 0.0 { -1.0 } else { 1.0 };
        wind + direction * self.gust() * 0.75
    }

    fn piles_on(&self) -> bool {
        self.tuning.piles && self.aura.effect == EffectKind::Snow
    }

    fn max_depth(&self) -> f32 {
        MAX_PILE_DEPTH * self.tuning.pile_depth
    }

    fn reseed(&mut self) {
        let n = self.capacity();
        self.particles.clear();
        self.particles.reserve_exact(n);
        for _ in 0..n {
            let p = self.spawn(true);
            self.particles.push(p);
        }
    }

    fn spawn(&mut self, anywhere: bool) -> Particle {
        let (w, h) = (self.width, self.height);
        let x = self.rng.range(-20.0, w + 20.0);
        let seed = self.rng.range(0.0, std::f32::consts::TAU);
        let mut p = Particle {
            x,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            size: 1.0,
            seed,
            depth: 1.0,
            phase: Phase::Moving,
        };
        let from_top = |rng: &mut Rng, reach: f32| {
            if anywhere {
                rng.range(-h, h)
            } else {
                -rng.range(4.0, reach)
            }
        };
        match self.aura.effect {
            EffectKind::Motes => {
                p.y = if anywhere {
                    self.rng.range(0.0, h)
                } else {
                    h + 10.0
                };
                p.vy = -self.rng.range(5.0, 18.0);
                p.size = self.rng.range(1.2, 3.4);
            }
            EffectKind::Snow => {
                // Plenty of small distant flakes for parallax depth alongside
                // plump midground and noticeably larger fluffy foreground flakes.
                let z = self.rng.next().powf(1.55);
                p.depth = z;
                p.y = if anywhere {
                    self.rng.range(-0.1 * h, h)
                } else {
                    -self.rng.range(6.0, 75.0)
                };
                // Now and then several crystals stick together into a bigger,
                // fluffier snow clump.
                let clump = if self.rng.next() < 0.14 {
                    self.rng.range(1.28, 1.72)
                } else {
                    1.0
                };
                p.size = (0.95 + z * 2.1 + z * z * 4.0) * self.rng.range(0.85, 1.18) * clump;
                p.vy = snow_fall_speed(z, seed);
                p.vx = self.wind_at(x, p.y) * 42.0 * (0.55 + 0.45 * z);
            }
            EffectKind::Rain => {
                p.y = from_top(&mut self.rng, 160.0);
                p.size = self.rng.range(0.7, 1.3);
                p.vy = self.rng.range(520.0, 760.0);
            }
            EffectKind::Fireflies => {
                p.y = self.rng.range(h * 0.2, h);
                p.size = self.rng.range(2.0, 3.4);
                p.vx = self.rng.range(-12.0, 12.0);
                p.vy = self.rng.range(-10.0, 10.0);
            }
            EffectKind::Fog => {
                p.y = h - self.rng.range(0.0, h * 0.2);
                p.size = self.rng.range(110.0, 240.0);
                p.vx = self.rng.range(4.0, 12.0);
            }
            EffectKind::Leaves => {
                p.y = from_top(&mut self.rng, 80.0);
                p.size = self.rng.range(6.0, 11.0);
            }
            EffectKind::Blossom => {
                p.y = from_top(&mut self.rng, 60.0);
                p.size = self.rng.range(4.2, 7.2);
            }
            EffectKind::Starfall => {
                let direction = if self.wind() < 0.0 { -1.0 } else { 1.0 };
                p.x = if direction > 0.0 {
                    self.rng.range(-0.15 * w, 0.8 * w)
                } else {
                    self.rng.range(0.2 * w, 1.15 * w)
                };
                p.y = self.rng.range(-40.0, h * 0.3);
                p.vx = direction * self.rng.range(240.0, 420.0);
                p.vy = self.rng.range(320.0, 560.0);
                p.size = self.rng.range(0.9, 1.7);
                let wait = if anywhere { 9.0 } else { 7.0 };
                p.phase = Phase::Waiting(self.rng.range(0.3, wait));
            }
        }
        p
    }

    pub fn step(&mut self, input: FrameInput<'_>) {
        let real_dt = input.dt.clamp(0.0, MAX_DT);
        let dt = real_dt * self.tuning.speed;
        self.time += dt;
        self.clock += real_dt;
        self.presence = input.presence.clamp(0.0, 1.0);
        self.focused = input.desktop.focused;
        self.track_pointer(real_dt, input.desktop);

        let mut windows = std::mem::take(&mut self.windows);
        windows.clear();
        windows.extend(input.desktop.windows.iter().map(|w| w.rect()));
        self.window_ids.clear();
        self.window_ids
            .extend(input.desktop.windows.iter().map(|w| w.id));
        self.docks.clear();
        self.docks
            .extend(input.desktop.docks.iter().map(|d| d.rect()));
        self.wake = (self.wake + real_dt / 0.6).min(1.0);
        let clear = self.keeps_windows_clear();
        self.masks.update(&input.desktop.windows, real_dt, clear);
        let (width, height) = (self.width, self.height);
        self.ledges.clear();
        self.ledges.extend(
            windows
                .iter()
                .map(|w| !(clear && w.is_large(width, height))),
        );
        self.reacts_to_windows = self.tuning.windows && self.aura.respond_to_windows;
        let no_windows: &[Rect] = &[];
        let surfaces = if self.reacts_to_windows {
            windows.as_slice()
        } else {
            no_windows
        };

        if self.piles_on() {
            let tracked = if self.reacts_to_windows {
                input.desktop.windows.as_slice()
            } else {
                &[]
            };
            self.piles.note_max(self.max_depth());
            let shed = self.piles.sync(
                tracked,
                &self.ledges,
                &input.desktop.docks,
                self.clock,
                real_dt,
            );
            self.throw_snow(&shed);
        }

        self.flurry = (self.flurry - real_dt / FLURRY_SECONDS).max(0.0);
        let flurry = 1.0 + (SNOW_HEADROOM - 1.0) * smoothstep(0.0, 1.0, self.flurry);
        let density =
            self.intensity * input.quality_scale.clamp(0.0, 1.0) * (0.45 + 0.55 * self.presence);
        let share = density * flurry / self.headroom();
        self.active =
            ((self.particles.len() as f32 * share).ceil() as usize).min(self.particles.len());
        self.movers = self.scenery.movers(self.height, |x| self.sleigh_path(x));
        let mut resting = 0;
        let active = self.active;
        self.spill_buf.clear();
        for i in 0..self.active {
            let mut p = self.particles[i];
            let respawn = match self.aura.effect {
                EffectKind::Motes => self.step_mote(&mut p, dt),
                EffectKind::Snow => self.step_snow(&mut p, dt, surfaces),
                EffectKind::Rain => self.step_rain(&mut p, dt, surfaces),
                EffectKind::Fireflies => self.step_firefly(&mut p, dt, i, active),
                EffectKind::Fog => self.step_fog(&mut p, dt),
                EffectKind::Leaves | EffectKind::Blossom => self.step_flutter(&mut p, dt, surfaces),
                EffectKind::Starfall => self.step_meteor(&mut p, dt, surfaces),
            };
            resting += usize::from(matches!(p.phase, Phase::Resting(_)));
            self.particles[i] = if respawn { self.spawn(false) } else { p };
        }
        if !self.spill_buf.is_empty() {
            let mut spilled = std::mem::take(&mut self.spill_buf);
            self.throw_snow(&spilled);
            spilled.clear();
            self.spill_buf = spilled;
        }
        self.resting = resting;
        self.step_debris(dt, surfaces);
        self.step_storm(real_dt, surfaces);
        self.step_scenery(real_dt, surfaces, &windows);
        self.step_visitor_schedule(real_dt, surfaces);
        self.step_visitors_motion(real_dt, surfaces);
        self.windows = windows;
    }

    fn track_pointer(&mut self, dt: f32, desktop: &DesktopModel) {
        let enabled = self.tuning.pointer && self.aura.respond_to_cursor;
        let presses = desktop.presses;
        let pressed = self.presses_seen.is_some_and(|seen| seen != presses);
        self.presses_seen = Some(presses);
        let Some([x, y]) = desktop.cursor.filter(|_| enabled) else {
            self.pointer = None;
            return;
        };
        let p = self.pointer.get_or_insert(Pointer {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            rest: 0.0,
            down: false,
            pressed: false,
        });
        p.down = desktop.pointer_down;
        p.pressed = pressed;
        if pressed {
            // A press is where the pointer really is, not where it is easing to.
            (p.x, p.y) = (x, y);
        }
        // The desktop reports the pointer less often than frames are drawn.
        let k = 1.0 - (-dt * 20.0).exp();
        let (nx, ny) = (p.x + (x - p.x) * k, p.y + (y - p.y) * k);
        if dt > 0.0 {
            p.vx += ((nx - p.x) / dt - p.vx) * 0.35;
            p.vy += ((ny - p.y) / dt - p.vy) * 0.35;
        }
        (p.x, p.y) = (nx, ny);
        p.rest = if p.speed() < 10.0 { p.rest + dt } else { 0.0 };
    }

    fn cursor_push(&self, p: &Particle) -> Option<(f32, f32, f32)> {
        let c = self.pointer?;
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        let d = (dx * dx + dy * dy).sqrt();
        (d < CURSOR_RADIUS && d > 0.01).then(|| (dx / d, dy / d, 1.0 - d / CURSOR_RADIUS))
    }

    fn add_debris(&mut self, debris: Debris) {
        if self.debris.len() < MAX_DEBRIS {
            self.debris.push(debris);
        } else {
            self.debris[self.debris_next] = debris;
            self.debris_next = (self.debris_next + 1) % MAX_DEBRIS;
        }
    }

    fn throw_snow(&mut self, shed: &[Shed]) {
        if shed.is_empty() {
            return;
        }
        // Keep room for clumps, trailing flakes, and fracture puffs even during
        // a full-width window shake.
        let share = (MAX_DEBRIS as f32 * 0.45 / shed.len() as f32).min(1.0);
        for s in shed {
            if self.rng.next() > share * (s.amount / 1.4).min(1.0) {
                continue;
            }
            if s.clump && s.amount > 0.9 {
                let clump = Debris {
                    x: s.x + self.rng.range(-6.0, 6.0),
                    y: s.y + self.rng.range(-s.amount * 0.45, s.amount * 0.35),
                    vx: s.vx * self.rng.range(0.65, 1.2) + self.rng.range(-55.0, 55.0),
                    vy: s.vy * self.rng.range(0.55, 1.15) + self.rng.range(-95.0, 65.0),
                    life: self.rng.range(4.2, 6.2),
                    size: (2.2 + s.amount.sqrt() * self.rng.range(0.75, 1.35)).clamp(2.6, 11.0),
                    kind: DebrisKind::Clump,
                    window_id: s.window_id,
                    angle: self.rng.range(0.0, std::f32::consts::TAU),
                    spin: self.rng.range(-7.5, 7.5),
                    mass: (s.amount * 0.65).min(14.0),
                    seed: self.rng.range(0.0, std::f32::consts::TAU),
                };
                self.add_debris(clump);
            }
            let flakes = if s.amount > 4.0 { 2 } else { 1 };
            for _ in 0..flakes {
                let flake = Debris {
                    x: s.x + self.rng.range(-6.0, 6.0),
                    y: s.y + self.rng.range(-4.0, 3.0),
                    vx: s.vx * self.rng.range(0.45, 0.95) + self.rng.range(-65.0, 65.0),
                    vy: s.vy * self.rng.range(0.35, 0.75) - self.rng.range(12.0, 85.0),
                    life: self.rng.range(3.6, 5.6),
                    size: self.rng.range(1.6, 3.8),
                    kind: DebrisKind::Flake,
                    window_id: s.window_id,
                    angle: 0.0,
                    spin: 0.0,
                    mass: (s.amount * 0.25).min(4.0),
                    seed: self.rng.range(0.0, std::f32::consts::TAU),
                };
                self.add_debris(flake);
            }
            if s.clump && s.amount > 3.5 && self.rng.next() < 0.32 {
                let puff = Debris {
                    x: s.x + self.rng.range(-6.0, 6.0),
                    y: s.y - self.rng.range(0.0, 6.0),
                    vx: s.vx * 0.35 + self.rng.range(-30.0, 30.0),
                    vy: s.vy * 0.2 - self.rng.range(10.0, 45.0),
                    life: self.rng.range(0.55, 1.0),
                    size: self.rng.range(4.5, 9.0),
                    kind: DebrisKind::Powder,
                    window_id: s.window_id,
                    angle: 0.0,
                    spin: 0.0,
                    mass: 0.0,
                    seed: self.rng.range(0.0, std::f32::consts::TAU),
                };
                self.add_debris(puff);
            }
        }
    }

    fn sparks(&mut self, x: f32, y: f32, count: usize) {
        for _ in 0..count {
            let angle = self.rng.range(-3.05, -0.1);
            let speed = self.rng.range(50.0, 210.0);
            let debris = Debris::plain(
                x,
                y,
                angle.cos() * speed,
                angle.sin() * speed,
                self.rng.range(0.3, 0.75),
                self.rng.range(0.8, 1.7),
                DebrisKind::Spark,
            );
            self.add_debris(debris);
        }
    }

    fn step_debris(&mut self, dt: f32, surfaces: &[Rect]) {
        let height = self.height;
        let piles = self.piles_on();
        let max = self.max_depth();
        let wind = self.wind() * 28.0;
        let time = self.time;
        let mut puffs = Vec::new();
        for d in &mut self.debris {
            let gravity = match d.kind {
                // Larger snow clumps fall faster than light broken flakes,
                // stretching a shaken snow slab into a natural ragged cascade.
                DebrisKind::Clump => 360.0 + (d.size * 26.0).min(260.0),
                DebrisKind::Flake => 240.0 + (d.size * 18.0).min(90.0),
                DebrisKind::Spark => 240.0,
                DebrisKind::Drop => 620.0,
                DebrisKind::Dust => -30.0,
                DebrisKind::Glint => 16.0,
                DebrisKind::Smoke => -14.0,
                DebrisKind::Powder => 55.0,
            };
            let y0 = d.y;
            d.vy += gravity * dt;
            let drag = match d.kind {
                DebrisKind::Clump => 0.35,
                DebrisKind::Flake => 0.85,
                _ => 0.7,
            };
            d.vx *= 1.0 - (drag * dt).min(1.0);
            if d.kind == DebrisKind::Flake {
                d.vx += (wind + (time * 3.8 + d.seed).sin() * 28.0 - d.vx) * (0.65 * dt).min(1.0);
            }
            d.x += d.vx * dt;
            d.y += d.vy * dt;
            d.angle += d.spin * dt;
            d.life -= dt;
            if matches!(d.kind, DebrisKind::Smoke | DebrisKind::Powder) {
                d.size += dt
                    * if d.kind == DebrisKind::Smoke {
                        8.0
                    } else {
                        12.0
                    };
            }
            if d.y > height + 16.0 {
                d.life = 0.0;
            }
            // Falling snow clumps & flakes land on lower windows (other than
            // the window they came from), on docks, or on the bottom snow drift.
            if piles
                && matches!(d.kind, DebrisKind::Flake | DebrisKind::Clump)
                && d.vy > 0.0
                && d.life > 0.0
            {
                let mut landed = false;
                if d.vy > 40.0 {
                    // Check uncovered lower windows other than the clump's own source window.
                    for (i, w) in surfaces.iter().enumerate() {
                        if self.window_ids.get(i).copied().unwrap_or(0) == d.window_id {
                            continue;
                        }
                        if !self.ledges.get(i).copied().unwrap_or(true) {
                            continue;
                        }
                        if d.x < w.x || d.x >= w.right() {
                            continue;
                        }
                        let top = w.y - self.piles.depth_on(i, d.x);
                        if y0 <= top + 2.0 && d.y >= top {
                            let covered =
                                surfaces[i + 1..].iter().any(|o| o.contains(d.x, w.y - 0.5));
                            if !covered {
                                let mass = d.mass.max(d.size * 0.45);
                                self.piles.deposit(
                                    i,
                                    d.x,
                                    mass,
                                    max,
                                    wind * 0.02,
                                    &mut self.spill_buf,
                                );
                                d.life = 0.0;
                                landed = true;
                                break;
                            }
                        }
                    }
                    if !landed {
                        for (i, dock) in self.docks.iter().enumerate() {
                            if d.x < dock.x || d.x >= dock.right() {
                                continue;
                            }
                            let top = dock.y - self.piles.depth_on_dock(i, d.x);
                            if y0 <= top + 2.0 && d.y >= top {
                                let mass = d.mass.max(d.size * 0.45);
                                self.piles.deposit_dock(
                                    i,
                                    d.x,
                                    mass,
                                    max,
                                    wind * 0.02,
                                    &mut self.spill_buf,
                                );
                                d.life = 0.0;
                                landed = true;
                                break;
                            }
                        }
                    }
                }
                if !landed && d.y >= height - self.piles.floor_height(d.x) - 1.0 {
                    d.life = 0.0;
                    if d.mass > 0.0 {
                        self.piles.deposit_floor(d.x, d.mass, max);
                    }
                    if d.kind == DebrisKind::Clump && d.size > 4.5 && puffs.len() < 12 {
                        puffs.push((d.x, height - self.piles.floor_height(d.x)));
                    }
                }
            }
        }
        for (px, py) in puffs {
            let puff = Debris::plain(
                px,
                py - 3.0,
                self.rng.range(-24.0, 24.0),
                -self.rng.range(12.0, 32.0),
                self.rng.range(0.35, 0.6),
                self.rng.range(4.0, 7.0),
                DebrisKind::Powder,
            );
            self.add_debris(puff);
        }
        if self.debris.iter().any(|d| d.life <= 0.0) {
            self.debris.retain(|d| d.life > 0.0);
            self.debris_next = 0;
        }
    }

    fn step_storm(&mut self, dt: f32, surfaces: &[Rect]) {
        let enabled = self.aura.lightning && self.tuning.lightning;
        if let Some(age) = self.storm.age {
            let age = age + dt;
            self.storm.age = (age < STORM_FLASH_SECONDS).then_some(age);
        } else if enabled && self.presence > 0.6 && self.clock >= self.storm.next {
            self.storm.next = self.clock + self.rng.range(9.0, 26.0);
            self.storm.age = Some(0.0);
            let x = self.rng.range(self.width * 0.1, self.width * 0.9);
            let end = landing(surfaces, x, 1.0, self.height).map_or(self.height, |(_, y)| y);
            let mut bx = x;
            for i in 0..BOLT_POINTS {
                let t = i as f32 / (BOLT_POINTS - 1) as f32;
                self.storm.bolt[i] = [bx, end * t];
                bx += self.rng.range(-34.0, 34.0);
            }
        }
    }

    fn step_mote(&self, p: &mut Particle, dt: f32) -> bool {
        let sway = (self.time * 0.35 + p.seed).sin() * 9.0;
        let mut vx = self.breeze() * 34.0 + sway;
        if let Some((nx, ny, f)) = self.cursor_push(p) {
            // Motes curl around the pointer instead of being shoved away.
            vx += (-ny + nx * 0.3) * f * 48.0;
            p.y += (nx + ny * 0.3) * f * 48.0 * dt;
        }
        p.x += vx * dt;
        p.y += p.vy * dt;
        p.y < -12.0 || p.x < -40.0 || p.x > self.width + 40.0
    }

    /// Highest uncovered window snow surface crossed between `y0` and `y1`.
    fn snow_landing(
        &self,
        windows: &[Rect],
        x: f32,
        y0: f32,
        y1: f32,
        piles: bool,
    ) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (i, w) in windows.iter().enumerate() {
            if x < w.x || x >= w.right() {
                continue;
            }
            let top = if piles {
                w.y - self.piles.depth_on(i, x)
            } else {
                w.y
            };
            if top < y0 - 2.0 || top > y1 {
                continue;
            }
            let covered = windows[i + 1..].iter().any(|o| o.contains(x, w.y - 0.5));
            if !covered && best.is_none_or(|(_, b)| top < b) {
                best = Some((i, top));
            }
        }
        best
    }

    /// Highest visible desktop dock snow surface crossed between `y0` and `y1`.
    fn dock_landing(
        &self,
        _windows: &[Rect],
        x: f32,
        y0: f32,
        y1: f32,
        piles: bool,
    ) -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for (i, d) in self.docks.iter().enumerate() {
            if x < d.x || x >= d.right() {
                continue;
            }
            let top = if piles {
                d.y - self.piles.depth_on_dock(i, x)
            } else {
                d.y
            };
            if top < y0 - 2.0 || top > y1 {
                continue;
            }
            if best.is_none_or(|(_, _, b)| top < b) {
                best = Some((i, d.y, top));
            }
        }
        best
    }

    fn step_snow(&mut self, p: &mut Particle, dt: f32, windows: &[Rect]) -> bool {
        if let Phase::Settling(left) = p.phase {
            let left = left - dt;
            // Cushion gently into the snowbank and follow local surface slope.
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            let damp = (-6.0 * dt).exp();
            p.vx *= damp;
            p.vy *= damp;
            p.phase = Phase::Settling(left);
            return left <= 0.0;
        }
        if let Phase::Resting(left) = p.phase {
            let left = left - dt;
            if left <= 0.0 {
                return true;
            }
            p.phase = if has_surface_at(windows, p.x, p.y)
                || has_surface_at(&self.docks, p.x, p.y)
                || p.y >= self.height - 2.0
            {
                Phase::Resting(left)
            } else {
                Phase::Moving // The window moved away, so the flake falls again.
            };
            return false;
        }
        let z = p.depth;
        let t = self.time;
        // Graceful multi-harmonic sinusoidal wind sway: foreground flakes trace
        // wider sinuous arcs while distant flakes drift softly.
        let rhythm = 0.55 + (p.seed * 0.37).fract().abs() * 0.55;
        let swing = t * rhythm + p.seed;
        let sway = swing.sin() * (12.0 + z * 28.0)
            + (t * rhythm * 0.45 + p.seed * 2.13).cos() * (4.0 + z * 10.0);
        // Far flakes drift less with the wind, which is how distance looks.
        let wind = self.wind_at(p.x, p.y) * 42.0 * (0.55 + 0.45 * z);
        let (ex, ey) = swirl(p.x, p.y, t);
        let eddy = 5.0 + z * 11.0;
        // Gentle aerodynamic glide: pauses subtly at sway turnarounds, glides through center.
        let dip = 0.92 + 0.16 * swing.cos().powi(2);
        // A gust lifts light flakes for a moment and stirs the eddies up.
        let lift = self.updraft_at(p.x);
        let eddy = eddy * (1.0 + lift * 0.65);
        let fall = snow_fall_speed(z, p.seed) * dip * (1.0 - lift * 0.32 * (1.2 - z)) + ey * eddy;
        // Framerate-independent exponential smoothing keeps paths silky at every speed.
        let follow_x = 1.0 - (-(2.2 + (1.0 - z) * 2.2) * dt).exp();
        let follow_y = 1.0 - (-2.8 * dt).exp();
        p.vx += (wind + sway + ex * eddy - p.vx) * follow_x;
        p.vy += (fall - p.vy) * follow_y;
        if let Some(c) = self.pointer {
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            let d2 = dx * dx + dy * dy;
            if d2 < CURSOR_RADIUS * CURSOR_RADIUS && d2 > 0.01 {
                let d = d2.sqrt();
                let f = (1.0 - d / CURSOR_RADIUS).powi(2);
                // A moving pointer drags the air, and the flakes in it, along.
                let moving = (c.speed() / 250.0).min(1.0);
                let k = (f * 6.0 * moving * dt).min(1.0);
                p.vx += (c.vx.clamp(-900.0, 900.0) * 0.7 - p.vx) * k;
                p.vy += (c.vy.clamp(-900.0, 900.0) * 0.7 - p.vy) * k;
                // Around a resting pointer the snow parts gently.
                p.vx += dx / d * f * 240.0 * dt;
                p.vy += dy / d * f * 70.0 * dt;
            }
        }
        // The sleigh and the plough stir the air they pass through.
        for m in self.movers.into_iter().flatten() {
            let (dx, dy) = ((p.x - m.x) / m.rx, (p.y - m.y) / m.ry);
            let d2 = dx * dx + dy * dy;
            if d2 < 1.0 {
                let k = 1.0 - d2;
                p.vx += (m.vx * 1.15 - p.vx) * (k * 3.0 * dt).min(1.0);
                p.vy += (dy.signum() * 150.0 - if dx < 0.0 { 70.0 } else { 0.0 }) * k * dt;
            }
        }
        let y1 = p.y + p.vy * dt;
        p.x += p.vx * dt;
        let piles = self.piles_on();
        let (max, amount) = (self.max_depth(), p.size * 1.15);
        let lands = z >= LANDING_DEPTH;
        // A share of close foreground flakes drift right in front of the dock
        // while the rest land and build the snow cap along the dock's top edge.
        let passes_dock = z > 0.72 && (p.seed * 13.7).fract().abs() < 0.35;
        let floor_top = if piles {
            (self.height - self.piles.floor_height(p.x)).min(self.height - 1.0)
        } else {
            self.height - 1.0
        };
        if let Some((window, surface_y)) = self
            .snow_landing(windows, p.x, p.y, y1, piles)
            .filter(|_| lands)
        {
            if !self.ledges.get(window).copied().unwrap_or(true) {
                // A fully opened window: snow never settles on it.
                return true;
            }
            if piles {
                let wind_here = self.wind_at(p.x, surface_y);
                let deposited =
                    self.piles
                        .deposit(window, p.x, amount, max, wind_here, &mut self.spill_buf);
                if deposited {
                    let slope = self.piles.slope_on(window, p.x);
                    p.y = windows[window].y - self.piles.depth_on(window, p.x);
                    p.vx = (-slope * 26.0).clamp(-35.0, 35.0);
                    p.vy = 5.0;
                    p.phase = Phase::Settling(SETTLE_SECONDS);
                    return false;
                }
            }
            p.y = windows[window].y;
            p.phase = Phase::Resting(3.0 + (p.seed * 1.7).abs() % 4.0);
        } else if let Some((dock, dock_top, surface_y)) = self
            .dock_landing(windows, p.x, p.y, y1, piles)
            .filter(|_| lands && !passes_dock)
        {
            if piles {
                let wind_here = self.wind_at(p.x, surface_y);
                if self
                    .piles
                    .deposit_dock(dock, p.x, amount, max, wind_here, &mut self.spill_buf)
                {
                    let slope = self.piles.slope_on_dock(dock, p.x);
                    p.y = dock_top - self.piles.depth_on_dock(dock, p.x);
                    p.vx = (-slope * 26.0).clamp(-35.0, 35.0);
                    p.vy = 5.0;
                    p.phase = Phase::Settling(SETTLE_SECONDS);
                    return false;
                }
            }
            p.y = dock_top;
            p.phase = Phase::Resting(3.0 + (p.seed * 1.7).abs() % 4.0);
        } else if lands && y1 >= floor_top {
            if piles {
                // Gentle spatial drift bias helps rolling snow hills grow organically.
                let mound = 0.82 + 0.36 * (p.x * 0.0045 + self.time * 0.005).sin().powi(2);
                self.piles.deposit_floor(p.x, amount * mound, max);
                let slope = self.piles.floor_slope(p.x);
                p.y = self.height - self.piles.floor_height(p.x);
                p.vx = (-slope * 26.0).clamp(-35.0, 35.0);
                p.vy = 5.0;
                p.phase = Phase::Settling(SETTLE_SECONDS);
                return false;
            }
            p.y = self.height - 1.0;
            p.phase = Phase::Resting(2.0 + (p.seed * 1.3).abs() % 3.0);
        } else if !lands && y1 >= self.height + 6.0 {
            return true;
        } else {
            p.y = y1;
        }
        p.y < -120.0 || p.x < -60.0 || p.x > self.width + 60.0
    }

    fn step_rain(&mut self, p: &mut Particle, dt: f32, windows: &[Rect]) -> bool {
        if let Phase::Splash(left) = p.phase {
            let left = left - dt;
            p.phase = Phase::Splash(left);
            return left <= 0.0;
        }
        p.vx = self.breeze() * 150.0;
        let y1 = p.y + p.vy * dt;
        p.x += p.vx * dt;
        let hit = landing(windows, p.x, p.y, y1)
            .map(|(_, top)| top)
            .or((y1 >= self.height).then_some(self.height - 1.0));
        if let Some(top) = hit {
            p.y = top;
            p.phase = Phase::Splash(0.13);
            // Some drops bounce up as two tiny droplets.
            if self.rng.next() < 0.18 {
                for side in [-1.0, 1.0] {
                    let drop = Debris::plain(
                        p.x,
                        top - 1.0,
                        side * self.rng.range(25.0, 70.0) + p.vx * 0.2,
                        -self.rng.range(70.0, 140.0),
                        self.rng.range(0.18, 0.3),
                        self.rng.range(0.6, 1.0),
                        DebrisKind::Drop,
                    );
                    self.add_debris(drop);
                }
            }
        } else {
            p.y = y1;
        }
        false
    }

    fn step_firefly(&self, p: &mut Particle, dt: f32, index: usize, active: usize) -> bool {
        if let Some((hx, hy, age)) = self.visitors.heart() {
            // Every light takes its own place on the outline of a heart.
            let t = index as f32 / active.max(1) as f32 * std::f32::consts::TAU;
            let [ox, oy] = heart_point(t);
            let size = (self.height * 0.009).clamp(5.0, 9.0);
            let (tx, ty) = (hx + ox * size, hy + oy * size);
            let hold = (1.0 - (age - HEART_SECONDS + 1.0).max(0.0)).clamp(0.0, 1.0);
            let (dvx, dvy) = ((tx - p.x) * 1.8, (ty - p.y) * 1.8);
            let k = (dt * 3.0).min(1.0) * hold;
            p.vx += (dvx.clamp(-240.0, 240.0) - p.vx) * k;
            p.vy += (dvy.clamp(-240.0, 240.0) - p.vy) * k;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            return false;
        }
        let t = self.time * 0.5 + p.seed;
        p.vx += ((t * 1.7).sin() + (t * 0.6).cos()) * 16.0 * dt;
        p.vy += ((t * 1.3).cos() - (t * 0.9).sin()) * 16.0 * dt;
        let gathering = self.pointer.filter(|c| c.rest > 0.8);
        if let Some(c) = gathering {
            // Lights gather in a loose cloud around a resting pointer.
            let orbit = 34.0 + (p.seed * 11.0).rem_euclid(70.0);
            let (dx, dy) = (c.x - p.x, c.y - p.y);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            if d < 620.0 {
                let pull = ((d - orbit) / 90.0).clamp(-1.0, 1.0) * 70.0;
                p.vx += (dx / d * pull - dy / d * 12.0) * dt;
                p.vy += (dy / d * pull + dx / d * 12.0) * dt;
            }
        } else if let Some((nx, ny, f)) = self.cursor_push(p) {
            // A moving pointer scatters them.
            p.vx += nx * f * 150.0 * dt;
            p.vy += ny * f * 150.0 * dt;
        }
        p.vx = p.vx.clamp(-46.0, 46.0) * (1.0 - 0.25 * dt);
        p.vy = p.vy.clamp(-40.0, 40.0) * (1.0 - 0.25 * dt);
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        if p.x < 0.0 || p.x > self.width {
            p.vx = -p.vx;
            p.x = p.x.clamp(0.0, self.width);
        }
        let top = if gathering.is_some() {
            0.0
        } else {
            self.height * 0.12
        };
        if p.y < top || p.y > self.height {
            p.vy = -p.vy;
            p.y = p.y.clamp(top, self.height);
        }
        false
    }

    fn step_fog(&self, p: &mut Particle, dt: f32) -> bool {
        p.x += (p.vx + self.breeze() * 22.0) * dt;
        p.y += (self.time * 0.08 + p.seed).sin() * 2.2 * dt;
        if p.x > self.width + p.size {
            p.x = -p.size;
        } else if p.x < -p.size {
            p.x = self.width + p.size;
        }
        false
    }

    /// Leaves and petals: they tumble down, lie where they land and scatter
    /// when the pointer sweeps through them.
    fn step_flutter(&mut self, p: &mut Particle, dt: f32, windows: &[Rect]) -> bool {
        if let Phase::Resting(left) = p.phase {
            let left = left - dt;
            if left <= 0.0 {
                return true;
            }
            p.phase = Phase::Resting(left);
            let whirled = self
                .visitors
                .whirl()
                .is_some_and(|wx| (p.x - wx).abs() < 70.0 && p.y > self.height - 400.0);
            let gusted = self.gust() > 0.5 && self.rng.next() < dt * self.gust() * 0.5;
            if !(has_surface_at(windows, p.x, p.y) || p.y >= self.height - 2.0) {
                p.phase = Phase::Moving;
                (p.vx, p.vy) = (0.0, 0.0);
            } else if whirled || gusted {
                p.phase = Phase::Moving;
                let direction = if self.wind() < 0.0 { -1.0 } else { 1.0 };
                p.vx = if whirled {
                    self.rng.range(-60.0, 60.0)
                } else {
                    direction * self.rng.range(40.0, 120.0)
                };
                p.vy = -self.rng.range(110.0, 220.0);
                p.y -= 3.0;
            } else if let Some(c) = self.pointer.filter(|c| c.speed() > 70.0) {
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                let d = (dx * dx + dy * dy).sqrt();
                if d < 54.0 {
                    let side = if dx == 0.0 { 1.0 } else { dx / d };
                    p.vx = side * c.speed().min(700.0) * 0.32 + c.vx.clamp(-500.0, 500.0) * 0.3;
                    p.vy = -self.rng.range(70.0, 170.0);
                    p.y -= 3.0;
                    p.phase = Phase::Moving;
                }
            }
            return false;
        }
        let leaf = self.aura.effect == EffectKind::Leaves;
        let keep = (1.0 - 1.5 * dt).max(0.0);
        p.vx *= keep;
        p.vy *= keep;
        if let Some((nx, ny, f)) = self.cursor_push(p) {
            let strength = 90.0 + self.pointer.map_or(0.0, |c| c.speed().min(600.0)) * 0.7;
            p.vx += nx * f * strength * dt * 3.0;
            p.vy += ny * f * strength * dt * 3.0;
        }
        if let Some(wx) = self.visitors.whirl() {
            // Leaves near the whirlwind spiral up around it.
            let dx = p.x - wx;
            let reach = 1.0 - dx.abs() / 120.0;
            if reach > 0.0 && p.y > self.height - 380.0 {
                let spin = (self.time * 6.0 + p.seed * 3.0).sin();
                p.vx += (-dx * 3.2 + spin * 220.0) * reach * dt;
                p.vy -= 300.0 * reach * dt;
            }
        }
        let t = self.time + p.seed;
        let fall = if leaf {
            26.0 + p.size * 3.2
        } else {
            15.0 + p.size * 4.0
        };
        let sway = (t * 0.9).sin() * if leaf { 24.0 } else { 30.0 };
        let vy = fall + (t * 1.4).cos() * 9.0 + p.vy;
        p.x += (self.breeze() * 62.0 + sway + p.vx) * dt;
        let y1 = p.y + vy * dt;
        let surface = if vy > 0.0 {
            landing(windows, p.x, p.y, y1).map(|(_, top)| top)
        } else {
            None
        }
        .or((y1 >= self.height - 1.0).then_some(self.height - 1.0));
        if let Some(top) = surface {
            p.y = top;
            let room = (self.resting as f32) < self.particles.len() as f32 * MAX_RESTING_SHARE;
            let stay = if room && self.tuning.piles {
                self.rng.range(22.0, 48.0)
            } else {
                self.rng.range(2.5, 5.0)
            };
            p.phase = Phase::Resting(stay);
            (p.vx, p.vy) = (0.0, 0.0);
        } else {
            p.y = y1;
        }
        p.x < -60.0 || p.x > self.width + 60.0
    }

    fn step_meteor(&mut self, p: &mut Particle, dt: f32, windows: &[Rect]) -> bool {
        if let Phase::Waiting(left) = p.phase {
            let left = left - dt;
            p.phase = if left <= 0.0 {
                Phase::Moving
            } else {
                Phase::Waiting(left)
            };
            return false;
        }
        let (x0, y0) = (p.x, p.y);
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        if self.rng.next() < dt * 34.0 {
            let trail = Debris::plain(
                x0,
                y0,
                p.vx * 0.04,
                p.vy * 0.04,
                self.rng.range(0.25, 0.6),
                self.rng.range(0.6, 1.1),
                DebrisKind::Spark,
            );
            self.add_debris(trail);
        }
        if let Some((_, top)) = landing(windows, p.x, y0, p.y) {
            let count = 9 + (self.rng.next() * 7.0) as usize;
            self.sparks(p.x, top, count);
            return true;
        }
        if p.y >= self.height {
            self.sparks(p.x, self.height - 1.0, 6);
            return true;
        }
        p.x < -320.0 || p.x > self.width + 320.0
    }

    fn alpha_at(&self, x: f32, y: f32, base: f32) -> f32 {
        let mut a = base * self.aura.opacity * (0.35 + 0.65 * self.presence);
        if self.focused.is_some_and(|r| r.contains(x, y)) {
            // During Stillness, the window being worked in clears first.
            a *= 0.12 + 0.88 * ((self.presence - 0.22) / 0.78).clamp(0.0, 1.0);
        }
        a
    }

    /// The layer for something resting on a surface at (x, y): the window
    /// whose uncovered top is there, or the desktop.
    pub(crate) fn layer_at(&self, x: f32, y: f32) -> f32 {
        self.layer_on(x, y, 2.5)
    }

    /// Like `layer_at`, for something that may stand on snow piled up to
    /// `reach` pixels above a window's top.
    pub(crate) fn layer_on(&self, x: f32, y: f32, reach: f32) -> f32 {
        let w = &self.windows;
        let index = (0..w.len()).rev().find(|&i| {
            w[i].y - y > -2.5
                && w[i].y - y < reach
                && x >= w[i].x
                && x < w[i].right()
                && !w[i + 1..].iter().any(|o| o.contains(x, w[i].y - 0.5))
        });
        self.masks.layer_for(index)
    }

    /// Mask layer for debris shed from window `window_id` (`0` = desktop/dock).
    fn layer_for_window_id(&self, window_id: u64) -> f32 {
        if window_id == 0 || self.masks.masks().is_empty() {
            return 0.0;
        }
        let index = self.window_ids.iter().position(|&id| id == window_id);
        self.masks.layer_for(index)
    }

    fn tint(&self, seed: f32) -> [f32; 3] {
        let colors = &self.aura.palette;
        if colors.is_empty() {
            self.aura.color
        } else {
            colors[((seed * 7.0).abs() as usize) % colors.len()]
        }
    }

    /// Piled snow as one smooth surface: a column per stretch whose top
    /// slopes between neighbouring heights. A few spots on top glint. Snow
    /// on a window hangs a little over its top edge in uneven scallops and
    /// casts a soft shadow onto the window.
    fn draw_piles(&self, paint: &mut Painter<'_>) {
        if !self.piles_on() {
            return;
        }
        let current = || self.piles.list.iter().filter(|p| self.piles.is_current(p));
        let cells: usize =
            self.piles.floor.cells.len() + current().map(|p| p.cells.len()).sum::<usize>();
        let stride = cells.div_ceil(MAX_PILE_INSTANCES).max(1);
        // Opaque, so neighbouring columns overlap without seams.
        let alpha = 1.0;
        let color = self.aura.color;
        let mut points = Vec::with_capacity(self.piles.floor.cells.len() / stride + 3);
        for pile in std::iter::once(&self.piles.floor).chain(current()) {
            let floor = std::ptr::eq(pile, &self.piles.floor);
            let ledge = !floor;
            let index = if floor || pile.is_dock {
                None
            } else {
                self.window_ids
                    .iter()
                    .position(|&id| id == pile.id)
                    .or_else(|| self.windows.iter().position(|w| *w == pile.rect))
            };
            paint.layer = if pile.is_dock {
                self.masks.top_layer()
            } else {
                self.masks.layer_for(index)
            };
            let n = pile.cells.len();
            points.clear();
            if floor {
                points.push((pile.rect.x, pile.cells[0]));
            } else {
                points.push((pile.rect.x, pile.cells[0] * 0.65));
            }
            points.extend(
                (0..n)
                    .step_by(stride)
                    .map(|i| (pile.cell_x(i), pile.cells[i])),
            );
            if floor {
                points.push((pile.rect.right(), pile.cells[n - 1]));
            } else {
                points.push((pile.rect.right(), pile.cells[n - 1] * 0.65));
                // Snow on a window or dock pushes out a little past each corner
                // and rounds off there, instead of stopping in a cliff.
                let shade = [color[0] * 0.87, color[1] * 0.90, color[2] * 0.98];
                let near_end = |cells: &mut dyn Iterator<Item = &f32>| {
                    cells.take(4).fold(0.0_f32, |a, &b| a.max(b))
                };
                for (x, h, side) in [
                    (pile.rect.x, near_end(&mut pile.cells.iter()), 1.0),
                    (
                        pile.rect.right(),
                        near_end(&mut pile.cells.iter().rev()),
                        -1.0,
                    ),
                ] {
                    let r = (h * 0.52).min(24.0);
                    let hidden = if let Some(i) = index {
                        self.windows[i + 1..]
                            .iter()
                            .any(|o| o.contains(x, pile.rect.y - 0.5))
                    } else {
                        false
                    };
                    if r > 1.5 && !hidden {
                        paint.put(
                            x + side * r * 0.25,
                            pile.rect.y,
                            r,
                            r,
                            0.0,
                            SHAPE_DOME,
                            shade,
                            alpha,
                        );
                    }
                }
            }
            let base = pile.rect.y;
            for (k, pair) in points.windows(2).enumerate() {
                let [(x0, h0), (x1, h1)] = [pair[0], pair[1]];
                if h0.max(h1) < 0.6 || x1 - x0 < 0.01 {
                    continue;
                }
                let mid = (x0 + x1) * 0.5;
                let hidden = if let Some(n) = index {
                    self.windows[n + 1..]
                        .iter()
                        .any(|o| o.contains(mid, base - 0.5))
                } else {
                    false
                };
                if hidden {
                    continue;
                }
                let top = h0.max(h1) + 1.5;
                paint.put_extra(
                    mid,
                    base - top * 0.5,
                    (x1 - x0) * 0.5 + 0.35,
                    top * 0.5,
                    SHAPE_SNOWBANK,
                    [h0 / top, h1 / top],
                    color,
                    alpha,
                );
                if ledge && k % 3 == 0 {
                    self.draw_snow_lip(paint, pile.rect, mid, x1 - x0, (h0 + h1) * 0.5);
                }
                // Fresh snow is soft and pillowy on top, never ruled flat,
                // while skipping steep side slopes so domes never poke out sideways.
                let lump = ((mid * 0.731 + base * 0.113).sin() * 24_634.6)
                    .fract()
                    .abs();
                let gentle_slope = (h1 - h0).abs() <= (x1 - x0) * 0.75;
                if gentle_slope
                    && lump < if floor { 0.45 } else { 0.72 }
                    && h0.min(h1) > 2.0
                    && stride == 1
                {
                    let x = x0 + (x1 - x0) * lump / 0.72;
                    let h = h0 + (h1 - h0) * lump / 0.72;
                    let rx = 6.0 + 10.0 * ((lump * 91.7).fract());
                    let ry = (rx * (0.32 + 0.25 * (lump * 37.3).fract())).min(h * 0.36);
                    let top = [color[0] * 0.98, color[1] * 0.99, color[2]];
                    paint.put(x, base - h + ry * 0.45, rx, ry, 0.0, SHAPE_DOME, top, alpha);
                }
                // Glints: rare, brief twinkles where the light catches.
                let hash = ((mid * 12.9898 + base * 78.233).sin() * 43_758.547)
                    .fract()
                    .abs();
                if hash < 0.06 && h0.min(h1) > 3.0 {
                    let twinkle = (self.clock * 1.3 + hash * 97.0 + k as f32)
                        .sin()
                        .max(0.0)
                        .powi(16);
                    if twinkle > 0.02 {
                        let y = base - (h0 + h1) * 0.5 + 1.5;
                        let r = 2.6 + hash * 20.0;
                        paint.put(mid, y, r, r, 0.6, SHAPE_SPARKLE, [1.0, 1.0, 1.0], twinkle);
                    }
                }
            }
        }
    }

    /// Snow bulging over a window or dock's top edge around `mid`, `span` wide,
    /// with the soft shadow it casts just below.
    fn draw_snow_lip(&self, paint: &mut Painter<'_>, rect: Rect, mid: f32, span: f32, depth: f32) {
        if depth < 2.0 {
            return;
        }
        let reach = (mid - rect.x).min(rect.right() - mid);
        let hw = (span * 1.9).min(reach);
        if hw < 1.0 {
            return;
        }
        let hash = ((mid * 4.123 + rect.y * 1.7).sin() * 9_871.3).fract().abs();
        // Mostly shallow scallops, with now and then a heavier bulge.
        let bulge = if hash > 0.86 { 1.55 } else { 0.48 + 0.7 * hash };
        let hang = (depth * 0.14).clamp(1.4, 7.0) * bulge;
        let c = self.aura.color;
        let underside = [c[0] * 0.85, c[1] * 0.89, c[2] * 0.97];
        let base = rect.y;
        let shadow = 0.24 * (depth / 18.0).min(1.0);
        paint.put(
            mid,
            base + hang + 2.0,
            hw * 1.15,
            5.0,
            0.0,
            SHAPE_PUFF,
            SNOW_SHADOW,
            shadow,
        );
        // A dome turned upside down hangs below the edge.
        paint.put(
            mid,
            base,
            hw,
            hang,
            std::f32::consts::PI,
            SHAPE_DOME,
            underside,
            1.0,
        );
    }

    fn draw_storm(&self, paint: &mut Painter<'_>) {
        let Some(age) = self.storm.age else { return };
        // A main flash followed by a weaker echo.
        let pulse = (-age * 14.0)
            .exp()
            .max(0.6 * (-(age - 0.17).max(0.0) * 16.0).exp() * f32::from(age > 0.17));
        let (w, h) = (self.width, self.height);
        let white = [0.86, 0.9, 1.0];
        let presence = 0.35 + 0.65 * self.presence;
        paint.put(
            w * 0.5,
            h * 0.2,
            w * 0.95,
            h * 0.9,
            0.0,
            SHAPE_PUFF,
            white,
            0.1 * pulse * presence,
        );
        // The bolt fades smoothly and outlasts the sky flash. Its dark edge
        // keeps it readable on pale wallpapers, its white core on dark ones.
        let bolt = (-age * 7.0).exp() * presence;
        for pair in self.storm.bolt.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            paint.line(a, b, 10.0, SHAPE_BEAM, [0.14, 0.18, 0.38], 0.5 * bolt);
            paint.line(a, b, 5.0, SHAPE_BEAM, white, 0.4 * bolt);
            paint.line(a, b, 2.0, SHAPE_BEAM, [1.0, 1.0, 1.0], bolt);
        }
    }

    fn draw_particle(&self, p: &Particle, paint: &mut Painter<'_>) {
        let color = self.aura.color;
        let scale = self.tuning.size;
        let tau = std::f32::consts::FRAC_PI_2;
        match self.aura.effect {
            EffectKind::Motes => {
                let twinkle = 0.45 + 0.55 * (self.time * 0.9 + p.seed).sin().abs();
                // Motes catch a diagonal shaft of light across the screen.
                let d = (p.x / self.width - p.y / self.height * 0.55 - 0.22).abs();
                let beam = 0.28 + 0.72 * (1.0 - (d * 2.4).min(1.0));
                let s = p.size * 2.6 * scale;
                let a = self.alpha_at(p.x, p.y, twinkle * beam);
                paint.put(p.x, p.y, s, s, 0.0, SHAPE_SOFT, color, a);
            }
            EffectKind::Snow => {
                let s = p.size * 2.0 * scale;
                if let Phase::Settling(left) = p.phase {
                    // Melting into the snow it landed on.
                    let k = (left / SETTLE_SECONDS).clamp(0.0, 1.0);
                    let r = s * (0.55 + 0.45 * k);
                    let a = self.alpha_at(p.x, p.y, 0.85 * k);
                    paint.put(p.x, p.y - r * 0.4, r, r, 0.0, SHAPE_FLAKE, color, a);
                    return;
                }
                if let Phase::Resting(left) = p.phase {
                    let y = p.y - p.size * 0.6;
                    let a = self.alpha_at(p.x, y, 0.9 * left.min(1.0));
                    paint.put(p.x, y, s, s, 0.0, SHAPE_SOFT, color, a);
                    return;
                }
                let z = p.depth;
                // Flakes catch the light differently as they turn.
                let shimmer = 0.86 + 0.14 * (self.time * 1.7 + p.seed * 5.0).sin();
                // Like a camera, the eye smears a moving flake a little along
                // its path; fast, near flakes the most.
                let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                let turn = -p.vx.atan2(p.vy);
                let smear = |s: f32| {
                    let along = (s + speed * MOTION_SMEAR * z).min(s * 1.45);
                    (along, (s / along).sqrt())
                };
                if z > 0.88 {
                    // Close foreground bokeh flakes: large, dreamy, softly visible disks.
                    let s = s * 1.45;
                    let (along, thin) = smear(s);
                    let a = self.alpha_at(p.x, p.y, 0.32 * thin);
                    paint.put_turned(p.x, p.y, s, along, turn, SHAPE_FLAKE, [1.0, 0.0], color, a);
                } else {
                    // Round fluffy flakes with depth scale: far ones small and
                    // faint; near foreground ones large, bright and sharp.
                    let near = (z / 0.88).clamp(0.0, 1.0);
                    let s = s * (1.0 + 0.22 * near * near);
                    let blur = 0.25 * (1.0 - near);
                    let (along, thin) = smear(s);
                    let a = self.alpha_at(p.x, p.y, (0.48 + 0.50 * near) * shimmer * thin);
                    paint.put_turned(p.x, p.y, s, along, turn, SHAPE_FLAKE, [blur, 0.0], color, a);
                }
            }
            EffectKind::Rain => match p.phase {
                Phase::Splash(left) => {
                    let k = (left / 0.13).clamp(0.0, 1.0);
                    let s = (2.0 + (1.0 - k) * 5.0) * scale;
                    let a = self.alpha_at(p.x, p.y - 2.0, 0.55 * k);
                    paint.put(p.x, p.y - 1.0, s, s * 0.45, 0.0, SHAPE_SOFT, color, a);
                }
                _ => {
                    let len = p.vy * 0.024 * p.size * scale;
                    let rot = p.vx.atan2(p.vy);
                    let a = self.alpha_at(p.x, p.y, 0.8);
                    paint.put(
                        p.x,
                        p.y,
                        2.0 * p.size * scale,
                        len,
                        -rot,
                        SHAPE_RAIN,
                        color,
                        a,
                    );
                }
            },
            EffectKind::Fireflies => {
                let mut pulse = ((self.time * 0.8 + p.seed * 3.0).sin() * 0.5 + 0.5).powi(3);
                if let Some((_, _, age)) = self.visitors.heart() {
                    let shown = (age * 0.8).min(1.0) * (HEART_SECONDS - age).clamp(0.0, 1.0);
                    pulse = pulse.max(0.75 * shown);
                }
                let s = p.size * (5.0 + pulse * 3.0) * scale;
                let a = self.alpha_at(p.x, p.y, 0.1 + 0.9 * pulse);
                paint.put(p.x, p.y, s, s, 0.0, SHAPE_GLOW, color, a);
            }
            EffectKind::Fog => {
                let mut a = 0.55 + 0.45 * (self.time * 0.1 + p.seed).sin().abs();
                if let Some((_, _, f)) = self.cursor_push(p) {
                    a *= 1.0 - 0.7 * f; // Fog thins around the pointer.
                }
                a *= self.lantern_clearing(p.x, p.y);
                let a = self.alpha_at(p.x, p.y, a);
                let s = p.size * scale;
                paint.put(p.x, p.y, s * 1.8, s * 0.62, 0.0, SHAPE_PUFF, color, a);
            }
            EffectKind::Leaves | EffectKind::Blossom => {
                let shape = if self.aura.effect == EffectKind::Leaves {
                    SHAPE_LEAF
                } else {
                    SHAPE_PETAL
                };
                let s = p.size * scale;
                let tint = self.tint(p.seed);
                if let Phase::Resting(left) = p.phase {
                    let lift = s * 0.3 + (p.seed * 3.7).fract().abs() * 4.0;
                    let y = p.y - lift;
                    let rot = tau + (p.seed - std::f32::consts::PI) * 0.07;
                    let a = self.alpha_at(p.x, y, left.min(1.0));
                    paint.put(p.x, y, s * 0.46, s, rot, shape, tint, a);
                } else {
                    let t = self.time + p.seed;
                    let turn = (t * 1.3).cos().abs();
                    let rot = p.seed + (t * 0.7).sin() * 1.3 + t * 0.35;
                    let a = self.alpha_at(p.x, p.y, 1.0);
                    paint.put(
                        p.x,
                        p.y,
                        s * 0.5 * (0.3 + 0.7 * turn),
                        s,
                        rot,
                        shape,
                        tint,
                        a,
                    );
                }
            }
            EffectKind::Starfall => {
                if p.phase == Phase::Moving {
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let (dx, dy) = (p.vx / speed, p.vy / speed);
                    let length = speed * 0.11 * p.size * scale;
                    let rot = (-dx).atan2(dy);
                    let (cx, cy) = (p.x - dx * length, p.y - dy * length);
                    let a = self.alpha_at(p.x, p.y, 0.9);
                    paint.put(
                        cx,
                        cy,
                        2.2 * p.size * scale,
                        length,
                        rot,
                        SHAPE_STREAK,
                        color,
                        a,
                    );
                    let glow = 7.0 * p.size * scale;
                    paint.put(p.x, p.y, glow, glow, 0.0, SHAPE_GLOW, color, a);
                }
            }
        }
    }

    /// Particles lying on a window's top are drawn on its layer; particles
    /// directly in front of or settling onto a visible dock sit above windows.
    fn draw_particle_layered(&self, p: &Particle, paint: &mut Painter<'_>) {
        let resting = matches!(
            p.phase,
            Phase::Resting(_) | Phase::Splash(_) | Phase::Settling(_)
        );
        let over_dock = !self.docks.is_empty()
            && self.docks.iter().any(|d| {
                let top = if resting {
                    d.y - self.max_depth() - 4.0
                } else {
                    d.y
                };
                p.x >= d.x && p.x <= d.right() && p.y >= top && p.y <= d.bottom()
            });
        paint.layer = if over_dock && !self.masks.masks().is_empty() {
            self.masks.top_layer()
        } else if resting && !self.masks.masks().is_empty() {
            // A flake sinking into piled snow sits above the window's edge.
            let reach = if matches!(p.phase, Phase::Settling(_)) {
                self.max_depth() + 4.0
            } else {
                2.5
            };
            self.layer_on(p.x, p.y, reach)
        } else {
            0.0
        };
        self.draw_particle(p, paint);
    }

    /// Replaces `out` with this frame's instances, reusing its allocation.
    pub fn write_instances(&self, out: &mut Vec<Instance>) {
        out.clear();
        let mut paint = Painter {
            out,
            width: self.width,
            height: self.height,
            layer: 0.0,
        };
        self.draw_storm(&mut paint);
        self.draw_piles(&mut paint);
        paint.layer = 0.0;
        self.draw_decorations(&mut paint);
        paint.layer = 0.0;
        for p in &self.particles[..self.active] {
            self.draw_particle_layered(p, &mut paint);
        }
        paint.layer = 0.0;
        let presence = 0.35 + 0.65 * self.presence;
        let clump_presence = 0.78 + 0.22 * self.presence;
        for d in &self.debris {
            paint.layer = self.layer_for_window_id(d.window_id);
            let fade = (d.life * 2.5).min(1.0) * presence;
            match d.kind {
                DebrisKind::Clump => {
                    // Tumbling snow clumps stay solid all the way down, fading
                    // only in the last moment if they do not land first.
                    let s = d.size * self.tuning.size.clamp(0.75, 1.45);
                    let a = (d.life / 0.35).min(1.0) * clump_presence;
                    paint.put_turned(
                        d.x,
                        d.y,
                        s * 1.12,
                        s * 0.94,
                        d.angle,
                        SHAPE_CLUMP,
                        [d.seed, 0.0],
                        self.aura.color,
                        a,
                    );
                }
                DebrisKind::Flake => {
                    let s = d.size * 2.0 * self.tuning.size;
                    let vis = if d.window_id != 0 {
                        (d.life / 0.4).min(1.0) * (0.72 + 0.28 * self.presence)
                    } else {
                        fade
                    };
                    paint.put(d.x, d.y, s, s, 0.0, SHAPE_SOFT, self.aura.color, 0.92 * vis);
                }
                DebrisKind::Drop => {
                    let s = d.size * 1.6 * self.tuning.size;
                    let color = [0.8, 0.88, 0.98];
                    paint.put(d.x, d.y, s, s * 1.3, 0.0, SHAPE_DISK, color, 0.75 * fade);
                    paint.put(
                        d.x,
                        d.y,
                        s * 0.6,
                        s * 0.8,
                        0.0,
                        SHAPE_DISK,
                        self.aura.color,
                        0.5 * fade,
                    );
                }
                DebrisKind::Dust => {
                    let s = d.size * 2.4 * self.tuning.size;
                    let color = self.tint(d.x * 0.01);
                    paint.put(d.x, d.y, s, s, d.life * 4.0, SHAPE_LEAF, color, 0.8 * fade);
                }
                DebrisKind::Glint => {
                    let twinkle = 0.45 + 0.55 * (d.life * 17.0 + d.x * 0.1).sin().abs();
                    let s = d.size * 3.6;
                    let warm = [1.0, 0.92, 0.7];
                    paint.put(
                        d.x,
                        d.y,
                        s,
                        s,
                        d.life * 1.5,
                        SHAPE_SPARKLE,
                        warm,
                        fade * twinkle,
                    );
                    paint.put(
                        d.x,
                        d.y,
                        s * 0.9,
                        s * 0.9,
                        0.0,
                        SHAPE_GLOW,
                        warm,
                        0.5 * fade,
                    );
                }
                DebrisKind::Smoke => {
                    let s = d.size * 2.2;
                    let a = 0.32 * d.life.min(1.0) * presence;
                    paint.put(d.x, d.y, s, s * 0.85, 0.0, SHAPE_PUFF, [0.86, 0.87, 0.9], a);
                }
                DebrisKind::Powder => {
                    let s = d.size * 2.0;
                    let a = 0.42 * d.life.min(1.0) * presence;
                    paint.put(d.x, d.y, s, s * 0.8, 0.0, SHAPE_PUFF, self.aura.color, a);
                }
                DebrisKind::Spark => {
                    let s = d.size * 3.4 * self.tuning.size;
                    let color = if self.aura.effect == EffectKind::Snow {
                        [1.0, 0.86, 0.6]
                    } else {
                        self.aura.color
                    };
                    paint.put(d.x, d.y, s, s, 0.0, SHAPE_GLOW, color, fade);
                }
            }
        }
        paint.layer = 0.0;
        self.draw_passers(&mut paint);
        paint.layer = 0.0;
        self.draw_visitor(&mut paint);
        if self.wake < 1.0 {
            let k = smoothstep(0.0, 1.0, self.wake);
            for item in paint.out.iter_mut() {
                item.color = item.color.map(|c| c * k);
            }
        }
    }
}

#[cfg(test)]
mod tests;
