#!/usr/bin/env bash
# Extracts the official EPOMAKER driver installer (NSIS + Electron) into vendor/ for analysis.
# Usage: tools/re-harness/extract.sh path/to/EPOMAKER_setup_x.y.z.exe
# Requires: 7z, node. Nothing from the installer is ever executed.
set -euo pipefail
EXE="$1"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
V="$ROOT/vendor"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$V/extracted" "$V/app" "$V/re"
7z x -y -o"$V/extracted" "$EXE" >/dev/null
7z x -y -o"$V/app" "$V/extracted/\$PLUGINSDIR/app-32.7z" >/dev/null
DIST="$V/app/resources/app/dist"
node "$HERE/split.js" "$DIST/index.js" "$V/re/index"
node "$HERE/split.js" "$DIST/main.js" "$V/re/main"
node "$HERE/deps.js" "$DIST/index.js" "$V/re/index/_deps.json"
node "$HERE/names.js" "$DIST/index.js" "$V/re/index/_names.tsv"
echo "Extracted to $V"
