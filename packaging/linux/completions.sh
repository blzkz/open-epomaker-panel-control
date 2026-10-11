#!/usr/bin/env bash
# Generates shell completion scripts for `epomaker` into target/completions (used by packaging).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${1:-$ROOT/target/release/epomaker}"
OUT="$ROOT/target/completions"
mkdir -p "$OUT"
"$BIN" completions bash > "$OUT/epomaker.bash"
"$BIN" completions zsh > "$OUT/_epomaker"
"$BIN" completions fish > "$OUT/epomaker.fish"
