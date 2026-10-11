//! End-to-end tests of the high level API against the firmware emulator.

use epomaker_core::emulator::Emulator;
use epomaker_core::keymap::{KeyAction, Layer, MacroMode, Modifier, MouseAction, SpecialFn};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::profile_file::Backup;
use epomaker_core::settings::{KeyboardOptions, ReportRate, SleepTimes};
use epomaker_core::Keyboard;

fn kb() -> (Keyboard, Emulator) {
    let emu = Emulator::new(1118).unwrap();
    let kb = Keyboard::connect(Box::new(emu.clone()), false).unwrap();
    (kb, emu)
}

#[test]
fn identifies_th80() {
    let (kb, _) = kb();
    assert_eq!(kb.info.name, "yc500_th80");
    assert_eq!(kb.raw_id, 1118);
    let dongle = Keyboard::emulated(-2147484766).unwrap();
    assert_eq!(dongle.info.id, -2147484766);
    assert!(dongle.info.dongle);
}

#[test]
fn lighting_roundtrip_every_mode() {
    let (mut kb, emu) = kb();
    for caps in kb.model.light_modes.clone() {
        for option in 0..caps.options.len().max(1) as u8 {
            let e = LightEffect {
                mode: caps.mode,
                speed: if caps.mode.uses_speed() { 3 } else { 0 },
                brightness: 2,
                option,
                color: if caps.rgb { Rgb(0x123456) } else { Rgb(0) },
                dazzle: false,
            };
            kb.set_light(&e).unwrap();
            let back = kb.light().unwrap().unwrap();
            assert_eq!(back.mode, e.mode);
            assert_eq!(back.brightness, e.brightness, "{:?}", e.mode);
            if caps.mode.uses_speed() {
                assert_eq!(back.speed, 3, "{:?}", e.mode);
            }
            if !caps.options.is_empty() {
                assert_eq!(back.option, option, "{:?}", e.mode);
            }
            if caps.rgb && caps.mode != LightMode::UserPicture {
                assert_eq!(back.color, Rgb(0x123456), "{:?}", e.mode);
            }
        }
    }
    assert_eq!(emu.state.lock().unwrap().checksum_errors, 0);
}

#[test]
fn white_and_dazzle_survive() {
    let (mut kb, _) = kb();
    let e =
        LightEffect { mode: LightMode::Breath, speed: 1, brightness: 4, option: 0, color: Rgb(0xffffff), dazzle: true };
    kb.set_light(&e).unwrap();
    assert_eq!(kb.light().unwrap().unwrap(), e);
}

#[test]
fn settings_roundtrip() {
    let (mut kb, _) = kb();
    kb.set_profile(2).unwrap();
    assert_eq!(kb.profile().unwrap(), 2);
    kb.set_report_rate(1, ReportRate::Hz250).unwrap();
    assert_eq!(kb.report_rate(1).unwrap(), Some(ReportRate::Hz250));
    assert_eq!(kb.report_rate(0).unwrap(), Some(ReportRate::Hz1000));
    let o = KeyboardOptions { win_key_lock: true, wasd_arrows_swap: true, fn_key_swap: true, ..Default::default() };
    kb.set_options(1, &o).unwrap();
    assert_eq!(kb.options(1).unwrap(), o);
    kb.set_debounce(12).unwrap();
    assert_eq!(kb.debounce().unwrap(), 12);
    let s = SleepTimes { bt_light_off: 300, dongle_light_off: 600, bt_deep_sleep: 1800, dongle_deep_sleep: 3600 };
    kb.set_sleep_times(&s).unwrap();
    assert_eq!(kb.sleep_times().unwrap(), s);
    kb.set_auto_os(true).unwrap();
    assert!(kb.auto_os().unwrap());
    assert_eq!(kb.battery().unwrap().percent, 87);
    assert!(kb.firmware_version().unwrap() > 0);
}

#[test]
fn keymap_roundtrip() {
    let (mut kb, _) = kb();
    let a = kb.model.keys.iter().find(|k| k.label == "A").unwrap().matrix_index.unwrap();
    let caps = kb.model.keys.iter().find(|k| k.label.starts_with("Caps")).unwrap().matrix_index.unwrap();
    let f1 = kb.model.keys.iter().find(|k| k.label == "F1").unwrap().matrix_index.unwrap();
    let base = kb.read_layer(Layer::Profile(1)).unwrap();
    assert_eq!(base.to_bytes(), kb.model.default_matrix);

    let combo = KeyAction::Key { modifier: Modifier::Ctrl, key: 0x06, key2: 0 };
    kb.set_key(Layer::Profile(1), a, combo).unwrap();
    kb.set_key(Layer::Profile(1), caps, KeyAction::Disabled).unwrap();
    kb.set_key(Layer::Profile(1), f1, KeyAction::Mouse(MouseAction::Middle)).unwrap();
    kb.set_key(Layer::Fn(0), f1, KeyAction::Function(SpecialFn::PlayPause)).unwrap();

    let l = kb.read_layer(Layer::Profile(1)).unwrap();
    assert_eq!(l.action(a), Some(combo));
    assert_eq!(l.action(caps), Some(KeyAction::Disabled));
    assert_eq!(l.action(f1), Some(KeyAction::Mouse(MouseAction::Middle)));
    assert_eq!(kb.read_layer(Layer::Fn(0)).unwrap().action(f1), Some(KeyAction::Function(SpecialFn::PlayPause)));
    // other profiles untouched
    assert_eq!(kb.read_layer(Layer::Profile(0)).unwrap().to_bytes(), kb.model.default_matrix);

    kb.reset_layer(Layer::Profile(1)).unwrap();
    assert_eq!(kb.read_layer(Layer::Profile(1)).unwrap().to_bytes(), kb.model.default_matrix);

    // whole-layer write
    let mut l = kb.read_layer(Layer::Profile(2)).unwrap();
    l.set(a, KeyAction::key(0x05));
    kb.write_layer(Layer::Profile(2), &l).unwrap();
    assert_eq!(kb.read_layer(Layer::Profile(2)).unwrap().action(a), Some(KeyAction::key(0x05)));
}

#[test]
fn macro_roundtrip_and_binding() {
    let (mut kb, _) = kb();
    let m = Macro {
        repeat_count: 3,
        events: vec![
            MacroEvent::KeyDown(0xE1),
            MacroEvent::Delay(20),
            MacroEvent::KeyDown(0x04),
            MacroEvent::Delay(500),
            MacroEvent::KeyUp(0x04),
            MacroEvent::Delay(20),
            MacroEvent::KeyUp(0xE1),
            MacroEvent::Delay(20),
            MacroEvent::MouseDown(MouseAction::Left),
            MacroEvent::Delay(1000),
            MacroEvent::MouseUp(MouseAction::Left),
            MacroEvent::Delay(1),
        ],
    };
    kb.write_macro(4, &m).unwrap();
    assert_eq!(kb.read_macro(4).unwrap(), m);
    let k = kb.model.keys.iter().find(|k| k.label == "F5").unwrap().matrix_index.unwrap();
    kb.set_key(Layer::Profile(0), k, KeyAction::Macro { index: 4, mode: MacroMode::OnOff }).unwrap();
    assert_eq!(kb.used_macro_slots().unwrap(), vec![4]);
}

#[test]
fn key_colors_roundtrip() {
    let (mut kb, _) = kb();
    let n = kb.model.matrix_len();
    let colors: Vec<Rgb> = (0..n).map(|i| Rgb((i as u32) * 0x010203)).collect();
    kb.set_key_colors(&colors).unwrap();
    let back = kb.key_colors().unwrap();
    assert_eq!(&back[..n], &colors[..]);
    kb.set_key_color(0, 9, Rgb(0xabcdef)).unwrap();
    assert_eq!(kb.key_colors().unwrap()[9], Rgb(0xabcdef));
}

#[test]
fn backup_restore() {
    let (mut kb, _) = kb();
    let a = kb.model.keys.iter().find(|k| k.label == "A").unwrap().matrix_index.unwrap();
    kb.set_key(Layer::Profile(0), a, KeyAction::key(0x05)).unwrap();
    kb.write_macro(0, &Macro { repeat_count: 1, events: vec![MacroEvent::KeyDown(4), MacroEvent::KeyUp(4)] }).unwrap();
    kb.set_key(Layer::Profile(0), a + 1, KeyAction::Macro { index: 0, mode: MacroMode::RepeatTimes }).unwrap();
    kb.set_light(&LightEffect { mode: LightMode::Snake, option: 1, ..Default::default() }).unwrap();
    let backup = Backup::capture(&mut kb).unwrap();
    let json = serde_json::to_string(&backup).unwrap();

    let (mut other, _) = self::kb();
    let parsed: Backup = serde_json::from_str(&json).unwrap();
    parsed.restore(&mut other).unwrap();
    let again = Backup::capture(&mut other).unwrap();
    assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::to_value(&backup).unwrap());
}

#[test]
fn factory_reset() {
    let (mut kb, _) = kb();
    kb.set_key(Layer::Profile(0), 9, KeyAction::Disabled).unwrap();
    kb.factory_reset().unwrap();
    assert_eq!(kb.read_layer(Layer::Profile(0)).unwrap().to_bytes(), kb.model.default_matrix);
}
