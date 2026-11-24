use super::*;
use aura_core::{Placement, SnowmanDecoration, TreeDecoration, Window};
use aura_format::{built_in, built_in_by_id};

fn input(desktop: &DesktopModel) -> FrameInput<'_> {
    FrameInput {
        dt: 1.0 / 30.0,
        presence: 1.0,
        quality_scale: 1.0,
        desktop,
    }
}

fn window(id: u64, x: f32, y: f32, w: f32, h: f32) -> Window {
    Window { id, x, y, w, h }
}

fn full(id: &str) -> Aura {
    let mut aura = built_in_by_id(id).unwrap();
    aura.intensity = 1.0;
    aura
}

fn run(sim: &mut Simulation, desktop: &DesktopModel, frames: usize) {
    for _ in 0..frames {
        sim.step(input(desktop));
    }
}

fn drawn(sim: &Simulation) -> Vec<Instance> {
    let mut out = Vec::new();
    sim.write_instances(&mut out);
    out
}

fn count_shape(items: &[Instance], shape: f32) -> usize {
    items.iter().filter(|i| i.shape == shape).count()
}

#[test]
fn every_aura_stays_bounded_and_finite_for_ten_minutes() {
    let d = DesktopModel {
        windows: vec![window(1, 100.0, 200.0, 900.0, 500.0)],
        cursor: Some([400.0, 300.0]),
        ..Default::default()
    };
    for aura in built_in() {
        let mut sim = Simulation::new(aura, 7, 1920.0, 1080.0);
        sim.set_decorations(Decorations {
            trees: vec![TreeDecoration::default(); 3],
            snowmen: vec![SnowmanDecoration::default(); 3],
            ..Default::default()
        });
        sim.trigger(Trigger::Sleigh);
        sim.trigger(Trigger::Plough);
        run(&mut sim, &d, 18_000);
        let out = drawn(&sim);
        assert!(out.len() <= MAX_INSTANCES);
        assert!(out.iter().all(|i| {
            bytemuck::bytes_of(i).len() == 52
                && i.center
                    .iter()
                    .chain(&i.half_size)
                    .chain(&i.color)
                    .all(|v| v.is_finite())
                && i.color[0] <= i.color[3] + 1e-4
        }));
    }
}

#[test]
fn same_seed_gives_same_frames() {
    let d = DesktopModel::default();
    let frames = || {
        let mut s = Simulation::new(built_in_by_id("quiet-snow").unwrap(), 42, 1280.0, 720.0);
        run(&mut s, &d, 200);
        drawn(&s)
    };
    assert_eq!(frames(), frames());
}

fn pile_total(s: &Simulation) -> f32 {
    s.piles.list.iter().flat_map(|p| &p.cells).sum()
}

#[test]
fn snow_builds_on_a_window_and_a_drag_throws_it_off() {
    let mut d = DesktopModel {
        windows: vec![window(5, 100.0, 300.0, 900.0, 200.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 3, 1280.0, 720.0);
    run(&mut s, &d, 1200);
    let before = pile_total(&s);
    assert!(before > 200.0, "{before}");
    assert!(count_shape(&drawn(&s), SHAPE_SNOWBANK) > 20);
    d.windows[0].x += 240.0;
    s.step(input(&d));
    assert!(pile_total(&s) < before * 0.1);
    assert!(
        s.debris
            .iter()
            .filter(|d| d.kind == DebrisKind::Flake)
            .count()
            > 30
    );
}

#[test]
fn piled_snow_respects_the_depth_option_and_can_be_turned_off() {
    let d = DesktopModel {
        windows: vec![window(5, 0.0, 300.0, 1280.0, 200.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 3, 1280.0, 720.0);
    s.set_tuning(Tuning {
        pile_depth: 0.1,
        ..Tuning::default()
    });
    run(&mut s, &d, 1500);
    let limit = MAX_PILE_DEPTH * 0.1 + 0.01;
    assert!(s.piles.list[0].cells.iter().all(|c| *c <= limit));
    s.set_tuning(Tuning {
        piles: false,
        ..Tuning::default()
    });
    run(&mut s, &d, 60);
    assert_eq!(count_shape(&drawn(&s), SHAPE_SNOWBANK), 0);
}

#[test]
fn resting_snow_falls_when_a_window_without_an_id_moves() {
    let mut d = DesktopModel {
        windows: vec![window(0, 0.0, 300.0, 1280.0, 200.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("quiet-snow"), 3, 1280.0, 720.0);
    run(&mut s, &d, 300);
    // Flakes held in reserve are neither moved nor drawn, so only count
    // the ones in play.
    let resting = |s: &Simulation| {
        s.particles[..s.active]
            .iter()
            .filter(|p| matches!(p.phase, Phase::Resting(_)) && (p.y - 300.0).abs() < 0.01)
            .count()
    };
    assert!(resting(&s) > 10);
    d.windows[0].y = 500.0;
    s.step(input(&d));
    assert_eq!(resting(&s), 0);
}

#[test]
fn covered_window_tops_do_not_catch_particles() {
    let lower = Rect {
        x: 0.0,
        y: 100.0,
        w: 100.0,
        h: 100.0,
    };
    let upper = Rect {
        x: 0.0,
        y: 50.0,
        w: 100.0,
        h: 200.0,
    };
    assert_eq!(landing(&[lower, upper], 50.0, 90.0, 110.0), None);
    assert_eq!(
        landing(&[upper, lower], 50.0, 90.0, 110.0),
        Some((1, 100.0))
    );
}

#[test]
fn leaves_settle_and_a_sweeping_pointer_scatters_them() {
    let mut d = DesktopModel {
        windows: vec![window(2, 0.0, 300.0, 1280.0, 300.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("autumn-leaves"), 11, 1280.0, 720.0);
    run(&mut s, &d, 900);
    let near = |s: &Simulation| {
        s.particles
            .iter()
            .filter(|p| {
                matches!(p.phase, Phase::Resting(_))
                    && (p.y - 300.0).abs() < 1.0
                    && (p.x - 640.0).abs() < 200.0
            })
            .count()
    };
    let before = near(&s);
    assert!(before > 3, "{before}");
    for i in 0..30 {
        d.cursor = Some([440.0 + i as f32 * 14.0, 296.0]);
        s.step(input(&d));
    }
    assert!(near(&s) < before);
}

#[test]
fn fireflies_gather_at_a_resting_pointer() {
    let d = DesktopModel {
        cursor: Some([640.0, 360.0]),
        ..Default::default()
    };
    let mut s = Simulation::new(full("fireflies"), 4, 1280.0, 720.0);
    let close = |s: &Simulation| {
        s.particles[..s.active]
            .iter()
            .filter(|p| (p.x - 640.0).hypot(p.y - 360.0) < 130.0)
            .count()
    };
    s.step(input(&d));
    let before = close(&s);
    run(&mut s, &d, 900);
    assert!(close(&s) > before + 5, "{before} {}", close(&s));
}

#[test]
fn meteors_break_into_sparks_on_windows() {
    let d = DesktopModel {
        windows: vec![window(1, 0.0, 300.0, 1280.0, 300.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("falling-stars"), 8, 1280.0, 720.0);
    let mut seen = 0;
    for _ in 0..600 {
        s.step(input(&d));
        seen = seen.max(
            s.debris
                .iter()
                .filter(|p| (p.y - 300.0).abs() < 40.0)
                .count(),
        );
    }
    assert!(seen > 4, "{seen}");
}

#[test]
fn lightning_is_rare_and_can_be_turned_off() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("night-storm"), 8, 1280.0, 720.0);
    let (mut flashes, mut was_on) = (0, false);
    for _ in 0..(30 * 120) {
        s.step(input(&d));
        let on = s.storm.age.is_some();
        flashes += usize::from(on && !was_on);
        was_on = on;
    }
    assert!((4..=14).contains(&flashes), "{flashes}");
    let mut quiet = Simulation::new(full("night-storm"), 8, 1280.0, 720.0);
    quiet.set_tuning(Tuning {
        lightning: false,
        ..Tuning::default()
    });
    for _ in 0..(30 * 60) {
        quiet.step(input(&d));
        assert!(quiet.storm.age.is_none());
    }
}

#[test]
fn lightning_bolt_is_strong_and_outlasts_the_sky_flash() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("night-storm"), 8, 1280.0, 720.0);
    while s.storm.age.is_none() {
        s.step(input(&d));
    }
    let strongest = |items: &[Instance]| {
        items
            .iter()
            .filter(|i| i.shape == SHAPE_BEAM)
            .map(|i| i.color[3])
            .fold(0.0, f32::max)
    };
    let sky = |items: &[Instance]| {
        items
            .iter()
            .find(|i| i.shape == SHAPE_PUFF)
            .map(|i| i.color[3])
    };
    let first = drawn(&s);
    assert!(strongest(&first) > 0.85);
    assert!(sky(&first).unwrap() <= 0.1, "the sky flash must stay faint");
    run(&mut s, &d, 5);
    let later = drawn(&s);
    assert!(strongest(&later) > 0.25);
    assert!(sky(&later).unwrap_or(0.0) < strongest(&later));
}

#[test]
fn rain_uses_the_two_tone_drop() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("soft-rain"), 3, 1280.0, 720.0);
    run(&mut s, &d, 10);
    assert!(count_shape(&drawn(&s), SHAPE_RAIN) > 100);
}

#[test]
fn visitors_follow_their_frequency_setting() {
    use aura_core::Frequency::{Off, Sometimes};
    let d = DesktopModel::default();
    let long_step = |d| FrameInput {
        dt: 0.1,
        ..input(d)
    };
    let quiet_snow = |sleigh, plough, seed| {
        let mut s = Simulation::new(full("deep-snow"), seed, 640.0, 360.0);
        s.set_decorations(Decorations {
            sleigh,
            plough,
            ..Default::default()
        });
        s.set_tuning(Tuning {
            pile_depth: 0.1,
            ..Tuning::default()
        });
        s
    };
    let seen_within = |s: &mut Simulation, seconds: usize| {
        (0..seconds * 10).any(|_| {
            s.step(long_step(&d));
            s.scenery_has_visitors()
        })
    };
    assert!(
        seen_within(&mut quiet_snow(Sometimes, Off, 4), 260),
        "sleigh"
    );
    assert!(
        seen_within(&mut quiet_snow(Off, Sometimes, 4), 200),
        "plough"
    );
    assert!(!seen_within(&mut quiet_snow(Off, Off, 4), 400), "both off");
}

#[test]
fn decorations_appear_and_step_aside_for_windows() {
    let mut d = DesktopModel::default();
    let mut s = Simulation::new(full("first-light"), 1, 1280.0, 720.0);
    s.set_decorations(Decorations {
        trees: vec![TreeDecoration {
            x: 0.5,
            ..Default::default()
        }],
        snowmen: vec![SnowmanDecoration {
            x: 0.2,
            ..Default::default()
        }],
        ..Default::default()
    });
    run(&mut s, &d, 30);
    let visible = drawn(&s);
    // Only the tree's 3 tiers and star draw; static snowman decorations are removed.
    assert_eq!(count_shape(&visible, SHAPE_TRIANGLE), 3);
    assert_eq!(count_shape(&visible, SHAPE_SPARKLE), 1);
    d.windows = vec![window(1, 0.0, 0.0, 1280.0, 720.0)];
    run(&mut s, &d, 30);
    let hidden = drawn(&s);
    assert_eq!(
        count_shape(&hidden, SHAPE_TRIANGLE) + count_shape(&hidden, SHAPE_SPARKLE),
        0
    );
}

#[test]
fn sleigh_crosses_and_plough_clears_the_ground() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 640.0, 360.0);
    run(&mut s, &d, 900);
    let deep = s.piles.floor_mean();
    assert!(deep > 2.0, "{deep}");
    s.set_aura(full("deep-snow"));
    s.set_intensity(0.0);
    run(&mut s, &d, 1);
    s.trigger(Trigger::Plough);
    s.trigger(Trigger::Sleigh);
    run(&mut s, &d, 30);
    assert!(s.scenery_has_visitors());
    run(&mut s, &d, 30 * 12);
    assert!(s.piles.floor_mean() < deep * 0.35);
    run(&mut s, &d, 30 * 6);
    assert!(!s.scenery_has_visitors());
}

fn press(d: &mut DesktopModel) {
    d.pointer_down = true;
    d.presses += 1;
}

/// Frames that cover the hold needed to pack a snowball, with some to spare.
const HOLD_FRAMES: usize = (scenery::HOLD_SECONDS * 30.0) as usize + 3;

/// Presses, holds the button still until a ball is packed, and lets go.
fn hold_and_release(s: &mut Simulation, d: &mut DesktopModel) {
    press(d);
    run(s, d, HOLD_FRAMES);
    d.pointer_down = false;
    s.step(input(d));
}

fn deep_snow_scene() -> (Simulation, DesktopModel) {
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_tuning(Tuning {
        pile_depth: 1.0,
        ..Tuning::default()
    });
    let d = DesktopModel::default();
    run(&mut s, &d, 1500);
    assert!(s.piles.floor_mean() > 3.0);
    (s, d)
}

#[test]
fn rolling_three_balls_builds_a_snowman() {
    let (mut s, mut d) = deep_snow_scene();
    for pass in 0..3 {
        // Press in the snow, roll the ball along, then let go.
        let x0 = 380.0 + pass as f32 * 180.0;
        d.cursor = Some([x0, 700.0]);
        run(&mut s, &d, 10);
        press(&mut d);
        run(&mut s, &d, HOLD_FRAMES);
        assert!(s.scenery.holding().is_some());
        for i in 0..24 {
            d.cursor = Some([x0 + i as f32 * 12.0, 700.0]);
            s.step(input(&d));
        }
        d.cursor = Some([640.0, 660.0]);
        run(&mut s, &d, 12);
        d.pointer_down = false;
        run(&mut s, &d, 45);
    }
    assert_eq!(s.built_snowmen(), 1, "stacks: {}", s.scenery_stacks());
}

#[test]
fn moving_the_pointer_brushes_snow_off_without_making_balls() {
    let (mut s, mut d) = deep_snow_scene();
    let before = s.piles.floor_mean();
    for sweep in 0..4 {
        for i in 0..40 {
            let x = 200.0 + i as f32 * 22.0;
            d.cursor = Some([x, 712.0 - sweep as f32 * 4.0]);
            s.step(input(&d));
            assert!(s.scenery.holding().is_none(), "no ball without a press");
        }
    }
    run(&mut s, &d, 60);
    assert_eq!(s.scenery_stacks(), 0);
    assert!(
        s.piles.floor_mean() < before - 1.0,
        "{before} -> {}",
        s.piles.floor_mean()
    );
}

#[test]
fn holding_the_button_on_snow_packs_a_ball_and_three_build_a_snowman() {
    let (mut s, mut d) = deep_snow_scene();
    d.cursor = Some([640.0, 712.0]);
    run(&mut s, &d, 30);
    for _ in 0..3 {
        hold_and_release(&mut s, &mut d);
        run(&mut s, &d, 50);
    }
    assert_eq!(s.built_snowmen(), 1, "stacks: {}", s.scenery_stacks());
}

#[test]
fn quick_clicks_and_wandering_presses_make_no_snowball() {
    let (mut s, mut d) = deep_snow_scene();
    d.cursor = Some([640.0, 712.0]);
    run(&mut s, &d, 30);
    let snow = s.piles.floor_mean();
    // A press and release between two frames, then an ordinary click.
    d.presses += 1;
    s.step(input(&d));
    press(&mut d);
    run(&mut s, &d, 4);
    assert!(
        s.scenery.packing(),
        "packing shows while the button is held"
    );
    d.pointer_down = false;
    run(&mut s, &d, HOLD_FRAMES);
    assert!(!s.scenery.packing());
    // Held long enough, but dragged away before the ball was packed.
    press(&mut d);
    for i in 0..HOLD_FRAMES {
        d.cursor = Some([640.0 + i as f32 * 4.0, 712.0]);
        s.step(input(&d));
    }
    d.pointer_down = false;
    run(&mut s, &d, 60);
    assert!(s.scenery.holding().is_none());
    assert_eq!(s.scenery_stacks(), 0);
    assert!(s.piles.floor_mean() > snow - 1.0, "only brushing took snow");
}

#[test]
fn clicks_inside_windows_leave_snow_alone_and_the_bare_desktop_gives_a_small_ball() {
    let mut s = Simulation::new(full("quiet-snow"), 3, 1280.0, 720.0);
    let mut d = DesktopModel {
        windows: vec![window(1, 300.0, 200.0, 600.0, 400.0)],
        ..Default::default()
    };
    run(&mut s, &d, 900);
    // On the title bar, just inside the window: nothing.
    d.cursor = Some([600.0, 212.0]);
    run(&mut s, &d, 20);
    let snow = pile_total(&s);
    press(&mut d);
    run(&mut s, &d, HOLD_FRAMES);
    assert!(!s.scenery.packing());
    assert!(s.scenery.holding().is_none());
    assert!(pile_total(&s) >= snow - 0.5);
    d.pointer_down = false;
    run(&mut s, &d, 5);
    // On the snow on top of the window: a ball from that snow.
    d.cursor = Some([600.0, 196.0]);
    run(&mut s, &d, 20);
    press(&mut d);
    run(&mut s, &d, HOLD_FRAMES);
    assert!(s.scenery.holding().is_some_and(|r| r > 7.0));
    d.pointer_down = false;
    run(&mut s, &d, 60);
    // Up in the open sky over the bare desktop: a small one packed from the air.
    d.cursor = Some([1100.0, 100.0]);
    run(&mut s, &d, 20);
    press(&mut d);
    run(&mut s, &d, HOLD_FRAMES);
    assert!(s.scenery.holding().is_some());
}

#[test]
fn a_snowman_on_a_window_falls_apart_when_the_window_moves() {
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_intensity(0.0);
    let mut d = DesktopModel {
        windows: vec![window(1, 300.0, 300.0, 600.0, 300.0)],
        ..Default::default()
    };
    run(&mut s, &d, 30);
    d.cursor = Some([600.0, 290.0]);
    run(&mut s, &d, 20);
    for _ in 0..3 {
        hold_and_release(&mut s, &mut d);
        run(&mut s, &d, 50);
    }
    assert_eq!(s.built_snowmen(), 1, "stacks: {}", s.scenery_stacks());
    d.cursor = None;
    d.windows[0].x += 200.0;
    d.windows[0].y += 80.0;
    run(&mut s, &d, 2);
    assert_eq!(s.built_snowmen(), 0);
}

#[test]
fn speed_wind_and_size_options_change_the_motion_and_look() {
    let d = DesktopModel::default();
    let position = |tuning: Tuning| {
        let mut s = Simulation::new(full("quiet-snow"), 2, 1280.0, 720.0);
        s.set_tuning(tuning);
        run(&mut s, &d, 20);
        s.particles[0]
    };
    let normal = position(Tuning::default());
    let fast = position(Tuning {
        speed: 2.0,
        ..Tuning::default()
    });
    let windy = position(Tuning {
        wind: Some(1.0),
        ..Tuning::default()
    });
    assert!(fast.y - normal.y > 1.0);
    assert!(windy.x - normal.x > 1.0);
    let half = |size: f32| {
        let mut s = Simulation::new(full("quiet-snow"), 2, 1280.0, 720.0);
        s.set_tuning(Tuning {
            size,
            ..Tuning::default()
        });
        s.step(input(&d));
        let flake = drawn(&s)
            .into_iter()
            .find(|i| i.shape == SHAPE_SOFT || i.shape == SHAPE_FLAKE)
            .unwrap();
        flake.half_size[0]
    };
    assert!((half(2.0) / half(1.0) - 2.0).abs() < 1e-3);
}

#[test]
fn desktop_placement_masks_windows_and_lets_snow_sit_on_top() {
    let d = DesktopModel {
        windows: vec![
            window(1, 200.0, 300.0, 600.0, 300.0),
            window(2, 700.0, 200.0, 400.0, 300.0),
        ],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 2, 1280.0, 720.0);
    run(&mut s, &d, 60);
    // The default placement keeps every window's inside clear.
    assert_eq!(s.masks().len(), 2);
    assert_eq!(s.masks()[0].rect, [200.0, 300.0, 600.0, 300.0]);
    let items = drawn(&s);
    // Snow lying on the lower window is hidden only by the window above it.
    let caps: Vec<&Instance> = items.iter().filter(|i| i.shape == SHAPE_SNOWBANK).collect();
    assert!(caps.iter().any(|i| i.layer == 1.0 && i.center[0] < 700.0));
    assert!(caps.iter().any(|i| i.layer == 2.0));
    assert!(
        caps.iter().any(|i| i.layer == 0.0),
        "the floor drift is on the desktop"
    );
    // Falling flakes are hidden by every window; only those settling into
    // the snow on a window's top are drawn on that window's layer.
    let reach = s.max_depth() + 6.0;
    for flake in items
        .iter()
        .filter(|i| i.shape == SHAPE_FLAKE && i.layer > 0.0)
    {
        let w = &d.windows[flake.layer as usize - 1];
        let [x, y] = flake.center;
        assert!(
            x >= w.x - 4.0 && x < w.x + w.w + 4.0 && y < w.y + 3.0 && y > w.y - reach,
            "{x} {y}"
        );
    }

    s.set_tuning(Tuning {
        placement: Placement::Everywhere,
        ..Tuning::default()
    });
    run(&mut s, &d, 2);
    assert!(s.masks().is_empty());
    assert!(drawn(&s).iter().all(|i| i.layer == 0.0));
}

#[test]
fn fully_opened_windows_hold_no_snow() {
    let small = window(1, 200.0, 300.0, 600.0, 300.0);
    let big = window(1, 0.0, 0.0, 1280.0, 720.0);
    let mut s = Simulation::new(full("deep-snow"), 5, 1280.0, 720.0);
    let d = DesktopModel {
        windows: vec![small],
        ..Default::default()
    };
    run(&mut s, &d, 200);
    let on_window = |s: &Simulation| {
        drawn(s)
            .iter()
            .filter(|i| i.shape == SHAPE_SNOWBANK && i.layer == 1.0)
            .count()
    };
    assert!(on_window(&s) > 0);
    // Maximizing throws the snow off, and none settles while it stays open.
    let d = DesktopModel {
        windows: vec![big],
        ..Default::default()
    };
    run(&mut s, &d, 200);
    assert_eq!(on_window(&s), 0);
    assert!(s.piles.list.iter().all(|p| p.id != 1));
}

#[test]
fn fade_in_eases_effects_back() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("quiet-snow"), 3, 1280.0, 720.0);
    run(&mut s, &d, 30);
    let alpha = |v: &[Instance]| v.iter().map(|i| i.color[3]).sum::<f32>();
    let before = alpha(&drawn(&s));
    s.fade_in();
    s.step(input(&d));
    assert!(alpha(&drawn(&s)) < before * 0.2);
    run(&mut s, &d, 30);
    assert!(alpha(&drawn(&s)) > before * 0.7);
}

#[test]
fn stillness_reduces_particles_and_clears_focused_window() {
    let focused = Rect {
        x: 0.0,
        y: 0.0,
        w: 1280.0,
        h: 720.0,
    };
    let d = DesktopModel {
        focused: Some(focused),
        ..Default::default()
    };
    let mut s = Simulation::new(built_in_by_id("first-light").unwrap(), 9, 1280.0, 720.0);
    s.step(input(&d));
    let (full_count, a) = (s.active_particles(), drawn(&s));
    let alpha = |v: &[Instance]| v.iter().map(|i| i.color[3]).sum::<f32>();
    s.step(FrameInput {
        presence: 0.22,
        ..input(&d)
    });
    let b = drawn(&s);
    assert!(s.active_particles() < full_count);
    assert!(alpha(&b) < alpha(&a) * 0.15);
}

#[test]
fn frames_after_a_long_stall_are_clamped() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(built_in_by_id("soft-rain").unwrap(), 5, 800.0, 600.0);
    let before = s.particles[0].y;
    s.step(FrameInput {
        dt: 3600.0,
        ..input(&d)
    });
    assert!((s.particles[0].y - before).abs() <= 760.0 * MAX_DT + 1.0);
}

#[test]
fn every_aura_has_a_visitor_that_comes_and_goes() {
    let d = DesktopModel {
        windows: vec![window(1, 200.0, 400.0, 700.0, 300.0)],
        cursor: Some([600.0, 200.0]),
        ..Default::default()
    };
    for aura in built_in() {
        let id = aura.id.clone();
        let mut s = Simulation::new(aura, 3, 1280.0, 720.0);
        s.set_decorations(Decorations {
            visitors: aura_core::Frequency::Off,
            ..Default::default()
        });
        run(&mut s, &d, 30);
        s.trigger(Trigger::Visitor);
        assert!(s.visitors.critter.is_some(), "{id} sent");
        run(&mut s, &d, 30);
        assert!(s.visitors.critter.is_some(), "{id} stays a while");
        let out = drawn(&s);
        assert!(
            out.iter()
                .all(|i| i.center.iter().chain(&i.color).all(|v| v.is_finite()))
        );
        // Every visitor has left within three minutes.
        run(&mut s, &d, 30 * 180);
        assert!(s.visitors.critter.is_none(), "{id} left");
    }
}

#[test]
fn visitors_follow_their_setting() {
    use aura_core::Frequency::{Off, Often};
    let d = DesktopModel::default();
    let long_step = FrameInput {
        dt: 0.1,
        ..input(&d)
    };
    let seen = |visitors, seconds: usize| {
        let mut s = Simulation::new(full("fireflies"), 8, 640.0, 360.0);
        s.set_decorations(Decorations {
            visitors,
            ..Default::default()
        });
        (0..seconds * 10).any(|_| {
            s.step(long_step);
            s.visitors.critter.is_some()
        })
    };
    assert!(seen(Often, 300));
    assert!(!seen(Off, 900));
}

#[test]
fn fireflies_draw_a_heart() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("fireflies"), 2, 1280.0, 720.0);
    run(&mut s, &d, 30);
    s.trigger(Trigger::Visitor);
    run(&mut s, &d, 30 * 4);
    let (hx, hy, _) = s.visitors.heart().expect("heart");
    let size = (720.0 * 0.009_f32).clamp(5.0, 9.0);
    let near = s.particles[..s.active]
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let t = *i as f32 / s.active as f32 * std::f32::consts::TAU;
            let [ox, oy] = heart_point(t);
            (p.x - hx - ox * size).hypot(p.y - hy - oy * size) < 12.0
        })
        .count();
    assert!(near * 10 >= s.active * 9, "{near} of {}", s.active);
}

#[test]
fn whirlwind_lifts_fallen_leaves() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("autumn-leaves"), 4, 1280.0, 720.0);
    s.set_tuning(Tuning {
        wind: Some(0.0),
        ..Tuning::default()
    });
    run(&mut s, &d, 30 * 40);
    let resting = |s: &Simulation| {
        s.particles[..s.active]
            .iter()
            .filter(|p| matches!(p.phase, Phase::Resting(_)))
            .count()
    };
    let before = resting(&s);
    assert!(before > 20, "{before}");
    s.visitors.critter = Some(visitors::Critter::Whirlwind {
        x: -60.0,
        dir: 1.0,
        age: 0.0,
    });
    let mut lifted = false;
    for _ in 0..30 * 20 {
        s.step(input(&d));
        if let Some(wx) = s.visitors.whirl() {
            lifted |= s.particles[..s.active]
                .iter()
                .any(|p| (p.x - wx).abs() < 120.0 && p.y < 720.0 - 120.0 && p.vy < -40.0);
        }
    }
    assert!(lifted);
}

#[test]
fn plough_bulldozes_a_snowman_into_a_burst_of_snow() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 640.0, 360.0);
    s.set_intensity(0.0);
    s.scenery.build_snowman(200.0, 359.0);
    run(&mut s, &d, 1);
    s.trigger(Trigger::Plough);
    let mut pushed = false;
    for _ in 0..30 * 4 {
        s.step(input(&d));
        pushed |= s.scenery.snowman_x(0).is_some_and(|x| x > 230.0);
    }
    assert!(pushed, "pushed along");
    run(&mut s, &d, 30 * 3);
    assert_eq!(s.built_snowmen(), 0, "fell apart");
}

#[test]
fn a_fast_snowball_knocks_the_head_off() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 640.0, 360.0);
    s.set_intensity(0.0);
    s.scenery.build_snowman(320.0, 359.0);
    run(&mut s, &d, 1);
    s.scenery.throw(200.0, 290.0, 700.0, -40.0);
    run(&mut s, &d, 15);
    assert_eq!(s.scenery.headless_count(), 1);
    assert_eq!(s.built_snowmen(), 1, "the body stays");
}

#[test]
fn gusts_come_and_go() {
    let s = Simulation::new(full("autumn-leaves"), 1, 640.0, 360.0);
    let mut sim = s;
    let mut seen = (false, false);
    for _ in 0..30 * 120 {
        sim.time += 1.0 / 30.0;
        let g = sim.gust();
        assert!((0.0..=1.0).contains(&g));
        seen.0 |= g < 0.01;
        seen.1 |= g > 0.6;
    }
    assert!(seen.0 && seen.1);
}

#[test]
fn snow_starts_out_already_lying() {
    let d = DesktopModel {
        windows: vec![window(4, 200.0, 300.0, 800.0, 200.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 5, 1280.0, 720.0);
    let max = MAX_PILE_DEPTH * Tuning::default().pile_depth;
    assert!(
        s.piles.floor_mean() > max * 0.35,
        "{}",
        s.piles.floor_mean()
    );
    assert!(s.piles.floor_mean() < max * PLOUGH_SHARE_FOR_TESTS);
    // Corners drift deeper than the middle.
    let floor = &s.piles.floor.cells;
    assert!(floor[0] > floor[floor.len() / 2]);
    s.step(input(&d));
    let cap = &s.piles.list[0].cells;
    let middle = cap[cap.len() / 2];
    assert!(middle > max * 0.25, "{middle}");
    assert!(cap[0] < middle * 0.3, "the cap rounds off at the ends");
    // A window opened later starts bare.
    run(&mut s, &d, 30 * 5);
    let later = DesktopModel {
        windows: vec![d.windows[0], window(9, 100.0, 100.0, 300.0, 100.0)],
        ..Default::default()
    };
    s.set_intensity(0.0);
    s.step(input(&later));
    assert!(s.piles.list[1].cells.iter().all(|c| *c < 1.0));
}

#[test]
fn starting_snow_follows_the_options() {
    let mut quiet = Simulation::new(full("quiet-snow"), 5, 1280.0, 720.0);
    quiet.set_intensity(0.3);
    quiet.set_aura(full("first-light"));
    assert_eq!(quiet.piles.floor_mean(), 0.0, "no snow outside snow auras");
    let mut s = Simulation::new(full("first-light"), 5, 1280.0, 720.0);
    s.set_tuning(Tuning {
        pile_depth: 0.2,
        ..Tuning::default()
    });
    s.set_aura(full("deep-snow"));
    let limit = MAX_PILE_DEPTH * 0.2 + 0.01;
    assert!(s.piles.floor.cells.iter().all(|c| *c <= limit));
    assert!(s.piles.floor_mean() > 0.0);
    // Lowering the depth option trims snow that is already there.
    s.set_tuning(Tuning {
        pile_depth: 0.05,
        ..Tuning::default()
    });
    assert!(
        s.piles
            .floor
            .cells
            .iter()
            .all(|c| *c <= MAX_PILE_DEPTH * 0.05 + 0.01)
    );
}

#[test]
fn snow_falls_in_layers_and_only_near_flakes_land() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 8, 1920.0, 1080.0);
    run(&mut s, &d, 60);
    let items = drawn(&s);
    let flakes: Vec<&Instance> = items.iter().filter(|i| i.shape == SHAPE_FLAKE).collect();
    // extra[0] is how far out of focus a flake is: 1 for the nearest.
    let sharp = flakes.iter().filter(|i| i.extra[0] < 0.1).count();
    let small = flakes
        .iter()
        .filter(|i| i.extra[0] > 0.1 && i.extra[0] < 1.0 && i.half_size[0] < 4.6)
        .count();
    let big = flakes
        .iter()
        .filter(|i| i.extra[0] >= 1.0 && i.half_size[0] > 9.0)
        .count();
    assert!(sharp > 20, "{sharp} near and sharp");
    assert!(small > 100 && big > 3, "{small} far, {big} out of focus");
    assert!(small > big * 4, "most flakes are far away: {small} {big}");
    // Far flakes are faster to settle into the wind but slower to fall.
    let active = &s.particles[..s.active];
    let speed = |far: bool| {
        let v: Vec<f32> = active
            .iter()
            .filter(|p| (p.depth < 0.2) == far && p.phase == Phase::Moving)
            .map(|p| p.vy)
            .collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    assert!(speed(true) < speed(false) * 0.7);
    let before = s.piles.floor.cells.clone();
    let mut far = s.particles[0];
    far.depth = 0.1;
    far.phase = Phase::Moving;
    far.y = 1079.0;
    far.x = 960.0;
    s.step_snow(&mut far, 0.1, &[]);
    assert_eq!(
        s.piles.floor.cells, before,
        "far flakes fall past the ground"
    );
}

#[test]
fn snow_opens_with_a_flurry_that_eases_off() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 8, 1920.0, 1080.0);
    s.step(input(&d));
    let opening = s.active_particles();
    run(&mut s, &d, 30 * 50);
    let settled = s.active_particles();
    assert!(
        opening as f32 > settled as f32 * 1.25,
        "{opening} {settled}"
    );
    assert!(settled > 400, "{settled}");
}

#[test]
fn wind_varies_across_the_display() {
    let mut s = Simulation::new(full("deep-snow"), 8, 1920.0, 1080.0);
    s.set_tuning(Tuning {
        wind: Some(0.6),
        ..Tuning::default()
    });
    let mut spread = 0.0_f32;
    for _ in 0..30 * 60 {
        s.time += 1.0 / 30.0;
        let here = s.wind_at(100.0, 200.0);
        let there = s.wind_at(1800.0, 900.0);
        assert!(here > 0.0 && there > 0.0, "the wind keeps its direction");
        spread = spread.max((here - there).abs());
    }
    assert!(spread > 0.3, "{spread}");
}

#[test]
fn the_sleigh_and_plough_are_drawn_from_the_blender_sprites() {
    let levels = sprites::atlas_levels().expect("atlas decodes");
    assert_eq!(levels.len() as u32, sprites::MIP_LEVELS);
    for f in sprites::FRAMES {
        let [x, y, w, h] = f.rect;
        assert!(x + w <= sprites::ATLAS_WIDTH && y + h <= sprites::ATLAS_HEIGHT);
        // Every frame has something in it.
        let base = &levels[0];
        let opaque = (y..y + h)
            .flat_map(|r| (x..x + w).map(move |c| (r, c)))
            .filter(|&(r, c)| base.pixels[((r * base.width + c) * 4 + 3) as usize] > 200)
            .count();
        assert!(opaque > 500, "{opaque}");
    }
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_intensity(0.0);
    s.trigger(Trigger::Sleigh);
    s.trigger(Trigger::Plough);
    let sprite_count = |s: &Simulation| {
        drawn(s)
            .iter()
            .filter(|i| i.shape >= sprites::SHAPE_SPRITE)
            .count()
    };
    run(&mut s, &d, 30 * 4);
    // Sleigh and two reindeer, plough body and two wheels.
    assert_eq!(sprite_count(&s), 6);
    let wheel_turn = |s: &Simulation| {
        let wheel = sprites::SHAPE_SPRITE + sprites::WHEEL as f32;
        drawn(s).iter().find(|i| i.shape == wheel).unwrap().rotation
    };
    let turn = wheel_turn(&s);
    run(&mut s, &d, 3);
    assert!(wheel_turn(&s) > turn, "the wheels roll forward");
}

#[test]
fn the_plough_throws_snow_as_it_clears() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_intensity(0.0);
    s.debris.clear();
    s.trigger(Trigger::Plough);
    let mut thrown = 0;
    let mut powder = false;
    for _ in 0..30 * 6 {
        s.step(input(&d));
        thrown = thrown.max(
            s.debris
                .iter()
                .filter(|d| d.kind == DebrisKind::Flake)
                .count(),
        );
        powder |= s.debris.iter().any(|d| d.kind == DebrisKind::Powder);
    }
    assert!(thrown > 15, "{thrown}");
    assert!(powder);
    assert!(
        s.debris.iter().any(|d| d.kind == DebrisKind::Smoke),
        "the exhaust puffs"
    );
}

#[test]
fn falling_snow_swirls_around_the_sleigh() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_tuning(Tuning {
        wind: Some(0.0),
        ..Tuning::default()
    });
    s.trigger(Trigger::Sleigh);
    run(&mut s, &d, 30 * 4);
    let x = s.scenery.sleigh_x().unwrap();
    let (cx, cy) = (x + 70.0, s.sleigh_path(x + 70.0) - 45.0);
    let (mut inside, mut outside) = (Vec::new(), Vec::new());
    for p in &s.particles[..s.active] {
        if p.phase != Phase::Moving {
            continue;
        }
        let d2 = ((p.x - cx) / 230.0).powi(2) + ((p.y - cy) / 70.0).powi(2);
        if d2 < 0.6 {
            inside.push(p.vx);
        } else if d2 > 4.0 {
            outside.push(p.vx.abs());
        }
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    assert!(inside.len() > 5, "{}", inside.len());
    assert!(mean(&inside) > 60.0, "dragged along: {}", mean(&inside));
    assert!(
        mean(&outside) < 30.0,
        "still air elsewhere: {}",
        mean(&outside)
    );
}

#[test]
fn snow_settles_on_trees_and_snowmen() {
    let d = DesktopModel::default();
    let decorated = Decorations {
        trees: vec![TreeDecoration::default()],
        snowmen: vec![SnowmanDecoration::default()],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    s.set_decorations(decorated.clone());
    s.scenery.snow_cover = 0.0;
    run(&mut s, &d, 30);
    let bare = count_shape(&drawn(&s), SHAPE_TRIANGLE);
    run(&mut s, &d, 30 * 90);
    assert!(s.scenery.snow_cover > 0.5);
    assert!(
        count_shape(&drawn(&s), SHAPE_TRIANGLE) > bare,
        "caps on the tiers"
    );
    // None outside snow.
    let mut clear = Simulation::new(full("first-light"), 6, 1280.0, 720.0);
    clear.set_decorations(decorated);
    run(&mut clear, &d, 30);
    assert_eq!(count_shape(&drawn(&clear), SHAPE_TRIANGLE), 3);
}

const PLOUGH_SHARE_FOR_TESTS: f32 = 0.6;

impl Simulation {
    fn scenery_has_visitors(&self) -> bool {
        self.scenery.has_visitors()
    }

    fn built_snowmen(&self) -> usize {
        self.scenery.built_count()
    }

    fn scenery_stacks(&self) -> usize {
        self.scenery.stack_count()
    }
}

fn snow_visitor(s: &Simulation) -> &'static str {
    use crate::visitors::Critter;
    match s.visitors.critter {
        Some(Critter::Penguin(_)) => "penguin",
        Some(Critter::Fox(_)) => "fox",
        Some(Critter::Owl(_)) => "owl",
        Some(_) => "other",
        None => "none",
    }
}

#[test]
fn snow_visitors_take_turns() {
    let mut s = Simulation::new(full("deep-snow"), 3, 1280.0, 720.0);
    let seen: Vec<&str> = (0..4)
        .map(|_| {
            s.trigger(Trigger::Visitor);
            snow_visitor(&s)
        })
        .collect();
    assert_eq!(seen, ["penguin", "fox", "owl", "penguin"]);
    // The penguin is a Blender sprite now, drawn on the desktop.
    let items = drawn(&s);
    let first = sprites::SHAPE_SPRITE + sprites::PENGUIN as f32;
    assert!(items.iter().any(|i| i.shape >= first && i.layer == 0.0));
}

#[test]
fn the_fox_dives_into_the_snow_and_leaves_a_hollow() {
    use crate::visitors::Critter;
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    run(&mut s, &d, 900);
    s.set_intensity(0.0);
    s.visitors.turn = 1;
    s.trigger(Trigger::Visitor);
    assert_eq!(snow_visitor(&s), "fox");
    let mut hollow = None;
    for _ in 0..30 * 40 {
        s.step(input(&d));
        match &s.visitors.critter {
            Some(Critter::Fox(f)) if f.buried() && hollow.is_none() => {
                let x = f.x();
                let around =
                    (s.piles.floor_height(x - 60.0) + s.piles.floor_height(x + 60.0)) * 0.5;
                hollow = Some((s.piles.floor_height(x), around));
            }
            None => break,
            _ => {}
        }
    }
    let (here, around) = hollow.expect("the fox dived");
    assert!(
        here < around - 1.0,
        "a hollow where it went in: {here} against {around}"
    );
    assert!(s.visitors.critter.is_none(), "then it went on its way");
}

#[test]
fn the_owl_brings_a_present_to_a_window_and_flies_off_when_startled() {
    use crate::visitors::Critter;
    let mut d = DesktopModel {
        windows: vec![window(1, 400.0, 300.0, 500.0, 300.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 8, 1280.0, 720.0);
    run(&mut s, &d, 900);
    s.set_intensity(0.0);
    s.visitors.turn = 2;
    s.trigger(Trigger::Visitor);
    let mut landed = None;
    for _ in 0..30 * 10 {
        s.step(input(&d));
        if let Some(Critter::Owl(o)) = &s.visitors.critter
            && o.perched()
        {
            landed = Some(o.at());
            break;
        }
    }
    let [x, y] = landed.expect("the owl landed");
    assert!(
        (400.0..900.0).contains(&x) && y <= 300.0 && y > 240.0,
        "on the window: {x} {y}"
    );
    let beside = (s.piles.depth_on(0, x - 70.0) + s.piles.depth_on(0, x + 70.0)) * 0.5;
    assert!(s.piles.depth_on(0, x) < beside, "landing knocked snow off");
    assert_eq!(s.visitors.gift.as_ref().and_then(|g| g.window()), Some(0));
    // Perched on the window, it is drawn on that window's layer.
    let owl = sprites::SHAPE_SPRITE + sprites::OWL as f32;
    assert!(
        drawn(&s)
            .iter()
            .any(|i| i.shape >= owl && i.shape < owl + 3.0 && i.layer == 1.0)
    );
    // A pointer close by startles it; the present stays behind.
    d.cursor = Some([x, y - 30.0]);
    run(&mut s, &d, 30 * 8);
    assert_eq!(snow_visitor(&s), "none");
    assert!(s.visitors.gift.is_some());
    run(&mut s, &d, 30 * 45);
    assert!(s.visitors.gift.is_none(), "and fades after a while");
}

#[test]
fn landing_snow_settles_into_the_drift_and_windows_spill_their_overhang() {
    let d = DesktopModel {
        windows: vec![window(1, 300.0, 300.0, 400.0, 200.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 9, 1280.0, 720.0);
    run(&mut s, &d, 600);
    assert!(
        s.particles[..s.active]
            .iter()
            .any(|p| matches!(p.phase, Phase::Settling(_)))
    );
    // Snow never heaps up at a window's ends much past what is just inside.
    let pile = s.piles.list.iter().find(|p| p.rect.x == 300.0).unwrap();
    let n = pile.cells.len();
    assert!(
        pile.cells[0] <= pile.cells[3] * 1.5 + 2.0,
        "{:?}",
        &pile.cells[..4]
    );
    assert!(pile.cells[n - 1] <= pile.cells[n - 4] * 1.5 + 2.0);
}

#[test]
fn static_snowman_decorations_are_never_drawn() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("first-light"), 1, 1280.0, 720.0);
    s.set_intensity(0.0);
    s.set_decorations(Decorations {
        snowmen: vec![SnowmanDecoration::default()],
        ..Default::default()
    });
    run(&mut s, &d, 30);
    let items = drawn(&s);
    assert_eq!(count_shape(&items, SHAPE_DISK), 0);
    assert_eq!(count_shape(&items, SHAPE_TRIANGLE), 0);
    assert_eq!(count_shape(&items, SHAPE_RECT), 0);
}

#[test]
fn full_window_spills_snow_over_left_and_right_corners() {
    let d = DesktopModel {
        windows: vec![window(1, 320.0, 260.0, 480.0, 240.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 7, 1280.0, 720.0);
    s.set_tuning(Tuning {
        pile_depth: 0.25,
        wind: Some(0.0),
        ..Tuning::default()
    });
    // Fill the window top to capacity and keep snowing so corners overflow.
    let mut spilled_left = 0usize;
    let mut spilled_right = 0usize;
    for _ in 0..30 * 40 {
        s.step(input(&d));
        for deb in &s.debris {
            if deb.kind == DebrisKind::Clump {
                if deb.x < 320.0 && deb.vx < 0.0 {
                    spilled_left += 1;
                }
                if deb.x > 800.0 && deb.vx > 0.0 {
                    spilled_right += 1;
                }
            }
        }
    }
    assert!(spilled_left > 10, "left corner spill: {spilled_left}");
    assert!(spilled_right > 10, "right corner spill: {spilled_right}");
}

#[test]
fn shaking_a_window_converts_snow_into_visible_falling_clumps_that_land_below() {
    let mut d = DesktopModel {
        windows: vec![window(5, 260.0, 240.0, 620.0, 260.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 4, 1280.0, 720.0);
    run(&mut s, &d, 30 * 10);
    // Pause new snowfall and clear floor so we measure snow arriving from the shaken window.
    s.set_intensity(0.0);
    s.piles.floor.cells.fill(0.0);
    let before_cap = pile_total(&s);
    assert!(before_cap > 800.0, "deep cap on window: {before_cap}");

    // Shake horizontally back and forth across several frames (simulating ~50ms desktop polls).
    let offsets = [55.0, -65.0, 70.0, -55.0, 0.0];
    let mut saw_clumps_on_window_layer = false;
    for dx in offsets {
        d.windows[0].x = 260.0 + dx;
        run(&mut s, &d, 2);
        let items = drawn(&s);
        saw_clumps_on_window_layer |= items
            .iter()
            .any(|i| i.shape == SHAPE_CLUMP && i.layer == 1.0);
    }
    assert!(
        saw_clumps_on_window_layer,
        "falling clumps stay visible over their own window"
    );
    assert!(
        pile_total(&s) < before_cap * 0.25,
        "shaking dislodged the snow cap"
    );
    assert!(
        s.debris
            .iter()
            .any(|deb| deb.kind == DebrisKind::Clump && deb.vx.abs() > 20.0),
        "clumps carry horizontal inertia"
    );

    // Let gravity pull the shaken clumps all the way down onto the bottom drift.
    run(&mut s, &d, 30 * 4);
    assert!(
        s.piles.floor_mean() > 0.5,
        "shaken snow landed on the bottom drift: {}",
        s.piles.floor_mean()
    );
}

#[test]
fn bottom_drift_and_window_caps_grow_deep_over_time() {
    let d = DesktopModel {
        windows: vec![window(1, 300.0, 300.0, 560.0, 220.0)],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 3, 1280.0, 720.0);
    let initial_floor = s.piles.floor_mean();
    let initial_cap = s.piles.depth_on(0, 580.0);
    run(&mut s, &d, 30 * 60);
    let later_floor = s.piles.floor_mean();
    let later_cap = s.piles.depth_on(0, 580.0);
    assert!(
        later_floor > initial_floor + 10.0 && later_floor > 70.0,
        "bottom drift grows deep: {initial_floor} -> {later_floor}"
    );
    assert!(
        later_cap > initial_cap + 5.0 && later_cap > 45.0,
        "window cap grows thick: {initial_cap} -> {later_cap}"
    );
}

#[test]
fn snowplough_pushes_a_wave_and_scatters_clumps_smoothly() {
    let d = DesktopModel::default();
    let mut s = Simulation::new(full("deep-snow"), 6, 1280.0, 720.0);
    run(&mut s, &d, 30 * 20);
    let before_peak = s.piles.floor.cells.iter().fold(0.0_f32, |a, &b| a.max(b));
    s.set_intensity(0.0);
    s.trigger(Trigger::Plough);
    let mut saw_clumps = false;
    let mut saw_wave = false;
    for _ in 0..30 * 6 {
        s.step(input(&d));
        saw_clumps |= s.debris.iter().any(|deb| deb.kind == DebrisKind::Clump);
        let peak = s.piles.floor.cells.iter().fold(0.0_f32, |a, &b| a.max(b));
        saw_wave |= peak > before_peak + 1.0;
    }
    assert!(saw_clumps, "plough flings visible snow clumps");
    assert!(saw_wave, "plough pushes a snow wave ahead of its blade");
}

#[test]
fn dock_accumulates_snow_along_its_top_without_masking_snowflakes() {
    let dock = window(0xD0CC_0001, 340.0, 648.0, 600.0, 64.0);
    let d = DesktopModel {
        docks: vec![dock],
        ..Default::default()
    };
    let mut s = Simulation::new(full("deep-snow"), 5, 1280.0, 720.0);
    run(&mut s, &d, 30 * 20);
    // Docks must NEVER enter window masks, so snowflakes render in front of the dock.
    assert!(s.masks().is_empty());
    // Snow piles directly along the dock's top edge.
    let dock_depth = s.piles.depth_on_dock(0, 640.0);
    assert!(dock_depth > 25.0, "snow cap on dock top: {dock_depth}");
    let items = drawn(&s);
    assert!(
        items
            .iter()
            .any(|i| i.shape == SHAPE_SNOWBANK && i.layer == 0.0 && i.center[1] < 648.0),
        "dock snow cap is rendered on desktop layer above the dock top edge"
    );
}
