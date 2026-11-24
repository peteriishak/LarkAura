//! LarkAura engine: one transparent, click-through overlay driven by an aura.

use aura_core::{
    DesktopModel, PowerState, Quality, Settings, Stillness, desktop_hidden, power_state,
};
use aura_effects::{FrameInput, Instance, MAX_INSTANCES, Simulation};
use aura_format::{Aura, built_in_by_id};
use aura_ipc::{Command, PROTOCOL_VERSION, parse_line};
use aura_linux_wayland::{Mode, Overlay};
use aura_render::{FrameResult, Renderer};
use std::{
    io::{BufRead, Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    process::ExitCode,
    sync::mpsc,
    time::{Duration, Instant},
};

const USAGE: &str = "usage: larkaura-engine --mode gnome|layer [--aura ID] [--intensity 0-1]
  [--quality battery|balanced|best] [--no-stillness] [--control] [--stats] [--allow-software]";
/// If the compositor stops sending frame callbacks, the surface is hidden.
/// Check back at this slow rate instead of drawing frames nobody can see.
const HIDDEN_RETRY: Duration = Duration::from_secs(1);
const STATS_EVERY: Duration = Duration::from_secs(5);
const DEFAULT_AURA: &str = "first-light";

struct Options {
    mode: Mode,
    aura: Aura,
    intensity: Option<f32>,
    quality: Quality,
    stillness: bool,
    control: bool,
    stats: bool,
    allow_software: bool,
}

fn parse_options() -> Result<Options, String> {
    let mut mode = None;
    let mut aura = DEFAULT_AURA.to_string();
    let mut intensity = None;
    let mut quality = Quality::default();
    let (mut stillness, mut control, mut stats, mut allow_software) = (true, false, false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--mode" => {
                mode = Some(match value()?.as_str() {
                    "gnome" => Mode::GnomeManaged,
                    "layer" => Mode::LayerShell,
                    other => return Err(format!("unknown mode {other}")),
                })
            }
            "--aura" => aura = value()?,
            "--intensity" => {
                let v: f32 = value()?.parse().map_err(|_| "intensity must be a number")?;
                if !(0.0..=1.0).contains(&v) {
                    return Err("intensity must be 0 to 1".into());
                }
                intensity = Some(v);
            }
            "--quality" => {
                quality = match value()?.as_str() {
                    "battery" => Quality::Battery,
                    "balanced" => Quality::Balanced,
                    "best" => Quality::Best,
                    other => return Err(format!("unknown quality {other}")),
                }
            }
            "--no-stillness" => stillness = false,
            "--control" => control = true,
            "--stats" => stats = true,
            "--allow-software" => allow_software = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    // A managed engine must keep running if settings name an aura from a newer
    // version. Direct use reports the mistake instead.
    let selected = match built_in_by_id(&aura) {
        Some(found) => found,
        None if control => {
            eprintln!("larkaura-engine: unknown aura {aura}, using {DEFAULT_AURA}");
            built_in_by_id(DEFAULT_AURA).ok_or("the default aura is missing")?
        }
        None => return Err(format!("unknown aura {aura}")),
    };
    Ok(Options {
        mode: mode.ok_or("--mode is required")?,
        aura: selected,
        intensity,
        quality,
        stillness,
        control,
        stats,
        allow_software,
    })
}

/// Reads commands on a thread and wakes the main loop through a socket, so the
/// main loop can sleep indefinitely while paused.
fn spawn_control() -> std::io::Result<(mpsc::Receiver<Command>, UnixStream)> {
    let (mut wake_tx, wake_rx) = UnixStream::pair()?;
    wake_rx.set_nonblocking(true)?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                match parse_line(&line) {
                    Ok(command) => {
                        if tx.send(command).is_err() {
                            return;
                        }
                        let _ = wake_tx.write_all(&[1]);
                    }
                    Err(e) => eprintln!("larkaura-engine: ignored command: {e}"),
                }
            }
            // The owner closed the pipe, so the engine must not outlive it.
            let _ = tx.send(Command::Quit {});
            let _ = wake_tx.write_all(&[1]);
        })?;
    Ok((rx, wake_rx))
}

struct Engine {
    settings: Settings,
    desktop: DesktopModel,
    /// The overlay's size in logical pixels.
    size: (f32, f32),
    idle_sampled: Instant,
    stillness: Stillness,
    sim: Simulation,
    quit: bool,
}

impl Engine {
    fn apply(&mut self, command: Command) {
        match command {
            Command::Hello { version } => {
                if version != PROTOCOL_VERSION {
                    eprintln!("larkaura-engine: protocol {version} is not supported");
                    self.quit = true;
                }
            }
            Command::Desktop {
                windows,
                docks,
                focused,
                cursor,
                pointer_down,
                fullscreen,
                idle_ms,
                locked,
                on_battery,
            } => {
                let pointer_down = pointer_down && cursor.is_some();
                let mut presses = self.desktop.presses;
                if pointer_down && !self.desktop.pointer_down {
                    presses = presses.wrapping_add(1);
                }
                self.desktop = DesktopModel {
                    windows,
                    docks,
                    focused,
                    cursor,
                    pointer_down,
                    presses,
                    fullscreen,
                    desktop_hidden: false,
                    idle_ms,
                    locked,
                    on_battery,
                };
                self.check_hidden();
                self.idle_sampled = Instant::now();
            }
            // Settings are resent as a group, so an unchanged aura must not restart.
            Command::SetAura { id } if id == self.sim.aura().id => {}
            Command::SetAura { id } => match built_in_by_id(&id) {
                Some(aura) => self.sim.set_aura(aura),
                None => eprintln!("larkaura-engine: unknown aura {id}"),
            },
            Command::SetIntensity { value } => self.sim.set_intensity(value),
            Command::SetStillness { enabled } => self.settings.stillness = enabled,
            Command::SetQuality { quality } => self.settings.quality = quality,
            Command::SetPaused { paused } => self.settings.paused = paused,
            Command::SetBattery { mode } => self.settings.battery = mode,
            Command::SetTuning { tuning } => {
                self.sim.set_tuning(tuning);
                self.check_hidden();
            }
            Command::SetDecorations { decorations } => self.sim.set_decorations(decorations),
            Command::Trigger { event } => self.sim.trigger(event),
            Command::Quit {} => self.quit = true,
        }
    }

    /// With effects kept off windows, windows covering the whole desktop
    /// leave nothing to draw, so the engine rests until one moves.
    fn check_hidden(&mut self) {
        let (w, h) = self.size;
        self.desktop.desktop_hidden =
            self.sim.keeps_windows_clear() && desktop_hidden(&self.desktop.windows, w, h);
    }

    fn idle_ms(&self) -> Option<u64> {
        let age = self.idle_sampled.elapsed().as_millis() as u64;
        self.desktop.idle_ms.map(|ms| ms.saturating_add(age))
    }
}

fn buffer_size(logical: (u32, u32), quality: Quality) -> (u32, u32) {
    let scale = quality.render_scale();
    (
        ((logical.0 as f32 * scale) as u32).max(1),
        ((logical.1 as f32 * scale) as u32).max(1),
    )
}

fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let mut overlay = Overlay::connect(options.mode)?;
    let mut logical = overlay.size();
    let (display, surface) = overlay.raw_handles();
    let mut quality = options.quality;
    let buffer = buffer_size(logical, quality);
    // SAFETY: `overlay` owns both objects and outlives `renderer`, which is
    // declared after it and therefore dropped first.
    let mut renderer = unsafe {
        Renderer::new_wayland(display, surface, buffer.0, buffer.1, options.allow_software)
    }?;
    overlay.set_logical_size(logical.0, logical.1);
    if options.stats {
        eprintln!("larkaura-engine: adapter {}", renderer.adapter_name());
    }

    let mut sim = Simulation::new(
        options.aura,
        0x51ed_270b,
        logical.0 as f32,
        logical.1 as f32,
    );
    if let Some(v) = options.intensity {
        sim.set_intensity(v);
    }
    let mut engine = Engine {
        settings: Settings {
            stillness: options.stillness,
            quality,
            ..Settings::default()
        },
        desktop: DesktopModel::default(),
        size: (logical.0 as f32, logical.1 as f32),
        idle_sampled: Instant::now(),
        stillness: Stillness::default(),
        sim,
        quit: false,
    };
    let control = if options.control {
        Some(spawn_control()?)
    } else {
        None
    };

    let mut instances: Vec<Instance> = Vec::with_capacity(MAX_INSTANCES);
    let mut last_step = Instant::now();
    let mut next_frame = Instant::now();
    let mut awaiting_since: Option<Instant> = None;
    let mut cleared = false;
    let mut was_paused = false;
    let (mut stats_at, mut frames, mut busy) = (Instant::now(), 0u32, Duration::ZERO);
    let mut simulating = Duration::ZERO;
    // LARKAURA_STATS=<milliseconds> turns statistics on for test runs.
    let stats_interval = std::env::var("LARKAURA_STATS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis);
    let stats_every = stats_interval.unwrap_or(STATS_EVERY);
    let show_stats = options.stats || stats_interval.is_some();

    while !engine.quit && !overlay.is_closed() {
        if renderer.has_failed() {
            return Err("the graphics device stopped working".into());
        }
        if let Some((commands, mut wake)) = control.as_ref().map(|(c, w)| (c, w)) {
            let mut sink = [0u8; 64];
            while matches!(wake.read(&mut sink), Ok(n) if n > 0) {}
            while let Ok(command) = commands.try_recv() {
                engine.apply(command);
            }
        }
        if overlay.take_frame_ready() {
            awaiting_since = None;
        }

        let size_changed = overlay.take_resized();
        let wanted_quality = engine.settings.effective_quality(&engine.desktop);
        if size_changed || quality != wanted_quality {
            quality = wanted_quality;
            logical = overlay.size();
            let buffer = buffer_size(logical, quality);
            renderer.resize(buffer.0, buffer.1);
            overlay.set_logical_size(logical.0, logical.1);
            engine.sim.resize(logical.0 as f32, logical.1 as f32);
            engine.size = (logical.0 as f32, logical.1 as f32);
            engine.check_hidden();
            cleared = false;
            awaiting_since = None;
            next_frame = Instant::now();
        }

        let now = Instant::now();
        let presence = engine.stillness.presence();
        let state = power_state(&engine.settings, &engine.desktop, presence);
        let Some(fps) = state.fps_for_display(quality, logical) else {
            // Paused: show nothing, then sleep until something changes.
            was_paused = state == PowerState::Paused;
            if !cleared {
                let result = renderer.render(&[], &[], [logical.0 as f32, logical.1 as f32], || {});
                cleared = result == FrameResult::Presented;
            }
            last_step = now;
            let timeout = (!cleared).then_some(HIDDEN_RETRY);
            overlay.wait(timeout, control.as_ref().map(|(_, w)| w.as_fd()))?;
            continue;
        };
        cleared = false;
        if std::mem::take(&mut was_paused) {
            // Coming back, for example when a maximized window is restored.
            engine.sim.fade_in();
        }
        let period = Duration::from_secs_f32(1.0 / fps as f32);
        let hidden_retry_due = awaiting_since.is_some_and(|t| now - t >= HIDDEN_RETRY);
        if now >= next_frame && (awaiting_since.is_none() || hidden_retry_due) {
            let started = Instant::now();
            let dt = (now - last_step).as_secs_f32();
            last_step = now;
            let idle = engine.idle_ms();
            let presence = engine.stillness.update(dt, engine.settings.stillness, idle);
            engine.sim.step(FrameInput {
                dt,
                presence,
                quality_scale: quality.particle_scale(),
                desktop: &engine.desktop,
            });
            engine.sim.write_instances(&mut instances);
            simulating += started.elapsed();
            let masks = engine.sim.masks();
            let result = renderer.render(
                &instances,
                masks,
                [logical.0 as f32, logical.1 as f32],
                || {
                    overlay.request_frame();
                },
            );
            if result == FrameResult::Presented {
                awaiting_since = Some(now);
                frames += 1;
            }
            busy += started.elapsed();
            next_frame = (next_frame + period).max(now + period / 2);
        }
        if show_stats && stats_at.elapsed() >= stats_every {
            let seconds = stats_at.elapsed().as_secs_f32();
            eprintln!(
                "larkaura-engine: aura={} state={:?} fps={:.1} frame_ms={:.2} instances={} presence={:.2} sim_ms={:.2}",
                engine.sim.aura().id,
                state,
                frames as f32 / seconds,
                busy.as_secs_f32() * 1000.0 / frames.max(1) as f32,
                instances.len(),
                engine.stillness.presence(),
                simulating.as_secs_f32() * 1000.0 / frames.max(1) as f32,
            );
            (stats_at, frames, busy) = (Instant::now(), 0, Duration::ZERO);
            simulating = Duration::ZERO;
        }
        let now = Instant::now();
        let timeout = match awaiting_since {
            Some(t) => (t + HIDDEN_RETRY)
                .max(next_frame)
                .saturating_duration_since(now),
            None => next_frame.saturating_duration_since(now),
        };
        overlay.wait(Some(timeout), control.as_ref().map(|(_, w)| w.as_fd()))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("larkaura-engine: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("larkaura-engine: {e}");
            ExitCode::FAILURE
        }
    }
}
