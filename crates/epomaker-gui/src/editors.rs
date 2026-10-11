//! Editors for key actions and macros.

use crate::hidmap::hid_from_key;
use crate::i18n::{tr, Lang};
use crate::picker::{key_button, picker_ui, Category, Pick, PickerState};
use eframe::egui::{self, ComboBox, DragValue, Event, RichText, Vec2};
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
    picker: PickerState,
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
            picker: PickerState::new(Category::Letters),
        };
        if action.to_bytes() != default {
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
        }
        // open the picker on the category of what the key does now
        let shown = match KeyAction::from_bytes(action.to_bytes()) {
            KeyAction::Key { key, .. } => Some(Pick::Key(key)),
            KeyAction::Mouse(m) => Some(Pick::Mouse(m)),
            KeyAction::Function(f) => Some(Pick::Func(f)),
            KeyAction::Macro { index, .. } => Some(Pick::Macro(index)),
            _ => None,
        };
        if let Some(p) = shown {
            e.picker.category = Category::of(p);
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

    #[cfg(feature = "autoshot")]
    pub fn set_search(&mut self, q: &str) {
        self.picker.search = q.to_string();
    }

    /// The item highlighted in the picker.
    fn selected(&self) -> Option<Pick> {
        match self.kind {
            Kind::Key => Some(Pick::Key(self.key)),
            Kind::Mouse => Some(Pick::Mouse(self.mouse)),
            Kind::Function => Some(Pick::Func(self.func)),
            Kind::Macro => Some(Pick::Macro(self.slot)),
            _ => None,
        }
    }

    /// Draws the editor. Returns true when the user asked to apply.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        macro_slots: u8,
        used_slots: &[u8],
        default: [u8; 4],
        enabled: bool,
    ) -> bool {
        let t = |s| tr(lang, s);
        self.handle_capture(ui);
        let mut apply = false;
        ui.horizontal(|ui| {
            if ui.add(egui::Button::selectable(self.kind == Kind::Default, t("Factory default"))).clicked() {
                self.kind = Kind::Default;
            }
            if ui.add(egui::Button::selectable(self.kind == Kind::Disabled, t("Disabled"))).clicked() {
                self.kind = Kind::Disabled;
            }
            ui.separator();
            ui.label(t("New action:"));
            let preview = match self.kind {
                Kind::Default => {
                    format!("{} ({})", t("Factory default"), describe(KeyAction::from_bytes(default), lang))
                }
                _ => self.action(default).map(|a| describe(a, lang)).unwrap_or_else(|| "?".into()),
            };
            ui.label(RichText::new(preview).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = self.action(default).is_some();
                let b = egui::Button::new(RichText::new(t("Apply")).strong()).min_size(Vec2::new(90.0, 0.0));
                apply = ui.add_enabled(enabled && ok, b).clicked();
            });
        });
        ui.separator();
        let height = (ui.available_height() - 8.0).max(120.0);
        ui.horizontal_top(|ui| {
            let details_w = 270.0;
            ui.allocate_ui(Vec2::new((ui.available_width() - details_w - 12.0).max(300.0), height), |ui| {
                let sel = self.selected();
                if let Some(p) = picker_ui(ui, lang, &mut self.picker, &Category::ALL, macro_slots, sel, height - 34.0)
                {
                    match p {
                        Pick::Key(k) => {
                            if self.kind != Kind::Key {
                                self.modifier = Modifier::None;
                                self.key2 = 0;
                            }
                            self.kind = Kind::Key;
                            self.key = k;
                        }
                        Pick::Mouse(m) => {
                            self.kind = Kind::Mouse;
                            self.mouse = m;
                        }
                        Pick::Func(f) => {
                            self.kind = Kind::Function;
                            self.func = f;
                        }
                        Pick::Macro(s) => {
                            self.kind = Kind::Macro;
                            self.slot = s;
                        }
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(details_w);
                self.details_ui(ui, lang, used_slots);
            });
        });
        apply
    }

    /// Options of the chosen action (modifier, second key, macro trigger, raw value).
    fn details_ui(&mut self, ui: &mut egui::Ui, lang: Lang, used_slots: &[u8]) {
        let t = |s| tr(lang, s);
        match self.kind {
            Kind::Key => {
                ui.label(t("Modifier"));
                ui.horizontal_wrapped(|ui| {
                    for m in Modifier::ALL {
                        ui.selectable_value(&mut self.modifier, m, t(m.label()));
                    }
                });
                ui.add_space(6.0);
                ui.label(t("Key"));
                ui.horizontal(|ui| {
                    key_button(ui, "key", &mut self.key, false, lang);
                    let label = if self.capture == Capture::Key { t("Press a key…") } else { t("Capture") };
                    if ui.button(label).on_hover_text(t("Press a key on your keyboard to pick it")).clicked() {
                        self.capture = Capture::Key;
                    }
                });
                ui.add_space(6.0);
                ui.label(t("Second key"));
                ui.horizontal(|ui| {
                    key_button(ui, "key2", &mut self.key2, true, lang);
                    let label = if self.capture == Capture::Key2 { t("Press a key…") } else { t("Capture") };
                    if ui.button(label).clicked() {
                        self.capture = Capture::Key2;
                    }
                });
            }
            Kind::Macro => {
                ui.label(format!("{} {}", t("Macro"), slot_label(self.slot, used_slots, lang)));
                ui.add_space(6.0);
                ui.label(t("Trigger"));
                for m in MacroMode::ALL {
                    ui.radio_value(&mut self.mode, m, t(m.label()));
                }
            }
            Kind::Default => {
                ui.weak(t("The key does what is printed on it."));
            }
            Kind::Disabled => {
                ui.weak(t("The key does nothing."));
            }
            Kind::Mouse | Kind::Function | Kind::Raw => {}
        }
        ui.add_space(10.0);
        egui::CollapsingHeader::new(t("Advanced: raw matrix entry")).default_open(self.kind == Kind::Raw).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.raw).desired_width(90.0).font(egui::TextStyle::Monospace),
                    );
                    if ui.button(t("Use")).clicked() {
                        self.kind = Kind::Raw;
                    }
                });
            },
        );
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
                Capture::Key => {
                    self.kind = Kind::Key;
                    self.key = code;
                }
                Capture::Key2 => self.key2 = code,
                Capture::None => {}
            }
            self.capture = Capture::None;
        }
    }
}

/// Full, translated description of a key action.
pub fn describe(a: KeyAction, lang: Lang) -> String {
    match a {
        KeyAction::Key { modifier, key, key2 } => {
            let mut parts = vec![];
            if modifier != Modifier::None {
                parts.push(modifier.label().to_string());
            }
            parts.push(keycodes::display(key));
            if key2 != 0 {
                parts.push(keycodes::display(key2));
            }
            parts.join(" + ")
        }
        KeyAction::Disabled => tr(lang, "Disabled").into(),
        KeyAction::Raw(b) => format!("{} {}", tr(lang, "Raw"), hex4(b)),
        KeyAction::Mouse(m) => tr(lang, m.label()).into(),
        KeyAction::Function(f) => tr(lang, f.label()).into(),
        KeyAction::Macro { index, mode } => format!("{} {index} ({})", tr(lang, "Macro"), tr(lang, mode.label())),
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
                    key_button(ui, ("mk", i), &mut code, false, lang);
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
                    ComboBox::from_id_salt(format!("mb{i}")).selected_text(t(btn.label())).show_ui(ui, |ui| {
                        for a in [
                            MouseAction::Left,
                            MouseAction::Right,
                            MouseAction::Middle,
                            MouseAction::Back,
                            MouseAction::Forward,
                        ] {
                            ui.selectable_value(&mut btn, a, t(a.label()));
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
                if ui.small_button("⬆").on_hover_text(t("Move up")).clicked() {
                    action = Some((i, -1));
                }
                if ui.small_button("⬇").on_hover_text(t("Move down")).clicked() {
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
