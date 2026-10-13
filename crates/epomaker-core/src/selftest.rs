//! Hardware self-test for keyboards nobody has validated yet.
//!
//! Every step is either read-only or a small write that is undone right away:
//! the backlight brightness is changed and the original effect written back, a rarely
//! used key of the third profile is remapped and restored, and an unused macro slot is
//! written and emptied again. Writes run with verify-after-write on, and each step
//! restores the original state even when the write itself failed.
//!
//! [`SelfTestReport::to_markdown`] produces a report meant to be pasted into a GitHub issue.

use crate::keymap::{KeyAction, Layer};
use crate::lighting::{LightEffect, LightMode};
use crate::macros::{Macro, MacroEvent};
use crate::protocol::{cmd, REPORT_LEN};
use crate::settings::PROFILES;
use crate::{Error, Keyboard};

/// The individual checks, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    Identify,
    ReadSettings,
    Lighting,
    Keymap,
    Macro,
}

impl Step {
    pub const ALL: [Step; 5] = [Step::Identify, Step::ReadSettings, Step::Lighting, Step::Keymap, Step::Macro];

    /// Short English title (the GUI translates it).
    pub fn title(self) -> &'static str {
        match self {
            Step::Identify => "Identify keyboard",
            Step::ReadSettings => "Read every setting",
            Step::Lighting => "Change brightness and restore",
            Step::Keymap => "Remap a key and restore",
            Step::Macro => "Write a macro and erase it",
        }
    }

    /// Whether the step writes to the keyboard (always undone).
    pub fn writes(self) -> bool {
        matches!(self, Step::Lighting | Step::Keymap | Step::Macro)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub step: Step,
    pub outcome: Outcome,
    /// What was read or written (English, for the report).
    pub details: Vec<String>,
    /// Why the step failed or was skipped.
    pub error: Option<String>,
    /// `Some(true)` when something was written and the original state is back,
    /// `Some(false)` when restoring failed (see `error`), `None` when nothing was written.
    pub restored: Option<bool>,
}

impl StepResult {
    fn new(step: Step) -> Self {
        StepResult { step, outcome: Outcome::Passed, details: vec![], error: None, restored: None }
    }

    fn fail(&mut self, e: impl std::fmt::Display) {
        self.outcome = Outcome::Failed;
        let msg = e.to_string();
        self.error = Some(match self.error.take() {
            Some(prev) => format!("{prev}; {msg}"),
            None => msg,
        });
    }

    fn skip(mut self, why: &str) -> Self {
        self.outcome = Outcome::Skipped;
        self.error = Some(why.to_string());
        self
    }
}

/// What the report says about the device, gathered with read-only queries.
#[derive(Clone, Debug)]
pub struct DeviceSummary {
    pub display_name: String,
    pub model: String,
    pub database_id: i64,
    pub raw_id: u32,
    pub firmware: Option<u16>,
    /// Transport description: `vid:pid ifN path` for USB.
    pub connection: String,
    pub emulated: bool,
    /// Every HID interface of the vendor (empty for the emulator).
    pub interfaces: Vec<String>,
    /// Raw answers to the `epomaker probe` queries.
    pub probe: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SelfTestReport {
    pub device: DeviceSummary,
    pub steps: Vec<StepResult>,
}

/// Collects the device information of the report (read-only).
pub fn summary(kb: &mut Keyboard) -> DeviceSummary {
    #[cfg(feature = "hid")]
    let interfaces = if kb.is_emulated() {
        vec![]
    } else {
        crate::probe::vendor_interfaces().unwrap_or_else(|e| vec![format!("   cannot enumerate: {e}")])
    };
    #[cfg(not(feature = "hid"))]
    let interfaces = vec![];
    DeviceSummary {
        display_name: kb.info.display_name.clone(),
        model: kb.info.name.clone(),
        database_id: kb.info.id,
        raw_id: kb.raw_id,
        firmware: kb.firmware_version().ok(),
        connection: kb.describe(),
        emulated: kb.is_emulated(),
        interfaces,
        probe: crate::probe::query_keyboard(kb),
    }
}

/// Runs every step.
pub fn run(kb: &mut Keyboard) -> SelfTestReport {
    let device = summary(kb);
    let steps = Step::ALL.iter().map(|s| run_step(kb, *s)).collect();
    SelfTestReport { device, steps }
}

/// Runs one step with verify-after-write on (the previous setting is kept afterwards).
pub fn run_step(kb: &mut Keyboard, step: Step) -> StepResult {
    let verify = kb.verify();
    kb.set_verify(true);
    let r = match step {
        Step::Identify => identify(kb),
        Step::ReadSettings => read_settings(kb),
        Step::Lighting => lighting(kb),
        Step::Keymap => keymap(kb),
        Step::Macro => macro_slot(kb),
    };
    kb.set_verify(verify);
    let mut r = r;
    if step.writes() && r.outcome == Outcome::Failed {
        still_answers(kb, &mut r);
    }
    r
}

/// After a failed write step: does the keyboard still answer at all? (Tells a rejected
/// command apart from a keyboard that stopped responding.)
fn still_answers(kb: &mut Keyboard, r: &mut StepResult) {
    let started = std::time::Instant::now();
    match kb.raw_query(cmd::GET_DEVICE_ID, 0, 0) {
        Ok(_) => r
            .details
            .push(format!("after the failure the keyboard answers again ({} ms later)", started.elapsed().as_millis())),
        Err(e) => r.details.push(format!("after the failure the keyboard does not answer: {e}")),
    }
}

/// Gives the firmware time to store the test value before it is overwritten again.
fn settle(kb: &Keyboard) {
    crate::transport::sleep(kb.pacing().after_big_command);
}

fn identify(kb: &mut Keyboard) -> StepResult {
    let mut r = StepResult::new(Step::Identify);
    r.details.push(format!("{} ({}, database id {})", kb.info.display_name, kb.info.name, kb.info.id));
    match kb.raw_query(cmd::GET_DEVICE_ID, 0, 0) {
        Ok(resp) => {
            let id = crate::settings::decode_device_id(&resp);
            r.details.push(format!("raw device id {id} ({id:#010x})"));
            if id != kb.raw_id {
                r.fail(format!("device id changed: {} at connection, {id} now", kb.raw_id));
            }
        }
        Err(e) => r.fail(format!("device id: {e}")),
    }
    match kb.firmware_version() {
        Ok(v) => r.details.push(format!("firmware {v:#06x}")),
        Err(e) => r.fail(format!("firmware: {e}")),
    }
    match kb.battery() {
        Ok(b) => r.details.push(format!("battery {}% ({:?})", b.percent, b.state)),
        Err(e) => r.fail(format!("battery: {e}")),
    }
    match kb.profile() {
        Ok(p) if p < PROFILES => r.details.push(format!("active profile {} (index {p})", p + 1)),
        Ok(p) => r.fail(format!("active profile index {p} is out of range")),
        Err(e) => r.fail(format!("profile: {e}")),
    }
    r
}

fn read_settings(kb: &mut Keyboard) -> StepResult {
    let mut r = StepResult::new(Step::ReadSettings);
    match kb.light_raw() {
        Ok(raw) => {
            let mut resp = [0u8; REPORT_LEN];
            resp[1..8].copy_from_slice(&raw);
            match LightEffect::decode(&resp) {
                Some(e) => {
                    r.details.push(format!("lighting {:?}, brightness {}, speed {}", e.mode, e.brightness, e.speed))
                }
                None => r.fail(format!("unknown lighting effect code {}", raw[0])),
            }
        }
        Err(e) => r.fail(format!("lighting: {e}")),
    }
    for p in 0..PROFILES {
        match kb.report_rate(p) {
            Ok(Some(rate)) => r.details.push(format!("profile {}: {} Hz", p + 1, rate.hz())),
            Ok(None) => r.fail(format!("profile {}: unknown report rate code", p + 1)),
            Err(e) => r.fail(format!("report rate: {e}")),
        }
        if let Err(e) = kb.options(p) {
            r.fail(format!("options: {e}"));
        }
    }
    match kb.debounce() {
        Ok(d) => r.details.push(format!("debounce {d} ms")),
        Err(e) => r.fail(format!("debounce: {e}")),
    }
    if let Err(e) = kb.sleep_times() {
        r.fail(format!("sleep timers: {e}"));
    }
    if let Err(e) = kb.auto_os() {
        r.fail(format!("automatic OS detection: {e}"));
    }
    for layer in kb.layers() {
        match kb.read_layer(layer) {
            Ok(m) => {
                let changed = (0..kb.model.matrix_len())
                    .filter(|i| m.entries.get(*i).copied() != kb.model.default_entry(*i))
                    .count();
                let Layer::Profile(p) = layer else { continue };
                r.details.push(format!("profile {}: {changed} keys differ from factory", p + 1));
            }
            Err(e) => r.fail(format!("{layer:?}: {e}")),
        }
    }
    match kb.key_colors() {
        Ok(c) => r.details.push(format!("{} per-key colors", c.len())),
        Err(e) => r.fail(format!("per-key colors: {e}")),
    }
    match kb.used_macro_slots() {
        Ok(s) => r.details.push(format!("macro slots bound to keys: {s:?}")),
        Err(e) => r.fail(format!("macros: {e}")),
    }
    r
}

/// Runs `restore`, retrying once, and records the outcome.
fn restore(r: &mut StepResult, original: &str, mut f: impl FnMut() -> Result<(), Error>) {
    let res = f().or_else(|e| {
        log::warn!("self-test restore failed ({e}), retrying");
        f()
    });
    match res {
        Ok(()) => r.restored = Some(true),
        Err(e) => {
            r.restored = Some(false);
            r.fail(format!("RESTORE FAILED, the original value was {original}: {e}"));
        }
    }
}

fn lighting(kb: &mut Keyboard) -> StepResult {
    let mut r = StepResult::new(Step::Lighting);
    let raw = match kb.light_raw() {
        Ok(raw) => raw,
        Err(e) => {
            r.fail(format!("reading the lighting: {e}"));
            return r;
        }
    };
    let mut resp = [0u8; REPORT_LEN];
    resp[1..8].copy_from_slice(&raw);
    let Some(original) = LightEffect::decode(&resp) else {
        return r.skip("the current effect code is unknown; not touching the lighting");
    };
    let has_brightness = !matches!(original.mode, LightMode::Off | LightMode::UserColor | LightMode::ScreenColor);
    let test = match kb.model.caps(original.mode).and_then(|c| c.max_brightness) {
        Some(max) if has_brightness && max > 0 => {
            let b = if original.brightness > 0 { original.brightness.min(max) - 1 } else { 1 };
            LightEffect { brightness: b, ..original }
        }
        _ => match kb.model.caps(LightMode::AlwaysOn) {
            Some(_) => LightEffect { mode: LightMode::AlwaysOn, brightness: 1, ..LightEffect::default() },
            None => return r.skip("no effect with adjustable brightness"),
        },
    };
    let hex = raw.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    r.details.push(format!("original: {:?} brightness {} [{hex}]", original.mode, original.brightness));
    r.details.push(format!("test: {:?} brightness {}", test.mode, test.brightness));
    if let Err(e) = kb.set_light(&test) {
        r.fail(format!("test write: {e}"));
    }
    settle(kb);
    restore(&mut r, &format!("[{hex}]"), || kb.set_light_raw(&raw));
    r
}

/// A key that is rarely used, preferring keys absent from compact layouts.
fn rare_key(kb: &Keyboard) -> Option<usize> {
    // Pause, Scroll Lock, Print Screen, Insert, right Ctrl, F12, Num Lock, keypad "."
    const PREFERRED: [u8; 8] = [0x48, 0x47, 0x46, 0x49, 0xE4, 0x45, 0x53, 0x63];
    let plain = |i: usize| kb.model.default_entry(i).filter(|e| e[0] == 0 && e[1] == 0 && e[2] != 0 && e[3] == 0);
    let on_layout = |i: usize| kb.model.key_for_matrix_index(i).is_some();
    PREFERRED
        .iter()
        .find_map(|code| (0..kb.model.matrix_len()).find(|i| on_layout(*i) && plain(*i).map(|e| e[2]) == Some(*code)))
        .or_else(|| (0..kb.model.matrix_len()).rev().find(|i| on_layout(*i) && plain(*i).is_some()))
}

fn keymap(kb: &mut Keyboard) -> StepResult {
    let mut r = StepResult::new(Step::Keymap);
    let layer = Layer::Profile(PROFILES - 1);
    let Some(index) = rare_key(kb) else { return r.skip("no suitable key on this layout") };
    let label = kb.model.key_for_matrix_index(index).map(|k| k.label.replace('\n', " ")).unwrap_or_default();
    let original = match kb.read_layer(layer).map(|m| m.entries.get(index).copied()) {
        Ok(Some(e)) => e,
        Ok(None) => {
            r.fail(format!("layer too short for key #{index}"));
            return r;
        }
        Err(e) => {
            r.fail(format!("reading profile {}: {e}", PROFILES));
            return r;
        }
    };
    // F13, or F14 if the key already sends F13
    let code = if original == KeyAction::key(0x68).to_bytes() { 0x69 } else { 0x68 };
    let test = KeyAction::key(code);
    let hex = format!("{:02x}{:02x}{:02x}{:02x}", original[0], original[1], original[2], original[3]);
    r.details.push(format!("key {label} (#{index}) on profile {}: {hex}", PROFILES));
    if let Ok(active) = kb.profile() {
        if active != PROFILES - 1 {
            r.details.push(format!("profile {} is active: profile {PROFILES} is activated for the writes", active + 1));
        }
    }
    r.details.push(format!("test: {}", crate::keycodes::display(code)));
    if let Err(e) = kb.set_key(layer, index, test) {
        r.fail(format!("test write: {e}"));
    }
    settle(kb);
    restore(&mut r, &hex, || kb.set_key(layer, index, KeyAction::Raw(original)));
    r
}

fn macro_slot(kb: &mut Keyboard) -> StepResult {
    let mut r = StepResult::new(Step::Macro);
    let used = match kb.used_macro_slots() {
        Ok(u) => u,
        Err(e) => {
            r.fail(format!("reading which macros keys use: {e}"));
            return r;
        }
    };
    // the highest slot no key refers to, preferring an empty one (looks at a few at most)
    let free: Vec<u8> = (0..kb.model.macro_slots).rev().filter(|s| !used.contains(s)).collect();
    let mut chosen: Option<(u8, Vec<u8>)> = None;
    for &slot in free.iter().take(4) {
        match kb.read_macro_raw(slot) {
            Ok(raw) => {
                let empty = raw.iter().all(|b| *b == 0);
                if chosen.is_none() || empty {
                    chosen = Some((slot, raw));
                }
                if empty {
                    break;
                }
            }
            Err(e) => {
                r.fail(format!("reading macro slot {slot}: {e}"));
                return r;
            }
        }
    }
    let Some((slot, original)) = chosen else { return r.skip("every macro slot is bound to a key") };
    let empty = original.iter().all(|b| *b == 0);
    r.details.push(format!("slot {slot} ({})", if empty { "empty" } else { "not bound to any key, not empty" }));
    let test = Macro {
        repeat_count: 1,
        events: vec![MacroEvent::KeyDown(0x68), MacroEvent::Delay(10), MacroEvent::KeyUp(0x68), MacroEvent::Delay(10)],
    };
    match kb.write_macro(slot, &test) {
        Ok(()) => match kb.read_macro(slot) {
            Ok(back) if back == test => r.details.push("test macro read back intact".into()),
            Ok(back) => r.fail(format!("macro decoded differently: {back:?}")),
            Err(e) => r.fail(format!("reading the test macro back: {e}")),
        },
        Err(e) => r.fail(format!("test write: {e}")),
    }
    settle(kb);
    let mut data = [0u8; crate::macros::MACRO_SLOT_LEN];
    let n = original.len().min(data.len());
    data[..n].copy_from_slice(&original[..n]);
    let desc = if empty { "an empty slot".to_string() } else { "the previous contents".to_string() };
    restore(&mut r, &desc, || if empty { kb.clear_macro(slot) } else { kb.write_macro_raw(slot, &data) });
    r
}

impl SelfTestReport {
    pub fn passed(&self) -> bool {
        self.steps.iter().all(|s| s.outcome != Outcome::Failed)
    }

    /// Markdown for a GitHub issue. `app` is e.g. `"epomaker 0.2.0"`.
    pub fn to_markdown(&self, app: &str) -> String {
        markdown(&self.device, &self.steps, app)
    }
}

/// Markdown report for a possibly partial run (the GUI runs steps one by one).
pub fn markdown(device: &DeviceSummary, steps: &[StepResult], app: &str) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let d = device;
    let _ = writeln!(s, "### Hardware self-test: {}\n", d.display_name);
    let _ = writeln!(s, "- App: {app} ({} {})", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(s, "- Model: {} (`{}`, database id {})", d.display_name, d.model, d.database_id);
    if d.model.ends_with("_uk") {
        let _ = writeln!(s, "- Layout: ISO (every ISO version reports the id of the UK one)");
    }
    let _ = writeln!(s, "- Raw device id: {} (`{:#010x}`)", d.raw_id, d.raw_id);
    let fw = d.firmware.map(|f| format!("`{f:#06x}`")).unwrap_or_else(|| "unknown".into());
    let _ = writeln!(s, "- Firmware: {fw}");
    let demo = if d.emulated { " (emulated, no hardware)" } else { "" };
    let _ = writeln!(s, "- Connection: `{}`{demo}", d.connection);
    if !steps.is_empty() {
        let _ = writeln!(s, "\n| Step | Result | Details |\n|---|---|---|");
        for r in steps {
            let result = match (r.outcome, r.restored) {
                (Outcome::Passed, _) => "passed",
                (Outcome::Skipped, _) => "skipped",
                (Outcome::Failed, Some(false)) => "**FAILED (not restored)**",
                (Outcome::Failed, _) => "**FAILED**",
            };
            let mut details = r.details.join("; ");
            if let Some(e) = &r.error {
                if !details.is_empty() {
                    details += "; ";
                }
                details += e;
            }
            if r.restored == Some(true) {
                details += "; restored";
            }
            let _ =
                writeln!(s, "| {} | {result} | {} |", r.step.title(), details.replace('|', "\\|").replace('\n', " "));
        }
    }
    let _ = writeln!(s, "\n<details><summary>Probe output</summary>\n\n```text");
    for l in d.interfaces.iter().chain(&d.probe) {
        let _ = writeln!(s, "{l}");
    }
    let _ = writeln!(s, "```\n</details>");
    s
}
