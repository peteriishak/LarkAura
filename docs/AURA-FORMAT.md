# Aura file format

An aura is a small, declarative TOML file in [`auras/`](../auras). It picks an effect and sets
its look. Aura files contain no scripts or shaders, so they are safe to share. The parser lives
in [`crates/aura-format`](../crates/aura-format/src/lib.rs).

## Example

```toml
format = 1
id = "autumn-leaves"
name = "Autumn Leaves"
effect = "leaves"
intensity = 0.55
wind = 0.2
color = [0.84, 0.42, 0.14]
opacity = 0.95
palette = [[0.84, 0.42, 0.14], [0.76, 0.26, 0.12], [0.9, 0.62, 0.2], [0.62, 0.3, 0.14]]
```

## Fields

| Field | Required | Type | Default | Meaning |
|---|---|---|---|---|
| `format` | yes | integer | | File format version. Must be `1`. |
| `id` | yes | string | | Unique id: lowercase letters, digits and hyphens, up to 48 characters. |
| `name` | yes | string | | Name shown in Studio and the top bar menu, 1 to 48 characters. |
| `effect` | yes | string | | One of the effect kinds below. |
| `color` | yes | `[r, g, b]` | | Main color, each channel 0 to 1. |
| `intensity` | no | 0 to 1 | `0.5` | How much is on screen. Users can override it with the Amount slider. |
| `wind` | no | -1 to 1 | `0` | Sideways drift. Negative blows left, positive blows right. |
| `opacity` | no | 0 to 1 | `1` | Overall opacity of the effect. |
| `palette` | no | up to 6 `[r, g, b]` | `[]` | Extra colors for effects with varied particles, such as leaves and petals. |
| `lightning` | no | bool | `false` | Occasional lightning bolts and a faint sky flash (rain only). |
| `respond_to_windows` | no | bool | `true` | Particles land on and fall from window tops. |
| `respond_to_cursor` | no | bool | `true` | Particles react to the pointer. |

Unknown fields are rejected, and files larger than 16 KiB are refused.

## Effect kinds

| `effect` | Used by | What it does |
|---|---|---|
| `motes` | First Light | Slow, glowing dust in the light |
| `snow` | Quiet Snow, Deep Snow | Layered flakes, drifts, window caps, snowballs, visitors |
| `rain` | Soft Rain, Night Storm | Streaks, splashes and optional lightning |
| `leaves` | Autumn Leaves | Tumbling leaves that settle and scatter from the pointer |
| `blossom` | Blossom Drift | Drifting petals that settle on windows |
| `starfall` | Falling Stars | Meteors that spark when they hit a window |
| `fireflies` | Fireflies | Wandering lights that gather where the pointer rests |
| `fog` | Low Fog | Slow banks of mist along the bottom of the screen |

## Adding a built-in aura

Aura files are compiled into the engine. To add one, create the file in `auras/`, register it
in `BUILT_IN` in `crates/aura-format/src/lib.rs`, and run `cargo test`. See
[CONTRIBUTING.md](../CONTRIBUTING.md#adding-an-aura) for the remaining steps.
