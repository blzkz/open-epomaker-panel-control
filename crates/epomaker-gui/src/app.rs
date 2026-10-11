//! Main window.

mod colors;
mod lighting;

use crate::editors::{self, ActionEditor, Recorder};
use crate::i18n::{tr, Lang};
use crate::keyboard_view::{self, KeyStyle};
use crate::worker::{snapshot, JobInfo, Snapshot, Update, Worker};
use colors::ColorsState;
use eframe::egui::{self, Color32, ComboBox, RichText, Slider};
use epomaker_core::devices::Database;
use epomaker_core::hid::{self, HidCandidate, HidTransport};
use epomaker_core::keymap::{KeyAction, KeyMatrix, Layer};
use epomaker_core::lighting::LightEffect;
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::profile_file::Backup;
use epomaker_core::settings::{ChargeState, ReportRate, SleepTimes};
use epomaker_core::Keyboard;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Lighting,
    Keymap,
    Colors,
    Macros,
    Settings,
    Backup,
}

impl Tab {
    const ALL: [Tab; 6] = [Tab::Lighting, Tab::Keymap, Tab::Colors, Tab::Macros, Tab::Settings, Tab::Backup];
    fn label(self) -> &'static str {
        match self {
            Tab::Lighting => "Lighting",
            Tab::Keymap => "Key mapping",
            Tab::Colors => "Per-key colors",
            Tab::Macros => "Macros",
            Tab::Settings => "Settings",
            Tab::Backup => "Backup",
        }
    }
}

/// What the device selector points at.
#[derive(Clone, PartialEq)]
enum Target {
    Hid(HidCandidate),
    Demo(i64),
}

/// Device remembered between runs (HID paths change across reboots, so match by ids).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum SavedTarget {
    Hid { vid: u16, pid: u16, interface: i32 },
    Demo(i64),
}

impl SavedTarget {
    fn of(t: &Target) -> SavedTarget {
        match t {
            Target::Hid(c) => SavedTarget::Hid { vid: c.vid, pid: c.pid, interface: c.interface },
            Target::Demo(id) => SavedTarget::Demo(*id),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];
    fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Follow system",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        }
    }
    fn egui(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

const ZOOMS: [f32; 6] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5];

/// Preferences kept between runs (eframe storage).
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    lang: Option<Lang>,
    theme: ThemeChoice,
    zoom: f32,
    last_device: Option<SavedTarget>,
    recent_colors: Vec<[u8; 3]>,
    light_auto: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            lang: None,
            theme: ThemeChoice::System,
            zoom: 1.0,
            last_device: None,
            recent_colors: vec![],
            light_auto: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Error,
}

struct Toast {
    text: String,
    kind: ToastKind,
    /// Time it was shown first (set when drawn).
    born: Option<f64>,
}

/// Keyboard state as last read/written.
struct DeviceState {
    snap: Snapshot,
    layers: BTreeMap<Layer, KeyMatrix>,
}

pub struct App {
    lang: Lang,
    theme: ThemeChoice,
    zoom: f32,
    worker: Worker,
    tab: Tab,
    candidates: Vec<HidCandidate>,
    target: Option<Target>,
    saved_target: Option<SavedTarget>,
    dev: Option<DeviceState>,
    status: String,
    error: Option<String>,
    toasts: Vec<Toast>,
    #[cfg(target_os = "linux")]
    udev: crate::udev::UdevHelper,
    // lighting
    light: LightEffect,
    light_auto: bool,
    light_dirty: bool,
    // keymap
    layer: Layer,
    selected: Option<usize>,
    editor: Option<ActionEditor>,
    // colors
    colors: ColorsState,
    // macros
    macro_slot: u8,
    macros: BTreeMap<u8, Macro>,
    recorder: Option<Recorder>,
    // settings
    confirm_reset: bool,
    backup_path: String,
    #[cfg(feature = "autoshot")]
    autoshot: Option<AutoShot>,
}

/// Development aid: renders every tab of the demo keyboard into PNG files.
#[cfg(feature = "autoshot")]
struct AutoShot {
    dir: std::path::PathBuf,
    step: usize,
    frames: u32,
    waiting: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let worker = Worker::new(cc.egui_ctx.clone());
        #[cfg(feature = "autoshot")]
        let autoshot = std::env::var_os("EPOMAKER_AUTOSHOT").map(|d| AutoShot {
            dir: d.into(),
            step: 0,
            frames: 0,
            waiting: false,
        });
        #[cfg(feature = "autoshot")]
        let fresh = autoshot.is_some();
        #[cfg(not(feature = "autoshot"))]
        let fresh = false;
        let prefs: Prefs = match cc.storage {
            Some(s) if !fresh => eframe::get_value(s, eframe::APP_KEY).unwrap_or_default(),
            _ => Prefs::default(),
        };
        let zoom = if ZOOMS.iter().any(|z| (z - prefs.zoom).abs() < 0.01) { prefs.zoom } else { 1.0 };
        let theme = if fresh { ThemeChoice::Dark } else { prefs.theme };
        cc.egui_ctx.set_theme(theme.egui());
        cc.egui_ctx.set_zoom_factor(zoom);
        let mut colors = ColorsState::default();
        colors.recent = prefs.recent_colors.iter().copied().take(colors::RECENT_MAX).collect();
        let mut app = App {
            lang: prefs.lang.unwrap_or_else(Lang::detect),
            theme,
            zoom,
            worker,
            tab: Tab::Lighting,
            candidates: vec![],
            target: None,
            saved_target: prefs.last_device.clone(),
            dev: None,
            status: String::new(),
            error: None,
            toasts: vec![],
            #[cfg(target_os = "linux")]
            udev: Default::default(),
            light: LightEffect::default(),
            light_auto: prefs.light_auto,
            light_dirty: false,
            layer: Layer::Profile(0),
            selected: None,
            editor: None,
            colors,
            macro_slot: 0,
            macros: BTreeMap::new(),
            recorder: None,
            confirm_reset: false,
            backup_path: default_backup_path(),
            #[cfg(feature = "autoshot")]
            autoshot,
        };
        if fresh {
            // EPOMAKER_AUTOSHOT_MODEL=<device id> picks another demo keyboard (default: TH80)
            let id = std::env::var("EPOMAKER_AUTOSHOT_MODEL").ok().and_then(|v| v.parse().ok()).unwrap_or(1118);
            app.lang = Lang::detect();
            app.target = Some(Target::Demo(id));
            app.connect(Target::Demo(id));
            return app;
        }
        app.refresh_devices();
        match (app.target.clone(), &app.saved_target) {
            (Some(t @ Target::Hid(_)), _) => app.connect(t),
            (None, Some(SavedTarget::Demo(id))) => {
                let t = Target::Demo(*id);
                app.target = Some(t.clone());
                app.connect(t);
            }
            _ => {}
        }
        app
    }

    fn t<'a>(&self, s: &'a str) -> &'a str {
        tr(self.lang, s)
    }

    fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), kind, born: None });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    fn refresh_devices(&mut self) {
        match hid::enumerate() {
            Ok(c) => {
                self.candidates = c;
                let remembered = self
                    .candidates
                    .iter()
                    .find(|c| self.saved_target.as_ref() == Some(&SavedTarget::of(&Target::Hid((*c).clone()))));
                if self.target.is_none() || remembered.is_some() {
                    self.target =
                        remembered.or(self.candidates.first()).cloned().map(Target::Hid).or(self.target.take());
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn connect(&mut self, target: Target) {
        self.dev = None;
        self.selected = None;
        self.editor = None;
        self.macros.clear();
        self.colors.reset();
        self.saved_target = Some(SavedTarget::of(&target));
        self.worker.run_raw(
            JobInfo { label: "Connect", notify: false, blocking: true },
            Box::new(move |slot: &mut Option<Keyboard>, progress| {
                *slot = None;
                let mut kb = match target {
                    Target::Demo(id) => Keyboard::emulated(id)?,
                    Target::Hid(c) => {
                        let dongle = Database::get().devices.iter().any(|d| d.pid == c.pid && d.dongle)
                            && !Database::get().devices.iter().any(|d| d.pid == c.pid && !d.dongle);
                        Keyboard::connect(Box::new(HidTransport::open(&c)?), dongle)?
                    }
                };
                let snap = snapshot(&mut kb, progress)?;
                *slot = Some(kb);
                Ok(Update::Connected(Box::new(snap)))
            }),
        );
    }

    fn apply(&mut self, u: Update) {
        match u {
            Update::None => {}
            Update::Connected(s) => {
                self.light = s.light.unwrap_or_default();
                self.layer = Layer::Profile(s.profile);
                let demo = if s.emulated { " [DEMO]" } else { "" };
                self.status = format!("{} — {}{demo}", s.info.display_name, s.describe);
                let text = format!("{}: {}{demo}", self.t("Connected"), s.info.display_name);
                self.toast(ToastKind::Success, text);
                let layers = s.layers.iter().cloned().collect();
                self.dev = Some(DeviceState { snap: *s, layers });
            }
            Update::Disconnected => self.dev = None,
            Update::Light(l) => {
                if let Some(l) = l {
                    self.light = l;
                }
                if let Some(d) = &mut self.dev {
                    d.snap.light = l;
                }
            }
            Update::Profile(p) => {
                if let Some(d) = &mut self.dev {
                    d.snap.profile = p;
                }
            }
            Update::Layer(l, m) => {
                if let Some(d) = &mut self.dev {
                    d.snap.used_slots = d
                        .layers
                        .iter()
                        .filter(|(k, _)| **k != l)
                        .flat_map(|(_, m)| m.macro_indices().collect::<Vec<_>>())
                        .chain(m.macro_indices())
                        .collect();
                    d.snap.used_slots.sort();
                    d.snap.used_slots.dedup();
                    d.layers.insert(l, m);
                }
            }
            Update::Colors(c) => {
                if let Some(d) = &mut self.dev {
                    d.snap.colors = c;
                }
            }
            Update::Macro(slot, m) => {
                self.macros.insert(slot, m);
            }
            Update::Battery(b) => {
                if let Some(d) = &mut self.dev {
                    d.snap.battery = b;
                }
            }
            Update::Message(m) => {
                self.toast(ToastKind::Success, m.clone());
                self.status = m;
            }
            Update::Many(v) => v.into_iter().for_each(|u| self.apply(u)),
        }
    }

    fn poll_worker(&mut self) {
        for (info, r) in self.worker.poll() {
            match r {
                Ok(u) => {
                    self.error = None;
                    if info.notify {
                        let text = format!("{} — {}", self.t(info.label), self.t("done"));
                        self.toast(ToastKind::Success, text);
                    }
                    self.apply(u);
                }
                Err(e) => {
                    let msg = format!("{}: {e}", self.t(info.label));
                    self.error = Some(msg.clone());
                    self.toast(ToastKind::Error, msg);
                    #[cfg(target_os = "linux")]
                    if info.label == "Connect" {
                        self.udev.show_for(&e.to_string());
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ top bar

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("⌨ Open Epomaker Panel");
            ui.separator();
            ui.label(self.t("Keyboard"));
            let demo_models: Vec<i64> = {
                let db = Database::get();
                let mut v: Vec<(i64, &str)> = Vec::new();
                for d in db.devices.iter().filter(|d| !d.dongle) {
                    if !v.iter().any(|(_, n)| *n == d.name) {
                        v.push((d.id, &d.name));
                    }
                }
                v.into_iter().map(|(id, _)| id).collect()
            };
            let lang = self.lang;
            let label = |t: &Option<Target>| match t {
                Some(Target::Hid(c)) => format!("{} {:04x}:{:04x} (if{})", c.product, c.vid, c.pid, c.interface),
                Some(Target::Demo(id)) => {
                    let d = Database::get().device_by_id(*id);
                    let (name, key) = d.map(|d| (d.display_name.as_str(), d.name.as_str())).unwrap_or_default();
                    format!("{name} ({key}) — {}", tr(lang, "demo, no hardware"))
                }
                None => tr(lang, "No keyboard connected").to_string(),
            };
            let current = label(&self.target);
            let mut new_target = None;
            let busy = self.worker.busy();
            ui.add_enabled_ui(!self.worker.blocked(), |ui| {
                ComboBox::from_id_salt("device").width(330.0).selected_text(current).show_ui(ui, |ui| {
                    for c in &self.candidates {
                        let t = Some(Target::Hid(c.clone()));
                        if ui.selectable_label(self.target == t, label(&t)).clicked() {
                            new_target = t;
                        }
                    }
                    if !self.candidates.is_empty() {
                        ui.separator();
                    }
                    for id in &demo_models {
                        let t = Some(Target::Demo(*id));
                        if ui.selectable_label(self.target == t, label(&t)).clicked() {
                            new_target = t;
                        }
                    }
                });
            });
            if let Some(t) = new_target {
                self.target = Some(t);
            }
            if ui.button("⟳").on_hover_text(self.t("Look for keyboards again")).clicked() {
                self.refresh_devices();
            }
            if ui.add_enabled(!busy && self.target.is_some(), egui::Button::new(self.t("Connect"))).clicked() {
                if let Some(t) = self.target.clone() {
                    self.connect(t);
                }
            }
            if self.dev.is_some() && ui.add_enabled(!busy, egui::Button::new(self.t("Disconnect"))).clicked() {
                self.worker.run_raw(
                    JobInfo::quiet("Disconnect"),
                    Box::new(|k: &mut Option<Keyboard>, _| {
                        *k = None;
                        Ok(Update::Disconnected)
                    }),
                );
            }
            if busy {
                ui.separator();
                let what = self.t(self.worker.current()).to_string();
                match self.worker.progress() {
                    Some(f) => {
                        ui.add(egui::ProgressBar::new(f).desired_width(140.0).show_percentage());
                    }
                    None => {
                        ui.spinner();
                    }
                }
                ui.label(RichText::new(format!("{what}…")).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.preferences_menu(ui));
        });
    }

    fn preferences_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        ui.menu_button(format!("⚙ {}", t("Preferences")), |ui| {
            ui.label(RichText::new(t("Language")).strong());
            ui.radio_value(&mut self.lang, Lang::En, "English");
            ui.radio_value(&mut self.lang, Lang::Es, "Español");
            ui.separator();
            ui.label(RichText::new(t("Theme")).strong());
            for th in ThemeChoice::ALL {
                if ui.radio_value(&mut self.theme, th, t(th.label())).changed() {
                    ui.ctx().set_theme(th.egui());
                }
            }
            ui.separator();
            ui.label(RichText::new(t("Interface size")).strong());
            for z in ZOOMS {
                if ui.radio(self.zoom == z, format!("{:.0} %", z * 100.0)).clicked() {
                    self.zoom = z;
                    ui.ctx().set_zoom_factor(z);
                }
            }
        });
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| {
            let life = if t.kind == ToastKind::Error { 9.0 } else { 3.5 };
            t.born.is_none_or(|b| now - b < life)
        });
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let mut close = None;
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -34.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                for (i, toast) in self.toasts.iter_mut().enumerate() {
                    toast.born.get_or_insert(now);
                    let v = ui.visuals();
                    let accent = match toast.kind {
                        ToastKind::Success => Color32::from_rgb(60, 170, 90),
                        ToastKind::Error => v.error_fg_color,
                    };
                    egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.5, accent))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(4.0, 18.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 2.0, accent);
                                ui.add(egui::Label::new(&toast.text).wrap());
                                if toast.kind == ToastKind::Error && ui.small_button("x").clicked() {
                                    close = Some(i);
                                }
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        if let Some(i) = close {
            self.toasts.remove(i);
        }
    }

    // ------------------------------------------------------------ key mapping

    fn layer_selector(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let fn_layers = dev.snap.info.fn_layers;
        let active = dev.snap.profile;
        let lang = self.lang;
        ui.horizontal(|ui| {
            ui.label(tr(lang, "Layer"));
            for p in 0..3u8 {
                let mut text = format!("{} {}", tr(lang, "Profile"), p + 1);
                if p == active {
                    text += " *";
                }
                if ui.selectable_value(&mut self.layer, Layer::Profile(p), text).changed() {
                    self.editor = None;
                }
            }
            for f in 0..fn_layers {
                let text = if fn_layers > 1 {
                    format!("{} {}", tr(lang, "Fn layer"), f + 1)
                } else {
                    tr(lang, "Fn layer").to_string()
                };
                if ui.selectable_value(&mut self.layer, Layer::Fn(f), text).changed() {
                    self.editor = None;
                }
            }
        });
    }

    fn keymap_tab(&mut self, ui: &mut egui::Ui) {
        let busy = self.worker.busy();
        ui.horizontal(|ui| {
            self.layer_selector(ui);
            ui.separator();
            let layer = self.layer;
            ui.add_enabled_ui(!busy, |ui| {
                if ui.button(self.t("Reset layer to factory")).clicked() {
                    self.worker.run_notify("Reset layer", move |kb| {
                        kb.reset_layer(layer)?;
                        Ok(Update::Layer(layer, kb.read_layer(layer)?))
                    });
                }
                if ui.button(self.t("Read from keyboard")).clicked() {
                    self.worker.run("Read layer", move |kb| Ok(Update::Layer(layer, kb.read_layer(layer)?)));
                }
            });
        });
        let Some(dev) = &self.dev else { return };
        let model = dev.snap.model;
        let Some(matrix) = dev.layers.get(&self.layer).cloned() else { return };
        let selected = self.selected;
        let lang = self.lang;
        let visuals = ui.visuals().clone();
        let max_h = (ui.available_height() * 0.5).max(180.0);
        let resp = keyboard_view::show(ui, model, max_h, |k| {
            let entry = matrix.entries.get(k.idx).copied().unwrap_or_default();
            let default = model.default_entry(k.idx).unwrap_or_default();
            let changed = entry != default;
            let action = KeyAction::from_bytes(entry);
            KeyStyle {
                fill: if changed { Color32::from_rgb(40, 90, 160) } else { visuals.widgets.inactive.bg_fill },
                text: if changed { editors::short(action) } else { k.label.clone() },
                text_color: if changed { Color32::WHITE } else { visuals.text_color() },
                highlight: selected == Some(k.idx),
                tooltip: format!("{} -> {}", k.label.replace('\n', " "), editors::describe(action, lang)),
            }
        });
        if let Some(i) = resp.clicked {
            self.selected = Some(i);
            let entry = matrix.entries.get(i).copied().unwrap_or_default();
            self.editor =
                Some(ActionEditor::new(KeyAction::from_bytes(entry), model.default_entry(i).unwrap_or_default()));
        }
        ui.add_space(6.0);
        let used = dev.snap.used_slots.clone();
        let layer = self.layer;
        match (self.selected, &mut self.editor) {
            (Some(i), Some(ed)) => {
                let label = model.key_for_matrix_index(i).map(|k| k.label.replace('\n', " ")).unwrap_or_default();
                let entry = matrix.entries.get(i).copied().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).heading().strong());
                    ui.label(RichText::new(format!(
                        "{}: {}",
                        tr(lang, "now"),
                        editors::describe(KeyAction::from_bytes(entry), lang)
                    )));
                });
                let default = model.default_entry(i).unwrap_or_default();
                if ed.ui(ui, lang, model.macro_slots, &used, default, !busy) {
                    if let Some(action) = ed.action(default) {
                        self.worker.run_notify("Set key", move |kb| {
                            kb.set_key(layer, i, action)?;
                            Ok(Update::Layer(layer, kb.read_layer(layer)?))
                        });
                    }
                }
            }
            _ => {
                ui.label(tr(lang, "Select a key on the keyboard."));
            }
        }
    }

    // ------------------------------------------------------------ macros

    fn macros_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let slots = dev.snap.model.macro_slots;
        let used = dev.snap.used_slots.clone();
        let lang = self.lang;
        let busy = self.worker.busy();
        let t = |s| tr(lang, s);
        egui::Panel::left("macro_slots").resizable(false).default_size(130.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in 0..slots {
                    let text = editors::slot_label(s, &used, lang);
                    if ui.selectable_label(self.macro_slot == s, text).clicked() {
                        self.macro_slot = s;
                        self.recorder = None;
                        if !self.macros.contains_key(&s) {
                            self.worker.run("Read macro", move |kb| Ok(Update::Macro(s, kb.read_macro(s)?)));
                        }
                    }
                }
            });
        });
        let slot = self.macro_slot;
        if !self.macros.contains_key(&slot) && !busy {
            self.macros.insert(slot, Macro { repeat_count: 1, events: vec![] });
            self.worker.run("Read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
        }
        let Some(m) = self.macros.get_mut(&slot) else { return };
        ui.horizontal(|ui| {
            ui.heading(format!("{} {}", t("Macro"), slot));
            ui.label(t("Repeat count"));
            ui.add(egui::DragValue::new(&mut m.repeat_count).range(1..=65535));
        });
        ui.horizontal(|ui| {
            match &mut self.recorder {
                Some(rec) => {
                    rec.capture(ui, m);
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("⏺ {}", t("Recording: type on this window…")),
                    );
                    if ui.button(t("Stop recording")).clicked() {
                        self.recorder = None;
                    }
                }
                None => {
                    if ui.button(format!("⏺ {}", t("Record"))).clicked() {
                        self.recorder = Some(Recorder::new(ui.input(|i| i.time)));
                    }
                }
            }
            if ui.button(t("Add key press")).clicked() {
                m.events.extend([MacroEvent::KeyDown(0x04), MacroEvent::Delay(20), MacroEvent::KeyUp(0x04)]);
            }
            if ui.button(t("Add delay")).clicked() {
                m.events.push(MacroEvent::Delay(100));
            }
            if ui.button(t("Add mouse click")).clicked() {
                m.events.extend([
                    MacroEvent::MouseDown(epomaker_core::keymap::MouseAction::Left),
                    MacroEvent::Delay(20),
                    MacroEvent::MouseUp(epomaker_core::keymap::MouseAction::Left),
                ]);
            }
            if ui.button(t("Clear")).clicked() {
                m.events.clear();
            }
        });
        let fits = m.encode().is_ok();
        let used_bytes = m.encode().map(|b| b.iter().rposition(|x| *x != 0).map(|i| i + 1).unwrap_or(0)).unwrap_or(256);
        ui.horizontal(|ui| {
            ui.add(egui::ProgressBar::new(used_bytes.min(256) as f32 / 256.0).desired_width(160.0));
            ui.label(format!("{}: {used_bytes} / 256", t("Bytes used")));
            if !fits {
                ui.colored_label(ui.visuals().error_fg_color, t("Macro too long"));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(ui.available_height() - 40.0).show(ui, |ui| {
            editors::macro_events_ui(ui, lang, m);
        });
        ui.separator();
        let m2 = m.clone();
        ui.horizontal(|ui| {
            if ui.add_enabled(fits && !busy, egui::Button::new(t("Write to keyboard"))).clicked() {
                self.worker.run_notify("Write macro", move |kb| {
                    kb.write_macro(slot, &m2)?;
                    Ok(Update::Macro(slot, kb.read_macro(slot)?))
                });
            }
            if ui.add_enabled(!busy, egui::Button::new(t("Read from keyboard"))).clicked() {
                self.worker.run("Read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
            }
        });
    }

    // ------------------------------------------------------------ settings

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let busy = self.worker.busy();
        let Some(dev) = &mut self.dev else { return };
        let s = &mut dev.snap;
        let model = s.model;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("info").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label(t("Device"));
                ui.label(format!("{} ({}, id {})", s.info.display_name, s.info.name, s.info.id));
                ui.end_row();
                ui.label("");
                ui.label(RichText::new(&s.describe).small());
                ui.end_row();
                ui.label(t("Firmware"));
                ui.label(format!("{:#06x}", s.firmware));
                ui.end_row();
                ui.label(t("Battery"));
                ui.horizontal(|ui| {
                    let state = match s.battery.state {
                        ChargeState::Discharging => t("discharging"),
                        ChargeState::Charging => t("charging"),
                        ChargeState::Full => t("full"),
                    };
                    ui.label(format!("{}% ({state})", s.battery.percent));
                    if ui.small_button("⟳").on_hover_text(t("Read battery")).clicked() {
                        self.worker.run("Read battery", |kb| Ok(Update::Battery(kb.battery()?)));
                    }
                });
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t("Active profile"));
                for p in 0..3u8 {
                    if ui.selectable_label(s.profile == p, format!("{}", p + 1)).clicked() {
                        s.profile = p;
                        self.worker.run("Set profile", move |kb| {
                            kb.set_profile(p)?;
                            Ok(Update::Profile(kb.profile()?))
                        });
                    }
                }
            });
            let p = s.profile as usize;
            ui.horizontal(|ui| {
                ui.label(t("Polling rate"));
                let cur = s.rates.get(p).copied().flatten();
                let text = cur.map(|r| format!("{} Hz", r.hz())).unwrap_or("?".into());
                ComboBox::from_id_salt("rate").selected_text(text).show_ui(ui, |ui| {
                    for r in ReportRate::ALL.iter().filter(|r| model.report_rates.contains(&r.hz())) {
                        if ui.selectable_label(cur == Some(*r), format!("{} Hz", r.hz())).clicked() {
                            s.rates[p] = Some(*r);
                            let (r, pp) = (*r, p as u8);
                            self.worker.run("Set polling rate", move |kb| {
                                kb.set_report_rate(pp, r)?;
                                Ok(Update::None)
                            });
                        }
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new(t("Options for this profile")).strong());
            let o = &mut s.options[p];
            let mut changed = false;
            changed |= ui.checkbox(&mut o.win_key_lock, t("Disable Windows key")).changed();
            changed |= ui.checkbox(&mut o.mac_mode, t("Mac mode")).changed();
            changed |= ui.checkbox(&mut o.wasd_arrows_swap, t("Swap WASD and arrows")).changed();
            changed |= ui.checkbox(&mut o.fn_key_swap, t("Swap F-keys and media keys")).changed();
            changed |= ui.checkbox(&mut o.led_off, t("Backlight off")).changed();
            changed |= ui.checkbox(&mut o.power_save, t("Power saving")).changed();
            if changed {
                let (o, pp) = (*o, p as u8);
                self.worker.run("Set options", move |kb| {
                    kb.set_options(pp, &o)?;
                    Ok(Update::None)
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t("Debounce (ms)"));
                let r = ui.add(Slider::new(&mut s.debounce, 1..=50));
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    let d = s.debounce;
                    self.worker.run("Set debounce", move |kb| {
                        kb.set_debounce(d)?;
                        Ok(Update::None)
                    });
                }
            });
            if ui.checkbox(&mut s.auto_os, t("Automatic OS detection")).changed() {
                let v = s.auto_os;
                self.worker.run("Set auto OS", move |kb| {
                    kb.set_auto_os(v)?;
                    Ok(Update::None)
                });
            }
            if model.sleep_bt.is_some() || model.sleep_dongle.is_some() {
                ui.separator();
                ui.label(RichText::new(t("Sleep timers (minutes)")).strong());
                let mut sl: SleepTimes = s.sleep;
                let mut changed = false;
                egui::Grid::new("sleep").num_columns(2).show(ui, |ui| {
                    if let Some(l) = model.sleep_bt {
                        changed |=
                            minutes(ui, t("Bluetooth: light off"), &mut sl.bt_light_off, l.min, l.max, true, lang);
                        changed |= minutes(
                            ui,
                            t("Bluetooth: deep sleep"),
                            &mut sl.bt_deep_sleep,
                            l.min_deep,
                            l.max_deep,
                            false,
                            lang,
                        );
                    }
                    if let Some(l) = model.sleep_dongle {
                        changed |=
                            minutes(ui, t("2.4 GHz: light off"), &mut sl.dongle_light_off, l.min, l.max, true, lang);
                        changed |= minutes(
                            ui,
                            t("2.4 GHz: deep sleep"),
                            &mut sl.dongle_deep_sleep,
                            l.min_deep,
                            l.max_deep,
                            false,
                            lang,
                        );
                    }
                });
                if changed {
                    s.sleep = sl;
                    self.worker.run("Set sleep timers", move |kb| {
                        kb.set_sleep_times(&sl)?;
                        Ok(Update::None)
                    });
                }
            }
            ui.separator();
            let reset = egui::Button::new(RichText::new(t("Factory reset…")).color(ui.visuals().error_fg_color));
            if ui.add_enabled(!busy, reset).clicked() {
                self.confirm_reset = true;
            }
        });
        if self.confirm_reset {
            let modal = egui::Modal::new(egui::Id::new("confirm_reset")).show(ui.ctx(), |ui| {
                ui.label(t("This erases every profile, key mapping, macro and lighting setting. Continue?"));
                ui.horizontal(|ui| {
                    if ui.button(t("Cancel")).clicked() {
                        self.confirm_reset = false;
                    }
                    if ui.button(RichText::new(t("Reset")).color(ui.visuals().error_fg_color)).clicked() {
                        self.confirm_reset = false;
                        self.macros.clear();
                        self.worker.run_with(JobInfo::blocking("Factory reset"), |kb, progress| {
                            kb.factory_reset()?;
                            Ok(Update::Connected(Box::new(snapshot(kb, progress)?)))
                        });
                    }
                });
            });
            if modal.should_close() {
                self.confirm_reset = false;
            }
        }
    }

    // ------------------------------------------------------------ backup

    fn backup_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let busy = self.worker.busy();
        ui.label(t("Save the whole configuration of the keyboard to a file, or load one."));
        ui.horizontal(|ui| {
            ui.label(t("File"));
            ui.add(egui::TextEdit::singleline(&mut self.backup_path).desired_width(420.0));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new(t("Save backup"))).clicked() {
                let path = self.backup_path.clone();
                let done = format!("{}: {path}", t("Saved"));
                self.worker.run_with(JobInfo::quiet("Save backup"), move |kb, _| {
                    let b = Backup::capture(kb)?;
                    std::fs::write(&path, serde_json::to_string_pretty(&b)?)?;
                    Ok(Update::Message(done))
                });
            }
            if ui.add_enabled(!busy, egui::Button::new(t("Load backup"))).clicked() {
                let path = self.backup_path.clone();
                let done = format!("{}: {path}", t("Restored"));
                self.macros.clear();
                let info = JobInfo { label: "Load backup", notify: false, blocking: true };
                self.worker.run_with(info, move |kb, progress| {
                    let b: Backup = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                    b.restore(kb)?;
                    Ok(Update::Many(vec![Update::Connected(Box::new(snapshot(kb, progress)?)), Update::Message(done)]))
                });
            }
        });
    }
}

fn minutes(ui: &mut egui::Ui, label: &str, secs: &mut u16, min: u16, max: u16, allow_never: bool, lang: Lang) -> bool {
    ui.label(label);
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut never = *secs == 0;
        if allow_never && ui.checkbox(&mut never, tr(lang, "never")).changed() {
            *secs = if never { 0 } else { min * 60 };
            changed = true;
        }
        if !never {
            let mut m = (*secs / 60).clamp(min, max);
            let r = ui.add(Slider::new(&mut m, min..=max).suffix(" min"));
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                *secs = m * 60;
                changed = true;
            } else if r.changed() {
                *secs = m * 60;
            }
        }
    });
    ui.end_row();
    changed
}

fn default_backup_path() -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("epomaker-backup.json").to_string_lossy().into_owned()
}

#[cfg(feature = "autoshot")]
impl App {
    const SHOTS: [&'static str; 10] = [
        "lighting",
        "lighting-ripple",
        "keymap",
        "keymap-search",
        "colors",
        "macros",
        "settings",
        "backup",
        "udev",
        "light-theme",
    ];

    /// Prepares the window for screenshot `name`.
    fn autoshot_setup(&mut self, name: &str, ctx: &egui::Context) {
        use epomaker_core::keymap::Modifier;
        use epomaker_core::lighting::{LightMode, Rgb};
        match name {
            "lighting" => self.tab = Tab::Lighting,
            "lighting-ripple" => {
                self.light = LightEffect {
                    mode: LightMode::Ripple,
                    color: Rgb(0x00C8FF),
                    dazzle: false,
                    ..LightEffect::default()
                };
            }
            "keymap" => {
                self.tab = Tab::Keymap;
                let Some(d) = &self.dev else { return };
                // show a remapped key and its editor
                let find = |p: &str| {
                    d.snap.model.keys.iter().find(|k| k.label.starts_with(p)).and_then(|k| k.matrix_index).unwrap_or(0)
                };
                let (i, caps) = (find("A"), find("Caps"));
                let layer = self.layer;
                let ctrl_c = KeyAction::Key { modifier: Modifier::Ctrl, key: 0x06, key2: 0 };
                self.worker.run("Demo remap", move |kb| {
                    kb.set_key(layer, caps, KeyAction::Disabled)?;
                    kb.set_key(layer, i, ctrl_c)?;
                    Ok(Update::Layer(layer, kb.read_layer(layer)?))
                });
                self.selected = Some(i);
                let def = d.snap.model.default_entry(i).unwrap_or_default();
                self.editor = Some(ActionEditor::new(ctrl_c, def));
            }
            "keymap-search" => {
                let Some(d) = &self.dev else { return };
                let knob = crate::keyboard_view::Geometry::new(d.snap.model).knob.map(|k| k.press);
                if let Some(i) = knob {
                    self.selected = Some(i);
                    let def = d.snap.model.default_entry(i).unwrap_or_default();
                    let mut ed = ActionEditor::new(KeyAction::from_bytes(def), def);
                    ed.set_search("vol");
                    self.editor = Some(ed);
                }
            }
            "colors" => {
                self.tab = Tab::Colors;
                self.colors.autoshot_demo(self.dev.as_mut().map(|d| &mut d.snap));
            }
            "macros" => {
                self.tab = Tab::Macros;
                self.macros.insert(
                    0,
                    Macro {
                        repeat_count: 1,
                        events: vec![
                            MacroEvent::KeyDown(0xE1),
                            MacroEvent::Delay(15),
                            MacroEvent::KeyDown(0x0B),
                            MacroEvent::Delay(40),
                            MacroEvent::KeyUp(0x0B),
                            MacroEvent::KeyUp(0xE1),
                        ],
                    },
                );
            }
            "settings" => self.tab = Tab::Settings,
            "backup" => self.tab = Tab::Backup,
            "udev" => {
                #[cfg(target_os = "linux")]
                self.udev.show_for("Permission denied");
            }
            "light-theme" => {
                #[cfg(target_os = "linux")]
                {
                    self.udev.open = false;
                }
                self.tab = Tab::Lighting;
                self.theme = ThemeChoice::Light;
                ctx.set_theme(self.theme.egui());
                self.toast(ToastKind::Success, format!("{} — {}", self.t("Set key"), self.t("done")));
                self.toast(ToastKind::Error, format!("{}: HID error: device disconnected", self.t("Write colors")));
            }
            _ => {}
        }
    }

    fn autoshot(&mut self, ctx: &egui::Context) {
        let Some(mut shot) = self.autoshot.take() else { return };
        ctx.request_repaint();
        if self.dev.is_none() || self.worker.busy() {
            self.autoshot = Some(shot);
            return;
        }
        let saved = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = saved {
            let name = format!("{}-{}-{:?}.png", shot.step, Self::SHOTS[shot.step], self.lang).to_lowercase();
            let file = std::io::BufWriter::new(std::fs::File::create(shot.dir.join(name)).expect("create png"));
            let mut enc = png::Encoder::new(file, img.size[0] as u32, img.size[1] as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("png header");
            let raw: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            w.write_image_data(&raw).expect("png data");
            shot.step += 1;
            shot.waiting = false;
            shot.frames = 0;
        }
        if shot.step >= Self::SHOTS.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if shot.frames == 0 {
            self.autoshot_setup(Self::SHOTS[shot.step], ctx);
        }
        shot.frames += 1;
        if !shot.waiting && shot.frames > 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            shot.waiting = true;
        }
        self.autoshot = Some(shot);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(feature = "autoshot")]
        self.autoshot(&ctx);
        self.zoom = ctx.zoom_factor();
        self.poll_worker();
        #[cfg(target_os = "linux")]
        if self.udev.ui(&ctx, self.lang) {
            self.toast(ToastKind::Success, self.t("Rule installed. Reconnecting…").to_string());
            self.refresh_devices();
            if let Some(t @ Target::Hid(_)) = self.target.clone() {
                self.connect(t);
            }
        }
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, format!("{}: {e}", self.t("Error")));
                } else {
                    ui.label(&self.status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.t("Experimental: this software is not affiliated with EPOMAKER."))
                            .small()
                            .weak(),
                    );
                });
            });
        });
        egui::Panel::left("tabs").resizable(false).default_size(170.0).show(ui, |ui| {
            ui.add_space(8.0);
            for tab in Tab::ALL {
                let text = RichText::new(self.t(tab.label())).size(15.0);
                let b =
                    egui::Button::selectable(self.tab == tab, text).min_size(egui::vec2(ui.available_width(), 28.0));
                if ui.add_enabled(self.dev.is_some(), b).clicked() {
                    self.tab = tab;
                }
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.dev.is_none() {
                ui.centered_and_justified(|ui| {
                    if self.worker.busy() {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new(self.t("No keyboard connected")).size(18.0));
                    }
                });
                return;
            }
            if self.worker.blocked() {
                ui.disable();
            }
            match self.tab {
                Tab::Lighting => self.lighting_tab(ui),
                Tab::Keymap => self.keymap_tab(ui),
                Tab::Colors => self.colors_tab(ui),
                Tab::Macros => self.macros_tab(ui),
                Tab::Settings => self.settings_tab(ui),
                Tab::Backup => self.backup_tab(ui),
            }
        });
        self.toasts_ui(&ctx);
    }

    #[cfg(feature = "autoshot")]
    fn persist_egui_memory(&self) -> bool {
        self.autoshot.is_none()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        #[cfg(feature = "autoshot")]
        if self.autoshot.is_some() {
            return;
        }
        let prefs = Prefs {
            lang: Some(self.lang),
            theme: self.theme,
            zoom: self.zoom,
            last_device: self.saved_target.clone(),
            recent_colors: self.colors.recent.clone(),
            light_auto: self.light_auto,
        };
        eframe::set_value(storage, eframe::APP_KEY, &prefs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_and_tolerate_missing_fields() {
        let p = Prefs {
            lang: Some(Lang::Es),
            theme: ThemeChoice::Light,
            zoom: 1.25,
            last_device: Some(SavedTarget::Hid { vid: 0x3151, pid: 0x4010, interface: 2 }),
            recent_colors: vec![[1, 2, 3]],
            light_auto: false,
        };
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.lang, Some(Lang::Es));
        assert!(back.theme == ThemeChoice::Light && back.zoom == 1.25 && !back.light_auto);
        assert!(back.last_device == p.last_device && back.recent_colors == p.recent_colors);
        let old: Prefs = serde_json::from_str(r#"{"lang":"En"}"#).unwrap();
        assert!(old.light_auto && old.zoom == 1.0 && old.theme == ThemeChoice::System);
    }
}
