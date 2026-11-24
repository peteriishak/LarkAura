# Project status

LarkAura is early software (0.1.0). This page lists what has been verified, on what, and
what is known not to work yet. If you try LarkAura on a setup not listed here, a report in the
issue tracker is very welcome, whether it worked or not.

## Verified

Tested on Fedora 44 Workstation, GNOME Shell 50.6 (Wayland), Intel Iris Xe graphics, in the
isolated headless GNOME harness ([TESTING.md](TESTING.md)) with one and two virtual displays.

- Clicks, drags, scrolling, hover and typing all reach the window under the overlay, for all
  ten auras and all three placement modes.
- The overlay stays on top, appears on every workspace, stays out of the window switcher and
  never takes focus.
- Stillness, live settings changes, pause, full-screen pause, crash recovery (the renderer is
  back in about 1 to 1.5 seconds) and clean disable.
- Repeated launches keep drawing (24 of 24 and 72 of 72 in soak runs).
- The top bar menu (aura choice, effects on and off, visitors, opening Studio) and the
  per-display switches.
- Studio pages, and applying, scheduling and restoring generated wallpapers.
- Snowball packing with a held pointer button, and the GNOME background long-press menu staying
  closed while a ball is packed.

## Performance

Measured in the headless harness at 30 fps (Balanced quality, plugged in). CPU and GPU figures
are percentages of one CPU core and of GPU busy time.

| Display | Engine CPU | GNOME Shell CPU | Total GPU | GPU memory |
|---|---|---|---|---|
| 1280×720 | 2.2 to 2.4% | 3.4 to 4.0% | 1.2 to 4.5% | about 40 MiB |
| 1920×1080 | 2.1 to 2.4% | 3.4 to 4.1% | 2.1 to 4.5% | about 45 MiB |
| 3840×2160 (24 fps) | 1.9 to 2.1% | 2.9 to 3.5% | 5.2 to 6.1% | about 68 MiB |

Engine memory is about 55 MiB. When paused, CPU and GPU use drop to zero. Virtual displays are
not real monitors, so numbers on your hardware may differ. About 35 MiB of the GPU memory is a
fixed driver cost seen even at small sizes. These figures predate the most recent visitor
sprites, which add roughly 6 MiB of GPU memory.

## Not yet verified

- A real logged-in desktop session on real monitors, display hot-plug, and real battery
  switching. All checks so far ran in the headless harness.
- Other GPUs (AMD, NVIDIA) and other distributions.
- Studio's **Add image** file dialog (the import logic itself has unit tests).

## Not supported yet

- **GNOME 49 and older, GNOME 51 and newer.** The extension declares GNOME Shell 50 only, and
  the installer refuses other versions.
- **X11 sessions.** LarkAura needs Wayland.
- **KDE Plasma, Hyprland, Sway and other wlroots compositors.** The engine has a
  wlr-layer-shell mode that compiles but has never been run, and there is no installer or
  settings integration for these desktops yet.
- **Windows and macOS.** No platform code.

## Known limits

- The overlay covers the work area, not the top bar.
- Snow collects on windows and the bottom of the screen, not on the Dock or desktop widgets.
- GNOME occasionally logged a non-fatal window stacking warning
  (`meta_window_set_stack_position_no_sync`) at startup in early builds. No check failed and the
  Shell did not crash; it has not appeared in recent runs.
