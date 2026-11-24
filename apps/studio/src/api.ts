import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type Quality = "battery" | "balanced" | "best";
export type Placement = "desktop" | "everywhere" | "behind_windows";

/** Settings written before "desktop" existed used these names for it. */
export function placementOf(value: string | undefined): Placement {
  if (value === "everywhere" || value === "behind_windows") return value;
  return "desktop";
}
export type BatteryMode = "reduce" | "pause" | "ignore";
export type Frequency = "off" | "sometimes" | "often";

export interface Tuning {
  speed: number;
  size: number;
  wind: number | null;
  pointer: boolean;
  windows: boolean;
  piles: boolean;
  pile_depth: number;
  lightning: boolean;
  placement: Placement;
}

export interface Tree {
  x: number;
  size: number;
  baubles: boolean;
  lights: boolean;
  candy: boolean;
  star: boolean;
}

export interface Snowman {
  x: number;
  size: number;
  hat: boolean;
  scarf: boolean;
}

export interface Decorations {
  trees: Tree[];
  snowmen: Snowman[];
  sleigh: Frequency;
  plough: Frequency;
  visitors: Frequency;
}

export interface Settings {
  aura: string;
  intensity: number | null;
  quality: Quality;
  stillness: boolean;
  paused: boolean;
  battery: BatteryMode;
  tuning: Tuning;
  decorations: Decorations;
  displays_off: number[];
  wallpaper_follows_aura: boolean;
}

export interface AuraInfo {
  id: string;
  name: string;
  effect: "motes" | "snow" | "rain" | "fireflies" | "fog" | "leaves" | "blossom" | "starfall";
  color: [number, number, number];
  intensity: number;
}

export interface MomentPreview {
  id: string;
  name: string;
  preview: string;
}

export interface Wallpaper {
  id: string;
  name: string;
  path: string;
  preview: string;
  moments: MomentPreview[];
  dynamic: boolean;
  imported: boolean;
}

export interface DesktopStatus {
  gnome: boolean;
  engine_installed: boolean;
  extension: "active" | "disabled" | "needs_login" | "error" | "missing" | "unknown";
}

export interface AppState {
  settings: Settings;
  auras: AuraInfo[];
  wallpapers: Wallpaper[];
  has_wallpaper_backup: boolean;
  desktop: DesktopStatus;
}

export const api = {
  state: () => invoke<AppState>("app_state"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  importWallpaper: (path: string) => invoke<AppState>("import_wallpaper", { path }),
  applyWallpaper: (id: string, moment: string | null) => invoke<AppState>("apply_wallpaper", { id, moment }),
  restoreWallpaper: () => invoke<AppState>("restore_wallpaper"),
  removeWallpaper: (id: string) => invoke<AppState>("remove_wallpaper", { id }),
  enableExtension: () => invoke<AppState>("enable_extension"),
};

export const imageUrl = (path: string) => convertFileSrc(path);
