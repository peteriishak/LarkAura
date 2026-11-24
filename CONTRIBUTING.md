# Contributing to LarkAura

Thanks for your interest in LarkAura. Bug reports, testing on new setups, new auras, wallpaper
scenes and code are all welcome.

## Ways to help

- **Report a bug.** Open an issue with your distribution, GNOME Shell version
  (`gnome-shell --version`), GPU, and what you saw. Logs help a lot; see
  [Getting logs](#getting-logs).
- **Test on a new setup.** LarkAura is only verified on GNOME 50 (Wayland). Reports from
  other GPUs, multi-monitor setups, HiDPI screens and laptops on battery are very useful.
- **Suggest an idea.** Open an issue describing what you'd like to see and why.
- **Send a pull request.** For anything larger than a small fix, please open an issue first so
  we can agree on the approach.

## Development setup

Follow [Build from source](README.md#build-from-source) in the README to install the build
dependencies, then:

```bash
cargo test                                              # unit tests (engine, effects, formats)
cargo clippy --workspace --all-targets -- -D warnings   # lints, including Studio
(cd apps/studio && npm ci --ignore-scripts && npm run build)
cargo build --release -p larkaura-engine -p larkaura-studio -p aura-wallpaper --bins
```

`cargo clippy --workspace` includes Studio, which needs the front end built first
(`apps/studio/dist`) and the WebKitGTK development packages.

The project layout and how the pieces talk to each other are described in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Testing your change

1. **Unit tests and lints** must pass: `cargo test` and the `clippy` command above.
2. **Look at it.** Effects and wallpapers can be checked by eye without a desktop:

   ```bash
   # Frames of an aura, drawn on the CPU over a generated wallpaper
   cargo run --release -p aura-effects --example snapshot -- /tmp/shots quiet-snow 4 8 12
   # Every wallpaper scene at every time of day
   cargo run --release -p aura-wallpaper --example render_scenes -- /tmp/scenes
   ```

3. **GNOME Shell changes** (anything in `platform/linux-gnome`) should be run in the isolated
   harness described in [docs/TESTING.md](docs/TESTING.md). It starts a private, headless
   GNOME Shell and never touches your running desktop.

> **Careful with your live session.** A broken GNOME Shell extension can crash the Shell, and on
> Wayland that logs you out. Test extension changes in the harness or in a nested/virtual
> session before installing them on your desktop.

## Code style

- Rust: `cargo fmt` and `cargo clippy -- -D warnings` clean. Keep `unsafe` out unless there is
  no alternative, and explain it when there is.
- TypeScript (Studio): `npm run typecheck` clean.
- GNOME Shell extension: plain ES modules, following the
  [GNOME extension review guidelines](https://gjs.guide/extensions/review-guidelines/review-guidelines.html).
  The extension must never draw, block or take input; all drawing stays in the engine process.
- Prefer small, focused pull requests with a clear description of what changed and how you
  checked it.

## Hard rules

These keep LarkAura safe to leave running all day:

- **Clicks always pass through.** Nothing LarkAura draws may ever take pointer or keyboard input.
- **The Shell must not be able to crash because of LarkAura.** Rendering lives in a separate
  process that the extension restarts if it fails.
- **Stay light.** Watch CPU and GPU use; effects pause when the screen is covered, in full
  screen, and (by default) slow down on battery.
- **Aura files stay declarative.** No scripts or shaders in `auras/*.toml`.

## Adding an aura

1. Add `auras/<id>.toml` (see [docs/AURA-FORMAT.md](docs/AURA-FORMAT.md)).
2. Register it in `BUILT_IN` in `crates/aura-format/src/lib.rs`.
3. Studio lists it automatically. To show it in the top bar menu too, add it to `AURAS` in
   `platform/linux-gnome/larkaura@larklabs.dev/extension.js`.
4. A new `effect` kind also needs simulation code in `crates/aura-effects` and a preview in
   `apps/studio/src/AuraPreview.tsx`; reusing an existing kind needs neither.
5. Run `cargo test`; the format tests check that every built-in aura parses and that ids are unique.

## Art

Wallpapers are drawn entirely from code in `crates/aura-wallpaper/src/scene/`. The visitor
sprites (sleigh, reindeer, plough, penguin, fox, owl) are modelled in Blender from
`art/visitors/build.py`, which writes `crates/aura-effects/assets/visitors.png` and
`crates/aura-effects/src/sprites/table.rs`:

```bash
blender -b --factory-startup --python art/visitors/build.py -- --samples 128
```

Only contribute art you made yourself or that is under a license compatible with
GPL-3.0-or-later, and say where it came from in your pull request.

## Getting logs

```bash
# Messages from the extension and the engine (the engine's output goes to GNOME Shell's log)
journalctl -b --no-pager | grep -i larkaura | tail -100

# Run the engine by hand to see its own output (Ctrl+C to stop)
~/.local/bin/larkaura-engine --mode layer --aura quiet-snow --stats
```

`--mode layer` uses the wlr-layer-shell protocol and does not work on GNOME; on GNOME the engine
is started by the extension with `--mode gnome`.

## License

By contributing, you agree that your contributions are licensed under the project's
[GPL-3.0-or-later](LICENSE) license.
