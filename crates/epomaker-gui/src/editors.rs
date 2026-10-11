//! Editors for key actions and macros.

use crate::hidmap::hid_from_key;
use crate::i18n::{tr, Lang};
use eframe::egui::{self, ComboBox, DragValue, Event};
use epomaker_core::keycodes;
use epomaker_core::keymap::{KeyAction, MacroMode, Modifier, MouseAction, SpecialFn};
use epomaker_core::macros::{Macro, MacroEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Default,
    Disabled,
    Key,
    Mouse,
    Function,
    Macro,
    Raw,
}

impl Kind {
    const ALL: [Kind; 7] =
        [Kind::Default, Kind::Disabled, Kind::Key, Kind::Mouse, Kind::Function, Kind::Macro, Kind::Raw];
    fn label(self) -> &'static str {
        match self {
            Kind::Default => "Factory default",
            Kind::Disabled => "Disabled",
            Kind::Key => "Key",
            Kind::Mouse => "Mouse",
            Kind::Function => "Function",
            Kind::Macro => "Macro",
            Kind::Raw => "Raw",
        }
    }
}

/// Which key field is waiting for a physical key press.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    None,
    Key,
    Key2,
}

pub struct ActionEditor {
    pub kind: Kind,
    modifier: Modifier,
    key: u8,
    key2: u8,
    mouse: MouseAction,
    func: SpecialFn,
    pub slot: u8,
    mode: MacroMode,
    raw: String,
    capture: Capture,
}

impl ActionEditor {
    pub fn new(action: KeyAction, default: [u8; 4]) -> ActionEditor {
        let mut e = ActionEditor {
            kind: Kind::Default,
            modifier: Modifier::None,
            key: 0x04,
            key2: 0,
            mouse: MouseAction::Left,
            func: SpecialFn::Mute,
            slot: 0,
            mode: MacroMode::RepeatTimes,
            raw: hex4(action.to_bytes()),
            capture: Capture::None,
        };
        if action.to_bytes() == default {
            return e;
        }
        match action {
            KeyAction::Disabled => e.kind = Kind::Disabled,
            KeyAction::Key { modifier, key, key2 } => {
                e.kind = Kind::Key;
                e.modifier = modifier;
                e.key = key;
                e.key2 = key2;
            }
            KeyAction::Mouse(m) => {
                e.kind = Kind::Mouse;
                e.mouse = m;
            }
            KeyAction::Function(f) => {
                e.kind = Kind::Function;
                e.func = f;
            }
            KeyAction::Macro { index, mode } => {
                e.kind = Kind::Macro;
                e.slot = index;
                e.mode = mode;
            }
            KeyAction::Raw(_) => e.kind = Kind::Raw,
        }
        e
    }

    pub fn action(&self, default: [u8; 4]) -> Option<KeyAction> {
        Some(match self.kind {
            Kind::Default => KeyAction::Raw(default),
            Kind::Disabled => KeyAction::Disabled,
            Kind::Key => KeyAction::Key { modifier: self.modifier, key: self.key, key2: self.key2 },
            Kind::Mouse => KeyAction::Mouse(self.mouse),
            Kind::Function => KeyAction::Function(self.func),
            Kind::Macro => KeyAction::Macro { index: self.slot, mode: self.mode },
            Kind::Raw => KeyAction::Raw(u32::from_str_radix(self.raw.trim(), 16).ok()?.to_be_bytes()),
        })
    }

    /// Draws the editor. Returns true when the user asked to apply.
    pub fn ui(&mut self, ui: &mut egui::Ui, lang: Lang, macro_slots: u8, used_slots: &[u8]) -> bool {
        let t = |s| tr(lang, s);
        self.handle_capture(ui);
        egui::Grid::new("action_editor").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(t("Action"));
            ComboBox::from_id_salt("kind").selected_text(t(self.kind.label())).show_ui(ui, |ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut self.kind, k, t(k.label()));
                }
            });
            ui.end_row();
            match self.kind {
                Kind::Key => {
                    ui.label(t("Modifier"));
                    ComboBox::from_id_salt("modifier").selected_text(self.modifier.label()).show_ui(ui, |ui| {
                        for m in Modifier::ALL {
                            ui.selectable_value(&mut self.modifier, m, m.label());
                        }
                    });
                    ui.end_row();
                    ui.label(t("Key"));
                    ui.horizontal(|ui| {
                        key_combo(ui, "key", &mut self.key, false);
                        let label = if self.capture == Capture::Key { t("Press a key…") } else { t("Capture") };
                        if ui.button(label).clicked() {
                            self.capture = Capture::Key;
                        }
                    });
                    ui.end_row();
                    ui.label(t("Second key"));
                    ui.horizontal(|ui| {
                        key_combo(ui, "key2", &mut self.key2, true);
                        let label = if self.capture == Capture::Key2 { t("Press a key…") } else { t("Capture") };
                        if ui.button(label).clicked() {
                            self.capture = Capture::Key2;
                        }
                    });
                    ui.end_row();
                }
                Kind::Mouse => {
                    ui.label(t("Mouse"));
                    ComboBox::from_id_salt("mouse").selected_text(self.mouse.label()).show_ui(ui, |ui| {
                        for m in MouseAction::ALL {
                            ui.selectable_value(&mut self.mouse, m, m.label());
                        }
                    });
                    ui.end_row();
                }
                Kind::Function => {
                    ui.label(t("Function"));
                    ComboBox::from_id_salt("func").selected_text(self.func.label()).height(400.0).show_ui(ui, |ui| {
                        for f in SpecialFn::ALL {
                            ui.selectable_value(&mut self.func, f, f.label());
                        }
                    });
                    ui.end_row();
                }
                Kind::Macro => {
                    ui.label(t("Slot"));
                    ComboBox::from_id_salt("slot")
                        .selected_text(slot_label(self.slot, used_slots, lang))
                        .height(400.0)
                        .show_ui(ui, |ui| {
                            for s in 0..macro_slots {
                                ui.selectable_value(&mut self.slot, s, slot_label(s, used_slots, lang));
                            }
                        });
                    ui.end_row();
                    ui.label(t("Trigger"));
                    ComboBox::from_id_salt("mode").selected_text(self.mode.label()).show_ui(ui, |ui| {
                        for m in MacroMode::ALL {
                            ui.selectable_value(&mut self.mode, m, m.label());
                        }
                    });
                    ui.end_row();
                }
                Kind::Raw => {
                    ui.label(t("Raw"));
                    ui.text_edit_singleline(&mut self.raw);
                    ui.end_row();
                }
                Kind::Default | Kind::Disabled => {}
            }
        });
        ui.add_space(8.0);
        ui.button(t("Apply")).clicked()
    }

    fn handle_capture(&mut self, ui: &egui::Ui) {
        if self.capture == Capture::None {
            return;
        }
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Key { key, physical_key, pressed: true, .. } => hid_from_key(physical_key.unwrap_or(*key)),
                _ => None,
            })
        });
        if let Some(code) = pressed {
            match self.capture {
                Capture::Key => self.key = code,
                Capture::Key2 => self.key2 = code,
                Capture::None => {}
            }
            self.capture = Capture::None;
        }
    }
}

pub fn slot_label(s: u8, used: &[u8], lang: Lang) -> String {
    if used.contains(&s) {
        format!("{s} ({})", tr(lang, "in use"))
    } else {
        s.to_string()
    }
}

fn hex4(b: [u8; 4]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn key_combo(ui: &mut egui::Ui, id: &str, value: &mut u8, allow_none: bool) {
    let text = if *value == 0 && allow_none { "—".to_string() } else { keycodes::display(*value) };
    ComboBox::from_id_salt(id).selected_text(text).height(400.0).show_ui(ui, |ui| {
        if allow_none {
            ui.selectable_value(value, 0, "—");
        }
        for (code, name) in keycodes::all() {
            ui.selectable_value(value, code, name);
        }
    });
}

/// Short label for a key action, to print on a keycap.
pub fn short(action: KeyAction) -> String {
    match action {
        KeyAction::Disabled => "OFF".into(),
        KeyAction::Key { modifier, key, key2 } => {
            let mut s = String::new();
            if modifier != Modifier::None {
                s += match modifier {
                    Modifier::Ctrl => "C+",
                    Modifier::Shift => "S+",
                    Modifier::Alt => "A+",
                    Modifier::Win => "W+",
                    Modifier::None => "",
                };
            }
            s += &keycodes::display(key);
            if key2 != 0 {
                s += "+";
                s += &keycodes::display(key2);
            }
            s
        }
        KeyAction::Mouse(m) => m.label().replace("Mouse ", "M-"),
        KeyAction::Function(f) => f.label().to_string(),
        KeyAction::Macro { index, .. } => format!("M{index}"),
        KeyAction::Raw(b) => hex4(b),
    }
}

// ---------------------------------------------------------------- macros

pub struct Recorder {
    last_time: f64,
}

impl Recorder {
    pub fn new(now: f64) -> Recorder {
        Recorder { last_time: now }
    }

    /// Appends key events typed into the window (with the pauses between them).
    pub fn capture(&mut self, ui: &egui::Ui, m: &mut Macro) {
        let (events, now) = ui.input(|i| (i.events.clone(), i.time));
        for e in events {
            if let Event::Key { key, physical_key, pressed, repeat: false, .. } = e {
                let Some(code) = hid_from_key(physical_key.unwrap_or(key)) else { continue };
                let gap = ((now - self.last_time) * 1000.0).round().clamp(0.0, 65535.0) as u16;
                if !m.events.is_empty() && gap > 0 {
                    m.events.push(MacroEvent::Delay(gap));
                }
                m.events.push(if pressed { MacroEvent::KeyDown(code) } else { MacroEvent::KeyUp(code) });
                self.last_time = now;
            }
        }
    }
}

/// Editable list of macro events. Returns true if anything changed.
pub fn macro_events_ui(ui: &mut egui::Ui, lang: Lang, m: &mut Macro) -> bool {
    let t = |s| tr(lang, s);
    let mut changed = false;
    let mut action: Option<(usize, i32)> = None; // (row, -1 up / +1 down / 0 delete)
    egui::Grid::new("macro_rows").striped(true).num_columns(4).show(ui, |ui| {
        for (i, ev) in m.events.iter_mut().enumerate() {
            ui.label(format!("{}", i + 1));
            ui.horizontal(|ui| match ev {
                MacroEvent::KeyDown(k) | MacroEvent::KeyUp(k) => {
                    let mut code = *k;
                    key_combo(ui, &format!("mk{i}"), &mut code, false);
                    let mut down = matches!(ev, MacroEvent::KeyDown(_));
                    let text = if down { t("pressed") } else { t("released") };
                    if ui.selectable_label(down, text).clicked() {
                        down = !down;
                    }
                    let new = if down { MacroEvent::KeyDown(code) } else { MacroEvent::KeyUp(code) };
                    if new != *ev {
                        *ev = new;
                        changed = true;
                    }
                }
                MacroEvent::MouseDown(b) | MacroEvent::MouseUp(b) => {
                    let mut btn = *b;
                    ComboBox::from_id_salt(format!("mb{i}")).selected_text(btn.label()).show_ui(ui, |ui| {
                        for a in [
                            MouseAction::Left,
                            MouseAction::Right,
                            MouseAction::Middle,
                            MouseAction::Back,
                            MouseAction::Forward,
                        ] {
                            ui.selectable_value(&mut btn, a, a.label());
                        }
                    });
                    let mut down = matches!(ev, MacroEvent::MouseDown(_));
                    let text = if down { t("pressed") } else { t("released") };
                    if ui.selectable_label(down, text).clicked() {
                        down = !down;
                    }
                    let new = if down { MacroEvent::MouseDown(btn) } else { MacroEvent::MouseUp(btn) };
                    if new != *ev {
                        *ev = new;
                        changed = true;
                    }
                }
                MacroEvent::Delay(ms) => {
                    ui.label(t("Delay"));
                    changed |= ui.add(DragValue::new(ms).range(1..=65535).suffix(" ms")).changed();
                }
                MacroEvent::MouseMove { dx, dy } => {
                    ui.label(t("Mouse move"));
                    changed |= ui.add(DragValue::new(dx).prefix("dx ")).changed();
                    changed |= ui.add(DragValue::new(dy).prefix("dy ")).changed();
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button("⬆").on_hover_text(t("Up")).clicked() {
                    action = Some((i, -1));
                }
                if ui.small_button("⬇").on_hover_text(t("Down")).clicked() {
                    action = Some((i, 1));
                }
                if ui.small_button("🗑").on_hover_text(t("Delete")).clicked() {
                    action = Some((i, 0));
                }
            });
            ui.end_row();
        }
    });
    if let Some((i, d)) = action {
        changed = true;
        match d {
            0 => {
                m.events.remove(i);
            }
            -1 if i > 0 => m.events.swap(i, i - 1),
            1 if i + 1 < m.events.len() => m.events.swap(i, i + 1),
            _ => {}
        }
    }
    changed
}
