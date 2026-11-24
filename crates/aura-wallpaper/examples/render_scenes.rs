//! Writes every scene at every time of day to a folder, for checking by eye.
//! Usage: cargo run --release -p aura-wallpaper --example render_scenes -- OUT [WIDTH HEIGHT]

use aura_wallpaper::{Moment, Theme, render_scene};
use std::{fs::File, io::BufWriter, path::PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output folder"));
    let w: u32 = args.next().map_or(960, |v| v.parse().unwrap());
    let h: u32 = args.next().map_or(540, |v| v.parse().unwrap());
    let only = std::env::var("THEME").ok();
    std::fs::create_dir_all(&out).unwrap();
    for theme in Theme::ALL {
        if only.as_deref().is_some_and(|t| t != theme.id()) {
            continue;
        }
        for moment in Moment::ALL {
            let start = std::time::Instant::now();
            let rgb = render_scene(theme, moment, w, h);
            let file = out.join(format!("{}-{}.png", theme.id(), moment.id()));
            let mut enc = png::Encoder::new(BufWriter::new(File::create(&file).unwrap()), w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&rgb).unwrap();
            eprintln!("{} {:?}", file.display(), start.elapsed());
        }
    }
}
