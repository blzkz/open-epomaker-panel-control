#!/usr/bin/env sh
# Keeps the cargo build directory from growing without bound: cargo never deletes old
# artifacts, so target/ only grows as dependencies, features and targets change.
#
#   tools/trim-target.sh          remove what is rebuilt on demand (incremental caches,
#                                 cross builds, packaging leftovers); clean everything
#                                 when target/ is still bigger than the limit
#   tools/trim-target.sh --all    cargo clean
#   TRIM_LIMIT_GB=5 tools/trim-target.sh
#
# Nothing installed on the system uses target/: removing it only means a slower next build.
set -eu
cd "$(dirname "$0")/.."
limit_gb=${TRIM_LIMIT_GB:-10}
target=target

size_gb() { du -s -B1G "$target" 2>/dev/null | cut -f1 || echo 0; }

if [ ! -d "$target" ]; then
    echo "target/ does not exist, nothing to do"
    exit 0
fi
echo "target/ before: $(du -sh "$target" | cut -f1)"

if [ "${1:-}" = "--all" ]; then
    cargo clean
    exit 0
fi

# incremental compilation caches: the largest part of a debug build, rebuilt as needed
find "$target" -maxdepth 3 -type d -name incremental -prune -exec rm -rf {} +
# cross builds (e.g. x86_64-pc-windows-gnu) and packaging leftovers
for d in "$target"/*-*-*-* "$target"/debian "$target"/generate-rpm "$target"/appimage "$target"/epomaker-gui "$target"/tmp; do
    [ -e "$d" ] && rm -rf "$d"
done

if [ "$(size_gb)" -ge "$limit_gb" ]; then
    echo "still over ${limit_gb} GB: cleaning everything"
    cargo clean
else
    echo "target/ after: $(du -sh "$target" | cut -f1)"
fi
