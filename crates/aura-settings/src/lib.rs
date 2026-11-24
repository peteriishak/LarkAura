//! The settings file shared by LarkAura Studio and the desktop integration.

use aura_core::{BatteryMode, Decorations, Quality, Tuning};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    pub aura: String,
    pub intensity: Option<f32>,
    pub quality: Quality,
    pub stillness: bool,
    pub paused: bool,
    pub battery: BatteryMode,
    pub tuning: Tuning,
    pub decorations: Decorations,
    /// Display numbers that should show no effects.
    pub displays_off: Vec<u32>,
    /// Change the wallpaper to a scene that suits each aura.
    pub wallpaper_follows_aura: bool,
    /// Replaced by `battery`. Still read so older settings keep their meaning.
    #[serde(skip_serializing)]
    pub pause_on_battery: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            aura: "first-light".into(),
            intensity: None,
            quality: Quality::Balanced,
            stillness: true,
            paused: false,
            battery: BatteryMode::default(),
            tuning: Tuning::default(),
            decorations: Decorations::default(),
            displays_off: Vec::new(),
            wallpaper_follows_aura: false,
            pause_on_battery: false,
        }
    }
}

impl UserSettings {
    /// Replaces out-of-range values, so a hand-edited file cannot break anything.
    pub fn sanitized(mut self) -> Self {
        let valid_id = !self.aura.is_empty()
            && self.aura.len() <= 48
            && self
                .aura
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !valid_id {
            self.aura = Self::default().aura;
        }
        self.intensity = self
            .intensity
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
        if std::mem::take(&mut self.pause_on_battery) {
            self.battery = BatteryMode::Pause;
        }
        self.tuning = self.tuning.sanitized();
        self.decorations = self.decorations.sanitized();
        self.displays_off.sort_unstable();
        self.displays_off.dedup();
        self.displays_off.truncate(16);
        self
    }
}

pub fn config_dir() -> Option<PathBuf> {
    let absolute = |p: PathBuf| p.is_absolute().then_some(p);
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .and_then(absolute)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .and_then(absolute)
        })
        .map(|p| p.join("larkaura"))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

/// Missing or damaged files give the defaults, and never an error.
pub fn load(path: &Path) -> UserSettings {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UserSettings>(&bytes).ok())
        .unwrap_or_default()
        .sanitized()
}

/// Writes through a temporary file, so readers never see a partial file.
pub fn save(path: &Path, settings: &UserSettings) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("settings path has no folder"))?;
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(".settings-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&settings.clone().sanitized())?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("larkaura-settings-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn saves_and_loads_without_leaving_temporary_files() {
        let path = temp("round");
        let settings = UserSettings {
            aura: "low-fog".into(),
            intensity: Some(0.4),
            paused: true,
            ..Default::default()
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), settings);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn missing_or_damaged_files_give_defaults() {
        let path = temp("damaged");
        assert_eq!(load(&path), UserSettings::default());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{not json").unwrap();
        assert_eq!(load(&path), UserSettings::default());
    }

    #[test]
    fn older_battery_setting_keeps_its_meaning() {
        let path = temp("old");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"aura":"low-fog","pause_on_battery":true}"#).unwrap();
        let s = load(&path);
        assert_eq!(s.battery, BatteryMode::Pause);
        assert_eq!(s.tuning, Tuning::default());
    }

    #[test]
    fn bad_values_are_replaced() {
        let s = UserSettings {
            aura: "../../etc".into(),
            intensity: Some(9.0),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.aura, "first-light");
        assert_eq!(s.intensity, None);
    }
}
