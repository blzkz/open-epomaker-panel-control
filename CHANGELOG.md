# Changelog

## 0.1.0 — 2026-10-11

First release. Experimental: verified against the official software, not yet on hardware.

- Protocol for the EPOMAKER TH80 family (TH80, TH80 Pro, TH80 SE, TH80 X, TH80 UK):
  model detection, lighting (19 effects), per-key RGB, key remapping on 3 profiles + Fn
  layer, 50 macros, polling rate, keyboard options, debounce, sleep timers, factory reset.
- `epomaker-panel` desktop app (English/Spanish) and `epomaker` command line tool,
  including a read-only `probe` command and a demo mode with an emulated keyboard.
- JSON backup and restore of the whole configuration.
- Packages for Windows, Debian/Ubuntu, Fedora/openSUSE, Arch and an AppImage.
