#!/usr/bin/env bash
# Installs LarkAura for the current user on GNOME 50 (Wayland).
# Nothing is written outside your home folder.
#   packaging/linux/install.sh            build and install
#   packaging/linux/install.sh --remove   remove LarkAura, keeping your settings
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UUID="larkaura@larklabs.dev"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN="$HOME/.local/bin"
EXT="$DATA/gnome-shell/extensions/$UUID"
DESKTOP="$DATA/applications/dev.larklabs.LarkAura.Studio.desktop"
ICON="$DATA/icons/hicolor/512x512/apps/dev.larklabs.LarkAura.Studio.png"

# GNOME cannot enable an extension it has not discovered yet, and it discovers
# them only at login, so the list is updated directly for the next session.
enable_at_login() {
  local current
  current="$(gsettings get org.gnome.shell enabled-extensions)"
  case "$current" in
    *"'$UUID'"*) ;;
    "@as []" | "[]") gsettings set org.gnome.shell enabled-extensions "['$UUID']" ;;
    *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$UUID']" ;;
  esac
  gsettings set org.gnome.shell disable-user-extensions false
}

forget_at_login() {
  local list
  list="$(gsettings get org.gnome.shell enabled-extensions)"
  list="${list//, \'$UUID\'/}"
  list="${list//\'$UUID\', /}"
  list="${list//\'$UUID\'/}"
  gsettings set org.gnome.shell enabled-extensions "$list"
}

if [ "${1:-}" = "--remove" ]; then
  gnome-extensions disable "$UUID" 2>/dev/null || true
  forget_at_login
  rm -rf "$EXT"
  rm -f "$BIN/larkaura-engine" "$BIN/larkaura-studio" "$BIN/larkaura-wallpaper" "$DESKTOP" "$ICON"
  echo "LarkAura was removed. Your settings remain in ~/.config/larkaura."
  echo "If LarkAura changed your wallpaper, restore it in Studio before removing it next time."
  exit 0
fi

SHELL_MAJOR="$(gnome-shell --version | awk '{split($3, v, "."); print v[1]}')"
if [ "$SHELL_MAJOR" != "50" ]; then
  echo "LarkAura has only been tested on GNOME 50, and this is GNOME $SHELL_MAJOR." >&2
  exit 1
fi

cd "$ROOT"
(cd apps/studio && npm ci --ignore-scripts --no-audit --no-fund && npm run build)
cargo build --release -p larkaura-engine -p larkaura-studio -p aura-wallpaper --bins
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}/release"

mkdir -p "$BIN" "$(dirname "$EXT")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"
install -m 755 "$TARGET/larkaura-engine" "$TARGET/larkaura-studio" "$TARGET/larkaura-wallpaper" "$BIN/"
rm -rf "$EXT"
cp -r "platform/linux-gnome/$UUID" "$EXT"
install -m 644 apps/studio/src-tauri/icons/icon.png "$ICON"
cat > "$DESKTOP" <<DESKTOP_FILE
[Desktop Entry]
Type=Application
Name=LarkAura Studio
Comment=Choose your desktop atmosphere
Exec=$BIN/larkaura-studio
Icon=dev.larklabs.LarkAura.Studio
Categories=Utility;Settings;
StartupWMClass=larkaura-studio
DESKTOP_FILE

enable_at_login
if gnome-extensions info "$UUID" 2>/dev/null | grep -q 'State: ACTIVE'; then
  echo "Updated. Log out and back in to load the new version."
else
  echo "Installed. Log out and back in to start LarkAura."
fi
