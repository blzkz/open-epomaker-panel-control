# Open Epomaker Panel

An open-source control panel for **EPOMAKER** keyboards on **Linux and Windows**, written in
Rust. It is a replacement for the official "EPOMAKER Driver" for the EPOMAKER keyboards built
on the RongYuan YC500 platform (see [Supported keyboards](#supported-keyboards)).

> 🇪🇸 Spanish version: [README.es.md](README.es.md)

> ⚠️ **Status: experimental.** The protocol was reverse engineered from the official
> software and is checked byte-for-byte against it (2,359 test vectors across 5 models), but it has **not yet
> been tested on a physical keyboard**. Start with the read-only `epomaker probe` command.
> This project is not affiliated with EPOMAKER.

## Features

| | GUI | CLI |
|---|---|---|
| Detect the keyboard model (firmware id) | ✅ | ✅ |
| Lighting: 19 effects, speed, brightness, direction, color, rainbow | ✅ | ✅ |
| Per-key RGB (custom effect), paint with the mouse | ✅ | ✅ |
| Key remapping on the 3 profiles and the Fn layer (keys, combos, mouse, media, macros) | ✅ | ✅ |
| Macros (50 slots): editor, recording, repeat / toggle / hold modes | ✅ | ✅ |
| Profiles, polling rate, Win-lock, Mac mode, WASD/arrow swap, Fn swap… | ✅ | ✅ |
| Debounce, wireless sleep timers, automatic OS detection | ✅ | ✅ |
| Full backup / restore to JSON, factory reset | ✅ | ✅ |
| Demo mode with an emulated keyboard (no hardware needed) | ✅ | ✅ |
| Hardware self-test with a report for GitHub issues, verify-after-write | ✅ | ✅ |
| Hotplug: connects when the keyboard is plugged in, remembers the last one | ✅ | |

Not implemented on purpose: firmware updates, TH80 X screen uploads, and music/screen
"follow" effects (those need the PC to stream data).

## Supported keyboards

Wired (USB) and 2.4 GHz dongle connections are supported for every model. Each one is
checked against byte vectors recorded from the official software; none has been tested on
hardware yet.

| Model | Firmware id (wired) | `--demo-model` |
|---|---|---|
| TH80, TH80 Pro | 1118, 742 | `1118` |
| TH80 SOC | 1091 | `1091` |
| TH80 SE | 886 | `886` |
| TH80 X (screen not supported) | 928 | `928` |
| TH80 UK | 741 | `741` |
| TH96 | 834 | `834` |
| TH68 | 988 | `988` |
| TH-21 (numpad) | 789 | `789` |
| EP84 | 1081 | `1081` |

Other YC500 models (e.g. RT100, which has a screen) can be added from the official
software's data; see [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md#adding-another-yc500-keyboard).

## Download

Ready-made packages are attached to every [GitHub release](https://github.com/blzkz/open-epomaker-panel-control/releases):

| System | Package |
|---|---|
| Windows 10/11 | `open-epomaker-panel-windows-x86_64.zip`: unzip and run `epomaker-panel.exe` |
| Debian, Ubuntu, Mint, Pop!_OS | `.deb`: `sudo apt install ./open-epomaker-panel_*_amd64.deb` |
| Fedora, RHEL, openSUSE | `.rpm`: `sudo dnf install ./open-epomaker-panel-*.x86_64.rpm` (or `zypper install`) |
| Arch, Manjaro, EndeavourOS, CachyOS | `.pkg.tar.zst`: `sudo pacman -U open-epomaker-panel-*.pkg.tar.zst`, or build `packaging/arch/PKGBUILD` |
| Any Linux | `open-epomaker-panel-x86_64.AppImage` (portable) or the `.tar.gz` with `linux/install.sh` |

The deb, rpm and Arch packages also install the udev rule that gives your user access to
the keyboard. With the AppImage, install `packaging/linux/70-epomaker.rules` yourself (see
below). Linux builds need glibc 2.35 or newer (Ubuntu 22.04+, Debian 12+, Fedora 36+…).

## Build from source

### Compile

You need Rust ≥ 1.88. On Linux you also need `libudev` development files (Debian/Ubuntu:
`sudo apt install libudev-dev libxkbcommon-dev libgl1-mesa-dev libwayland-dev`).

```bash
cargo build --release
```

This builds `target/release/epomaker-panel` (GUI) and `target/release/epomaker` (CLI).

### Linux permissions (only when not using a package)

Regular users cannot open HID devices by default. Install the udev rule (and the
binaries and desktop entry) with:

```bash
packaging/linux/install.sh
```

Or only the rule: `sudo cp packaging/linux/70-epomaker.rules /etc/udev/rules.d/`, then
replug the keyboard.

### Windows

No driver is needed. Run `epomaker-panel.exe`. Close the official EPOMAKER software
first so both programs don't talk to the keyboard at the same time.

## Usage

### GUI

Run `epomaker-panel`. It connects on its own when a supported keyboard is plugged in (the
last one used is preferred and remembered across restarts) and shows *Keyboard
disconnected* when it is unplugged. You can also pick a keyboard (or a **demo keyboard**)
and click *Connect*. The tabs are Lighting, Key mapping, Per-key colors, Macros, Settings,
Backup and Diagnostics. The interface is in English or Spanish.

The **Diagnostics** tab runs the hardware self-test (see below) step by step and has a
*Copy report* button that puts a Markdown report on the clipboard, ready to paste into a
GitHub issue: app version, model and raw device id, firmware, USB ids and interface, the
result of every step and the raw `probe` output. The same tab holds the **Verify every
write** switch (on by default): every change made in the app is read back from the
keyboard and a mismatch is shown as an error. Without a keyboard, the tab runs the
read-only probe instead, for keyboards that are not detected.

### CLI

```bash
epomaker probe                                   # read-only diagnostics (start here)
epomaker selftest --yes                          # reversible self-test, prints a report
epomaker info
epomaker light modes
epomaker light set wave --option left --speed 3 --color ff0080
epomaker keymap set "Caps Lock" disable
epomaker keymap set A key:ctrl+c --layer p1
epomaker keymap set F1 fn:play-pause --layer fn0
epomaker macro set 0 "down:LeftShift down:H delay:30 up:H up:LeftShift" --repeat 1
epomaker keymap set F5 macro:0:toggle
epomaker color set Esc ff0000
epomaker backup my-th80.json
epomaker --demo info                             # anything works on the emulator too
```

Use `epomaker keymap actions` to list key, mouse and function names, `--trace` to
print every report sent to the keyboard and `--verify` to read every write back (the
command fails if the keyboard did not store the value).

## Project layout

```
crates/epomaker-core   protocol, device database, HID transport, firmware emulator
crates/epomaker-cli    `epomaker` command line tool
crates/epomaker-gui    `epomaker-panel` desktop app (egui)
docs/PROTOCOL.md       the wire protocol
docs/REVERSE_ENGINEERING.md   how it was obtained, how to add more models
tools/re-harness       scripts that run the vendor's code against a fake keyboard
```

## How it was done

The official installer is an Electron app plus a small Rust service (`iot_driver.exe`).
The command layer was read from the JavaScript bundle. The original classes were then
executed under Node against a fake device to record the exact bytes they send
(`tools/re-harness`). The checksum and model-identification logic in the native service
were recovered with objdump and Ghidra. See [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md).

No vendor code or binaries are included in this repository. The `vendor/` directory is
git-ignored.

## Testing on real hardware

If you own one of the keyboards above (or another EPOMAKER YC500 keyboard), please help:

1. `epomaker probe` and share the output.
2. `epomaker selftest --yes` (or *Run self-test* in the GUI's Diagnostics tab) and paste
   the report into a GitHub issue.
3. `epomaker info`, then try a lighting change (`epomaker light set breath --color 00ff00 --verify`).
4. Back up before remapping keys: `epomaker backup before.json`.

The self-test only reads, except for three small writes that are undone right away: the
backlight brightness is lowered by one step and the original effect written back, a rarely
used key (Pause, Scroll Lock, right Ctrl…) on **profile 3** is remapped to F13 and restored,
and an unused macro slot (preferably empty, never one bound to a key) is written, read back
and emptied. Every write is verified, and each step restores the original state even when
it fails; a restore that does not stick is reported as *not restored*, with the original
value, so it can be fixed by hand.

## License

[MIT](LICENSE). Not affiliated with or endorsed by EPOMAKER.
