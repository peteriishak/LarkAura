//! Where windows hide effects. With the default placement, effects stay on
//! the desktop: the renderer hides every fragment that falls inside a window.
//! Snow and creatures resting on a window's top are drawn on that window's
//! layer, so only windows stacked above it hide them.
//!
//! Window outlines change smoothly. A window that grows, opens or moves
//! covers its new area at once, and reaches a little ahead of itself while
//! it is dragged, because positions arrive less often than frames. A window
//! that shrinks lets go of its old area over a fifth of a second, and one
//! that closes keeps its outline while it fades away, so effects never flash
//! over a window that GNOME is still animating.

use aura_core::{Rect, Window};

/// Room in the renderer's mask table.
pub const MAX_MASKS: usize = 32;
/// Seconds a closed or minimized window keeps hiding effects as it fades.
const GHOST_SECONDS: f32 = 0.3;
/// How quickly an outline lets go of area the window no longer covers.
const SHRINK_RATE: f32 = 14.0;
/// Seconds of motion a dragged window's outline reaches ahead by.
const LEAD_SECONDS: f32 = 0.12;
const MAX_LEAD: f32 = 160.0;

/// A rectangle the renderer keeps clear, in logical pixels, with how
/// strongly it hides what is behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mask {
    /// x, y, width and height.
    pub rect: [f32; 4],
    pub alpha: f32,
}

#[derive(Clone, Copy, Debug)]
struct Tracked {
    id: u64,
    shown: Rect,
    target: Rect,
    vx: f32,
    vy: f32,
    changed_at: f32,
    alpha: f32,
    seen: u64,
}

#[derive(Default)]
pub(crate) struct Masks {
    tracked: Vec<Tracked>,
    out: Vec<Mask>,
    /// Windows below this index are left out when there are too many.
    offset: usize,
    frame: u64,
    clock: f32,
}

fn ease_edge(shown: f32, target: f32, grows: bool, k: f32) -> f32 {
    if grows {
        target
    } else {
        shown + (target - shown) * k
    }
}

impl Masks {
    pub fn masks(&self) -> &[Mask] {
        &self.out
    }

    pub fn clear(&mut self) {
        self.tracked.clear();
        self.out.clear();
        self.offset = 0;
    }

    /// Follows the windows, which are ordered from the bottom of the stack.
    pub fn update(&mut self, windows: &[Window], dt: f32, enabled: bool) {
        self.frame += 1;
        self.clock += dt;
        if !enabled {
            self.clear();
            return;
        }
        let (frame, clock) = (self.frame, self.clock);
        let mut order = Vec::with_capacity(windows.len());
        for (i, w) in windows.iter().enumerate() {
            let rect = w.rect();
            let found = (w.id != 0)
                .then(|| {
                    self.tracked
                        .iter()
                        .position(|t| t.id == w.id && t.seen != frame)
                })
                .flatten();
            let index = match found {
                Some(index) => index,
                None => {
                    self.tracked.push(Tracked {
                        id: if w.id == 0 { u64::MAX - i as u64 } else { w.id },
                        shown: rect,
                        target: rect,
                        vx: 0.0,
                        vy: 0.0,
                        changed_at: clock,
                        alpha: 1.0,
                        seen: frame,
                    });
                    self.tracked.len() - 1
                }
            };
            let t = &mut self.tracked[index];
            t.seen = frame;
            t.alpha = 1.0;
            if rect != t.target {
                // Positions arrive every 50 to 150 ms while things move.
                let elapsed = (clock - t.changed_at).clamp(0.04, 0.2);
                if rect.w == t.target.w && rect.h == t.target.h {
                    t.vx = (rect.x - t.target.x) / elapsed;
                    t.vy = (rect.y - t.target.y) / elapsed;
                } else {
                    (t.vx, t.vy) = (0.0, 0.0);
                }
                t.target = rect;
                t.changed_at = clock;
            } else if clock - t.changed_at > 0.2 {
                // The window has stopped.
                let k = (-dt * 12.0).exp();
                t.vx *= k;
                t.vy *= k;
            }
            order.push(index);
        }
        // Windows that went away keep their outline while they fade.
        for t in &mut self.tracked {
            if t.seen != frame {
                t.alpha -= dt / GHOST_SECONDS;
                t.vx = 0.0;
                t.vy = 0.0;
            }
        }
        let k = 1.0 - (-dt * SHRINK_RATE).exp();
        for t in &mut self.tracked {
            let lead = |v: f32| (v * LEAD_SECONDS).clamp(-MAX_LEAD, MAX_LEAD);
            let (lx, ly) = (lead(t.vx), lead(t.vy));
            let goal = Rect {
                x: t.target.x + lx.min(0.0),
                y: t.target.y + ly.min(0.0),
                w: t.target.w + lx.abs(),
                h: t.target.h + ly.abs(),
            };
            let s = t.shown;
            let x0 = ease_edge(s.x, goal.x, goal.x < s.x, k);
            let y0 = ease_edge(s.y, goal.y, goal.y < s.y, k);
            let x1 = ease_edge(s.right(), goal.right(), goal.right() > s.right(), k);
            let y1 = ease_edge(s.bottom(), goal.bottom(), goal.bottom() > s.bottom(), k);
            t.shown = Rect {
                x: x0,
                y: y0,
                w: (x1 - x0).max(0.0),
                h: (y1 - y0).max(0.0),
            };
        }
        // Rebuild the list in stacking order, with fading windows on top.
        let ghosts: Vec<usize> = (0..self.tracked.len())
            .filter(|&i| self.tracked[i].seen != frame && self.tracked[i].alpha > 0.0)
            .collect();
        let room = MAX_MASKS.saturating_sub(ghosts.len().min(MAX_MASKS / 4));
        self.offset = order.len().saturating_sub(room);
        let mask = |t: &Tracked| Mask {
            rect: [t.shown.x, t.shown.y, t.shown.w, t.shown.h],
            alpha: t.alpha.clamp(0.0, 1.0),
        };
        self.out.clear();
        self.out
            .extend(order[self.offset..].iter().map(|&i| mask(&self.tracked[i])));
        let left = MAX_MASKS - self.out.len();
        self.out
            .extend(ghosts.iter().take(left).map(|&i| mask(&self.tracked[i])));
        self.tracked.retain(|t| t.seen == frame || t.alpha > 0.0);
    }

    /// The layer to draw something resting on window `index`: only windows
    /// above it hide it. Things on the desktop use layer 0.
    pub fn layer_for(&self, index: Option<usize>) -> f32 {
        match index {
            Some(i) if i >= self.offset && !self.out.is_empty() => (i - self.offset + 1) as f32,
            _ => 0.0,
        }
    }

    /// A layer above every normal window mask, used for snow sitting on or
    /// falling directly in front of a desktop dock that renders above windows.
    pub fn top_layer(&self) -> f32 {
        self.out.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, x: f32, w: f32) -> Window {
        Window {
            id,
            x,
            y: 100.0,
            w,
            h: 300.0,
        }
    }

    #[test]
    fn growing_is_instant_and_shrinking_is_smooth() {
        let mut m = Masks::default();
        m.update(&[window(1, 100.0, 400.0)], 1.0 / 30.0, true);
        m.update(&[window(1, 0.0, 1000.0)], 1.0 / 30.0, true);
        assert_eq!(m.masks()[0].rect[2], 1000.0);
        m.update(&[window(1, 100.0, 400.0)], 1.0 / 30.0, true);
        let w = m.masks()[0].rect[2];
        assert!(w > 400.0 && w < 1000.0, "{w}");
        for _ in 0..30 {
            m.update(&[window(1, 100.0, 400.0)], 1.0 / 30.0, true);
        }
        assert!((m.masks()[0].rect[2] - 400.0).abs() < 2.0);
    }

    #[test]
    fn closed_windows_fade_out() {
        let mut m = Masks::default();
        m.update(&[window(1, 100.0, 400.0)], 1.0 / 30.0, true);
        m.update(&[], 1.0 / 30.0, true);
        assert_eq!(m.masks().len(), 1);
        assert!(m.masks()[0].alpha < 1.0 && m.masks()[0].alpha > 0.5);
        for _ in 0..12 {
            m.update(&[], 1.0 / 30.0, true);
        }
        assert!(m.masks().is_empty());
    }

    #[test]
    fn dragged_windows_reach_ahead() {
        let mut m = Masks::default();
        m.update(&[window(1, 100.0, 400.0)], 0.05, true);
        m.update(&[window(1, 140.0, 400.0)], 0.05, true);
        let [x, _, w, _] = m.masks()[0].rect;
        assert!(
            x + w > 540.0 + 10.0,
            "the leading edge reaches ahead: {}",
            x + w
        );
    }

    #[test]
    fn layers_follow_the_stack() {
        let mut m = Masks::default();
        let windows: Vec<Window> = (0..40).map(|i| window(i + 1, i as f32, 100.0)).collect();
        m.update(&windows, 0.03, true);
        assert_eq!(m.masks().len(), MAX_MASKS);
        assert_eq!(m.layer_for(Some(39)), MAX_MASKS as f32);
        assert_eq!(m.layer_for(Some(0)), 0.0);
        assert_eq!(m.layer_for(None), 0.0);
        m.update(&windows, 0.03, false);
        assert!(m.masks().is_empty());
        assert_eq!(m.layer_for(Some(39)), 0.0);
    }
}
