//! Platform-free engine logic: desktop model, user options and power policy.

use serde::{Deserialize, Serialize};

/// Axis-aligned rectangle in overlay-local logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// A window this big counts as fully opened: effects never show over
    /// it and snow does not settle on it, because its top is not a ledge
    /// on the desktop but the edge of the work area.
    pub fn is_large(&self, width: f32, height: f32) -> bool {
        self.w >= width * 0.9 && self.h >= height * 0.85
    }
}

/// True when windows hide all of the desktop, so effects kept off windows
/// would draw nothing. Checked on a coarse grid with a small margin, so a
/// gap of a few pixels between tiled windows does not count as desktop.
pub fn desktop_hidden(windows: &[Window], width: f32, height: f32) -> bool {
    const STEPS: usize = 24;
    if windows.is_empty() || width <= 0.0 || height <= 0.0 {
        return false;
    }
    let margin = 6.0;
    (0..STEPS).all(|i| {
        (0..STEPS).all(|j| {
            let x = (i as f32 + 0.5) / STEPS as f32 * width;
            let y = (j as f32 + 0.5) / STEPS as f32 * height;
            windows.iter().any(|w| {
                x >= w.x - margin
                    && x < w.x + w.w + margin
                    && y >= w.y - margin
                    && y < w.y + w.h + margin
            })
        })
    })
}

/// A desktop window. The id keeps snow attached to the same window as it moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    #[serde(default)]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Window {
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
    }
}

/// What the desktop integration currently knows. A desktop that cannot supply
/// something leaves it empty and effects adapt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopModel {
    /// Ordered from the bottom of the stack to the top.
    pub windows: Vec<Window>,
    /// Visible desktop docks or bottom panels. Effects render in front of them
    /// (never masked out) and snow accumulates along their top edges.
    pub docks: Vec<Window>,
    pub focused: Option<Rect>,
    pub cursor: Option<[f32; 2]>,
    /// The primary pointer button is held down.
    pub pointer_down: bool,
    /// Counts presses of the primary button, so a click shorter than a frame
    /// is still seen.
    pub presses: u32,
    pub fullscreen: bool,
    /// Windows cover the whole desktop and effects are kept off windows,
    /// so there is nothing to draw.
    pub desktop_hidden: bool,
    pub idle_ms: Option<u64>,
    pub locked: bool,
    pub on_battery: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Battery,
    #[default]
    Balanced,
    Best,
}

impl Quality {
    pub fn render_scale(self) -> f32 {
        match self {
            Quality::Battery | Quality::Balanced => 0.5,
            Quality::Best => 1.0,
        }
    }

    pub fn particle_scale(self) -> f32 {
        match self {
            Quality::Battery => 0.5,
            Quality::Balanced => 0.8,
            Quality::Best => 1.0,
        }
    }
}

/// Where effects may appear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// On the desktop and along the tops of windows, never over what is
    /// inside a window. Settings written before this existed said
    /// `over_windows` (the old default) or `keep_windows_clear`, and both
    /// now mean this.
    #[default]
    #[serde(alias = "over_windows", alias = "keep_windows_clear")]
    Desktop,
    /// Across everything, windows included. Clicks still pass through.
    Everywhere,
    /// Under every window, drawn by the compositor directly above the wallpaper.
    BehindWindows,
}

impl Placement {
    /// Effects are hidden over the inside of windows.
    pub fn keeps_windows_clear(self) -> bool {
        self == Placement::Desktop
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryMode {
    /// Half the particles at a lower frame rate.
    #[default]
    Reduce,
    Pause,
    Ignore,
}

/// Options that shape how the current aura behaves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tuning {
    pub speed: f32,
    pub size: f32,
    /// `None` uses the aura's own wind.
    pub wind: Option<f32>,
    pub pointer: bool,
    pub windows: bool,
    pub piles: bool,
    pub pile_depth: f32,
    pub lightning: bool,
    pub placement: Placement,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            speed: 1.0,
            size: 1.0,
            wind: None,
            pointer: true,
            windows: true,
            piles: true,
            pile_depth: 0.5,
            lightning: true,
            placement: Placement::Desktop,
        }
    }
}

fn bounded(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

impl Tuning {
    pub fn sanitized(mut self) -> Self {
        self.wind = self
            .wind
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(-1.0, 1.0));
        self.speed = bounded(self.speed, 0.25, 2.5, 1.0);
        self.size = bounded(self.size, 0.5, 2.0, 1.0);
        self.pile_depth = bounded(self.pile_depth, 0.0, 1.0, 0.5);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TreeDecoration {
    /// Position across the display, 0 to 1.
    pub x: f32,
    pub size: f32,
    pub baubles: bool,
    pub lights: bool,
    pub candy: bool,
    pub star: bool,
}

impl Default for TreeDecoration {
    fn default() -> Self {
        Self {
            x: 0.12,
            size: 1.0,
            baubles: true,
            lights: true,
            candy: true,
            star: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnowmanDecoration {
    pub x: f32,
    pub size: f32,
    pub hat: bool,
    pub scarf: bool,
}

impl Default for SnowmanDecoration {
    fn default() -> Self {
        Self {
            x: 0.88,
            size: 1.0,
            hat: true,
            scarf: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Off,
    #[default]
    Sometimes,
    Often,
}

pub const MAX_TREES: usize = 3;
pub const MAX_SNOWMEN: usize = 3;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Decorations {
    pub trees: Vec<TreeDecoration>,
    pub snowmen: Vec<SnowmanDecoration>,
    pub sleigh: Frequency,
    pub plough: Frequency,
    /// Small weather visitors: in the snow a penguin, a fox or a snowy owl
    /// taking turns; otherwise a frog, butterflies, a whirlwind, a comet,
    /// a heart of fireflies, a lantern or a paper plane.
    pub visitors: Frequency,
}

impl Decorations {
    pub fn sanitized(mut self) -> Self {
        self.trees.truncate(MAX_TREES);
        // Static bottom-corner snowman decorations were removed so they never
        // overlap the bottom snow drift; keep the field only for settings compatibility.
        self.snowmen.clear();
        for t in &mut self.trees {
            t.x = bounded(t.x, 0.0, 1.0, 0.12);
            t.size = bounded(t.size, 0.5, 1.6, 1.0);
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Sleigh,
    Plough,
    ShakeOff,
    /// Sends the visitor that belongs to the current aura.
    Visitor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    Active,
    Still,
    Paused,
}

impl PowerState {
    /// Target frame rate. `None` means no frames are scheduled at all.
    pub fn fps(self, quality: Quality) -> Option<u32> {
        match (self, quality) {
            (PowerState::Paused, _) => None,
            (PowerState::Active, Quality::Battery) => Some(20),
            (PowerState::Active, Quality::Balanced) => Some(30),
            (PowerState::Active, Quality::Best) => Some(60),
            (PowerState::Still, Quality::Battery) => Some(12),
            (PowerState::Still, Quality::Balanced) => Some(20),
            (PowerState::Still, Quality::Best) => Some(30),
        }
    }
}

/// Above this many logical pixels, Balanced quality lowers its frame rate.
const LARGE_DISPLAY_PIXELS: u64 = 2560 * 1600;

impl PowerState {
    /// The frame rate for a display of this size. The compositor redraws the
    /// whole overlay every frame, which is its largest cost on very big displays.
    pub fn fps_for_display(self, quality: Quality, (w, h): (u32, u32)) -> Option<u32> {
        let fps = self.fps(quality)?;
        let large = u64::from(w) * u64::from(h) > LARGE_DISPLAY_PIXELS;
        Some(if large && quality == Quality::Balanced {
            fps.min(24)
        } else {
            fps
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub stillness: bool,
    pub paused: bool,
    pub battery: BatteryMode,
    pub quality: Quality,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            stillness: true,
            paused: false,
            battery: BatteryMode::Reduce,
            quality: Quality::Balanced,
        }
    }
}

impl Settings {
    /// The quality actually used right now, after the battery rule.
    pub fn effective_quality(&self, desktop: &DesktopModel) -> Quality {
        if desktop.on_battery && self.battery == BatteryMode::Reduce {
            Quality::Battery
        } else {
            self.quality
        }
    }
}

/// Input newer than this counts as "working".
const WORKING_IDLE_MS: u64 = 1_500;
/// Input older than this counts as "paused to look".
const RESTING_IDLE_MS: u64 = 4_000;
const STILL_PRESENCE: f32 = 0.22;
const FADE_OUT_SECONDS: f32 = 0.9;
const FADE_IN_SECONDS: f32 = 3.5;

/// Smoothly moves effect presence between "working" and "resting".
#[derive(Clone, Debug)]
pub struct Stillness {
    presence: f32,
}

impl Default for Stillness {
    fn default() -> Self {
        Self { presence: 1.0 }
    }
}

impl Stillness {
    pub fn presence(&self) -> f32 {
        self.presence
    }

    pub fn update(&mut self, dt: f32, enabled: bool, idle_ms: Option<u64>) -> f32 {
        let target = match (enabled, idle_ms) {
            (false, _) | (true, None) => 1.0,
            (true, Some(ms)) if ms <= WORKING_IDLE_MS => STILL_PRESENCE,
            (true, Some(ms)) if ms >= RESTING_IDLE_MS => 1.0,
            // Between the thresholds, hold the current level to avoid flicker.
            (true, Some(_)) => self.presence,
        };
        let seconds = if target < self.presence {
            FADE_OUT_SECONDS
        } else {
            FADE_IN_SECONDS
        };
        let step = (1.0 - STILL_PRESENCE) * dt.max(0.0) / seconds;
        self.presence = if target < self.presence {
            (self.presence - step).max(target)
        } else {
            (self.presence + step).min(target)
        };
        self.presence
    }
}

pub fn power_state(settings: &Settings, desktop: &DesktopModel, presence: f32) -> PowerState {
    if settings.paused
        || desktop.fullscreen
        || desktop.desktop_hidden
        || desktop.locked
        || (settings.battery == BatteryMode::Pause && desktop.on_battery)
    {
        PowerState::Paused
    } else if presence < 0.6 {
        PowerState::Still
    } else {
        PowerState::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_fades_and_resting_returns() {
        let mut s = Stillness::default();
        for _ in 0..60 {
            s.update(0.05, true, Some(100));
        }
        assert!((s.presence() - STILL_PRESENCE).abs() < 1e-3);
        for _ in 0..100 {
            s.update(0.05, true, Some(10_000));
        }
        assert!((s.presence() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn middle_idle_band_holds_level() {
        let mut s = Stillness::default();
        for _ in 0..8 {
            s.update(0.05, true, Some(100));
        }
        let held = s.presence();
        s.update(0.05, true, Some(2_500));
        assert_eq!(s.presence(), held);
    }

    #[test]
    fn missing_idle_signal_or_disabled_keeps_full_presence() {
        let mut s = Stillness::default();
        assert_eq!(s.update(1.0, true, None), 1.0);
        assert_eq!(s.update(1.0, false, Some(0)), 1.0);
    }

    #[test]
    fn fullscreen_lock_and_battery_rule_pause() {
        let settings = Settings {
            battery: BatteryMode::Pause,
            ..Settings::default()
        };
        for desktop in [
            DesktopModel {
                fullscreen: true,
                ..Default::default()
            },
            DesktopModel {
                locked: true,
                ..Default::default()
            },
            DesktopModel {
                on_battery: true,
                ..Default::default()
            },
        ] {
            assert_eq!(power_state(&settings, &desktop, 1.0), PowerState::Paused);
        }
        assert_eq!(PowerState::Paused.fps(Quality::Best), None);
        let active = PowerState::Active;
        assert_eq!(
            active.fps_for_display(Quality::Balanced, (1920, 1080)),
            Some(30)
        );
        assert_eq!(
            active.fps_for_display(Quality::Balanced, (3840, 2160)),
            Some(24)
        );
        assert_eq!(
            active.fps_for_display(Quality::Best, (3840, 2160)),
            Some(60)
        );
        assert_eq!(
            PowerState::Paused.fps_for_display(Quality::Balanced, (3840, 2160)),
            None
        );
    }

    #[test]
    fn hidden_desktop_pauses_and_needs_full_cover() {
        let window = |x: f32, y: f32, w: f32, h: f32| Window { id: 1, x, y, w, h };
        assert!(desktop_hidden(
            &[window(0.0, 0.0, 1920.0, 1040.0)],
            1920.0,
            1040.0
        ));
        // Two tiled halves with a hairline gap still hide everything.
        let halves = [
            window(0.0, 0.0, 958.0, 1040.0),
            window(962.0, 0.0, 958.0, 1040.0),
        ];
        assert!(desktop_hidden(&halves, 1920.0, 1040.0));
        assert!(!desktop_hidden(
            &[window(100.0, 100.0, 900.0, 700.0)],
            1920.0,
            1040.0
        ));
        assert!(!desktop_hidden(&[], 1920.0, 1040.0));
        let desktop = DesktopModel {
            desktop_hidden: true,
            ..Default::default()
        };
        assert_eq!(
            power_state(&Settings::default(), &desktop, 1.0),
            PowerState::Paused
        );
        let r = Rect {
            x: 0.0,
            y: 0.0,
            w: 1900.0,
            h: 1000.0,
        };
        assert!(r.is_large(1920.0, 1040.0));
        assert!(!Rect { w: 1200.0, ..r }.is_large(1920.0, 1040.0));
    }

    #[test]
    fn old_placements_mean_desktop() {
        for (text, placement) in [
            ("\"over_windows\"", Placement::Desktop),
            ("\"keep_windows_clear\"", Placement::Desktop),
            ("\"desktop\"", Placement::Desktop),
            ("\"everywhere\"", Placement::Everywhere),
            ("\"behind_windows\"", Placement::BehindWindows),
        ] {
            assert_eq!(
                serde_json::from_str::<Placement>(text).unwrap(),
                placement,
                "{text}"
            );
        }
        assert_eq!(
            serde_json::to_string(&Placement::Desktop).unwrap(),
            "\"desktop\""
        );
    }

    #[test]
    fn battery_reduces_quality_by_default() {
        let settings = Settings {
            quality: Quality::Best,
            ..Settings::default()
        };
        let unplugged = DesktopModel {
            on_battery: true,
            ..Default::default()
        };
        assert_eq!(settings.effective_quality(&unplugged), Quality::Battery);
        assert_eq!(
            settings.effective_quality(&DesktopModel::default()),
            Quality::Best
        );
        assert_eq!(power_state(&settings, &unplugged, 1.0), PowerState::Active);
    }

    #[test]
    fn low_presence_is_still_and_slower() {
        let state = power_state(&Settings::default(), &DesktopModel::default(), 0.3);
        assert_eq!(state, PowerState::Still);
        assert!(state.fps(Quality::Balanced) < PowerState::Active.fps(Quality::Balanced));
    }

    #[test]
    fn options_are_limited_to_safe_ranges() {
        let t = Tuning {
            speed: f32::NAN,
            size: 99.0,
            wind: Some(-4.0),
            pile_depth: -1.0,
            ..Tuning::default()
        }
        .sanitized();
        assert_eq!(
            (t.speed, t.size, t.wind, t.pile_depth),
            (1.0, 2.0, Some(-1.0), 0.0)
        );
        let d = Decorations {
            trees: vec![TreeDecoration::default(); 9],
            ..Default::default()
        };
        assert_eq!(d.sanitized().trees.len(), MAX_TREES);
    }
}
