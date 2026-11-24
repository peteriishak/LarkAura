//! Snow that builds up along window tops, desktop docks and the bottom of the display.

use aura_core::{Rect, Window};

const CELL: f32 = 8.0;
const MAX_CELLS: usize = 480;
const MAX_PILES: usize = 32;
const RELAX_EVERY: f32 = 0.2;

/// Maximum snow depth a window top can hold, scaled by the user's depth
/// setting and kept inside the display above the window.
pub(crate) fn window_cap(max_depth: f32, window_y: f32) -> f32 {
    (max_depth * 0.65)
        .min((window_y - 10.0).max(0.0))
        .min(max_depth)
}

/// Maximum snow depth along the top of a desktop dock.
pub(crate) fn dock_cap(max_depth: f32, dock_y: f32) -> f32 {
    (max_depth * 0.58)
        .min((dock_y - 10.0).max(0.0))
        .min(max_depth)
}

pub(crate) struct Pile {
    pub id: u64,
    pub rect: Rect,
    pub cells: Vec<f32>,
    /// True for desktop docks (drawn on desktop layer, never masked out).
    pub is_dock: bool,
    seen: u64,
    changed_at: f32,
    /// Recent motion velocity in px/s, held briefly between desktop polls so
    /// back-and-forth shaking and sudden stops trigger physical inertia.
    vx: f32,
    vy: f32,
    /// Accumulated shake agitation; decays smoothly once still.
    shake: f32,
}

impl Pile {
    fn new(id: u64, rect: Rect, is_dock: bool, clock: f32) -> Self {
        let count = ((rect.w / CELL).ceil() as usize).clamp(1, MAX_CELLS);
        Self {
            id,
            rect,
            cells: vec![0.0; count],
            is_dock,
            seen: 0,
            changed_at: clock,
            vx: 0.0,
            vy: 0.0,
            shake: 0.0,
        }
    }

    pub fn cell_width(&self) -> f32 {
        self.rect.w / self.cells.len() as f32
    }

    pub fn cell_x(&self, index: usize) -> f32 {
        self.rect.x + (index as f32 + 0.5) * self.cell_width()
    }

    fn index_at(&self, x: f32) -> Option<usize> {
        let i = ((x - self.rect.x) / self.cell_width()).floor();
        (i >= 0.0 && (i as usize) < self.cells.len()).then_some(i as usize)
    }

    /// Surface slope dh/dx in pixels per pixel at column `x`, positive when
    /// snow rises to the right (so downhill is toward negative slope).
    pub fn slope_at(&self, x: f32) -> f32 {
        let Some(i) = self.index_at(x) else {
            return 0.0;
        };
        let n = self.cells.len();
        if n < 2 {
            return 0.0;
        }
        let left = self.cells[i.saturating_sub(1)];
        let right = self.cells[(i + 1).min(n - 1)];
        let span = self.cell_width() * if i == 0 || i == n - 1 { 1.0 } else { 2.0 };
        (right - left) / span.max(1.0)
    }

    /// Natural sculpted height ceiling for cell `j` so full piles form organic
    /// arched domes (on windows/docks) or rolling hills (on the floor) instead
    /// of flat horizontal plateaus.
    fn cell_cap(&self, j: usize, max: f32, drifts: bool) -> f32 {
        let x = self.cell_x(j);
        if drifts {
            let (left, right) = (x - self.rect.x, self.rect.right() - x);
            let hill = 0.72
                + 0.14 * (x * 0.0052 + 0.8).sin()
                + 0.09 * (x * 0.015 + 2.1).sin()
                + 0.05 * (x * 0.038 + 0.4).sin();
            let corners = 1.0 + 0.32 * (-left / 220.0).exp() + 0.32 * (-right / 220.0).exp();
            (max * hill * corners).clamp(0.0, max)
        } else {
            let w = self.rect.w.max(1.0);
            let u = ((x - self.rect.x) / w).clamp(0.0, 1.0);
            let (left, right) = (x - self.rect.x, self.rect.right() - x);
            let t = (left.min(right) / 48.0).clamp(0.0, 1.0);
            let shoulder = 0.24 + 0.76 * t * t * (3.0 - 2.0 * t);
            let arch = 0.68 + 0.32 * (u * std::f32::consts::PI).sin().powf(0.7);
            let lump = 0.92 + 0.05 * (x * 0.018 + 1.1).sin() + 0.03 * (x * 0.051 + 2.7).sin();
            (max * arch * shoulder * lump).clamp(0.0, max)
        }
    }

    /// Smooth multi-cell snow deposit. Returns any overflow snow that could not
    /// fit under each cell's natural sculpted ceiling.
    fn deposit_kernel(&mut self, x: f32, amount: f32, max: f32, drifts: bool) -> f32 {
        let Some(i) = self.index_at(x) else {
            return 0.0;
        };
        if max <= 0.01 {
            return 0.0;
        }
        // Soft 5-cell Gaussian-like footprint builds rounded mounds instead of spikes.
        const KERNEL: [(isize, f32); 5] = [(-2, 0.14), (-1, 0.48), (0, 0.90), (1, 0.48), (2, 0.14)];
        let n = self.cells.len() as isize;
        let mut excess = 0.0;
        for (offset, weight) in KERNEL {
            let j = i as isize + offset;
            if j >= 0 && j < n {
                let ju = j as usize;
                let cap = self.cell_cap(ju, max, drifts);
                let c = &mut self.cells[ju];
                let next = *c + amount * weight;
                if next > cap {
                    excess += next - cap;
                    *c = (*c).max(cap);
                } else {
                    *c = next;
                }
            } else {
                // Near the very edge of a ledge, snow spills outward.
                excess += amount * weight * 0.5;
            }
        }
        // First spread local excess into nearby lower cells so the mound widens
        // naturally into a full dome before spilling over the corners.
        if excess > 0.01 {
            let mut left = excess;
            for step in 1..=7_isize {
                for dir in [-1_isize, 1_isize] {
                    let j = i as isize + dir * step;
                    if j >= 0 && j < n && left > 0.01 {
                        let ju = j as usize;
                        let cap = self.cell_cap(ju, max, drifts);
                        let c = &mut self.cells[ju];
                        let room = (cap - *c).max(0.0);
                        let give = room.min(left * 0.5);
                        *c += give;
                        left -= give;
                    }
                }
            }
            excess = left;
        }
        excess
    }

    /// Deposits onto a window or dock ledge. When the top reaches capacity,
    /// excess snow cascades to the left/right corners and spills down the sides.
    fn deposit_ledge(
        &mut self,
        x: f32,
        amount: f32,
        max: f32,
        wind: f32,
        window_id: u64,
        out: &mut Vec<Shed>,
    ) {
        let excess = self.deposit_kernel(x, amount, max, false);
        if excess <= 0.15 {
            return;
        }
        // Decide which corner(s) the overflow spills from: nearer edge, gently
        // biased by wind, or both corners when landing near the middle of a full ledge.
        let rel = ((x - self.rect.x) / self.rect.w.max(1.0)).clamp(0.0, 1.0);
        let biased = (rel + wind * 0.18).clamp(0.0, 1.0);
        let emit = |side: f32, share: f32, out: &mut Vec<Shed>| {
            let edge_x = if side < 0.0 {
                self.rect.x
            } else {
                self.rect.right()
            };
            let end_idx = if side < 0.0 { 0 } else { self.cells.len() - 1 };
            let h = self.cells[end_idx].max(4.0);
            out.push(Shed {
                x: edge_x + side * 4.0,
                y: self.rect.y - h * 0.65,
                vx: side * (26.0 + share * 12.0).min(72.0) + wind * 18.0,
                vy: 14.0,
                amount: (share * 2.2).max(1.8),
                window_id,
                clump: true,
            });
        };
        if (0.35..=0.65).contains(&biased) {
            emit(-1.0, excess * 0.55, out);
            emit(1.0, excess * 0.55, out);
        } else if biased < 0.5 {
            emit(-1.0, excess, out);
        } else {
            emit(1.0, excess, out);
        }
    }

    /// Mass-conserving slope relaxation: snow flows downhill from steep crests
    /// into neighbouring hollows without ever leaking mass out the ends.
    fn relax(&mut self, max_slope: f32) {
        let n = self.cells.len();
        if n < 2 {
            return;
        }
        let mut delta = vec![0.0_f32; n];
        for i in 0..n - 1 {
            let diff = self.cells[i] - self.cells[i + 1];
            let rate = if diff.abs() > max_slope { 0.30 } else { 0.12 };
            let flow = diff * rate;
            delta[i] -= flow;
            delta[i + 1] += flow;
        }
        for (cell, d) in self.cells.iter_mut().zip(delta) {
            *cell = (*cell + d).max(0.0);
        }
    }

    fn resample(&mut self) {
        let count = ((self.rect.w / CELL).ceil() as usize).clamp(1, MAX_CELLS);
        if count != self.cells.len() {
            let old = std::mem::take(&mut self.cells);
            self.cells = (0..count).map(|i| old[i * old.len() / count]).collect();
        }
    }

    /// Converts `fraction` of every cell into falling snow clumps and flakes,
    /// carrying inertia `(vx, vy)` plus subtle lateral slide away from the center.
    fn shed(
        &mut self,
        fraction: f32,
        vx: f32,
        vy: f32,
        window_id: u64,
        clump: bool,
        out: &mut Vec<Shed>,
    ) {
        let mid = self.rect.x + self.rect.w * 0.5;
        let half_w = (self.rect.w * 0.5).max(1.0);
        for i in 0..self.cells.len() {
            let before = self.cells[i];
            let lost = before * fraction;
            self.cells[i] -= lost;
            if lost > 0.35 {
                let cx = self.cell_x(i);
                let edge_push = ((cx - mid) / half_w) * 28.0;
                out.push(Shed {
                    x: cx,
                    y: self.rect.y - before * 0.55,
                    vx: vx + edge_push,
                    vy,
                    amount: lost,
                    window_id,
                    clump,
                });
            }
        }
    }

    /// How much to take from each cell within `radius` of a point, at most
    /// `depth`. Nothing when the point is more than `inside` below the top
    /// of the surface under the pile.
    fn cuts(
        &self,
        x: f32,
        y: f32,
        radius: f32,
        depth: f32,
        inside: f32,
    ) -> impl Iterator<Item = (usize, f32)> + '_ {
        let tallest = self.cells.iter().fold(0.0_f32, |a, &b| a.max(b));
        let reach = x >= self.rect.x - radius
            && x <= self.rect.right() + radius
            && y <= self.rect.y + inside
            && y >= self.rect.y - tallest - radius - 54.0
            && y <= self.rect.y + radius + 54.0;
        let count = if reach { self.cells.len() } else { 0 };
        (0..count).filter_map(move |i| {
            let cx = self.cell_x(i);
            let top = self.rect.y - self.cells[i];
            // A pointer inside deep snow scoops from where it is.
            let below = (y - top).max(0.0).min(self.rect.y - top);
            let d = ((cx - x).powi(2) + (top + below - y).powi(2)).sqrt();
            (d < radius).then(|| (i, self.cells[i].min(depth * (1.0 - d / radius))))
        })
    }

    /// Snow pushed to the left and right ends of a window or dock spills over
    /// the corners in tumbling clumps, keeping rounded shoulders and cascading
    /// down the sides once the top approaches capacity.
    fn spill(&mut self, max: f32, window_id: u64, out: &mut Vec<Shed>) {
        let n = self.cells.len();
        if n < 6 || max < 2.0 {
            return;
        }
        for (end, inner, side) in [(0, 3, -1.0), (n - 1, n - 4, 1.0)] {
            let (here, held) = (self.cells[end], self.cells[inner]);
            // Natural rounded shoulder at each corner: lower than the inner crown,
            // and never taller than 68% of max capacity.
            let shoulder = (held * 0.82).min(max * 0.68).max(max * 0.14);
            let near_full = held >= max * 0.85 && here > max * 0.42;
            if here > shoulder + 0.8 || near_full {
                let excess = if here > shoulder + 0.8 {
                    (here - shoulder) * 0.65
                } else {
                    (here * 0.14).max(1.2)
                };
                self.cells[end] = (self.cells[end] - excess).max(0.0);
                let next = if side < 0.0 { end + 1 } else { end - 1 };
                let next_over = (self.cells[next] - shoulder * 1.15).max(0.0) * 0.35;
                self.cells[next] = (self.cells[next] - next_over).max(0.0);
                let total = excess + next_over;
                let edge_x = if side < 0.0 {
                    self.rect.x
                } else {
                    self.rect.right()
                };
                out.push(Shed {
                    x: edge_x + side * 4.5,
                    y: self.rect.y - here * 0.6,
                    vx: side * (28.0 + total * 2.5).min(75.0),
                    vy: 14.0,
                    amount: (total * 1.8).max(1.6),
                    window_id,
                    clump: true,
                });
            }
        }
    }

    fn mean(&self) -> f32 {
        self.cells.iter().sum::<f32>() / self.cells.len().max(1) as f32
    }

    /// Fills the pile with a natural sculpted layer about `depth` deep.
    /// `drifts` builds rolling hills and deep corner mounds along the bottom of
    /// the display; otherwise the layer forms a plush pillowed cap that rounds
    /// off cleanly at both ends.
    fn lay(&mut self, depth: f32, max: f32, phase: f32, drifts: bool) {
        let (x0, w) = (self.rect.x, self.rect.w.max(1.0));
        for i in 0..self.cells.len() {
            let x = self.cell_x(i);
            let wave = if drifts {
                0.78 + 0.16 * (x * 0.0055 + phase).sin()
                    + 0.10 * (x * 0.017 + phase * 1.9).sin()
                    + 0.05 * (x * 0.048 + phase * 4.3).sin()
            } else {
                0.86 + 0.10 * (x * 0.013 + phase).sin()
                    + 0.06 * (x * 0.039 + phase * 2.3).sin()
                    + 0.03 * (x * 0.11 + phase * 5.1).sin()
            };
            let (left, right) = (x - x0, x0 + w - x);
            let shape = if drifts {
                1.0 + 0.68 * (-left / 200.0).exp() + 0.68 * (-right / 200.0).exp()
            } else {
                let u = ((x - x0) / w).clamp(0.0, 1.0);
                let t = (left.min(right) / 36.0).clamp(0.0, 1.0);
                let end_taper = t * t * (3.0 - 2.0 * t);
                let arch = 0.74 + 0.26 * (u * std::f32::consts::PI).sin();
                arch * end_taper
            };
            self.cells[i] = (depth * wave * shape).clamp(0.0, max);
        }
    }
}

/// Snow that has just left a pile, for the simulation to turn into falling
/// clumps and flakes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shed {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub amount: f32,
    /// Stable id of the window this snow fell from (`0` for floor/dock), so
    /// falling clumps remain visible in front of their own window while still
    /// hidden by windows stacked above it.
    pub window_id: u64,
    /// True when snow breaks off in solid clumps (window shake, spillover).
    pub clump: bool,
}

pub(crate) struct Piles {
    pub list: Vec<Pile>,
    pub floor: Pile,
    /// For each current window, its pile index in `list`. Windows without an id have none.
    pub slots: Vec<Option<usize>>,
    /// For each current dock, its pile index in `list`.
    pub dock_slots: Vec<Option<usize>>,
    frame: u64,
    relax_in: f32,
    max_seen: f32,
    /// Depth, max, end time, and phase of the starting blanket: windows and
    /// docks found before `until` start with snow already resting on top.
    blanket: Option<(f32, f32, f32, f32)>,
}

impl Piles {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            list: Vec::new(),
            floor: Pile::new(
                0,
                Rect {
                    x: 0.0,
                    y: height,
                    w: width,
                    h: 0.0,
                },
                false,
                0.0,
            ),
            slots: Vec::new(),
            dock_slots: Vec::new(),
            frame: 0,
            relax_in: RELAX_EVERY,
            max_seen: 130.0,
            blanket: None,
        }
    }

    /// Lays snow along the bottom now, and on windows/docks found in the next
    /// moments, so the scene starts out already snowed in.
    pub fn lay_blanket(&mut self, depth: f32, max: f32, until: f32, phase: f32) {
        self.floor.lay(depth, max, phase, true);
        for pile in &mut self.list {
            let cap = if pile.is_dock {
                dock_cap(max, pile.rect.y)
            } else {
                window_cap(max, pile.rect.y)
            };
            pile.lay((depth * 0.95).min(cap), cap, phase, false);
        }
        self.enforce_dock_floor_rim(max);
        self.blanket = Some((depth.min(max), max, until, phase));
    }

    /// Trims every pile to a new maximum depth.
    pub fn limit(&mut self, max: f32) {
        self.floor.cells.iter_mut().for_each(|c| *c = c.min(max));
        for pile in &mut self.list {
            let cap = if pile.is_dock {
                dock_cap(max, pile.rect.y)
            } else {
                window_cap(max, pile.rect.y)
            };
            pile.cells.iter_mut().for_each(|c| *c = c.min(cap));
        }
        if let Some((depth, bmax, _, _)) = &mut self.blanket {
            *depth = depth.min(max);
            *bmax = (*bmax).min(max);
        }
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.slots.clear();
        self.dock_slots.clear();
        self.floor.cells.fill(0.0);
        self.blanket = None;
    }

    pub fn is_current(&self, pile: &Pile) -> bool {
        pile.seen == self.frame
    }

    /// Follows windows and desktop docks. Rapid moves or back-and-forth shakes
    /// convert accumulated snow into falling clumps with inertia instead of
    /// deleting it. Maximized (non-ledge) windows drop their snow cleanly.
    pub fn sync(
        &mut self,
        windows: &[Window],
        ledges: &[bool],
        docks: &[Window],
        clock: f32,
        dt: f32,
    ) -> Vec<Shed> {
        let prev_frame = self.frame;
        self.frame += 1;
        let frame = self.frame;
        let mut shed = Vec::new();
        let ledge = |i: usize| ledges.get(i).copied().unwrap_or(true);

        for (i, window) in windows.iter().enumerate().filter(|(_, w)| w.id != 0) {
            let rect = window.rect();
            let found = self
                .list
                .iter()
                .position(|p| !p.is_dock && p.id == window.id);
            if !ledge(i) {
                if let Some(index) = found {
                    let mut pile = self.list.swap_remove(index);
                    pile.shed(1.0, 0.0, 50.0, 0, true, &mut shed);
                }
                continue;
            }
            let index = self.ensure_pile(window.id, rect, false, clock);
            self.step_pile_motion(index, rect, window.id, clock, dt, &mut shed);
            self.list[index].seen = frame;
        }

        for (i, dock) in docks.iter().enumerate() {
            let id = if dock.id != 0 {
                dock.id
            } else {
                0xD0CC_0000 + i as u64 + 1
            };
            let rect = dock.rect();
            let index = self.ensure_pile(id, rect, true, clock);
            self.step_pile_motion(index, rect, 0, clock, dt, &mut shed);
            self.list[index].seen = frame;
        }

        // Windows or docks that just closed/minimized/autohid drop their snow
        // instead of letting it vanish abruptly.
        let mut gone = Vec::new();
        for (i, pile) in self.list.iter_mut().enumerate() {
            if pile.seen == prev_frame {
                pile.shed(1.0, pile.vx * 0.35, 30.0, 0, true, &mut shed);
                gone.push(i);
            }
        }
        for i in gone.into_iter().rev() {
            self.list.swap_remove(i);
        }

        self.slots.clear();
        for (i, window) in windows.iter().enumerate() {
            let slot = (window.id != 0 && ledge(i))
                .then(|| {
                    self.list
                        .iter()
                        .position(|p| !p.is_dock && p.id == window.id)
                })
                .flatten();
            self.slots.push(slot);
        }

        self.dock_slots.clear();
        for (i, dock) in docks.iter().enumerate() {
            let id = if dock.id != 0 {
                dock.id
            } else {
                0xD0CC_0000 + i as u64 + 1
            };
            let slot = self.list.iter().position(|p| p.is_dock && p.id == id);
            self.dock_slots.push(slot);
        }

        let max_seen = self.max_seen.max(1.0);
        self.relax_in -= dt;
        if self.relax_in <= 0.0 {
            self.relax_in = RELAX_EVERY;
            self.floor.relax(3.5);
            for pile in self.list.iter_mut().filter(|p| p.seen == frame) {
                pile.relax(4.0);
                let (cap, wid) = if pile.is_dock {
                    (dock_cap(max_seen, pile.rect.y), 0)
                } else {
                    (window_cap(max_seen, pile.rect.y), pile.id)
                };
                pile.spill(cap, wid, &mut shed);
            }
        }
        if !docks.is_empty() {
            self.enforce_dock_floor_rim(max_seen);
        }
        shed
    }

    fn ensure_pile(&mut self, id: u64, rect: Rect, is_dock: bool, clock: f32) -> usize {
        if let Some(index) = self
            .list
            .iter()
            .position(|p| p.is_dock == is_dock && p.id == id)
        {
            return index;
        }
        if self.list.len() >= MAX_PILES
            && let Some(oldest) = (0..self.list.len()).min_by_key(|&i| self.list[i].seen)
        {
            self.list.swap_remove(oldest);
        }
        let mut pile = Pile::new(id, rect, is_dock, clock);
        if let Some((depth, max, until, phase)) = self.blanket
            && clock < until
        {
            let cap = if is_dock {
                dock_cap(max, rect.y)
            } else {
                window_cap(max, rect.y)
            };
            let initial = if is_dock { depth * 0.9 } else { depth * 0.95 };
            pile.lay(initial.min(cap), cap, phase + (id & 0xFFFF) as f32, false);
        }
        self.list.push(pile);
        self.list.len() - 1
    }

    /// Maximum floor height allowed at `x`: full depth over open desktop,
    /// tapering to a crisp low frosty rim across any visible bottom dock so
    /// dock icons remain clear and readable while snow piles high on top of
    /// the dock and in the deep flanking drifts to its left and right.
    fn floor_cap_at(&self, x: f32, max: f32) -> f32 {
        let floor_y = self.floor.rect.y;
        let rim = (max * 0.085).clamp(6.0, 14.0);
        let feather = 92.0;
        let mut cap = max;
        for pile in self
            .list
            .iter()
            .filter(|p| p.is_dock && p.seen == self.frame)
        {
            let dock = pile.rect;
            if dock.bottom() < floor_y - 48.0 {
                continue;
            }
            if x >= dock.x - feather && x <= dock.right() + feather {
                let inside_l = ((x - (dock.x - feather)) / feather).clamp(0.0, 1.0);
                let inside_r = (((dock.right() + feather) - x) / feather).clamp(0.0, 1.0);
                let k = inside_l.min(inside_r);
                let blend = k * k * (3.0 - 2.0 * k);
                cap = cap.min(max * (1.0 - blend) + rim * blend);
            }
        }
        cap
    }

    fn enforce_dock_floor_rim(&mut self, max: f32) {
        for i in 0..self.floor.cells.len() {
            let cx = self.floor.cell_x(i);
            let cap = self.floor_cap_at(cx, max);
            if self.floor.cells[i] > cap {
                self.floor.cells[i] = cap;
            }
        }
    }

    fn step_pile_motion(
        &mut self,
        index: usize,
        rect: Rect,
        window_id: u64,
        clock: f32,
        dt: f32,
        shed: &mut Vec<Shed>,
    ) {
        let pile = &mut self.list[index];
        let (dx, dy) = (rect.x - pile.rect.x, rect.y - pile.rect.y);
        let width_changed = (rect.w - pile.rect.w).abs() > 1.0;

        if dx.abs() > 1.5 || dy.abs() > 1.5 {
            // Desktop positions arrive roughly every 50..150 ms while moving.
            let elapsed = (clock - pile.changed_at).clamp(0.03, 0.15);
            let (raw_vx, raw_vy) = (dx / elapsed, dy / elapsed);
            let raw_speed = raw_vx.hypot(raw_vy);
            let speed = raw_speed.min(2200.0);
            let scale = speed / raw_speed.max(1.0);
            let (vx, vy) = (raw_vx * scale, raw_vy * scale);

            let (pvx, pvy) = (pile.vx, pile.vy);
            let jerk = (vx - pvx).hypot(vy - pvy);
            let reversed_x = pvx.abs() > 75.0 && vx.abs() > 75.0 && pvx.signum() != vx.signum();
            let reversed_y = pvy.abs() > 75.0 && vy.abs() > 75.0 && pvy.signum() != vy.signum();
            let reversal = if reversed_x || reversed_y { 0.45 } else { 0.0 };

            pile.shake = (pile.shake + speed / 1050.0 + jerk / 1300.0 + reversal).min(3.0);

            if speed > 90.0 || pile.shake > 0.35 || reversed_x || reversed_y {
                let fraction =
                    (speed / 540.0 + jerk / 900.0 + reversal + pile.shake * 0.22).clamp(0.18, 1.0);
                // Inertia: on a direction reversal, snow keeps its prior momentum
                // and shoots off; on a sudden start, snow lags behind the window
                // and tumbles downward under gravity.
                let (svx, svy) = if reversed_x || reversed_y {
                    (
                        (pvx * 0.52 - vx * 0.10).clamp(-650.0, 650.0),
                        (pvy * 0.35 - vy * 0.08).clamp(-200.0, 140.0),
                    )
                } else {
                    (
                        (vx * 0.28 + pvx * 0.22).clamp(-650.0, 650.0),
                        (vy * 0.22).clamp(-180.0, 90.0),
                    )
                };
                pile.shed(fraction, svx, svy, window_id, true, shed);
            }
            pile.vx = vx;
            pile.vy = vy;
            pile.rect = rect;
            pile.changed_at = clock;
            if width_changed {
                pile.resample();
            }
        } else {
            if width_changed {
                pile.rect = rect;
                pile.changed_at = clock;
                pile.resample();
            } else {
                pile.rect = rect;
            }
            // If the window just braked from fast motion, momentum throws a
            // final cascade of snow off the top before velocity settles.
            if clock - pile.changed_at > 0.16 {
                let prev_speed = pile.vx.hypot(pile.vy);
                if prev_speed > 360.0 {
                    pile.shed(
                        0.38,
                        (pile.vx * 0.5).clamp(-600.0, 600.0),
                        (pile.vy * 0.25).clamp(-160.0, 80.0),
                        window_id,
                        true,
                        shed,
                    );
                    pile.vx *= 0.35;
                    pile.vy *= 0.35;
                }
                let decay = (-dt * 12.0).exp();
                pile.vx *= decay;
                pile.vy *= decay;
                pile.shake = (pile.shake - dt * 1.8).max(0.0);
            }
        }
    }

    /// The depth limit last passed to `deposit`, for spilling overhangs.
    pub fn note_max(&mut self, max: f32) {
        self.max_seen = max;
    }

    /// How deep the snow on window `window` is at `x`.
    pub fn depth_on(&self, window: usize, x: f32) -> f32 {
        match self.slots.get(window).copied().flatten() {
            Some(i) => {
                let pile = &self.list[i];
                pile.index_at(x).map_or(0.0, |c| pile.cells[c])
            }
            None => 0.0,
        }
    }

    /// Local surface slope on window `window` at `x`.
    pub fn slope_on(&self, window: usize, x: f32) -> f32 {
        self.slots
            .get(window)
            .copied()
            .flatten()
            .map_or(0.0, |i| self.list[i].slope_at(x))
    }

    /// How deep the snow on dock `dock` is at `x`.
    pub fn depth_on_dock(&self, dock: usize, x: f32) -> f32 {
        match self.dock_slots.get(dock).copied().flatten() {
            Some(i) => {
                let pile = &self.list[i];
                pile.index_at(x).map_or(0.0, |c| pile.cells[c])
            }
            None => 0.0,
        }
    }

    /// Local surface slope on dock `dock` at `x`.
    pub fn slope_on_dock(&self, dock: usize, x: f32) -> f32 {
        self.dock_slots
            .get(dock)
            .copied()
            .flatten()
            .map_or(0.0, |i| self.list[i].slope_at(x))
    }

    /// Local surface slope on the bottom snow drift at `x`.
    pub fn floor_slope(&self, x: f32) -> f32 {
        self.floor.slope_at(x)
    }

    /// Packs the floor snow down near `x` to `keep` of its depth and at
    /// most `cap` pixels (used by sliding penguins), returning how much moved.
    pub fn pack_floor_near(&mut self, x: f32, radius: f32, keep: f32, cap: f32) -> f32 {
        let mut moved = 0.0;
        for i in 0..self.floor.cells.len() {
            if (self.floor.cell_x(i) - x).abs() < radius {
                let c = &mut self.floor.cells[i];
                let target = (*c * keep).min(cap);
                if *c > target {
                    moved += *c - target;
                    *c = target;
                }
            }
        }
        moved
    }

    /// Smoothly ploughs the floor snow: scoops along a curved blade profile,
    /// pushes a soft rolling snow wave ahead of the blade, and returns the
    /// volume thrown into the air for spray and the blade berm.
    pub fn plough_floor(&mut self, blade_x: f32, max_depth: f32) -> f32 {
        let mut scooped = 0.0;
        let back = blade_x - 28.0;
        let front = blade_x + 18.0;
        let span = (front - back).max(1.0);
        for i in 0..self.floor.cells.len() {
            let cx = self.floor.cell_x(i);
            if cx >= back && cx <= front {
                let t = ((cx - back) / span).clamp(0.0, 1.0);
                // Smooth curved ramp from firm packed track behind the blade to full height ahead.
                let ramp = t * t * (3.0 - 2.0 * t);
                let c = &mut self.floor.cells[i];
                let track = (*c * 0.12).min(5.5);
                let target = track + (*c - track) * ramp;
                if *c > target {
                    scooped += *c - target;
                    *c = target;
                }
            }
        }
        if scooped <= 0.01 {
            return 0.0;
        }
        // Push ~18% forward into a broad, rounded bow wave ahead of the blade;
        // the rest feeds the rounded berm carried on the blade and airborne spray.
        let wave_start = blade_x + 14.0;
        let wave_end = blade_x + 110.0;
        let wave_mid = blade_x + 56.0;
        let push_budget = scooped * 0.18;
        let mut thrown = scooped - push_budget;
        let mut weights = Vec::new();
        let mut wsum = 0.0_f32;
        for i in 0..self.floor.cells.len() {
            let cx = self.floor.cell_x(i);
            if cx >= wave_start && cx <= wave_end {
                let d = ((cx - wave_mid) / 48.0).abs().min(1.0);
                let w = (1.0 - d * d).powi(2);
                if w > 0.0 {
                    weights.push((i, w));
                    wsum += w;
                }
            }
        }
        if wsum > 0.0 {
            for (i, w) in weights {
                let cx = self.floor.cell_x(i);
                let local_cap = self.floor_cap_at(cx, max_depth);
                let wave_crest = (local_cap * 1.12).max(18.0);
                let give = push_budget * (w / wsum);
                let c = &mut self.floor.cells[i];
                let next = *c + give;
                if next > wave_crest {
                    thrown += next - wave_crest;
                    *c = wave_crest;
                } else {
                    *c = next;
                }
            }
            // Smooth only the cells strictly ahead of the blade so the bow wave
            // forms a soft rounded crest without diffusing back into the track.
            let first = (((wave_start - self.floor.rect.x) / self.floor.cell_width())
                .ceil()
                .max(0.0) as usize)
                .min(self.floor.cells.len());
            if first + 2 < self.floor.cells.len() {
                for _ in 0..2 {
                    let ahead = &mut self.floor.cells[first..];
                    for j in 1..ahead.len() - 1 {
                        let avg = (ahead[j - 1] + ahead[j + 1]) * 0.5;
                        ahead[j] += (avg - ahead[j]) * 0.3;
                    }
                }
            }
        } else {
            // Blade is exiting the right edge: the pushed wave spills off-screen.
            thrown += push_budget;
        }
        thrown
    }

    /// A small dent in the floor snow, such as a footprint.
    pub fn dent_floor(&mut self, x: f32, depth: f32) {
        if let Some(i) = self.floor.index_at(x) {
            let c = &mut self.floor.cells[i];
            *c = (*c - depth).max(*c * 0.4);
        }
    }

    /// Snow taken off window `window` near `x`, as when a bird lands on it.
    pub fn knock_off(&mut self, window: usize, x: f32, radius: f32, share: f32) -> Vec<Shed> {
        let mut out = Vec::new();
        if let Some(i) = self.slots.get(window).copied().flatten() {
            let pile = &mut self.list[i];
            let wid = pile.id;
            for c in 0..pile.cells.len() {
                let cx = pile.cell_x(c);
                if (cx - x).abs() < radius {
                    let lost = pile.cells[c] * share;
                    pile.cells[c] -= lost;
                    if lost > 0.3 {
                        out.push(Shed {
                            x: cx,
                            y: pile.rect.y - lost,
                            vx: (cx - x) * 3.0,
                            vy: -40.0,
                            amount: lost,
                            window_id: wid,
                            clump: true,
                        });
                    }
                }
            }
        }
        out
    }

    /// Adds snow to a window's pile, returning any overflow clumps that spill
    /// off the left/right corners once the top reaches capacity.
    pub fn deposit(
        &mut self,
        window: usize,
        x: f32,
        amount: f32,
        max: f32,
        wind: f32,
        out: &mut Vec<Shed>,
    ) -> bool {
        match self.slots.get(window).copied().flatten() {
            Some(i) => {
                let pile = &mut self.list[i];
                let cap = window_cap(max, pile.rect.y);
                let wid = pile.id;
                pile.deposit_ledge(x, amount, cap, wind, wid, out);
                true
            }
            None => false,
        }
    }

    /// Adds snow to a desktop dock's pile, spilling over its corners when full.
    pub fn deposit_dock(
        &mut self,
        dock: usize,
        x: f32,
        amount: f32,
        max: f32,
        wind: f32,
        out: &mut Vec<Shed>,
    ) -> bool {
        match self.dock_slots.get(dock).copied().flatten() {
            Some(i) => {
                let pile = &mut self.list[i];
                let cap = dock_cap(max, pile.rect.y);
                pile.deposit_ledge(x, amount, cap, wind, 0, out);
                true
            }
            None => false,
        }
    }

    pub fn deposit_floor(&mut self, x: f32, amount: f32, max: f32) {
        let cap = self.floor_cap_at(x, max);
        self.floor.deposit_kernel(x, amount, cap, true);
        if let Some(center) = self.floor.index_at(x) {
            let start = center.saturating_sub(3);
            let end = (center + 3).min(self.floor.cells.len().saturating_sub(1));
            for i in start..=end {
                let cx = self.floor.cell_x(i);
                let local_cap = self.floor_cap_at(cx, max);
                if self.floor.cells[i] > local_cap {
                    self.floor.cells[i] = local_cap;
                }
            }
        }
    }

    pub fn floor_mean(&self) -> f32 {
        self.floor.mean()
    }

    pub fn floor_height(&self, x: f32) -> f32 {
        self.floor.index_at(x).map_or(0.0, |i| self.floor.cells[i])
    }

    /// Scoops snow near a point on any visible surface, returning how much.
    pub fn carve(&mut self, x: f32, y: f32, radius: f32, depth: f32) -> f32 {
        self.take_near(x, y, radius, depth, f32::INFINITY)
    }

    /// Like `carve`, but only from snow the pointer is on. A pointer inside
    /// a window, such as on its title bar, leaves that window's snow alone,
    /// so ordinary clicks and drags do not take any.
    pub fn scoop(&mut self, x: f32, y: f32, radius: f32, depth: f32) -> f32 {
        self.take_near(x, y, radius, depth, 3.0)
    }

    /// How much `scoop` would take here, without taking it.
    pub fn snow_under(&self, x: f32, y: f32, radius: f32, depth: f32) -> f32 {
        let frame = self.frame;
        let current = self.list.iter().filter(|p| p.seen == frame);
        std::iter::once(&self.floor)
            .chain(current)
            .map(|pile| {
                pile.cuts(x, y, radius, depth, 3.0)
                    .map(|(_, cut)| cut)
                    .sum::<f32>()
            })
            .sum()
    }

    fn take_near(&mut self, x: f32, y: f32, radius: f32, depth: f32, inside: f32) -> f32 {
        let frame = self.frame;
        let mut taken = 0.0;
        let floor = std::iter::once(&mut self.floor);
        for pile in floor.chain(self.list.iter_mut().filter(|p| p.seen == frame)) {
            let cuts: Vec<(usize, f32)> = pile.cuts(x, y, radius, depth, inside).collect();
            for (i, cut) in cuts {
                pile.cells[i] -= cut;
                taken += cut;
            }
        }
        taken
    }

    pub fn shake_all(&mut self) -> Vec<Shed> {
        let mut shed = Vec::new();
        let frame = self.frame;
        for pile in self.list.iter_mut().filter(|p| p.seen == frame) {
            let wid = if pile.is_dock { 0 } else { pile.id };
            pile.shed(1.0, 0.0, 45.0, wid, true, &mut shed);
        }
        self.floor.shed(1.0, 0.0, -70.0, 0, true, &mut shed);
        shed
    }
}
