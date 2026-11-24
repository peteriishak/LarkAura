//! Newline-delimited JSON commands sent to the engine on standard input.
//! The desktop integration owns the pipe, so the engine exits when it closes.

use aura_core::{BatteryMode, Decorations, Quality, Rect, Trigger, Tuning, Window};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Hello {
        version: u32,
    },
    Desktop {
        #[serde(default)]
        windows: Vec<Window>,
        #[serde(default)]
        docks: Vec<Window>,
        #[serde(default)]
        focused: Option<Rect>,
        #[serde(default)]
        cursor: Option<[f32; 2]>,
        /// The primary pointer button is held down.
        #[serde(default)]
        pointer_down: bool,
        #[serde(default)]
        fullscreen: bool,
        #[serde(default)]
        idle_ms: Option<u64>,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        on_battery: bool,
    },
    SetAura {
        id: String,
    },
    SetIntensity {
        value: f32,
    },
    SetStillness {
        enabled: bool,
    },
    SetQuality {
        quality: Quality,
    },
    SetPaused {
        paused: bool,
    },
    SetBattery {
        mode: BatteryMode,
    },
    SetTuning {
        tuning: Tuning,
    },
    SetDecorations {
        decorations: Decorations,
    },
    Trigger {
        event: Trigger,
    },
    // A struct variant, because serde ignores unknown fields on unit variants.
    Quit {},
}

pub fn parse_line(line: &str) -> Result<Command, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_update_with_defaults() {
        let c = parse_line(r#"{"cmd":"desktop","cursor":[4,5],"idle_ms":90,"windows":[{"x":1,"y":2,"w":3,"h":4}]}"#)
            .unwrap();
        let Command::Desktop {
            windows,
            cursor,
            idle_ms,
            fullscreen,
            ..
        } = c
        else {
            panic!()
        };
        assert_eq!(windows[0].id, 0);
        assert_eq!(cursor, Some([4.0, 5.0]));
        assert_eq!(idle_ms, Some(90));
        assert!(!fullscreen);
    }

    #[test]
    fn parses_a_held_pointer_button() {
        let c = parse_line(r#"{"cmd":"desktop","cursor":[4,5],"pointer_down":true}"#).unwrap();
        let Command::Desktop { pointer_down, .. } = c else {
            panic!()
        };
        assert!(pointer_down);
    }

    #[test]
    fn parses_docks_and_defaults_when_omitted() {
        let c =
            parse_line(r#"{"cmd":"desktop","docks":[{"id":99,"x":200,"y":650,"w":880,"h":64}]}"#)
                .unwrap();
        let Command::Desktop { docks, .. } = c else {
            panic!()
        };
        assert_eq!(docks.len(), 1);
        assert_eq!(docks[0].id, 99);
        assert_eq!(docks[0].y, 650.0);
    }

    #[test]
    fn rejects_unknown_commands_and_fields() {
        assert!(parse_line(r#"{"cmd":"format_disk"}"#).is_err());
        assert!(parse_line(r#"{"cmd":"quit","now":true}"#).is_err());
        assert!(parse_line(r#"{"cmd":"set_tuning","tuning":{"shader":"x"}}"#).is_err());
    }

    #[test]
    fn parses_options_and_triggers() {
        assert_eq!(
            parse_line(r#"{"cmd":"set_quality","quality":"battery"}"#).unwrap(),
            Command::SetQuality {
                quality: Quality::Battery
            }
        );
        assert_eq!(
            parse_line(r#"{"cmd":"trigger","event":"shake_off"}"#).unwrap(),
            Command::Trigger {
                event: Trigger::ShakeOff
            }
        );
        let c =
            parse_line(r#"{"cmd":"set_tuning","tuning":{"speed":2,"placement":"behind_windows"}}"#)
                .unwrap();
        let Command::SetTuning { tuning } = c else {
            panic!()
        };
        assert_eq!(tuning.speed, 2.0);
        assert_eq!(tuning.placement, aura_core::Placement::BehindWindows);
        assert!(tuning.piles);
    }
}
