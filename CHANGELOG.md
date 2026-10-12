# Changelog

## Unreleased

- Writes on real hardware: failed transfers are retried with growing pauses (and the
  device reopened), lighting writes wait 500 ms first like the vendor app, and the
  self-test pauses before restoring. On Windows the firmware made writes fail with
  `os error 31` (ERROR_GEN_FAILURE) right after a lighting change. `EPOMAKER_PACING=2`
  doubles every delay, for troubleshooting.
- Self-test reports say which operation failed and whether the keyboard still answers.
- Keycap legends: the firmware reports ISO Spanish keyboards as the ISO UK model (same
  keys, other legends). The panel now prints the legends of the system keyboard layout,
  or the ones chosen in *Preferences* (US, UK, Spanish, Latin American, Portuguese,
  German, French, Italian). Raw labels such as `r_Shift` are cleaned up.
- Key tester: Caps Lock, Scroll Lock, Pause, Num Lock, Print Screen and Menu are now
  detected (window messages on Windows, the keyboard's own reports on Linux), and the ISO
  key next to Enter lights up.
- New look: Fira Sans, dark theme by default with a choice of accent color, sidebar with
  icons and sections, device header (battery, profile switcher, connection), card-based
  tabs, switches and segmented buttons. The redundant title bar text is gone.

## 0.3.0 — 2026-10-12

- Music-follow (two variants) and screen-color effects: the firmware has them, the
  official app only offers them on Windows. The PC streams the data: system audio
  (PulseAudio/PipeWire on Linux, WASAPI on Windows) analysed exactly like the vendor
  app, and the screen color (Windows, X11, and Wayland through the screen-sharing portal).
- CPU load bar on the number row (software effect).
- Tray agent `epomaker-agent`: per-application profiles (Windows, X11, KDE Plasma on
  Wayland), live effects, low-battery notifications, reconnection, start with the system.
  It releases the keyboard while the control panel is open.
- Control panel redesign: animated effect previews, rotary knob dial, searchable action
  picker, selection and gradient tools for per-key colors, toasts and progress,
  light/dark theme, interface size, remembered preferences, udev rule helper on Linux.
- New tabs: key tester (with chatter detection) and tray agent settings.
- Macro library: export/import macros as JSON files, mouse-move events, automatic free slot.
- CLI: `live music|screen|cpu`, `macro export|import`, `--json`, shell completions.
- Hardware self-test: `epomaker selftest --yes` and the GUI's new Diagnostics tab run
  read-only checks plus three writes that are undone right away (brightness, one key on
  profile 3, an unused macro slot) and produce a Markdown report for GitHub issues.
- Verify-after-write: `Keyboard::set_verify` / `epomaker --verify` read every write back
  and fail with a mismatch error; on by default in the GUI.
- The GUI connects when a keyboard is plugged in, shows when it is unplugged and
  remembers the last keyboard across restarts.
- The firmware emulator can inject faults (dropped or corrupted writes, corrupted
  answers, I/O errors) for tests.

## 0.2.0 — 2026-10-11

- New models: EPOMAKER TH96, TH68, TH-21 (numpad) and EP84, wired and 2.4 GHz dongle, each
  verified against byte vectors recorded from the official software.
- `tools/re-harness/gen_golden.js` takes the device id (and optionally the class) of the
  model to record; the golden test-suite runs for every supported model.

## 0.1.1 — 2026-10-11

- The project is now licensed under MIT; every package and archive ships the license.
- App icon (window, Windows executable, Linux desktop entry).

## 0.1.0 — 2026-10-11

First release. Experimental: verified against the official software, not yet on hardware.

- Protocol for the EPOMAKER TH80 family (TH80, TH80 Pro, TH80 SE, TH80 X, TH80 UK):
  model detection, lighting (19 effects), per-key RGB, key remapping on 3 profiles + Fn
  layer, 50 macros, polling rate, keyboard options, debounce, sleep timers, factory reset.
- `epomaker-panel` desktop app (English/Spanish) and `epomaker` command line tool,
  including a read-only `probe` command and a demo mode with an emulated keyboard.
- JSON backup and restore of the whole configuration.
- Packages for Windows, Debian/Ubuntu, Fedora/openSUSE, Arch and an AppImage.
