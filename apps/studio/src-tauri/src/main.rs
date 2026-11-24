//! LarkAura Studio. It only edits settings, so closing it ends the process
//! while the effects keep running without it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aura_format::{EffectKind, built_in};
use aura_settings::{UserSettings, load, save, settings_path};
use aura_wallpaper::{Library, Wallpaper};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tauri::Manager;

const EXTENSION: &str = "larkaura@larklabs.dev";

#[derive(Serialize)]
struct AuraInfo {
    id: String,
    name: String,
    effect: &'static str,
    color: [f32; 3],
    intensity: f32,
}

#[derive(Serialize)]
struct DesktopStatus {
    gnome: bool,
    engine_installed: bool,
    extension: &'static str,
}

#[derive(Serialize)]
struct AppState {
    settings: UserSettings,
    auras: Vec<AuraInfo>,
    wallpapers: Vec<Wallpaper>,
    has_wallpaper_backup: bool,
    desktop: DesktopStatus,
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn config_path() -> Result<PathBuf, String> {
    settings_path().ok_or_else(|| "could not find your settings folder".to_string())
}

fn library() -> Result<Library, String> {
    Library::default_root()
        .map(Library::new)
        .ok_or_else(|| "could not find your data folder".to_string())
}

fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|v| v.split(':').any(|d| d.eq_ignore_ascii_case("gnome")))
        && std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland")
}

fn require_gnome() -> Result<(), String> {
    if is_gnome() {
        Ok(())
    } else {
        Err("Setting the wallpaper is only available on GNOME for now.".into())
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn engine_installed() -> bool {
    let configured = std::env::var_os("LARKAURA_ENGINE").map(PathBuf::from);
    let local =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin/larkaura-engine"));
    let on_path = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("larkaura-engine"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    configured
        .iter()
        .chain(&local)
        .chain(&on_path)
        .any(|p| is_executable(p))
}

fn extension_files_exist() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    data.is_some_and(|d| {
        d.join("gnome-shell/extensions")
            .join(EXTENSION)
            .join("metadata.json")
            .is_file()
    })
}

fn extension_status() -> &'static str {
    let Ok(output) = Command::new("gnome-extensions")
        .args(["info", EXTENSION])
        .output()
    else {
        return "unknown";
    };
    if !output.status.success() {
        // GNOME on Wayland only discovers new extensions when a session starts.
        return if extension_files_exist() {
            "needs_login"
        } else {
            "missing"
        };
    }
    let info = String::from_utf8_lossy(&output.stdout);
    let field = |name: &str| {
        info.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .map(|v| v.trim().to_ascii_uppercase())
    };
    match field("State:").as_deref() {
        Some("ACTIVE") | Some("ENABLED") => "active",
        Some("INITIALIZED") if field("Enabled:").as_deref() == Some("YES") => "needs_login",
        Some("INACTIVE") | Some("DISABLED") | Some("INITIALIZED") => "disabled",
        Some("ERROR") | Some("OUT OF DATE") => "error",
        _ => "unknown",
    }
}

fn effect_name(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Motes => "motes",
        EffectKind::Snow => "snow",
        EffectKind::Rain => "rain",
        EffectKind::Fireflies => "fireflies",
        EffectKind::Fog => "fog",
        EffectKind::Leaves => "leaves",
        EffectKind::Blossom => "blossom",
        EffectKind::Starfall => "starfall",
    }
}

fn current_state() -> Result<AppState, String> {
    let library = library()?;
    let auras = built_in()
        .into_iter()
        .map(|a| AuraInfo {
            effect: effect_name(a.effect),
            id: a.id,
            name: a.name,
            color: a.color,
            intensity: a.intensity,
        })
        .collect();
    let gnome = is_gnome();
    Ok(AppState {
        settings: load(&config_path()?),
        auras,
        wallpapers: library.list().map_err(text)?,
        has_wallpaper_backup: library.has_backup(),
        desktop: DesktopStatus {
            gnome,
            engine_installed: engine_installed(),
            extension: if gnome { extension_status() } else { "unknown" },
        },
    })
}

// Generating the wallpapers can take a moment, so state requests run off the UI thread.
#[tauri::command(async)]
fn app_state() -> Result<AppState, String> {
    current_state()
}

#[tauri::command(async)]
fn save_settings(settings: UserSettings) -> Result<UserSettings, String> {
    let settings = settings.sanitized();
    save(&config_path()?, &settings).map_err(text)?;
    Ok(settings)
}

#[tauri::command(async)]
fn import_wallpaper(path: PathBuf) -> Result<AppState, String> {
    library()?.import(&path).map_err(text)?;
    current_state()
}

#[tauri::command(async)]
fn apply_wallpaper(id: String, moment: Option<String>) -> Result<AppState, String> {
    require_gnome()?;
    library()?
        .apply_gnome(&id, moment.as_deref())
        .map_err(text)?;
    current_state()
}

#[tauri::command(async)]
fn restore_wallpaper() -> Result<AppState, String> {
    require_gnome()?;
    library()?.restore_gnome().map_err(text)?;
    current_state()
}

#[tauri::command(async)]
fn remove_wallpaper(id: String) -> Result<AppState, String> {
    library()?.remove_imported(&id).map_err(text)?;
    current_state()
}

#[tauri::command(async)]
fn enable_extension() -> Result<AppState, String> {
    require_gnome()?;
    let output = Command::new("gnome-extensions")
        .args(["enable", EXTENSION])
        .output()
        .map_err(text)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    current_state()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = Library::default_root().ok_or("could not find your data folder")?;
            std::fs::create_dir_all(&root)?;
            app.asset_protocol_scope().allow_directory(&root, true)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state,
            save_settings,
            import_wallpaper,
            apply_wallpaper,
            restore_wallpaper,
            remove_wallpaper,
            enable_extension
        ])
        .run(tauri::generate_context!())
        .expect("LarkAura Studio could not start");
}
