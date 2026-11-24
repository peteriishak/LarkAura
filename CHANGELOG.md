# Changelog

All notable changes to LarkAura are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

First public release.

### Added

- Ten built-in auras: First Light, Quiet Snow, Deep Snow, Soft Rain, Night Storm, Autumn
  Leaves, Blossom Drift, Falling Stars, Fireflies and Low Fog.
- Click-through overlay for GNOME Shell 50 on Wayland, rendered in a separate process with
  wgpu (Vulkan), with automatic restart if the renderer stops.
- Three placement modes: Desktop (effects stay off window contents), Behind windows and
  Everywhere.
- Snow that drifts, piles up on window tops and the bottom of the screen, falls off moving
  windows and can be packed into snowballs and snowmen with the pointer.
- Decorations (up to three trees) and visitors: a sleigh, a snowplough, and weather visitors
  such as a penguin, fox, owl, frog, butterflies, comet and paper plane.
- Nine illustrated wallpaper scenes drawn from code, each at eight times of day, with an
  option to change the wallpaper to match the weather.
- LarkAura Studio settings app (Atmosphere, Decorations, Wallpaper and System pages) and a
  top bar menu.
- Power saving: Stillness while you work, Battery/Balanced/Best quality, pause in full screen,
  and reduce or pause on battery.
- Per-user installer and uninstaller (`packaging/linux/install.sh`).
- Isolated headless GNOME Shell test harness.

[0.1.0]: https://github.com/peteriishak/LarkAura/releases/tag/v0.1.0
