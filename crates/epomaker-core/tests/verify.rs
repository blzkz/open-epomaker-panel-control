//! Verify-after-write against the emulator, with injected firmware faults.

use epomaker_core::emulator::{Emulator, Fault};
use epomaker_core::keymap::{KeyAction, Layer};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::profile_file::Backup;
use epomaker_core::protocol::cmd;
use epomaker_core::settings::{KeyboardOptions, ReportRate, SleepTimes};
use epomaker_core::{Error, Keyboard};

fn kb() -> (Keyboard, Emulator) {
    let emu = Emulator::new(1118).unwrap();
    let mut kb = Keyboard::connect(Box::new(emu.clone()), false).unwrap();
    kb.set_verify(true);
    (kb, emu)
}

fn is_mismatch(r: Result<(), Error>) -> bool {
    matches!(r, Err(Error::VerifyMismatch { .. }))
}

fn sample_macro() -> Macro {
    Macro { repeat_count: 2, events: vec![MacroEvent::KeyDown(4), MacroEvent::Delay(30), MacroEvent::KeyUp(4)] }
}

#[test]
fn verify_is_off_by_default_and_silent_on_good_firmware() {
    let emu = Emulator::new(1118).unwrap();
    let plain = Keyboard::connect(Box::new(emu), false).unwrap();
    assert!(!plain.verify());

    let (mut kb, _) = kb();
    let a = kb.model.keys.iter().find(|k| k.label == "A").unwrap().matrix_index.unwrap();
    kb.set_light(&LightEffect { mode: LightMode::Wave, option: 2, color: Rgb(0x123456), ..Default::default() })
        .unwrap();
    kb.set_light(&LightEffect { mode: LightMode::AlwaysOn, color: Rgb(0xffffff), ..Default::default() }).unwrap();
    kb.set_profile(1).unwrap();
    kb.set_report_rate(2, ReportRate::Hz500).unwrap();
    kb.set_options(1, &KeyboardOptions { mac_mode: true, power_save: true, ..Default::default() }).unwrap();
    kb.set_debounce(9).unwrap();
    kb.set_sleep_times(&SleepTimes { bt_light_off: 60, ..Default::default() }).unwrap();
    kb.set_auto_os(true).unwrap();
    kb.set_key(Layer::Profile(1), a, KeyAction::key(0x05)).unwrap();
    kb.set_key(Layer::Fn(0), a, KeyAction::Disabled).unwrap();
    let mut l = kb.read_layer(Layer::Profile(2)).unwrap();
    l.set(a, KeyAction::key(0x07));
    kb.write_layer(Layer::Profile(2), &l).unwrap();
    kb.reset_layer(Layer::Profile(1)).unwrap();
    kb.write_macro(3, &sample_macro()).unwrap();
    kb.clear_macro(3).unwrap();
    assert_eq!(kb.read_macro(3).unwrap(), Macro::default());
    kb.set_key_color(0, 5, Rgb(0x00ff00)).unwrap();
    kb.set_key_color(1, 5, Rgb(0x00ff00)).unwrap();
    kb.set_key_colors(&vec![Rgb(0x0000ff); kb.model.matrix_len()]).unwrap();

    // a full restore runs every setter with verification
    let backup = Backup::capture(&mut kb).unwrap();
    backup.restore(&mut kb).unwrap();
}

#[test]
fn dropped_light_write_is_detected() {
    let (mut kb, emu) = kb();
    emu.inject(Fault::DropWrites(cmd::SET_LED));
    let r = kb.set_light(&LightEffect { mode: LightMode::Breath, brightness: 1, ..Default::default() });
    let Err(Error::VerifyMismatch { what, expected, got }) = r else { panic!("{r:?}") };
    assert_eq!(what, "lighting");
    assert!(expected.contains("Breath"), "{expected}");
    assert!(got.contains("Wave"), "{got}");
}

#[test]
fn verify_off_ignores_faults() {
    let (mut kb, emu) = kb();
    kb.set_verify(false);
    emu.inject(Fault::DropWrites(cmd::SET_LED));
    emu.inject(Fault::DropWrites(cmd::SET_KEY_SIMPLE));
    kb.set_light(&LightEffect::off()).unwrap();
    kb.set_key(Layer::Profile(0), 3, KeyAction::Disabled).unwrap();
}

#[test]
fn corrupted_key_write_is_detected() {
    let (mut kb, emu) = kb();
    // byte 10 is the key code of the entry
    emu.inject_times(Fault::CorruptWrite { command: cmd::SET_KEY_SIMPLE, byte: 10, xor: 0x01 }, 1);
    let r = kb.set_key(Layer::Profile(0), 9, KeyAction::key(0x04));
    let Err(Error::VerifyMismatch { what, expected, got }) = r else { panic!("{r:?}") };
    assert!(what.contains("#9"), "{what}");
    assert_eq!(expected, "00000400");
    assert_eq!(got, "00000500");
    // the fault was one-shot
    kb.set_key(Layer::Profile(0), 9, KeyAction::key(0x04)).unwrap();
}

#[test]
fn fn_layer_and_reset_are_verified() {
    let (mut kb, emu) = kb();
    emu.inject(Fault::DropWrites(cmd::SET_FN_SIMPLE));
    assert!(is_mismatch(kb.set_key(Layer::Fn(0), 2, KeyAction::key(0x04))));
    emu.clear_faults();
    kb.set_key(Layer::Profile(0), 2, KeyAction::Disabled).unwrap();
    emu.inject(Fault::DropWrites(cmd::SET_KEY_SIMPLE));
    assert!(is_mismatch(kb.reset_layer(Layer::Profile(0))));
}

#[test]
fn corrupted_layer_write_is_detected() {
    let (mut kb, emu) = kb();
    let l = kb.read_layer(Layer::Profile(1)).unwrap();
    // first byte of the third entry carried by the first two packets
    emu.inject_times(Fault::CorruptWrite { command: cmd::SET_KEY_MATRIX, byte: 8 + 4 * 2, xor: 0xff }, 2);
    let r = kb.write_layer(Layer::Profile(1), &l);
    let Err(Error::VerifyMismatch { what, .. }) = r else { panic!("{r:?}") };
    assert!(what.contains("key #2"), "{what}");
}

#[test]
fn bad_readback_of_settings_is_detected() {
    let (mut kb, emu) = kb();
    emu.inject_times(Fault::CorruptResponse { command: cmd::GET_REPORT_RATE, byte: 2, xor: 0x03 }, 1);
    assert!(is_mismatch(kb.set_report_rate(0, ReportRate::Hz1000)));
    emu.inject_times(Fault::CorruptResponse { command: cmd::GET_KB_OPTION, byte: 2, xor: 0x01 }, 1);
    assert!(is_mismatch(kb.set_options(0, &KeyboardOptions::default())));
    emu.inject(Fault::DropWrites(cmd::SET_DEBOUNCE));
    assert!(is_mismatch(kb.set_debounce(20)));
    emu.inject(Fault::DropWrites(cmd::SET_PROFILE));
    assert!(is_mismatch(kb.set_profile(2)));
    emu.inject(Fault::DropWrites(cmd::SET_AUTO_OS));
    assert!(is_mismatch(kb.set_auto_os(true)));
    emu.inject_times(Fault::CorruptResponse { command: cmd::GET_SLEEP_TIME, byte: 4, xor: 0x10 }, 1);
    let r = kb.set_sleep_times(&SleepTimes::default());
    let Err(Error::VerifyMismatch { what, .. }) = r else { panic!("{r:?}") };
    assert!(what.contains("byte 3"), "{what}");
}

#[test]
fn macro_and_color_faults_are_detected() {
    let (mut kb, emu) = kb();
    emu.inject_times(Fault::CorruptWrite { command: cmd::SET_MACRO_SIMPLE, byte: 12, xor: 0x80 }, 1);
    assert!(is_mismatch(kb.write_macro(7, &sample_macro())));
    kb.write_macro(7, &sample_macro()).unwrap();
    emu.inject(Fault::DropWrites(cmd::SET_MACRO_SIMPLE));
    assert!(is_mismatch(kb.clear_macro(7)));
    emu.clear_faults();

    emu.inject(Fault::DropWrites(cmd::SET_USER_PIC_SIMPLE));
    assert!(is_mismatch(kb.set_key_color(0, 4, Rgb(0x010203))));
    // slot 1 cannot be read back, so it is not verified
    kb.set_key_color(1, 4, Rgb(0x010203)).unwrap();
    emu.inject_times(Fault::CorruptResponse { command: cmd::GET_USER_PIC, byte: 0, xor: 1 }, 1);
    assert!(is_mismatch(kb.set_key_colors(&vec![Rgb(0); kb.model.matrix_len()])));
}

#[test]
fn injected_io_error_is_reported() {
    let (mut kb, emu) = kb();
    emu.inject_times(Fault::Fail(cmd::GET_LED), 1);
    assert!(matches!(kb.light(), Err(Error::Hid(_))));
    assert!(kb.light().is_ok());
}

#[test]
fn raw_light_roundtrip() {
    let (mut kb, _) = kb();
    let raw = kb.light_raw().unwrap();
    kb.set_light(&LightEffect::off()).unwrap();
    kb.set_light_raw(&raw).unwrap();
    assert_eq!(kb.light_raw().unwrap(), raw);
}

#[test]
fn failed_transfers_are_retried() {
    use epomaker_core::transport::Pacing;
    use std::time::Duration;
    const THREE: [Duration; 3] = [Duration::ZERO; 3];
    let (mut kb, emu) = kb();
    kb.set_pacing(Pacing { retry_backoff: &THREE, ..Pacing::none() });
    // the firmware stalls a few requests (os error 31 on Windows) and then recovers
    emu.inject_times(Fault::Fail(cmd::SET_LED), 3);
    kb.set_light(&LightEffect { mode: LightMode::Breath, ..LightEffect::default() }).unwrap();
    assert_eq!(kb.light().unwrap().unwrap().mode, LightMode::Breath);
    emu.inject_times(Fault::Fail(cmd::GET_LED), 2);
    assert!(kb.light().is_ok());
    // more failures than retries: the error comes through
    emu.inject_times(Fault::Fail(cmd::GET_LED), 4);
    assert!(matches!(kb.light(), Err(Error::Hid(_))));
    emu.clear_faults();
    // continuous frames are never retried
    kb.set_light(&LightEffect { mode: LightMode::ScreenColor, ..LightEffect::default() }).unwrap();
    emu.inject_times(Fault::Fail(epomaker_core::live::SET_SCREEN_COLOR), 1);
    assert!(kb.send_screen_color(Rgb(0x102030)).is_err());
    assert!(kb.send_screen_color(Rgb(0x102030)).is_ok());
}
