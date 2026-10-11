#!/usr/bin/env bash
# Builds open-epomaker-panel-x86_64.AppImage from target/release (run `cargo build --release` first).
# Usage: packaging/linux/build-appimage.sh [output-dir]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/appimage}"
TOOL_VERSION=1.9.1
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/AppDir"

install -Dm755 "$ROOT/target/release/epomaker-panel" "$APPDIR/usr/bin/epomaker-panel"
install -Dm755 "$ROOT/target/release/epomaker" "$APPDIR/usr/bin/epomaker"
install -Dm644 "$ROOT/packaging/linux/open-epomaker-panel.desktop" "$APPDIR/usr/share/applications/open-epomaker-panel.desktop"
install -Dm644 "$ROOT/packaging/icons/open-epomaker-panel.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/open-epomaker-panel.png"
install -Dm644 "$ROOT/packaging/linux/70-epomaker.rules" "$APPDIR/usr/share/open-epomaker-panel/70-epomaker.rules"
cp "$APPDIR/usr/share/applications/open-epomaker-panel.desktop" "$APPDIR/"
cp "$ROOT/packaging/icons/open-epomaker-panel.png" "$APPDIR/open-epomaker-panel.png"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
# `open-epomaker-panel.AppImage cli <args>` runs the command line tool
HERE="$(dirname "$(readlink -f "$0")")"
if [ "$1" = "cli" ]; then shift; exec "$HERE/usr/bin/epomaker" "$@"; fi
exec "$HERE/usr/bin/epomaker-panel" "$@"
RUN
chmod +x "$APPDIR/AppRun"

TOOL="$WORK/appimagetool"
curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/$TOOL_VERSION/appimagetool-x86_64.AppImage"
chmod +x "$TOOL"
mkdir -p "$OUT"
ARCH=x86_64 "$TOOL" --appimage-extract-and-run --no-appstream "$APPDIR" "$OUT/open-epomaker-panel-x86_64.AppImage"
echo "Built $OUT/open-epomaker-panel-x86_64.AppImage"
