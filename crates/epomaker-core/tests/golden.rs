//! Conformance tests against byte vectors recorded from the vendor's own code
//! (`tools/re-harness/gen_golden.js`). Every report this crate builds must be
//! byte-identical to what the official EPOMAKER driver sends, and every response
//! must be interpreted the same way.
//!
//! The same checks run against the vectors of every model listed in `golden_suite!`.

use epomaker_core::devices::{code_to_entry, Database, ModelInfo};
use epomaker_core::keymap::{
    encode_read_matrix, encode_set_key, encode_write_matrix, KeyAction, KeyMatrix, Layer, Modifier, MouseAction,
    SpecialFn,
};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{self, Macro, MacroEvent};
use epomaker_core::protocol::{Checksum, Frame, Report};
use epomaker_core::settings::{self, Battery, ChargeState, KeyboardOptions, ReportRate, SleepTimes};
use serde_json::{json, Value};

macro_rules! golden_suite {
    ($($module:ident: $model:literal => $file:literal),* $(,)?) => {$(
        mod $module {
            fn golden() -> super::Golden {
                super::Golden::load($model, include_str!($file))
            }

            golden_suite!(@tests lighting_encode, lighting_decode, simple_settings, key_remap_simple, macros_write,
                macros_read, matrix_write_full, matrix_read_full, key_colors, every_case_is_covered);
        }
    )*};
    (@tests $($test:ident),*) => {$(
        #[test]
        fn $test() {
            super::$test(&golden());
        }
    )*};
}

golden_suite! {
    th80: "yc500_th80" => "data/golden_th80.json",
    th96: "yc500_th96" => "data/golden_th96.json",
    th68_soc: "yc500_th68_soc" => "data/golden_th68_soc.json",
    th21: "yc500_th21" => "data/golden_th21.json",
    ep84_soc: "yc500_ep84_soc" => "data/golden_ep84_soc.json",
}

/// Vectors recorded for one model.
struct Golden {
    model: &'static ModelInfo,
    keys: Keys,
    cases: Vec<Case>,
}

/// Keys of the model's matrix that the generator used for each role (`meta.keys`).
struct Keys {
    a: u64,
    b: u64,
    esc: u64,
    f: Vec<u64>,
    macros: Vec<u64>,
}

struct Case {
    name: String,
    writes: Vec<(u8, Vec<u8>)>,
    result: Value,
}

impl Golden {
    fn load(model: &str, src: &str) -> Golden {
        let v: Value = serde_json::from_str(src).unwrap();
        let model = Database::get().model(model).unwrap();
        let k = &v["meta"]["keys"];
        let list = |v: &Value| v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect::<Vec<_>>();
        let keys = Keys {
            a: k["a"].as_u64().unwrap(),
            b: k["b"].as_u64().unwrap(),
            esc: k["esc"].as_u64().unwrap(),
            f: list(&k["f"]),
            macros: list(&k["macros"]),
        };
        let cases = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| Case {
                name: c["name"].as_str().unwrap().to_string(),
                writes: c["writes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|w| (w["ck"].as_u64().unwrap() as u8, unhex(w["data"].as_str().unwrap())))
                    .collect(),
                result: c["result"].clone(),
            })
            .collect();
        Golden { model, keys, cases }
    }

    fn cases(&self) -> impl Iterator<Item = &Case> {
        self.cases.iter()
    }

    /// Matrix index addressed by a vendor key config (`original` + `index`).
    fn matrix_index(&self, original: u64, nth: u64) -> u8 {
        let want = code_to_entry(original as u32);
        self.model
            .default_matrix
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, e)| **e == want)
            .map(|(i, _)| i)
            .nth(nth as usize)
            .unwrap() as u8
    }
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn report(prefix: &[u8]) -> Report {
    let mut r = [0u8; 64];
    r[..prefix.len()].copy_from_slice(prefix);
    r
}

/// Asserts that `frames` produce exactly the recorded writes (checksum kind included).
fn assert_frames(case: &Case, frames: &[Frame]) {
    assert_eq!(frames.len(), case.writes.len(), "{}: number of reports", case.name);
    for (i, (f, (ck, data))) in frames.iter().zip(&case.writes).enumerate() {
        assert_eq!(f.checksum.driver_id(), *ck, "{}: checksum kind of report {i}", case.name);
        // the recorded bytes already have the checksum applied by the (emulated) driver
        assert_eq!(f.wire_bytes().to_vec(), *data, "{}: bytes of report {i}", case.name);
    }
}

fn arg(name: &str) -> Value {
    let j = name.find(' ').map(|i| &name[i + 1..]).unwrap_or("");
    serde_json::from_str(j).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------- lighting

fn effect_from_vendor(cfg: &Value) -> LightEffect {
    let mode = LightMode::from_vendor_name(cfg["type"].as_str().unwrap()).unwrap();
    let option = cfg["option"].as_str().and_then(|o| mode.options().iter().position(|x| *x == o)).unwrap_or(0) as u8;
    LightEffect {
        mode,
        speed: cfg["speed"].as_u64().unwrap_or(0) as u8,
        brightness: cfg["value"].as_u64().unwrap_or(0) as u8,
        option,
        color: Rgb(cfg["rgb"].as_u64().unwrap_or(0) as u32),
        dazzle: cfg["dazzle"].as_bool().unwrap_or(false),
    }
}

/// Shape of the object returned by the vendor's `getLightSetting`.
fn effect_to_vendor(e: &LightEffect) -> Value {
    let t = e.mode.vendor_name();
    let opt = |names: &[&'static str]| -> &'static str { names.get(e.option as usize).copied().unwrap_or(names[0]) };
    let full = |o: Option<&str>| {
        let mut v = json!({"type": t, "value": e.brightness, "speed": e.speed, "rgb": e.color.0, "dazzle": e.dazzle});
        if let Some(o) = o {
            v["option"] = json!(o);
        }
        v
    };
    match e.mode {
        LightMode::Off | LightMode::UserColor => json!({"type": t}),
        LightMode::AlwaysOn => json!({"type": t, "value": e.brightness, "rgb": e.color.0, "dazzle": e.dazzle}),
        LightMode::Neon => {
            json!({"type": t, "value": e.brightness, "speed": e.speed, "option": opt(&["Default", "Random"])})
        }
        LightMode::UserPicture => {
            json!({"type": t, "value": e.brightness, "option": opt(LightMode::UserPicture.options())})
        }
        LightMode::MusicFollow2 | LightMode::MusicFollow3 => {
            json!({"type": t, "value": e.brightness, "option": opt(e.mode.options()), "rgb": e.color.0, "dazzle": e.dazzle})
        }
        LightMode::ScreenColor => json!({"type": t, "value": 4}),
        m if !m.options().is_empty() => full(Some(opt(m.options()))),
        _ => full(None),
    }
}

fn lighting_encode(g: &Golden) {
    let mut n = 0;
    for c in g.cases().filter(|c| c.name.starts_with("led_set ")) {
        let e = effect_from_vendor(&arg(&c.name));
        assert_frames(c, &[e.encode()]);
        n += 1;
    }
    assert!(n >= 100, "only {n} lighting cases");
}

fn lighting_decode(g: &Golden) {
    let mut n = 0;
    for c in g.cases().filter(|c| c.name.starts_with("led_get ")) {
        assert_frames(c, &[LightEffect::query()]);
        let resp = report(&unhex(c.name.split(' ').nth(1).unwrap()));
        let got = LightEffect::decode(&resp).map(|e| effect_to_vendor(&e)).unwrap_or(Value::Null);
        assert_eq!(got, c.result, "{}", c.name);
        n += 1;
    }
    assert!(n > 150, "only {n} lighting cases");
}

// ---------------------------------------------------------------- settings

fn simple_settings(g: &Golden) {
    let m = g.model;
    for c in g.cases() {
        let name = c.name.as_str();
        let num = || name.rsplit(' ').next().unwrap().parse::<u64>().unwrap();
        match name.split(' ').next().unwrap() {
            "rate_set" => assert_frames(c, &[ReportRate::from_hz(num() as u16).unwrap().encode(0)]),
            "rate_set_profile2" => assert_frames(c, &[ReportRate::Hz1000.encode(2)]),
            "rate_get" => {
                assert_frames(c, &[ReportRate::query(0)]);
                let got =
                    ReportRate::decode(&report(&[0x84, 0, num() as u8])).map(|r| json!(r.hz())).unwrap_or(json!("err"));
                assert_eq!(got, c.result, "{name}");
            }
            "profile_set" => assert_frames(c, &[settings::encode_set_profile(num() as u8)]),
            "profile_get" => {
                assert_frames(c, &[epomaker_core::protocol::query(0x85, 0, 0)]);
                assert_eq!(json!(settings::decode_profile(&report(&[0x85, num() as u8]))), c.result);
            }
            "opts_set" => {
                let o = arg(name);
                let b = |k: &str| o[k].as_bool().unwrap_or(false);
                let opts = KeyboardOptions {
                    win_key_lock: b("winKeyLockControl"),
                    mac_mode: b("system"),
                    wasd_arrows_swap: b("wasdKeyAndArrowKeyExchange"),
                    led_off: b("ledOff"),
                    side_led_off: b("sLedOff"),
                    gaming_mode: b("keyboardMode"),
                    keyboard_lock: b("keyboardLockControl"),
                    fn_key_swap: b("keyboardFnKeyMatrix"),
                    power_save: b("powerSaveMode"),
                };
                assert_frames(c, &[opts.encode(0)]);
            }
            "opts_get" => {
                assert_frames(c, &[KeyboardOptions::query(0)]);
                let raw = unhex(name.split(' ').nth(1).unwrap());
                let o = KeyboardOptions::decode(&report(&[0x86, 0, raw[0], raw[1], raw[2]]));
                let got = json!({
                    "winKeyLockControl": o.win_key_lock, "system": o.mac_mode,
                    "wasdKeyAndArrowKeyExchange": o.wasd_arrows_swap, "ledOff": o.led_off,
                    "sLedOff": o.side_led_off, "keyboardMode": o.gaming_mode,
                    "keyboardLockControl": o.keyboard_lock, "keyboardFnKeyMatrix": o.fn_key_swap,
                    "powerSaveMode": o.power_save,
                });
                assert_eq!(got, c.result, "{name}");
            }
            "debounce_set" => assert_frames(c, &[settings::encode_set_debounce(num() as u8)]),
            "debounce_get" => {
                assert_frames(c, &[epomaker_core::protocol::query(0x91, 0, 0)]);
                assert_eq!(json!(settings::decode_debounce(&report(&[0x91, 0, 7]))), c.result);
            }
            "sleep_set" => {
                let s = arg(name);
                let t = SleepTimes {
                    bt_light_off: s["time_bt"].as_u64().unwrap() as u16,
                    dongle_light_off: s["time_24"].as_u64().unwrap() as u16,
                    bt_deep_sleep: s["deepTime_bt"].as_u64().unwrap() as u16,
                    dongle_deep_sleep: s["deepTime_24"].as_u64().unwrap() as u16,
                };
                assert_frames(c, &[t.encode()]);
            }
            "sleep_get" | "sleep_get_clamp" => {
                let resp = if name == "sleep_get" {
                    report(&[0x92, 0x58, 0x02, 0x2c, 0x01, 0x08, 0x07, 0x10, 0x0e])
                } else {
                    report(&[0x92, 10, 0, 0, 0, 1, 0, 0xff, 0xff])
                };
                let s = SleepTimes::decode(&resp, m.sleep_bt, m.sleep_dongle);
                let got = json!({"time_bt": s.bt_light_off, "time_24": s.dongle_light_off, "deepTime_bt": s.bt_deep_sleep, "deepTime_24": s.dongle_deep_sleep});
                assert_eq!(got, c.result, "{name}");
            }
            "battery_get" | "battery_get_full" => {
                let resp = if name == "battery_get" { report(&[0x83, 77, 1]) } else { report(&[0x83, 100, 2]) };
                let b: Battery = Battery::decode(&resp);
                let status = match b.state {
                    ChargeState::Discharging => "uncharing",
                    ChargeState::Charging => "charing",
                    ChargeState::Full => "full",
                };
                assert_eq!(json!({"battery": b.percent, "status": status}), c.result);
            }
            "fw_get" => {
                assert_frames(c, &[epomaker_core::protocol::query(0x80, 0, 0)]);
                assert_eq!(json!(settings::decode_firmware(&report(&[0x80, 0x34, 0x12]))), c.result);
            }
            "reset" => assert_frames(c, &[settings::encode_factory_reset()]),
            "autoosen_set" => assert_frames(c, &[settings::encode_set_auto_os(true)]),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- key mapping

const MOUSE_KEYS: [(i64, Option<MouseAction>); 18] = [
    (-1000, Some(MouseAction::Left)),
    (-999, Some(MouseAction::Right)),
    (-998, Some(MouseAction::Middle)),
    (-997, Some(MouseAction::Back)),
    (-996, Some(MouseAction::Forward)),
    (-995, None),
    (-994, Some(MouseAction::WheelForward)),
    (-993, Some(MouseAction::WheelBack)),
    (-992, Some(MouseAction::WheelLeft)),
    (-991, Some(MouseAction::WheelRight)),
    (-990, None),
    (-989, None),
    (-988, Some(MouseAction::ScrollUp)),
    (-987, Some(MouseAction::ScrollDown)),
    (-986, Some(MouseAction::MoveLeft)),
    (-985, Some(MouseAction::MoveRight)),
    (-984, Some(MouseAction::MoveUp)),
    (-983, Some(MouseAction::MoveDown)),
];

fn mouse_from_vendor(k: i64) -> Option<MouseAction> {
    MOUSE_KEYS.iter().find(|(v, _)| *v == k).and_then(|(_, m)| *m)
}

fn mouse_to_vendor(m: MouseAction) -> i64 {
    MOUSE_KEYS.iter().find(|(_, x)| *x == Some(m)).unwrap().0
}

fn modifier_from_vendor(s: &str) -> Modifier {
    match s {
        "ctrl" => Modifier::Ctrl,
        "shift" => Modifier::Shift,
        "alt" => Modifier::Alt,
        "win" => Modifier::Win,
        _ => Modifier::None,
    }
}

fn action_from_vendor(cfg: &Value) -> KeyAction {
    match cfg["type"].as_str().unwrap() {
        "forbidden" => KeyAction::Disabled,
        "combo" => KeyAction::Key {
            modifier: modifier_from_vendor(cfg["skey"].as_str().unwrap()),
            key: cfg["key"].as_u64().unwrap() as u8,
            key2: cfg["key2"].as_u64().unwrap() as u8,
        },
        "ConfigChangeToMouseBtn" => {
            mouse_from_vendor(cfg["key"].as_i64().unwrap()).map(KeyAction::Mouse).unwrap_or(KeyAction::Disabled)
        }
        "ConfigFunction" => KeyAction::Function(SpecialFn::from_vendor_name(cfg["key"].as_str().unwrap()).unwrap()),
        t => panic!("unexpected {t}"),
    }
}

fn key_remap_simple(g: &Golden) {
    let mut n = 0;
    for c in g.cases() {
        let (layer, json) = if let Some(j) = c.name.strip_prefix("key_simple ") {
            (Layer::Profile(0), j)
        } else if let Some(j) = c.name.strip_prefix("key_simple_p1 ") {
            (Layer::Profile(1), j)
        } else if let Some(j) = c.name.strip_prefix("fn_simple ") {
            (Layer::Fn(0), j)
        } else {
            continue;
        };
        let cfg: Value = serde_json::from_str(json).unwrap();
        let idx = g.matrix_index(cfg["original"].as_u64().unwrap(), cfg["index"].as_u64().unwrap_or(0));
        assert_frames(c, &[encode_set_key(layer, idx, action_from_vendor(&cfg))]);
        n += 1;
    }
    assert!(n > 80, "{n}");
}

fn vendor_macro_events(cfg: &Value) -> Vec<MacroEvent> {
    cfg["macro"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let down = e["action"].as_str() == Some("down");
            match e["type"].as_str().unwrap() {
                "keyboard" => {
                    let k = e["value"].as_u64().unwrap() as u8;
                    if down {
                        MacroEvent::KeyDown(k)
                    } else {
                        MacroEvent::KeyUp(k)
                    }
                }
                "mouse_button" => {
                    let m = mouse_from_vendor(e["value"].as_i64().unwrap()).unwrap();
                    if down {
                        MacroEvent::MouseDown(m)
                    } else {
                        MacroEvent::MouseUp(m)
                    }
                }
                "delay" => MacroEvent::Delay(e["value"].as_u64().unwrap() as u16),
                t => panic!("{t}"),
            }
        })
        .collect()
}

fn macro_mode(s: &str) -> epomaker_core::keymap::MacroMode {
    use epomaker_core::keymap::MacroMode::*;
    match s {
        "on_off" => OnOff,
        "touch_repeat" => WhileHeld,
        _ => RepeatTimes,
    }
}

fn long_macro() -> Macro {
    let mut events = Vec::new();
    for i in 0..40u16 {
        let k = 4 + (i >> 1) as u8;
        events.push(if i % 2 == 1 { MacroEvent::KeyUp(k) } else { MacroEvent::KeyDown(k) });
        events.push(MacroEvent::Delay(10 + i));
    }
    Macro { repeat_count: 2, events }
}

fn macros_write(g: &Golden) {
    let mut n = 0;
    for c in g.cases() {
        if let Some(j) = c.name.strip_prefix("macro_key ") {
            let (m, original, mode) = if j == "long" {
                (long_macro(), g.keys.macros[3], epomaker_core::keymap::MacroMode::RepeatTimes)
            } else {
                let cfg: Value = serde_json::from_str(j).unwrap();
                let m = Macro {
                    repeat_count: cfg["repeatCount"].as_u64().unwrap() as u16,
                    events: vendor_macro_events(&cfg),
                };
                (m, cfg["original"].as_u64().unwrap(), macro_mode(cfg["macroType"].as_str().unwrap()))
            };
            // the vendor app allocates the first free slot: 0 on an empty keyboard
            let mut frames = macros::encode_write(0, &m.encode().unwrap()).unwrap();
            frames.push(encode_set_key(
                Layer::Profile(0),
                g.matrix_index(original, 0),
                KeyAction::Macro { index: 0, mode },
            ));
            assert_frames(c, &frames);
            n += 1;
        }
        if let Some(which) = c.name.strip_prefix("macro_encode ") {
            let original = if which == "macro1" { g.keys.macros[0] } else { g.keys.macros[1] };
            let src = g
                .cases()
                .find(|x| x.name.starts_with("macro_key ") && arg(&x.name)["original"] == json!(original))
                .unwrap();
            let cfg = arg(&src.name);
            let m =
                Macro { repeat_count: cfg["repeatCount"].as_u64().unwrap() as u16, events: vendor_macro_events(&cfg) };
            let buf = m.encode().unwrap();
            let slot = if which == "macro1" { 3 } else { 7 };
            assert_frames(c, &macros::encode_write(slot, &buf).unwrap());
            let hexbuf: String = buf.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(json!(hexbuf), c.result, "{}", c.name);
            n += 1;
        }
    }
    assert_eq!(n, 6);
}

fn macro_to_vendor(m: &Macro) -> Value {
    let ev: Vec<Value> = m
        .events
        .iter()
        .map(|e| match e {
            MacroEvent::KeyDown(k) => json!({"type": "keyboard", "action": "down", "value": k}),
            MacroEvent::KeyUp(k) => json!({"type": "keyboard", "action": "up", "value": k}),
            MacroEvent::MouseDown(b) => json!({"type": "mouse_button", "action": "down", "value": mouse_to_vendor(*b)}),
            MacroEvent::MouseUp(b) => json!({"type": "mouse_button", "action": "up", "value": mouse_to_vendor(*b)}),
            MacroEvent::MouseMove { dx, dy } => json!({"type": "mouse_move", "dx": *dx as u8, "dy": *dy as u8}),
            MacroEvent::Delay(d) => json!({"type": "delay", "value": d}),
        })
        .collect();
    json!({"macroEvents": ev, "repeatCount": m.repeat_count})
}

fn macros_read(g: &Golden) {
    let c = g.cases().find(|c| c.name == "macro_read 5").unwrap();
    assert_frames(c, &macros::encode_read(5));
    let mut buf = vec![0u8; 256];
    buf[..2].copy_from_slice(&2u16.to_le_bytes());
    buf[2..12].copy_from_slice(&[4, 0x80 | 30, 4, 0x00, 0x2c, 0x01, 0xf0, 0x80 | 5, 0xf0, 0x05]);
    assert_eq!(macro_to_vendor(&Macro::decode(&buf)), c.result);
}

fn matrix_write_full(g: &Golden) {
    let mut m = g.model.default_matrix.clone();
    m[4 * 9] = 0;
    m[4 * 9 + 2] = 5;
    for c in g.cases() {
        match c.name.as_str() {
            "matrix_write_raw" => assert_frames(c, &encode_write_matrix(Layer::Profile(0), &m)),
            "fn_matrix_write_raw" => assert_frames(c, &encode_write_matrix(Layer::Fn(0), &m)),
            _ => {}
        }
    }
}

/// Vendor `matrixToConfigValues`: only entries that differ from the factory matrix.
fn matrix_to_vendor(model: &ModelInfo, matrix: &KeyMatrix) -> Value {
    let def = &model.default_matrix;
    let mut out = Vec::new();
    for (i, s) in matrix.entries.iter().enumerate() {
        let n = [def[4 * i], def[4 * i + 1], def[4 * i + 2], def[4 * i + 3]];
        if *s == n {
            continue;
        }
        let a = def[..4 * i].as_chunks::<4>().0.iter().filter(|e| **e == n).count();
        let original = match SpecialFn::from_bytes(n) {
            Some(_) => u32::from_be_bytes(n) as u64,
            None => n[2] as u64,
        };
        let mut v = match KeyAction::from_bytes(*s) {
            KeyAction::Disabled => json!({"type": "forbidden", "original": original}),
            KeyAction::Key { modifier, key, key2 } => {
                let skey = match modifier {
                    Modifier::None => "none",
                    Modifier::Ctrl => "ctrl",
                    Modifier::Shift => "shift",
                    Modifier::Alt => "alt",
                    Modifier::Win => "win",
                };
                json!({"type": "combo", "original": original, "skey": skey, "key": key, "key2": key2})
            }
            KeyAction::Mouse(m) => {
                json!({"type": "ConfigChangeToMouseBtn", "original": original, "key": mouse_to_vendor(m)})
            }
            KeyAction::Function(f) => json!({"type": "ConfigFunction", "original": original, "key": f.vendor_name()}),
            KeyAction::Raw(b) => json!({"type": "ConfigUnknown", "original": original, "value": b}),
            KeyAction::Macro { .. } => unreachable!("not in this vector"),
        };
        if a > 0 && *s != [0, 0, 1, 0] {
            v["index"] = json!(a);
        }
        out.push(v);
    }
    Value::Array(out)
}

fn matrix_read_full(g: &Golden) {
    let mut mm = g.model.default_matrix.clone();
    let [k1, k2, k3, k4, k5, ..] = g.keys.f[..] else { panic!("meta.keys.f") };
    let find = |m: &[u8], hid: u64| m.as_chunks::<4>().0.iter().position(|e| *e == [0, 0, hid as u8, 0]).unwrap() * 4;
    let i = find(&mm, g.keys.a);
    mm[i..i + 4].copy_from_slice(&[0, 224, 6, 0]);
    let i = find(&mm, k1);
    mm[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
    let i = find(&mm, k2);
    mm[i..i + 4].copy_from_slice(&[1, 0, 240, 0]);
    let i = find(&mm, k3);
    mm[i..i + 4].copy_from_slice(&[3, 0, 205, 0]);
    let i = find(&mm, k4);
    mm[i..i + 4].copy_from_slice(&[0, 0, 1, 0]);
    let i = find(&mm, k5);
    mm[i..i + 4].copy_from_slice(&[3, 0, 0x99, 0x09]);
    let decoded = KeyMatrix::from_bytes(&mm);
    for c in g.cases() {
        let layer = match c.name.as_str() {
            "matrix_read" => Layer::Profile(0),
            "fn_matrix_read" => Layer::Fn(0),
            _ => continue,
        };
        assert_frames(c, &encode_read_matrix(layer));
        // the vendor lists changes in matrix order; compare as sets
        let mut got = matrix_to_vendor(g.model, &decoded).as_array().unwrap().clone();
        let mut want = c.result.as_array().unwrap().clone();
        let key = |v: &Value| v.to_string();
        got.sort_by_key(key);
        want.sort_by_key(key);
        assert_eq!(got, want, "{}", c.name);
    }
}

// ---------------------------------------------------------------- per-key colors

fn key_colors(g: &Golden) {
    let m = g.model;
    let idx = |hid: u64| g.matrix_index(hid, 0);
    for c in g.cases() {
        match c.name.as_str() {
            "userpic_simple" => assert_frames(c, &[settings::encode_set_key_color(1, idx(g.keys.a), Rgb(0x112233))]),
            "userpic_simple_esc" => {
                assert_frames(c, &[settings::encode_set_key_color(0, idx(g.keys.esc), Rgb(0xff8000))])
            }
            "userpic_full" => {
                let mut colors = vec![Rgb(0); m.matrix_len()];
                colors[idx(g.keys.esc) as usize] = Rgb(0xff0000);
                colors[idx(g.keys.a) as usize] = Rgb(0x00ff00);
                colors[idx(g.keys.b) as usize] = Rgb(0x0000ff);
                assert_frames(c, &settings::encode_set_all_colors(&colors, m.pics_count()));
            }
            "userpic_read" => {
                assert_frames(c, &settings::encode_read_colors(m.pics_count()));
                let mut data = vec![0u8; 64 * m.pics_count()];
                data[..3].copy_from_slice(&[0xff, 0, 0]);
                data[27..30].copy_from_slice(&[0, 0xff, 0]);
                let colors = settings::decode_colors(&data);
                // vendor: one {hid, rgb, index?} per matrix entry with a known key
                let mut out = Vec::new();
                for (i, col) in colors.iter().enumerate() {
                    let Some(e) = m.default_entry(i) else { continue };
                    let hid = if SpecialFn::from_bytes(e).is_some() || e[0] != 0 || e[1] != 0 || e[3] != 0 {
                        u32::from_be_bytes(e)
                    } else {
                        e[2] as u32
                    };
                    if hid == 0 {
                        continue;
                    }
                    let prior = out.iter().filter(|v: &&Value| v["hid"] == json!(hid)).count();
                    let mut v = json!({"hid": hid, "rgb": col.0});
                    if prior > 0 {
                        v["index"] = json!(prior);
                    }
                    out.push(v);
                }
                assert_eq!(Value::Array(out), c.result);
            }
            _ => {}
        }
    }
}

fn every_case_is_covered(g: &Golden) {
    let known = [
        "led_set",
        "led_get",
        "rate_set",
        "rate_get",
        "profile_set",
        "profile_get",
        "rate_set_profile2",
        "opts_set",
        "opts_get",
        "debounce_set",
        "debounce_get",
        "sleep_set",
        "sleep_get",
        "sleep_get_clamp",
        "battery_get",
        "battery_get_full",
        "fw_get",
        "reset",
        "autoosen_set",
        "autoosen_get",
        "key_simple",
        "key_simple_p1",
        "fn_simple",
        "macro_key",
        "macro_encode",
        "matrix_write_raw",
        "fn_matrix_write_raw",
        "matrix_read",
        "fn_matrix_read",
        "macro_read",
        "userpic_simple",
        "userpic_simple_esc",
        "userpic_full",
        "userpic_read",
    ];
    for c in g.cases() {
        let head = c.name.split(' ').next().unwrap();
        assert!(known.contains(&head), "uncovered golden case {}", c.name);
    }
    let _ = Checksum::Bit7;
}
