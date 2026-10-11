# EPOMAKER / RongYuan "YC500" configuration protocol

This document describes the USB HID protocol used by the official *EPOMAKER Driver*
(v1.2.66, built by RongYuan) to configure keyboards of the YC500 family, which includes the
**EPOMAKER TH80 / TH80 Pro, TH80 SE, TH80 X and TH80 UK**.

It was obtained for interoperability by static analysis of the Windows installer:

* the Electron front-end (`resources/app/dist/index.js`) contains the whole command
  layer in JavaScript;
* the native helper `iot_driver.exe` (Rust, talks to the device and serves gRPC on
  `[::]:3814`) adds the checksums and identifies the model. It was analysed with
  objdump and Ghidra.

Every statement below is backed by byte vectors recorded from the vendor's own code
(`crates/epomaker-core/tests/data/golden_th80.json`, 479 cases), which the Rust
implementation reproduces exactly (`cargo test`).

> Status: derived from the vendor software, **not yet verified on real hardware**.

## Transport

| Item | Value |
|------|-------|
| USB vendor id | `0x3151` (RongYuan / "ROYUAN") |
| Product ids | `0x4010` wired, `0x4011` 2.4 GHz dongle, `0x4015` wired (newer PCB) |
| HID interface | interface **2**, usage page **`0xFFFF`**, usage **`0x02`** |
| Report | Feature report, report id `0`, **64 bytes** payload (65 with the id byte) |
| Write | `HidD_SetFeature` / `hid_send_feature_report` |
| Read | `HidD_GetFeature` / `hid_get_feature_report` after the request |

The vendor app waits ~10 ms before each write and ~5 ms before each read, 500 ms after
lighting/debounce changes and 1 s after "big" writes (full layers, macros, colour tables).

### Checksums

Applied by `iot_driver.exe` (function at `0x4a5340`), selected per call:

| Type | Rule | Used by |
|------|------|---------|
| 0 "Bit7" | `buf[7] = ~(buf[0] + … + buf[6])` | almost every command |
| 1 "Bit8" | `buf[8] = ~(buf[0] + … + buf[7])` | lighting (`0x07`) |
| 2 none | sent as-is | full-layer writes (header checksum already computed by the app) |

### Model identification

The vendor/product ids are shared by many keyboards, so the driver asks the firmware:

```
-> 8F 00 00 00 00 00 00 70 …        (GET_DEVICE_ID, Bit7 checksum)
<- 8F i0 i1 i2 i3 …                 id = u32 little endian (TH80 = 1118, 742, 1091…)
```

Behind a 2.4 GHz dongle the driver sets bit 31 of the id, and the app looks the model up
as `-(id | 0x80000000)` (e.g. `-2147484766` for a TH80 on its dongle).

## Commands

`GET` commands are the `SET` command with bit 7 set. Unless stated, responses echo the
command in byte 0.

| Cmd | Name | Request (after byte 0) | Response |
|-----|------|------------------------|----------|
| `0x80` | GET_FIRMWARE | – | `[1..3]` u16 LE version |
| `0x8F` | GET_DEVICE_ID | – | `[1..5]` u32 LE model id |
| `0x02` | FACTORY_RESET | – | – |
| `0x83` | GET_BATTERY | – | `[1]` percent, `[2]` 0 discharging / 1 charging / 2 full |
| `0x04`/`0x84` | REPORT_RATE | `[1]` profile, `[2]` 1=1000 Hz, 2=500, 4=250, 8=125 | `[2]` code |
| `0x05`/`0x85` | PROFILE | `[1]` profile 0..2 | `[1]` active profile |
| `0x06`/`0x86` | KB_OPTION | `[1]` profile, `[2]` flags, `[3]` Fn swap, `[4]` power save | same layout |
| `0x07`/`0x87` | LED | see *Lighting* | `[1..8]` as written |
| `0x09`/`0x89` | KEY_MATRIX | see *Key matrix* | 64 raw bytes per chunk |
| `0x0C`/`0x8C` | USER_PIC | per-key colours, see below | 64 raw bytes per chunk |
| `0x8B` | GET_MACRO | `[1]` slot, `[2]` chunk 0..3 | 64 raw bytes |
| `0x10`/`0x90` | FN_MATRIX | like `0x09`/`0x89` for the Fn layer | |
| `0x11`/`0x91` | DEBOUNCE | `[2]` milliseconds | `[2]` ms |
| `0x12`/`0x92` | SLEEP_TIME | `[8..16]` 4 × u16 LE seconds | `[1..9]` 4 × u16 LE |
| `0x13` | SET_KEY | `[1]` profile, `[2]` matrix index, `[8..12]` entry | |
| `0x14` | SET_KEY_COLOR | `[1]` custom slot 0..2, `[2]` matrix index, `[8..11]` RGB | |
| `0x15` | SET_FN_KEY | `[1]` Fn layer, `[2]` matrix index, `[8..12]` entry | |
| `0x16` | SET_MACRO | `[1]` slot, `[2]` chunk, `[3]` 56, `[4]` last-chunk flag, `[8..64]` data | |
| `0x17`/`0x97` | AUTO_OS | `[1]` 0/1 | `[1]` |

Sleep times are, in order: Bluetooth light-off, 2.4 GHz light-off, Bluetooth deep sleep,
2.4 GHz deep sleep (0 = never for light-off). The TH80 accepts 1–60 min and 10–60 min.

Keyboard option flags (byte 2): bit0 Windows-key lock, bit2 Mac mode, bit3 WASD/arrow
swap, bit4 backlight off, bit5 side LEDs off, bit6 "keyboard mode", bit7 keyboard lock.

## Lighting

```
07 MM SS VV OO RR GG BB CK
```

* `MM` effect: 0 off, 1 static, 2 breathing, 3 neon, 4 wave, 5 ripple, 6 raindrop,
  7 snake, 8 reactive, 9 converge, 10 sine wave, 11 kaleidoscope, 12 line wave,
  13 custom per-key, 14 laser, 15 circle wave, 16 dazzling, 17 rain down, 18 meteor,
  19 reactive-off, 20/22 music follow, 21 screen colour, 23 train, 24 fireworks, 66 user colour.
* `SS` = `4 - speed` (speed 0..4), `VV` brightness 0..4.
* `OO` = `option << 4 | colour mode`; colour mode 7 = fixed colour, 8 = rainbow ("dazzle");
  0..6 in responses select a preset colour. Wave options: right/left/down/up = 0..3;
  binary options (snake z/return, kaleidoscope out/in, …) use `0x00` / `0x10`; the custom
  per-key effect uses `OO` = slot × 16.
* Pure white is sent as `FA FF FA`.
* Checksum type 1 at byte 8.

## Key matrix

A layer is 128 entries × 4 bytes (512 bytes), column-major over the switch matrix
(entry `6·column + row`). The factory TH80 matrix is in `crates/epomaker-core/data`.

| Entry | Meaning |
|-------|---------|
| `00 00 00 00` | disabled |
| `00 MM KK K2` | HID usage `KK` (+ second usage `K2`) with modifier `MM` (`E0`..`E3`) |
| `01 00 BB XX` | mouse: `F0` left, `F1` right, `F2` middle, `F3` forward, `F4` back, `F5..F8` wheel, `XX` amount |
| `03 00 LL HH` | consumer usage `0xHHLL` (e.g. `E2` mute, `E9`/`EA` volume) |
| `09 TT II 00` | macro `II`; `TT` 0 = repeat N times, 1 = toggle, 2 = while held |
| `0A 01 00 00` | Fn key; other `0A`/`0B`/`12`/`13` values are firmware functions |

The vendor app remaps one key at a time with `0x13`/`0x15`. A whole layer can be written
with 9 packets `09 PP F8 01 NN 00 00 CK` + 56 data bytes (checksum computed by the app,
driver type 2), and read with 8 × `89 PP NN` (64 data bytes each).

## Macros

50 slots of 256 bytes: `u16 LE repeat count`, then events:

* key / mouse button: `code, attr`; bit 7 of `attr` = pressed; bits 0..6 = delay after
  the event (1..127 ms). If those bits are 0, a `u16 LE` delay follows.
* mouse move: `F9, attr, dx, dy`.
* The stream ends with four zero bytes.

Written with `0x16` in 56-byte chunks (only non-empty chunks are sent, the last one has
byte 4 = 1); read with `8B SS CC` (4 × 64 bytes).

An empty slot therefore produces no packet at all with the vendor's routine. To empty a
slot (`Keyboard::clear_macro`, used by the self-test) this implementation sends chunk 0
zeroed with the last-chunk flag set: `16 SS 00 38 01 …` followed by 56 zero bytes. This
packet is not used by the vendor software and has not been tested on hardware yet.

## Per-key colours

Custom effect (mode 13) colours are stored as RGB triplets indexed by matrix entry
(128 × 3 = 384 bytes). One key: `14 SLOT IDX … R G B`. All keys: 7 packets
`0C 00 80 01 NN 00 00 CK` + 56 bytes (`0x0180` = 384 = 64 × 6). Read: 6 × `8C 00 NN`.

## Not covered

Firmware/bootloader updates (`0x7F`, OLED boot commands), screen uploads of the TH80 X,
music/screen-follow streaming (`0x0D`, `0x0E`) and the Bluetooth (`0x4012/0x4013`) vendor
pages are intentionally not implemented.
