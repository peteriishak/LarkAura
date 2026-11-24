//! Original illustrated scenes, one little world for each kind of weather.
//! Each scene can be drawn at eight times of day: the sky, the light on the
//! land and the lit windows follow the hour.
//!
//! Everything is painted from code: noise gives organic edges and texture,
//! light is layered with glows and rim light, and a fine paper grain finishes
//! the picture. No artwork files are needed and any size looks the same.
//!
//! Every scene hides a lark somewhere, perched or in flight.

#![allow(clippy::too_many_arguments)]

mod blossom;
mod canvas;
mod christmas;
mod coast;
mod glade;
mod harvest;
mod mist;
mod noise;
mod observatory;
mod painter;
mod rain;
mod winter;

use canvas::{C, lerp, mul, rgb};
use painter::Painter;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Moment {
    Dawn,
    Sunrise,
    Morning,
    Midday,
    GoldenHour,
    Sunset,
    Dusk,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Body {
    Sun,
    Moon,
    None,
}

#[derive(Clone, Copy, Debug)]
struct Light {
    top: C,
    horizon: C,
    body: Body,
    body_color: C,
    body_at: (f32, f32),
    body_size: f32,
    /// Colour of daylight falling on the land.
    tint: C,
    /// Colour of light catching edges that face the sun or moon.
    rim: C,
    /// 0 is full daylight, 1 is deep night.
    darkness: f32,
}

impl Moment {
    pub const ALL: [Moment; 8] = [
        Moment::Dawn,
        Moment::Sunrise,
        Moment::Morning,
        Moment::Midday,
        Moment::GoldenHour,
        Moment::Sunset,
        Moment::Dusk,
        Moment::Night,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Moment::Dawn => "dawn",
            Moment::Sunrise => "sunrise",
            Moment::Morning => "morning",
            Moment::Midday => "midday",
            Moment::GoldenHour => "golden-hour",
            Moment::Sunset => "sunset",
            Moment::Dusk => "dusk",
            Moment::Night => "night",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Moment::Dawn => "Dawn",
            Moment::Sunrise => "Sunrise",
            Moment::Morning => "Morning",
            Moment::Midday => "Midday",
            Moment::GoldenHour => "Golden hour",
            Moment::Sunset => "Sunset",
            Moment::Dusk => "Dusk",
            Moment::Night => "Night",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    fn light(self) -> Light {
        let l = |top, horizon, body, body_color, body_at, body_size, tint, rim, darkness| Light {
            top: rgb(top),
            horizon: rgb(horizon),
            body,
            body_color: rgb(body_color),
            body_at,
            body_size,
            tint,
            rim: rgb(rim),
            darkness,
        };
        match self {
            Moment::Dawn => l(
                0x2b355f,
                0xeab3a4,
                Body::Sun,
                0xffe0c8,
                (0.2, 0.5),
                0.034,
                [0.84, 0.8, 0.95],
                0xffc6b0,
                0.4,
            ),
            Moment::Sunrise => l(
                0x557ab8,
                0xffcf9c,
                Body::Sun,
                0xfff0d2,
                (0.24, 0.4),
                0.036,
                [1.02, 0.9, 0.82],
                0xffd29a,
                0.12,
            ),
            Moment::Morning => l(
                0x5f9ddf,
                0xd6ebf7,
                Body::Sun,
                0xfff7e4,
                (0.32, 0.2),
                0.032,
                [1.0, 0.99, 0.96],
                0xfff1d0,
                0.0,
            ),
            Moment::Midday => l(
                0x4a90d8,
                0xcbe4f6,
                Body::Sun,
                0xfffbee,
                (0.64, 0.12),
                0.032,
                [1.02, 1.02, 1.0],
                0xffffff,
                0.0,
            ),
            Moment::GoldenHour => l(
                0x6a8fcb,
                0xffd59c,
                Body::Sun,
                0xffe2a4,
                (0.76, 0.32),
                0.038,
                [1.1, 0.94, 0.76],
                0xffc070,
                0.05,
            ),
            Moment::Sunset => l(
                0x3c4686,
                0xff905c,
                Body::Sun,
                0xffcf9e,
                (0.8, 0.45),
                0.044,
                [1.05, 0.76, 0.68],
                0xff9a5a,
                0.3,
            ),
            Moment::Dusk => l(
                0x1b2350,
                0xa86a88,
                Body::Moon,
                0xf4ecd8,
                (0.82, 0.18),
                0.024,
                [0.82, 0.74, 0.9],
                0xc8b0e0,
                0.64,
            ),
            Moment::Night => l(
                0x0a0f26,
                0x25315c,
                Body::Moon,
                0xf6f0dc,
                (0.78, 0.14),
                0.026,
                [0.8, 0.86, 1.0],
                0x9fb4f0,
                0.9,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Theme {
    Hills,
    Snowline,
    Rainfall,
    Starfield,
    Meadow,
    Amber,
    Blossom,
    Mist,
    Christmas,
}

impl Theme {
    pub const ALL: [Theme; 9] = [
        Theme::Hills,
        Theme::Snowline,
        Theme::Rainfall,
        Theme::Starfield,
        Theme::Meadow,
        Theme::Amber,
        Theme::Blossom,
        Theme::Mist,
        Theme::Christmas,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Theme::Hills => "hills",
            Theme::Snowline => "snowline",
            Theme::Rainfall => "rainfall",
            Theme::Starfield => "starfield",
            Theme::Meadow => "meadow",
            Theme::Amber => "amber",
            Theme::Blossom => "blossom",
            Theme::Mist => "mist",
            Theme::Christmas => "christmas",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Hills => "Lark Cliffs",
            Theme::Snowline => "Winter Hollow",
            Theme::Rainfall => "Rainy Lane",
            Theme::Starfield => "Star Ridge",
            Theme::Meadow => "Firefly Pond",
            Theme::Amber => "Harvest Valley",
            Theme::Blossom => "Blossom Valley",
            Theme::Mist => "Misty Lake",
            Theme::Christmas => "Christmas Eve",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    /// The scene that suits an aura, so the wallpaper can follow the weather.
    /// Known auras are matched by id, others by the words in their id.
    pub fn for_aura(aura: &str) -> Theme {
        let has = |words: &[&str]| words.iter().any(|w| aura.contains(w));
        if has(&["deep-snow", "christmas", "yule", "festive"]) {
            Theme::Christmas
        } else if has(&["snow", "frost", "winter", "blizzard"]) {
            Theme::Snowline
        } else if has(&["rain", "storm", "drizzle", "thunder"]) {
            Theme::Rainfall
        } else if has(&["leaf", "leaves", "autumn", "harvest"]) {
            Theme::Amber
        } else if has(&["blossom", "petal", "sakura", "spring"]) {
            Theme::Blossom
        } else if has(&["star", "meteor", "comet"]) {
            Theme::Starfield
        } else if has(&["firefl", "glow"]) {
            Theme::Meadow
        } else if has(&["fog", "mist", "haze"]) {
            Theme::Mist
        } else {
            Theme::Hills
        }
    }

    /// Each scene keeps its own mood at every hour.
    fn adjust(self, mut l: Light) -> Light {
        let day = 1.0 - l.darkness;
        match self {
            Theme::Rainfall => {
                let grey = lerp(rgb(0x8a949e), rgb(0x161c29), l.darkness);
                l.top = lerp(l.top, lerp(grey, rgb(0x5d6672), 0.3), 0.82);
                l.horizon = lerp(l.horizon, lerp(grey, rgb(0xc3cad1), 0.45), 0.8);
                l.body = Body::None;
                l.tint = lerp(l.tint, [0.8, 0.85, 0.92], 0.65);
                l.rim = lerp(l.rim, rgb(0xc8d2dc), 0.7);
                l.darkness = (l.darkness + 0.16).min(1.0);
            }
            Theme::Starfield => {
                let night = Moment::Night.light();
                l.top = lerp(l.top, rgb(0x080a22), 0.86);
                l.horizon = lerp(l.horizon, rgb(0x3b2f6a), 0.7);
                l.body = Body::Moon;
                l.body_color = night.body_color;
                l.body_at = (0.84, 0.13);
                l.body_size = 0.02;
                l.darkness = l.darkness.max(0.88);
                l.tint = lerp(l.tint, night.tint, 0.85);
                l.rim = lerp(l.rim, rgb(0xa8b8ff), 0.7);
            }
            Theme::Meadow => {
                // An evening that lingers: twilight teal above a glowing pond.
                let evening = lerp(rgb(0x14304a), rgb(0x0b1528), l.darkness);
                l.top = lerp(l.top, evening, 0.7);
                l.horizon = lerp(
                    l.horizon,
                    lerp(rgb(0x6f9a9a), rgb(0x2a4258), l.darkness),
                    0.6,
                );
                if l.body == Body::Sun {
                    l.body = Body::Moon;
                    l.body_color = rgb(0xf6f0dc);
                    l.body_at = (0.3, 0.17);
                    l.body_size = 0.022;
                }
                l.darkness = l.darkness.max(0.62);
                l.tint = lerp(l.tint, [0.62, 0.82, 0.86], 0.6);
                l.rim = lerp(l.rim, rgb(0xb8e0d0), 0.6);
            }
            Theme::Mist => {
                let fog = lerp(rgb(0xd6dcd8), rgb(0x283240), l.darkness);
                l.top = lerp(l.top, fog, 0.5);
                l.horizon = lerp(l.horizon, fog, 0.72);
                l.tint = lerp(l.tint, [0.92, 0.95, 0.95], 0.55);
                l.rim = lerp(l.rim, fog, 0.4);
                l.body_size *= 1.6;
            }
            Theme::Blossom => {
                l.horizon = lerp(l.horizon, rgb(0xf9d8e0), 0.4 * day);
                l.top = lerp(l.top, rgb(0x8fb4e8), 0.25 * day);
                l.tint = lerp(l.tint, [1.04, 0.98, 1.0], 0.5 * day);
            }
            Theme::Amber => {
                l.horizon = lerp(l.horizon, rgb(0xffc77a), 0.45 * day);
                l.top = lerp(l.top, rgb(0x86a8d6), 0.2 * day);
                l.tint = mul(l.tint, [1.07, 0.97, 0.84]);
                l.rim = lerp(l.rim, rgb(0xffb45a), 0.5 * day);
            }
            Theme::Christmas => {
                // Always a Christmas evening: a violet-blue sky over a last
                // rosy glow, and the house lights on. Night deepens it.
                let evening = lerp(rgb(0x2c3a7a), rgb(0x0b1030), l.darkness);
                l.top = lerp(l.top, evening, 0.75);
                let glow = lerp(rgb(0xf2b8a0), rgb(0x3a3468), l.darkness);
                l.horizon = lerp(l.horizon, glow, 0.7);
                l.body = Body::Moon;
                l.body_color = rgb(0xf8f0d8);
                l.body_at = (0.86, 0.15);
                l.body_size = 0.022;
                l.darkness = l.darkness.max(0.6);
                l.tint = lerp(l.tint, [0.78, 0.8, 1.0], 0.6);
                l.rim = lerp(l.rim, rgb(0xffd6a0), 0.5);
            }
            Theme::Snowline => {
                l.top = lerp(l.top, rgb(0x3a4f8a), 0.15);
                l.tint = lerp(l.tint, [0.96, 0.99, 1.05], 0.55);
            }
            Theme::Hills => {
                l.horizon = lerp(l.horizon, rgb(0xfff0d8), 0.15 * day);
            }
        }
        l
    }
}

pub fn render_scene(theme: Theme, moment: Moment, width: u32, height: u32) -> Vec<u8> {
    let mut p = Painter::new(
        width.max(1) as usize,
        height.max(1) as usize,
        theme.adjust(moment.light()),
        theme as u32 + 1,
    );
    match theme {
        Theme::Hills => coast::paint(&mut p),
        Theme::Snowline => winter::paint(&mut p),
        Theme::Rainfall => rain::paint(&mut p),
        Theme::Starfield => observatory::paint(&mut p),
        Theme::Meadow => glade::paint(&mut p),
        Theme::Amber => harvest::paint(&mut p),
        Theme::Blossom => blossom::paint(&mut p),
        Theme::Mist => mist::paint(&mut p),
        Theme::Christmas => christmas::paint(&mut p),
    }
    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(p: &[u8; 3]) -> f32 {
        (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 255.0
    }

    fn mean_top(image: &[u8], w: u32, rows: u32) -> f32 {
        let px = &image[..(w * rows * 3) as usize];
        let pixels = px.as_chunks::<3>().0;
        pixels.iter().map(luminance).sum::<f32>() / pixels.len() as f32
    }

    #[test]
    fn scenes_have_the_right_size_and_are_repeatable() {
        let a = render_scene(Theme::Amber, Moment::Dusk, 64, 36);
        assert_eq!(a.len(), 64 * 36 * 3);
        assert_eq!(a, render_scene(Theme::Amber, Moment::Dusk, 64, 36));
    }

    #[test]
    fn odd_sizes_draw_without_trouble() {
        for theme in Theme::ALL {
            for (w, h) in [(1, 1), (7, 300), (300, 7), (120, 90)] {
                assert_eq!(
                    render_scene(theme, Moment::Night, w, h).len(),
                    (w * h * 3) as usize
                );
            }
        }
    }

    #[test]
    fn every_theme_and_moment_looks_different() {
        let mut seen = Vec::new();
        for theme in Theme::ALL {
            for moment in Moment::ALL {
                let image = render_scene(theme, moment, 96, 54);
                assert!(!seen.contains(&image), "{theme:?} {moment:?}");
                seen.push(image);
            }
        }
    }

    /// Scenes must not be recoloured copies of one layout: the brightness
    /// pattern of each pair of scenes at midday should differ clearly.
    #[test]
    fn scenes_have_their_own_composition() {
        let (w, h) = (48u32, 27u32);
        let shapes: Vec<Vec<f32>> = Theme::ALL
            .iter()
            .map(|t| {
                let img = render_scene(*t, Moment::Midday, w, h);
                let lum: Vec<f32> = img.as_chunks::<3>().0.iter().map(luminance).collect();
                let mean = lum.iter().sum::<f32>() / lum.len() as f32;
                let dev = (lum.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / lum.len() as f32)
                    .sqrt()
                    .max(1e-3);
                lum.iter().map(|v| (v - mean) / dev).collect()
            })
            .collect();
        for i in 0..shapes.len() {
            for j in i + 1..shapes.len() {
                let corr = shapes[i]
                    .iter()
                    .zip(&shapes[j])
                    .map(|(a, b)| a * b)
                    .sum::<f32>()
                    / shapes[i].len() as f32;
                assert!(
                    corr < 0.9,
                    "{:?} and {:?} look alike: {corr}",
                    Theme::ALL[i],
                    Theme::ALL[j]
                );
            }
        }
    }

    #[test]
    fn night_is_darker_than_midday() {
        for theme in Theme::ALL {
            let day = render_scene(theme, Moment::Midday, 160, 90);
            let night = render_scene(theme, Moment::Night, 160, 90);
            assert!(
                mean_top(&night, 160, 30) < mean_top(&day, 160, 30) - 0.05
                    || theme == Theme::Starfield,
                "{theme:?}"
            );
        }
    }

    #[test]
    fn stars_stay_dark_and_rain_has_no_sun() {
        let stars = render_scene(Theme::Starfield, Moment::Midday, 160, 90);
        let day = render_scene(Theme::Hills, Moment::Midday, 160, 90);
        let (dark, light) = (mean_top(&stars, 160, 30), mean_top(&day, 160, 30));
        assert!(dark < 0.4 && dark < light * 0.6, "{dark} {light}");
        let rain = Theme::Rainfall.adjust(Moment::Midday.light());
        assert_eq!(rain.body, Body::None);
        let image = render_scene(Theme::Rainfall, Moment::Midday, 160, 90);
        // An overcast sky: grey, with little colour in it.
        let top = &image[..160 * 20 * 3];
        let saturation = top
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| f32::from(*p.iter().max().unwrap() - *p.iter().min().unwrap()) / 255.0)
            .sum::<f32>()
            / (160.0 * 20.0);
        assert!(saturation < 0.12, "{saturation}");
    }

    #[test]
    fn auras_find_a_matching_scene() {
        let cases = [
            ("quiet-snow", Theme::Snowline),
            ("deep-snow", Theme::Christmas),
            ("christmas-eve", Theme::Christmas),
            ("soft-rain", Theme::Rainfall),
            ("night-storm", Theme::Rainfall),
            ("autumn-leaves", Theme::Amber),
            ("blossom-drift", Theme::Blossom),
            ("falling-stars", Theme::Starfield),
            ("fireflies", Theme::Meadow),
            ("low-fog", Theme::Mist),
            ("first-light", Theme::Hills),
            ("someone-elses-aura", Theme::Hills),
        ];
        for (aura, theme) in cases {
            assert_eq!(Theme::for_aura(aura), theme, "{aura}");
        }
    }

    #[test]
    fn ids_round_trip() {
        assert!(
            Theme::ALL
                .iter()
                .all(|t| Theme::from_id(t.id()) == Some(*t))
        );
        assert!(
            Moment::ALL
                .iter()
                .all(|m| Moment::from_id(m.id()) == Some(*m))
        );
    }
}
