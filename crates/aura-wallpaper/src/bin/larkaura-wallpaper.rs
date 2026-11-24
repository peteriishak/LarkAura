//! Sets the GNOME wallpaper to the scene that suits an aura. The GNOME
//! extension runs this when the aura changes and "Match the weather" is on.
//!
//!   larkaura-wallpaper follow AURA_ID    use the matching scene through the day
//!   larkaura-wallpaper theme AURA_ID     print the matching scene's id

use aura_wallpaper::{Library, Theme};
use std::process::ExitCode;

fn valid_aura(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, aura) = match args.as_slice() {
        [c, a] if valid_aura(a) => (c.as_str(), a.as_str()),
        _ => {
            eprintln!("usage: larkaura-wallpaper follow|theme AURA_ID");
            return ExitCode::from(2);
        }
    };
    let theme = Theme::for_aura(aura);
    match command {
        "theme" => {
            println!("{}", theme.id());
            ExitCode::SUCCESS
        }
        "follow" => {
            let Some(root) = Library::default_root() else {
                eprintln!("larkaura-wallpaper: no home folder");
                return ExitCode::FAILURE;
            };
            match Library::new(root).apply_gnome(theme.id(), None) {
                Ok(()) => {
                    println!("{}", theme.id());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("larkaura-wallpaper: could not set the wallpaper: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: larkaura-wallpaper follow|theme AURA_ID");
            ExitCode::from(2)
        }
    }
}
