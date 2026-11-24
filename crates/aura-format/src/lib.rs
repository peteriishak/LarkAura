//! Aura files: small declarative descriptions with no scripts or shaders.

use serde::Deserialize;
use std::fmt;

pub const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 16 * 1024;
const MAX_PALETTE: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Motes,
    Snow,
    Rain,
    Fireflies,
    Fog,
    Leaves,
    Blossom,
    Starfall,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aura {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub effect: EffectKind,
    #[serde(default = "half")]
    pub intensity: f32,
    #[serde(default)]
    pub wind: f32,
    /// Display RGB, 0 to 1.
    pub color: [f32; 3],
    /// Extra colors for effects with varied particles, such as leaves.
    #[serde(default)]
    pub palette: Vec<[f32; 3]>,
    #[serde(default)]
    pub lightning: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "yes")]
    pub respond_to_windows: bool,
    #[serde(default = "yes")]
    pub respond_to_cursor: bool,
}

fn half() -> f32 {
    0.5
}
fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

#[derive(Debug, PartialEq)]
pub enum AuraError {
    TooLarge,
    Parse(String),
    Invalid(&'static str),
}

impl fmt::Display for AuraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuraError::TooLarge => write!(f, "aura file is too large"),
            AuraError::Parse(e) => write!(f, "aura file could not be read: {e}"),
            AuraError::Invalid(what) => write!(f, "aura file is invalid: {what}"),
        }
    }
}

impl std::error::Error for AuraError {}

impl Aura {
    pub fn parse(text: &str) -> Result<Self, AuraError> {
        if text.len() > MAX_FILE_BYTES {
            return Err(AuraError::TooLarge);
        }
        let aura: Aura = toml::from_str(text).map_err(|e| AuraError::Parse(e.to_string()))?;
        aura.validate()?;
        Ok(aura)
    }

    fn validate(&self) -> Result<(), AuraError> {
        if self.format != FORMAT_VERSION {
            return Err(AuraError::Invalid("unsupported format version"));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 48
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            return Err(AuraError::Invalid(
                "id must be lowercase letters, digits or hyphens",
            ));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 48 {
            return Err(AuraError::Invalid("name must be 1 to 48 characters"));
        }
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        if !unit(self.intensity) || !unit(self.opacity) || !self.color.iter().all(|c| unit(*c)) {
            return Err(AuraError::Invalid(
                "intensity, opacity and color must be 0 to 1",
            ));
        }
        if self.palette.len() > MAX_PALETTE || !self.palette.iter().flatten().all(|c| unit(*c)) {
            return Err(AuraError::Invalid(
                "palette must be up to 6 colors from 0 to 1",
            ));
        }
        if !self.wind.is_finite() || !(-1.0..=1.0).contains(&self.wind) {
            return Err(AuraError::Invalid("wind must be -1 to 1"));
        }
        Ok(())
    }
}

const BUILT_IN: [&str; 10] = [
    include_str!("../../../auras/first-light.toml"),
    include_str!("../../../auras/quiet-snow.toml"),
    include_str!("../../../auras/deep-snow.toml"),
    include_str!("../../../auras/soft-rain.toml"),
    include_str!("../../../auras/night-storm.toml"),
    include_str!("../../../auras/autumn-leaves.toml"),
    include_str!("../../../auras/blossom-drift.toml"),
    include_str!("../../../auras/falling-stars.toml"),
    include_str!("../../../auras/fireflies.toml"),
    include_str!("../../../auras/low-fog.toml"),
];

pub fn built_in() -> Vec<Aura> {
    BUILT_IN
        .iter()
        .map(|t| Aura::parse(t).expect("built-in aura is valid"))
        .collect()
}

pub fn built_in_by_id(id: &str) -> Option<Aura> {
    built_in().into_iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"format = 1
id = "test"
name = "Test"
effect = "snow"
color = [1.0, 1.0, 1.0]"#;

    #[test]
    fn all_built_ins_load_with_unique_ids() {
        let auras = built_in();
        assert_eq!(auras.len(), BUILT_IN.len());
        let mut ids: Vec<_> = auras.iter().map(|a| a.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), BUILT_IN.len());
    }

    #[test]
    fn valid_file_uses_defaults() {
        let a = Aura::parse(VALID).unwrap();
        assert_eq!(a.intensity, 0.5);
        assert!(a.respond_to_windows);
    }

    #[test]
    fn rejects_unknown_keys_so_files_cannot_smuggle_shaders() {
        assert!(matches!(
            Aura::parse(&format!("{VALID}\nshader = \"x\"")),
            Err(AuraError::Parse(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_and_bad_ids() {
        let bad = VALID.replace("[1.0, 1.0, 1.0]", "[2.0, 1.0, 1.0]");
        assert!(matches!(Aura::parse(&bad), Err(AuraError::Invalid(_))));
        let bad = VALID.replace("\"test\"", "\"../evil\"");
        assert!(matches!(Aura::parse(&bad), Err(AuraError::Invalid(_))));
    }

    #[test]
    fn rejects_oversized_files() {
        assert_eq!(
            Aura::parse(&" ".repeat(MAX_FILE_BYTES + 1)),
            Err(AuraError::TooLarge)
        );
    }
}
