# Architecture

LarkAura is split so that GNOME Shell never draws effects itself. The extension only places a
window and tells the renderer what is happening on the desktop; all drawing happens in a separate
process. If the renderer crashes, the Shell keeps running and the extension restarts it.

```
 ┌──────────────────────────┐   settings.json   ┌─────────────────────────────┐
 │ LarkAura Studio (Tauri)  │ ────────────────▶ │ GNOME Shell extension       │
 │ apps/studio              │  (~/.config/      │ platform/linux-gnome        │
 └──────────────────────────┘   larkaura/)      │ • top bar menu              │
                                                │ • watches settings          │
                                                │ • tracks windows & pointer  │
                                                └──────────────┬──────────────┘
                                         newline-delimited     │ spawns, places the
                                         JSON on stdin         │ overlay window, restarts
                                                               ▼ it if it stops
                                                ┌─────────────────────────────┐
                                                │ larkaura-engine             │
                                                │ apps/engine                 │
                                                │ simulation → wgpu (Vulkan)  │
                                                │ → click-through Wayland     │
                                                │   surface                   │
                                                └─────────────────────────────┘
```

## Processes and binaries

| Binary | Source | Role |
|---|---|---|
| `larkaura-engine` | `apps/engine` | Runs the simulation and renders one transparent, click-through overlay. The extension starts one engine per display. Reads commands on stdin and exits when the pipe closes. |
| `larkaura-studio` | `apps/studio` | The settings app (Tauri 2, React, TypeScript, Tailwind). Writes `~/.config/larkaura/settings.json`. |
| `larkaura-wallpaper` | `crates/aura-wallpaper/src/bin` | Small helper the extension runs to switch the wallpaper when "Match the weather" is on. |
| GNOME Shell extension | `platform/linux-gnome/larkaura@larklabs.dev` | Starts an engine for each display, keeps its window on top, on every workspace and out of the window switcher, and sends it window positions, pointer position, idle time, full-screen and battery state. Adds the top bar menu. |

The extension looks for the engine and helper in `$LARKAURA_ENGINE`, then `~/.local/bin`, then
`$PATH`.

## Crates

| Crate | What it holds |
|---|---|
| `crates/aura-core` | Shared types: the desktop model (windows, pointer, docks), quality and battery modes, placement, tuning, decorations, power states and frame rates. |
| `crates/aura-format` | The [aura file format](AURA-FORMAT.md) and the ten built-in auras from `auras/`. |
| `crates/aura-effects` | The simulation: particles, wind, snow piles and drifts, snowballs and snowmen, decorations, visitors, window masks and the sprite atlas. Pure Rust, no GPU. |
| `crates/aura-render` | The wgpu renderer and its WGSL shader (`shader.wgsl`). Draws up to 1,600 instances a frame. |
| `crates/aura-ipc` | The stdin control protocol (`Hello`, `Desktop`, `SetAura`, `SetTuning`, `Trigger`, …). |
| `crates/aura-settings` | Reads and writes the shared settings file, with sanitizing so a hand-edited file cannot break anything. |
| `crates/aura-wallpaper` | Procedural wallpaper scenes drawn from code, time-of-day slideshows and the GNOME background integration. |
| `platform/linux-wayland` | The click-through Wayland surface. Its input region is empty from the first commit, so the compositor never routes input to it. Supports a GNOME-managed toplevel and a wlr-layer-shell mode. |

## Placement modes

- **Desktop** (default): effects show on the desktop and along the tops of smaller windows,
  never over a window's contents. The extension sends up to 32 window rectangles, and the shader
  masks each effect by the windows stacked above it.
- **Behind windows**: the overlay sits directly above the wallpaper, under every window.
- **Everywhere**: effects drift over everything. Clicks still pass through.

## Power use

- Frame rate depends on quality (Battery 20 fps, Balanced 30 fps, Best 60 fps) and drops when
  you are actively working ("Stillness"). Balanced drops to 24 fps on very large displays.
- Nothing is drawn when the effects are paused, the screen is locked, a full-screen app is in
  front, or windows cover the whole desktop.
- On battery the default is to halve the particles and lower the frame rate; it can also pause
  or change nothing.

## Files written

| Path | Written by | Contents |
|---|---|---|
| `~/.config/larkaura/settings.json` | Studio, extension menu | All user settings |
| `~/.local/share/larkaura/wallpapers/` | Studio, wallpaper helper | Rendered scenes, imported images and a backup of your previous wallpaper |
| `~/.local/bin/larkaura-*` | `install.sh` | The three binaries |
| `~/.local/share/gnome-shell/extensions/larkaura@larklabs.dev/` | `install.sh` | The extension |
| `~/.local/share/applications/dev.larklabs.LarkAura.Studio.desktop` | `install.sh` | Studio launcher |
