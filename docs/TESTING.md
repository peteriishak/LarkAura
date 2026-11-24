# Testing

## Unit tests and lints

```bash
cargo test
cargo clippy --workspace --all-targets -- -D warnings
(cd apps/studio && npm run typecheck)
```

`cargo test` covers the default workspace members: the engine, effects (simulation, piles,
snowballs, visitors, window masks), the aura format, settings, the IPC protocol, the renderer's
shader (validated with naga) and the wallpaper library.

## Checking effects and wallpapers by eye

These tools draw on the CPU, so they need no desktop or GPU.

```bash
# Frames of an aura at the given seconds, over a generated wallpaper
cargo run --release -p aura-effects --example snapshot -- /tmp/shots quiet-snow 4 8 12

# Every wallpaper scene at every time of day (default 960x540)
cargo run --release -p aura-wallpaper --example render_scenes -- /tmp/scenes 1920 1080
```

`snapshot` reads extra settings from the environment, for example:

| Variable | Example | Meaning |
|---|---|---|
| `THEME`, `MOMENT` | `snowline`, `dusk` | Wallpaper scene and time of day |
| `WINDOW`, `WINDOW2`, `WINDOW3` | `380,260,520,260` | Window rectangles (`x,y,w,h`), stacked in that order |
| `PLACEMENT` | `behind_windows` | `desktop`, `behind_windows` or `everywhere` |
| `SLEIGH_AT`, `PLOUGH_AT` | `2` | Send the sleigh or snowplough at this second |
| `VISITOR_AT` | `2,10` | Send the next visitor at each listed second |
| `SNOWMAN`, `TREE` | `1100`, `0.1` | Add a built snowman (x in pixels) or a decorated tree (x from 0 to 1) |
| `CURSOR`, `HOLDS` | `640,600`, `3,5` | Pointer position, and times to press and hold it (packs a snowball) |
| `INTENSITY`, `CROP` | `1.0`, `0,360,640,360` | Override the amount, crop the output |

The README previews in `docs/media/` were made with these two tools.

## The isolated GNOME Shell harness

`tests/gnome-shell/` starts a private, headless GNOME Shell (its own D-Bus session, config
folders and Wayland socket, with virtual monitors) and drives the real extension, engine and
Studio with a virtual pointer and keyboard. It never touches your running desktop.

It needs GNOME Shell 50, `dbus-run-session`, Python 3 and a GPU with Vulkan. A small
test-only helper extension (`tests/gnome-shell/extension/`) exposes window state, input
injection and screenshots over D-Bus.

```bash
cargo build --release -p larkaura-engine -p larkaura-studio
cd tests/gnome-shell

# One scenario: showcase, storm, perf, product, panel, displays, snowball or launch-soak
SPIKE_PRODUCT=1 SPIKE_DRIVER=engine_check.py SPIKE_SCENARIO=product \
  LARKAURA_ENGINE="$PWD/../../target/release/larkaura-engine" LARKAURA_STATS=1000 \
  ./run.sh /tmp/larkaura-check
```

Each run writes `results.json`, `shell.log` and screenshots into the folder you give it.
`summarize.py /tmp/larkaura-check/results.json` prints a short summary.

Useful variables: `SPIKE_TIMEOUT` (seconds, default 170), `SPIKE_MONITOR` and `SPIKE_MONITOR2`
(virtual display sizes, default `1280x720`), `SPIKE_SETTINGS` (start from a settings file) and
`SPIKE_STUDIO` (path to `larkaura-studio`, for scenarios that open it).

### Full check

`full-check.sh` runs every scenario (showcase, storms, performance at 720p, 1080p and 4K,
five product runs, menu, displays, launch soak and Studio) and takes about 30 minutes. Keep the
machine awake, because suspend ends the test session:

```bash
systemd-inhibit --what=idle:sleep --who=LarkAura-tests --why="Display tests" \
  --mode=block tests/gnome-shell/full-check.sh /tmp/larkaura-full
```

It builds into `target-dev/` so it does not disturb your normal build.

### Before installing on your own desktop

New extension code is loaded only when you log in, and a GNOME Shell crash on Wayland logs you
out. Run at least the `product` and `panel` scenarios before installing a changed extension.
