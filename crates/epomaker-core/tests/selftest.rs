//! The hardware self-test must leave the keyboard exactly as it found it.

use epomaker_core::devices::Database;
use epomaker_core::emulator::{Emulator, EmulatorState, Fault};
use epomaker_core::keymap::{KeyAction, Layer, MacroMode};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::protocol::cmd;
use epomaker_core::selftest::{self, Outcome, Step};
use epomaker_core::Keyboard;

fn kb(id: i64) -> (Keyboard, Emulator) {
    let emu = Emulator::new(id).unwrap();
    let kb = Keyboard::connect(Box::new(emu.clone()), id < 0).unwrap();
    (kb, emu)
}

/// The parts of the firmware state a self-test could touch:
/// lighting, layers, Fn layers, macros, per-key colors, active profile.
type State = (Vec<u8>, Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<[u8; 256]>, Vec<Vec<u8>>, u8);

fn state(emu: &Emulator) -> State {
    let s: EmulatorState = emu.state.lock().unwrap().clone();
    (s.led.to_vec(), s.matrices, s.fn_matrices, s.macros, s.pics, s.profile)
}

fn outcome(r: &selftest::SelfTestReport, step: Step) -> &selftest::StepResult {
    r.steps.iter().find(|s| s.step == step).unwrap()
}

#[test]
fn passes_and_restores_on_every_model() {
    let mut ids: Vec<i64> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for d in Database::get().devices.iter().filter(|d| !d.dongle) {
        if !seen.contains(&d.name.as_str()) {
            seen.push(&d.name);
            ids.push(d.id);
        }
    }
    assert!(ids.len() >= 5);
    for id in ids {
        let (mut kb, emu) = kb(id);
        let before = state(&emu);
        let report = selftest::run(&mut kb);
        for s in &report.steps {
            assert_eq!(s.outcome, Outcome::Passed, "model {id}: {s:?}");
            assert_eq!(s.restored.is_some(), s.step.writes(), "model {id}: {s:?}");
            assert_ne!(s.restored, Some(false));
        }
        assert!(report.passed());
        assert_eq!(state(&emu), before, "model {id}: state changed");
        assert!(!kb.verify(), "verify flag must be restored");
        assert_eq!(emu.state.lock().unwrap().checksum_errors, 0);
    }
}

#[test]
fn restores_non_default_state() {
    let (mut kb, emu) = kb(1118);
    // every lighting mode, including ones without brightness
    for mode in [LightMode::Off, LightMode::UserPicture, LightMode::Neon, LightMode::AlwaysOn] {
        kb.set_light(&LightEffect { mode, brightness: 3, color: Rgb(0x00ff00), ..Default::default() }).unwrap();
        let before = state(&emu);
        let r = selftest::run_step(&mut kb, Step::Lighting);
        assert_eq!(r.outcome, Outcome::Passed, "{mode:?}: {r:?}");
        assert_eq!(state(&emu), before, "{mode:?}");
    }
    // every macro slot but one bound to keys, the free one with old contents
    let free = kb.model.macro_slots - 1;
    let old = Macro { repeat_count: 5, events: vec![MacroEvent::KeyDown(0x1e), MacroEvent::KeyUp(0x1e)] };
    kb.write_macro(free, &old).unwrap();
    for slot in 0..free {
        kb.set_key(Layer::Profile(0), slot as usize, KeyAction::Macro { index: slot, mode: MacroMode::OnOff }).unwrap();
    }
    let before = state(&emu);
    let r = selftest::run_step(&mut kb, Step::Macro);
    assert_eq!(r.outcome, Outcome::Passed, "{r:?}");
    assert!(r.details[0].contains(&format!("slot {free}")), "{r:?}");
    assert_eq!(state(&emu), before);
    assert_eq!(kb.read_macro(free).unwrap(), old);
}

#[test]
fn macro_step_skips_when_every_slot_is_bound() {
    let (mut kb, emu) = kb(1118);
    for slot in 0..kb.model.macro_slots {
        let layer = if slot < 64 { Layer::Profile(0) } else { Layer::Profile(1) };
        kb.set_key(layer, slot as usize % 64, KeyAction::Macro { index: slot, mode: MacroMode::OnOff }).unwrap();
    }
    let before = state(&emu);
    let r = selftest::run_step(&mut kb, Step::Macro);
    assert_eq!(r.outcome, Outcome::Skipped);
    assert_eq!(r.restored, None);
    assert_eq!(state(&emu), before);
}

#[test]
fn failed_write_is_still_restored() {
    let (mut kb, emu) = kb(1118);
    let before = state(&emu);
    // the test remap lands corrupted once; the restore goes through
    emu.inject_times(Fault::CorruptWrite { command: cmd::SET_KEY_SIMPLE, byte: 10, xor: 0x40 }, 1);
    // the brightness change fails with an I/O error
    emu.inject_times(Fault::Fail(cmd::SET_LED), 1);
    // the test macro is silently dropped; the empty slot stays empty
    emu.inject_times(Fault::DropWrites(cmd::SET_MACRO_SIMPLE), 1);
    let report = selftest::run(&mut kb);
    for step in [Step::Keymap, Step::Lighting, Step::Macro] {
        let r = outcome(&report, step);
        assert_eq!(r.outcome, Outcome::Failed, "{r:?}");
        assert_eq!(r.restored, Some(true), "{r:?}");
    }
    assert!(outcome(&report, Step::Keymap).error.as_deref().unwrap().contains("read-back mismatch"));
    assert!(!report.passed());
    assert_eq!(state(&emu), before);
}

#[test]
fn restore_is_retried_once() {
    let (mut kb, emu) = kb(1118);
    let before = state(&emu);
    // test write and first restore attempt fail
    emu.inject_times(Fault::Fail(cmd::SET_LED), 2);
    let r = selftest::run_step(&mut kb, Step::Lighting);
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.restored, Some(true));
    assert_eq!(state(&emu), before);
}

#[test]
fn restore_failure_is_reported() {
    let (mut kb, emu) = kb(1118);
    emu.inject(Fault::CorruptWrite { command: cmd::SET_KEY_SIMPLE, byte: 10, xor: 0x40 });
    let r = selftest::run_step(&mut kb, Step::Keymap);
    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.restored, Some(false));
    assert!(r.error.as_deref().unwrap().contains("RESTORE FAILED"), "{r:?}");
}

#[test]
fn read_only_steps_do_not_write() {
    let (mut kb, emu) = kb(1118);
    let n = emu.state.lock().unwrap().log.len();
    for step in [Step::Identify, Step::ReadSettings] {
        assert_eq!(selftest::run_step(&mut kb, step).outcome, Outcome::Passed);
    }
    let _ = selftest::summary(&mut kb);
    let log = emu.state.lock().unwrap().log.clone();
    assert!(log[n..].iter().all(|r| r[0] & 0x80 != 0), "only GET commands expected");
}

#[test]
fn unreadable_settings_fail_the_read_step() {
    let (mut kb, emu) = kb(1118);
    emu.inject(Fault::CorruptResponse { command: cmd::GET_REPORT_RATE, byte: 2, xor: 0x70 });
    let r = selftest::run_step(&mut kb, Step::ReadSettings);
    assert_eq!(r.outcome, Outcome::Failed);
    assert!(r.error.unwrap().contains("unknown report rate"));
}

#[test]
fn markdown_report() {
    let (mut kb, _) = kb(1118);
    let report = selftest::run(&mut kb);
    let md = report.to_markdown("epomaker 9.9.9");
    for needle in [
        "epomaker 9.9.9",
        "yc500_th80",
        "1118",
        "0x0000045e",
        "Firmware: `0x0105`",
        "emulated",
        "| Remap a key",
        "device id",
    ] {
        assert!(md.contains(needle), "missing {needle}:\n{md}");
    }
    assert_eq!(md.matches("| passed |").count(), Step::ALL.len());
}
