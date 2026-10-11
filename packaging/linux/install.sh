#!/usr/bin/env bash
# Installs the binaries, the udev rule, the desktop entry and the icon.
# Works from the release tarball (binaries next to this folder) or from a source checkout
# after `cargo build --release`.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." 2>/dev/null && pwd)"
if [ -x "$HERE/../epomaker-panel" ]; then
  BIN="$(cd "$HERE/.." && pwd)"; ICONS="$BIN/icons"
else
  BIN="$ROOT/target/release"; ICONS="$ROOT/packaging/icons"
fi
PREFIX="${PREFIX:-/usr/local}"
sudo install -Dm755 "$BIN/epomaker-panel" "$PREFIX/bin/epomaker-panel"
sudo install -Dm755 "$BIN/epomaker" "$PREFIX/bin/epomaker"
[ -x "$BIN/epomaker-agent" ] && sudo install -Dm755 "$BIN/epomaker-agent" "$PREFIX/bin/epomaker-agent"
sudo install -Dm644 "$HERE/70-epomaker.rules" /etc/udev/rules.d/70-epomaker.rules
sudo install -Dm644 "$HERE/open-epomaker-panel.desktop" "$PREFIX/share/applications/open-epomaker-panel.desktop"
sudo install -Dm644 "$HERE/open-epomaker-agent.desktop" "$PREFIX/share/applications/open-epomaker-agent.desktop"
sudo install -Dm644 "$ICONS/open-epomaker-panel.svg" "$PREFIX/share/icons/hicolor/scalable/apps/open-epomaker-panel.svg"
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
echo "Installed. Unplug and replug the keyboard if it was connected."
