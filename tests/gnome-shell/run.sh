#!/usr/bin/env bash
# Runs the GNOME overlay spike in an isolated headless GNOME Shell.
# It uses a private session bus, private config dirs and its own Wayland socket.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:?usage: run.sh OUTPUT_DIR}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
RUNTIME="/run/user/$(id -u)/larkaura-spike"
rm -rf "$RUNTIME" "$OUT/home"
mkdir -m 700 "$RUNTIME"
mkdir -p "$OUT/home/.local/share/gnome-shell/extensions" "$OUT/home/.config" "$OUT/home/.cache"
cp -r "$HERE/extension/larkaura-spike@larklabs.dev" "$OUT/home/.local/share/gnome-shell/extensions/"
if [ -n "${SPIKE_SETTINGS:-}" ]; then
  # Starts from a copy of existing settings, to test upgrades.
  mkdir -p "$OUT/home/.config/larkaura"
  cp "$SPIKE_SETTINGS" "$OUT/home/.config/larkaura/settings.json"
fi
export SPIKE_ENABLED='["larkaura-spike@larklabs.dev"]'
if [ -n "${SPIKE_PRODUCT:-}" ]; then
  # The real extension runs beside the test-only input and state helper.
  cp -r "${SPIKE_EXT_DIR:-$HERE/../../platform/linux-gnome/larkaura@larklabs.dev}" "$OUT/home/.local/share/gnome-shell/extensions/"
  export SPIKE_ENABLED='["larkaura-spike@larklabs.dev","larkaura@larklabs.dev"]'
fi
if [ -n "${SPIKE_STUDIO:-}" ]; then
  mkdir -p "$OUT/home/.local/share/applications"
  printf '[Desktop Entry]\nType=Application\nName=LarkAura Studio\nExec=%s\n' "$SPIKE_STUDIO" \
    > "$OUT/home/.local/share/applications/dev.larklabs.LarkAura.Studio.desktop"
fi
cleanup() {
  # Stop helpers started by the isolated session, then remove its runtime dir.
  for p in /proc/[0-9]*; do
    if { tr '\0' '\n' < "$p/environ"; } 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$RUNTIME"; then
      kill "${p#/proc/}" 2>/dev/null || true
    fi
  done
  sleep 1
  fusermount3 -u "$RUNTIME/doc" 2>/dev/null || true
  rm -rf "$RUNTIME"
}
trap cleanup EXIT
timeout "${SPIKE_TIMEOUT:-170}" env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS -u XAUTHORITY \
  HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config" XDG_CACHE_HOME="$OUT/home/.cache" \
  XDG_DATA_HOME="$OUT/home/.local/share" XDG_STATE_HOME="$OUT/home/.local/state" \
  XDG_RUNTIME_DIR="$RUNTIME" XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=GNOME \
  GDK_BACKEND=wayland SPIKE_DIR="$HERE" SPIKE_OUT="$OUT" \
  dbus-run-session -- bash -c '
    gsettings set org.gnome.shell enabled-extensions "$SPIKE_ENABLED"
    gsettings set org.gnome.shell welcome-dialog-last-shown-version "999"
    gsettings set org.gnome.desktop.interface enable-hot-corners false
    gnome-shell --headless --wayland --wayland-display larkaura-0 --virtual-monitor "${SPIKE_MONITOR:-1280x720}" ${SPIKE_MONITOR2:+--virtual-monitor "$SPIKE_MONITOR2"} >"$SPIKE_OUT/shell.log" 2>&1 &
    SHELL_PID=$!
    WAYLAND_DISPLAY=larkaura-0 python3 -I "$SPIKE_DIR/${SPIKE_DRIVER:-driver.py}"; CODE=$?
    kill $SHELL_PID 2>/dev/null || true
    wait $SHELL_PID 2>/dev/null || true
    exit $CODE' 2>"$OUT/session.log"
