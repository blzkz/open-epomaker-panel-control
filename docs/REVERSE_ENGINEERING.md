# How the protocol was obtained (and how to extend it)

Everything here runs locally on the **vendor installer you downloaded yourself**; nothing
from it is redistributed in this repository and nothing from it is executed natively.

## 1. Extract the installer

```bash
tools/re-harness/extract.sh "EPOMAKER_setup_1.2.66(WIN20230421).exe"
```

The installer is NSIS → `app-32.7z` → an Electron app (`resources/app`). The script
unpacks it into `vendor/` (git-ignored) and splits the Parcel bundle `dist/index.js` into one
file per module (`vendor/re/index/<id>.js`) plus a dependency map.

Useful modules (ids are build-specific, search by content):

| Content | What it is |
|---------|------------|
| `exports.supportYC500Dev=` | device list: model id, VID/PID, usage page, layers, lighting capabilities |
| `class … extends …YC200Common` (`YC500Common`) | command encoders/decoders for this family |
| `exports.defaultMatrix_yc500_th80=` | factory key matrix |
| `yc500_th80:{layout:` | on-screen key geometry of every model |
| `exports.specialFunTablectionMap=` | media / firmware function codes |

## 2. Run the vendor code against a fake keyboard

`tools/re-harness/loader.js` is a tiny Parcel module loader. `fake.js` replaces the gRPC
client that talks to `iot_driver.exe` with a recorder that applies the driver's
checksums, and stubs the UI store. This lets the original classes run under Node:

```bash
NODE_PATH=vendor/app/resources/app/node_modules node tools/re-harness/gen_golden.js crates/epomaker-core/tests/data/golden_th80.json
NODE_PATH=vendor/app/resources/app/node_modules node tools/re-harness/dump_devices.js '^yc500_th80' crates/epomaker-core/data/th80_family.json
```

`gen_golden.js` records the exact reports produced for ~480 operations and how canned
responses are decoded; the Rust test-suite (`crates/epomaker-core/tests/golden.rs`) must
reproduce all of them byte for byte. `dump_devices.js` exports device facts in this
project's own schema.

## 3. The native driver

`iot_driver.exe` is a Rust binary (tokio + tonic + hidapi). Findings:

* checksum routine at `0x4a5340` (sum of 7 or 8 bytes, bitwise NOT, stored at byte 7/8);
* an embedded TOML list of supported interfaces (`support = [ { vid = 0x3151, pid = 0x4010,
  usage = 2, usage_page = 0xffff, interface_number = 2 }, … ]`);
* model identification: request `[0x8F]` (constant at `0x7ecdf0`), id = `u32` from response
  bytes 1..5, bit 31 set for dongles (`or byte [id+3], 0x80`).

Analysis was done with `objdump -d -M intel` and Ghidra 12 headless
(`analyzeHeadless … -postScript DecompAt.java`).

## Adding another YC500 keyboard

1. Find its `name` in `supportYC500Dev` (e.g. `yc500_rt100`).
2. Check that its class does not override the YC500 commands (compare with
   `YC500Common`; screen models add extra commands).
3. Export it: `dump_devices.js '^yc500_rt100$' crates/epomaker-core/data/rt100.json` and add the
   file to `Database::get()` in `devices.rs`.
4. Optionally adapt `gen_golden.js` to the new class and add its vectors to the tests.
