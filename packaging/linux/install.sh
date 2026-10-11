#!/usr/bin/env bash
# Installs the binaries, the udev rule and the desktop entry (run from the repository root
# after `cargo build --release`).
set -euo pipefail
PREFIX="${PREFIX:-/usr/local}"
sudo install -Dm755 target/release/epomaker-panel "$PREFIX/bin/epomaker-panel"
sudo install -Dm755 target/release/epomaker "$PREFIX/bin/epomaker"
sudo install -Dm644 packaging/linux/70-epomaker.rules /etc/udev/rules.d/70-epomaker.rules
sudo install -Dm644 packaging/linux/open-epomaker-panel.desktop "$PREFIX/share/applications/open-epomaker-panel.desktop"
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
echo "Installed. Unplug and replug the keyboard if it was connected."
