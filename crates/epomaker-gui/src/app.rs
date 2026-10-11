//! Main window.

use crate::editors::{self, ActionEditor, Recorder};
use crate::i18n::{tr, Lang};
use crate::keyboard_view::{self, KeyStyle};
use crate::persist::{self, Saved, SavedDevice};
use crate::worker::{snapshot, Hotplug, Snapshot, Update, Worker};
use eframe::egui::{self, Color32, ComboBox, RichText, Slider};
use epomaker_core::devices::Database;
use epomaker_core::hid::{self, HidCandidate, HidTransport};
use epomaker_core::keymap::{KeyAction, KeyMatrix, Layer};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::profile_file::Backup;
use epomaker_core::selftest::{self, DeviceSummary, Outcome, Step, StepResult};
use epomaker_core::settings::{ReportRate, SleepTimes};
use epomaker_core::Keyboard;
use std::collections::BTreeMap;
use std::time::Duration;

const APP: &str = concat!("epomaker-panel ", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Lighting,
    Keymap,
    Colors,
    Macros,
    Settings,
    Backup,
    Diagnostics,
}

impl Tab {
    const ALL: [Tab; 7] =
        [Tab::Lighting, Tab::Keymap, Tab::Colors, Tab::Macros, Tab::Settings, Tab::Backup, Tab::Diagnostics];
    fn label(self) -> &'static str {
        match self {
            Tab::Lighting => "Lighting",
            Tab::Keymap => "Key mapping",
            Tab::Colors => "Per-key colors",
            Tab::Macros => "Macros",
            Tab::Settings => "Settings",
            Tab::Backup => "Backup",
            Tab::Diagnostics => "Diagnostics",
        }
    }
}

/// What the device selector points at.
#[derive(Clone, PartialEq)]
enum Target {
    Hid(HidCandidate),
    Demo(i64),
}

impl Target {
    fn saved(&self) -> SavedDevice {
        match self {
            Target::Hid(c) => SavedDevice::from_candidate(c),
            Target::Demo(id) => SavedDevice::Demo(*id),
        }
    }
}

/// Progress of a self-test started from the Diagnostics tab (one worker job per step).
struct SelfTestRun {
    device: Option<DeviceSummary>,
    results: Vec<StepResult>,
    running: bool,
}

/// Keyboard state as last read/written.
struct DeviceState {
    snap: Snapshot,
    layers: BTreeMap<Layer, KeyMatrix>,
}

pub struct App {
    lang: Lang,
    worker: Worker,
    tab: Tab,
    candidates: Vec<HidCandidate>,
    target: Option<Target>,
    dev: Option<DeviceState>,
    status: String,
    error: Option<String>,
    // lighting
    light: LightEffect,
    light_auto: bool,
    light_dirty: bool,
    // keymap
    layer: Layer,
    selected: Option<usize>,
    editor: Option<ActionEditor>,
    // colors
    brush: [u8; 3],
    paint_live: bool,
    last_painted: Option<usize>,
    // macros
    macro_slot: u8,
    macros: BTreeMap<u8, Macro>,
    recorder: Option<Recorder>,
    // settings
    confirm_reset: bool,
    backup_path: String,
    // diagnostics
    verify: bool,
    selftest: Option<SelfTestRun>,
    probe: Option<Vec<String>>,
    // hotplug
    hotplug: Option<Hotplug>,
    saved: Saved,
    /// Target of the connection in progress.
    connecting: Option<Target>,
    /// Target of the open connection.
    connected: Option<Target>,
    /// The connected keyboard disappeared from USB.
    unplugged: bool,
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
        let screenshots = std::env::var_os("EPOMAKER_AUTOSHOT").is_some();
        #[cfg(not(feature = "autoshot"))]
        let screenshots = false;
        // screenshots must not depend on (or change) the user's saved state
        let saved = if screenshots { Saved::default() } else { persist::load() };
        let mut app = App {
            lang: Lang::detect(),
            worker,
            tab: Tab::Lighting,
            candidates: vec![],
            target: None,
            dev: None,
            status: String::new(),
            error: None,
            light: LightEffect::default(),
            light_auto: true,
            light_dirty: false,
            layer: Layer::Profile(0),
            selected: None,
            editor: None,
            brush: [255, 0, 128],
            paint_live: true,
            last_painted: None,
            macro_slot: 0,
            macros: BTreeMap::new(),
            recorder: None,
            confirm_reset: false,
            backup_path: default_backup_path(),
            verify: saved.verify.unwrap_or(true),
            selftest: None,
            probe: None,
            hotplug: None,
            saved,
            connecting: None,
            connected: None,
            unplugged: false,
            #[cfg(feature = "autoshot")]
            autoshot: std::env::var_os("EPOMAKER_AUTOSHOT").map(|d| AutoShot {
                dir: d.into(),
                step: 0,
                frames: 0,
                waiting: false,
            }),
        };
        #[cfg(feature = "autoshot")]
        if app.autoshot.is_some() {
            app.target = Some(Target::Demo(1118));
            app.connect(Target::Demo(1118));
            return app;
        }
        app.refresh_devices();
        if app.connecting.is_none() {
            if let Some(SavedDevice::Demo(id)) = app.saved.last_device {
                app.target = Some(Target::Demo(id));
                app.connect(Target::Demo(id));
            }
        }
        app.hotplug = Some(Hotplug::start(cc.egui_ctx.clone(), Duration::from_secs(2)));
        app
    }

    fn t<'a>(&self, s: &'a str) -> &'a str {
        tr(self.lang, s)
    }

    fn refresh_devices(&mut self) {
        match hid::enumerate() {
            Ok(c) => self.devices_changed(c),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Handles a new USB device list (startup, refresh button, hotplug thread):
    /// notices when the open keyboard is gone and connects to one that just appeared.
    fn devices_changed(&mut self, list: Vec<HidCandidate>) {
        let appeared: Vec<HidCandidate> =
            list.iter().filter(|c| !self.candidates.iter().any(|o| o.path == c.path)).cloned().collect();
        self.candidates = list;
        let present = |t: &Option<Target>, list: &[HidCandidate]| match t {
            Some(Target::Hid(c)) => list.iter().any(|x| x.path == c.path),
            _ => true,
        };
        if !present(&self.connected, &self.candidates) {
            self.connected = None;
            self.dev = None;
            self.unplugged = true;
            self.error = None;
            self.status = self.t("Keyboard disconnected").to_string();
            self.worker.run_raw(
                "unplug",
                Box::new(|k: &mut Option<Keyboard>| {
                    *k = None;
                    Ok(Update::None)
                }),
            );
        }
        if !present(&self.target, &self.candidates) {
            self.target = None;
        }
        let real = |t: &Option<Target>| matches!(t, Some(Target::Hid(_)));
        if !real(&self.connected) && !real(&self.connecting) && !appeared.is_empty() {
            // prefer the keyboard used last time
            let saved = self.saved.last_device.clone();
            let pick = appeared.iter().find(|c| saved.as_ref().is_some_and(|s| s.matches(c))).unwrap_or(&appeared[0]);
            let t = Target::Hid(pick.clone());
            self.target = Some(t.clone());
            self.connect(t);
        }
        if self.target.is_none() {
            self.target = self.candidates.first().cloned().map(Target::Hid);
        }
    }

    fn connect(&mut self, target: Target) {
        self.dev = None;
        self.selected = None;
        self.editor = None;
        self.macros.clear();
        self.selftest = None;
        self.connecting = Some(target.clone());
        let verify = self.verify;
        self.worker.run_raw(
            "connect",
            Box::new(move |slot: &mut Option<Keyboard>| {
                *slot = None;
                let mut kb = match target {
                    Target::Demo(id) => Keyboard::emulated(id)?,
                    Target::Hid(c) => {
                        let dongle = Database::get().devices.iter().any(|d| d.pid == c.pid && d.dongle)
                            && !Database::get().devices.iter().any(|d| d.pid == c.pid && !d.dongle);
                        Keyboard::connect(Box::new(HidTransport::open(&c)?), dongle)?
                    }
                };
                kb.set_verify(verify);
                let snap = snapshot(&mut kb)?;
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
                let layers = s.layers.iter().cloned().collect();
                self.dev = Some(DeviceState { snap: *s, layers });
                self.unplugged = false;
                // a connection made by connect() (factory reset and restore also send Connected)
                if let Some(t) = self.connecting.take() {
                    self.saved.last_device = Some(t.saved());
                    self.connected = Some(t);
                    self.save();
                }
            }
            Update::Disconnected => {
                self.dev = None;
                self.connected = None;
            }
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
            Update::Message(m) => self.status = m,
            Update::SelfTestDevice(d) => {
                if let Some(st) = &mut self.selftest {
                    st.device = Some(*d);
                }
            }
            Update::SelfTestStep(r) => {
                if let Some(st) = &mut self.selftest {
                    st.results.push(*r);
                    st.running = st.results.len() < Step::ALL.len();
                }
            }
            Update::Probe(lines) => self.probe = Some(lines),
            Update::Many(v) => v.into_iter().for_each(|u| self.apply(u)),
        }
    }

    // ------------------------------------------------------------ top bar

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("⌨ Open Epomaker Panel");
            ui.separator();
            ui.label(self.t("Keyboard"));
            let demo_models: Vec<(i64, String)> = {
                let db = Database::get();
                let mut v: Vec<(i64, String)> = Vec::new();
                for d in db.devices.iter().filter(|d| !d.dongle) {
                    if !v.iter().any(|(_, n)| *n == d.name) {
                        v.push((d.id, d.name.clone()));
                    }
                }
                v
            };
            let label = |t: &Option<Target>| match t {
                Some(Target::Hid(c)) => format!("{} {:04x}:{:04x} (if{})", c.product, c.vid, c.pid, c.interface),
                Some(Target::Demo(id)) => {
                    let d = Database::get().device_by_id(*id).map(|d| d.name.clone()).unwrap_or_default();
                    format!("{} — {}", tr(self.lang, "Demo keyboard (no hardware)"), d)
                }
                None => tr(self.lang, "No keyboard connected").to_string(),
            };
            let current = label(&self.target);
            let mut new_target = None;
            ComboBox::from_id_salt("device").width(320.0).selected_text(current).show_ui(ui, |ui| {
                for c in &self.candidates {
                    let t = Some(Target::Hid(c.clone()));
                    if ui.selectable_label(self.target == t, label(&t)).clicked() {
                        new_target = t;
                    }
                }
                for (id, _) in &demo_models {
                    let t = Some(Target::Demo(*id));
                    if ui.selectable_label(self.target == t, label(&t)).clicked() {
                        new_target = t;
                    }
                }
            });
            if let Some(t) = new_target {
                self.target = Some(t);
            }
            if ui.button("⟳").on_hover_text(self.t("Refresh")).clicked() {
                self.refresh_devices();
            }
            let busy = self.worker.busy();
            if ui.add_enabled(!busy && self.target.is_some(), egui::Button::new(self.t("Connect"))).clicked() {
                if let Some(t) = self.target.clone() {
                    self.connect(t);
                }
            }
            if self.dev.is_some() && ui.add_enabled(!busy, egui::Button::new(self.t("Disconnect"))).clicked() {
                self.connected = None;
                self.worker.run_raw(
                    "disconnect",
                    Box::new(|k: &mut Option<Keyboard>| {
                        *k = None;
                        Ok(Update::Disconnected)
                    }),
                );
            }
            ui.separator();
            ComboBox::from_id_salt("lang")
                .selected_text(if self.lang == Lang::Es { "Español" } else { "English" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.lang, Lang::En, "English");
                    ui.selectable_value(&mut self.lang, Lang::Es, "Español");
                });
            if busy {
                ui.spinner();
                ui.label(self.t("Working…"));
            }
        });
    }

    // ------------------------------------------------------------ lighting

    fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let model = dev.snap.model;
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let mut changed = false;
        let mut modes: Vec<LightMode> = vec![LightMode::Off];
        modes.extend(model.light_modes.iter().map(|c| c.mode));
        egui::Grid::new("light").num_columns(2).spacing([16.0, 12.0]).show(ui, |ui| {
            ui.label(t("Effect"));
            ComboBox::from_id_salt("mode").width(260.0).height(500.0).selected_text(self.light.mode.label()).show_ui(
                ui,
                |ui| {
                    for m in &modes {
                        if ui.selectable_value(&mut self.light.mode, *m, m.label()).changed() {
                            self.light.option = 0;
                            changed = true;
                        }
                    }
                },
            );
            ui.end_row();
            let caps = model.caps(self.light.mode).cloned();
            if let Some(c) = &caps {
                if let Some(max) = c.max_speed.filter(|_| self.light.mode.uses_speed()) {
                    ui.label(t("Speed"));
                    changed |= ui.add(Slider::new(&mut self.light.speed, 0..=max)).changed();
                    ui.end_row();
                }
                if let Some(max) = c.max_brightness {
                    ui.label(t("Brightness"));
                    changed |= ui.add(Slider::new(&mut self.light.brightness, 0..=max)).changed();
                    ui.end_row();
                }
                if !c.options.is_empty() {
                    ui.label(t("Direction / variant"));
                    ui.horizontal_wrapped(|ui| {
                        for (i, o) in c.options.iter().enumerate() {
                            changed |= ui.selectable_value(&mut self.light.option, i as u8, o.as_str()).changed();
                        }
                    });
                    ui.end_row();
                }
                if c.dazzle {
                    ui.label("");
                    changed |= ui.checkbox(&mut self.light.dazzle, t("Rainbow colors")).changed();
                    ui.end_row();
                }
                if c.rgb && !self.light.dazzle {
                    ui.label(t("Color"));
                    ui.horizontal(|ui| {
                        let mut rgb = self.light.color.to_array();
                        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                            self.light.color = Rgb::new(rgb[0], rgb[1], rgb[2]);
                            changed = true;
                        }
                        for preset in [0xFF0000, 0xFF8000, 0xFFFF00, 0x00FF00, 0x00FFFF, 0x0000FF, 0xFF00FF, 0xFFFFFF] {
                            let c = Rgb(preset);
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
                            ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(c.r(), c.g(), c.b()));
                            if resp.clicked() {
                                self.light.color = c;
                                changed = true;
                            }
                        }
                    });
                    ui.end_row();
                }
            }
        });
        if self.light.mode.needs_host() {
            ui.colored_label(
                Color32::YELLOW,
                t("The effect needs the host to stream audio/screen data; the keyboard will stay dark without it."),
            );
        }
        if changed {
            self.light_dirty = true;
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.light_auto, t("Apply automatically"));
            if ui.button(t("Apply")).clicked() {
                self.send_light();
            }
            if ui.button(t("Read from keyboard")).clicked() {
                self.worker.run("read light", |kb| Ok(Update::Light(kb.light()?)));
            }
        });
        let dragging = ui.input(|i| i.pointer.any_down());
        if self.light_auto && self.light_dirty && !dragging && !self.worker.busy() {
            self.send_light();
        }
    }

    fn send_light(&mut self) {
        self.light_dirty = false;
        let e = self.light;
        self.worker.run("set light", move |kb| {
            kb.set_light(&e)?;
            Ok(Update::Light(Some(e)))
        });
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
        ui.horizontal(|ui| {
            self.layer_selector(ui);
            ui.separator();
            let layer = self.layer;
            if ui.button(self.t("Reset layer to factory")).clicked() {
                self.worker.run("reset layer", move |kb| {
                    kb.reset_layer(layer)?;
                    Ok(Update::Layer(layer, kb.read_layer(layer)?))
                });
            }
            if ui.button(self.t("Read from keyboard")).clicked() {
                self.worker.run("read layer", move |kb| Ok(Update::Layer(layer, kb.read_layer(layer)?)));
            }
        });
        let Some(dev) = &self.dev else { return };
        let model = dev.snap.model;
        let Some(matrix) = dev.layers.get(&self.layer).cloned() else { return };
        let selected = self.selected;
        let visuals = ui.visuals().clone();
        let resp = keyboard_view::show(ui, model, |idx, label| {
            let entry = matrix.entries.get(idx).copied().unwrap_or_default();
            let default = model.default_entry(idx).unwrap_or_default();
            let changed = entry != default;
            let action = KeyAction::from_bytes(entry);
            KeyStyle {
                fill: if changed { Color32::from_rgb(40, 90, 160) } else { visuals.widgets.inactive.bg_fill },
                text: if changed { editors::short(action) } else { label.to_string() },
                text_color: if changed { Color32::WHITE } else { visuals.text_color() },
                highlight: selected == Some(idx),
                tooltip: format!("{} -> {}", label.replace('\n', " "), describe(action)),
            }
        });
        if let Some(i) = resp.clicked {
            self.selected = Some(i);
            let entry = matrix.entries.get(i).copied().unwrap_or_default();
            self.editor =
                Some(ActionEditor::new(KeyAction::from_bytes(entry), model.default_entry(i).unwrap_or_default()));
        }
        ui.add_space(8.0);
        let lang = self.lang;
        let used = dev.snap.used_slots.clone();
        let layer = self.layer;
        match (self.selected, &mut self.editor) {
            (Some(i), Some(ed)) => {
                let label = model.key_for_matrix_index(i).map(|k| k.label.replace('\n', " ")).unwrap_or_default();
                let entry = matrix.entries.get(i).copied().unwrap_or_default();
                ui.heading(format!("{label}  ->  {}", describe(KeyAction::from_bytes(entry))));
                let default = model.default_entry(i).unwrap_or_default();
                if ed.ui(ui, lang, model.macro_slots, &used) {
                    if let Some(action) = ed.action(default) {
                        self.worker.run("set key", move |kb| {
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

    // ------------------------------------------------------------ per-key colors

    fn colors_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &mut self.dev else { return };
        let model = dev.snap.model;
        let lang = self.lang;
        let t = |s| tr(lang, s);
        ui.horizontal(|ui| {
            ui.label(t("Brush"));
            egui::color_picker::color_edit_button_srgb(ui, &mut self.brush);
            ui.checkbox(&mut self.paint_live, t("Paint live"));
            if ui.button(t("Fill all keys")).clicked() {
                let c = Rgb::new(self.brush[0], self.brush[1], self.brush[2]);
                dev.snap.colors = vec![c; model.matrix_len()];
                if self.paint_live {
                    let colors = dev.snap.colors.clone();
                    self.worker.run("write colors", move |kb| {
                        kb.set_key_colors(&colors)?;
                        Ok(Update::None)
                    });
                }
            }
        });
        ui.label(t("Click or drag over keys to paint them."));
        let colors = dev.snap.colors.clone();
        let resp = keyboard_view::show(ui, model, |idx, label| {
            let c = colors.get(idx).copied().unwrap_or_default();
            let fill = Color32::from_rgb(c.r(), c.g(), c.b());
            KeyStyle {
                fill,
                text: label.to_string(),
                text_color: keyboard_view::contrast(fill),
                highlight: false,
                tooltip: format!("{}  #{:06x}", label.replace('\n', " "), c.0),
            }
        });
        if !ui.input(|i| i.pointer.any_down()) {
            self.last_painted = None;
        }
        if let Some(i) = resp.dragged_over.or(resp.clicked) {
            if self.last_painted != Some(i) {
                self.last_painted = Some(i);
                let c = Rgb::new(self.brush[0], self.brush[1], self.brush[2]);
                if i < dev.snap.colors.len() {
                    dev.snap.colors[i] = c;
                }
                if self.paint_live {
                    let slot = match dev.snap.light {
                        Some(l) if l.mode == LightMode::UserPicture => l.option.min(2),
                        _ => 0,
                    };
                    self.worker.run("paint key", move |kb| {
                        kb.set_key_color(slot, i, c)?;
                        Ok(Update::None)
                    });
                }
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(t("Write all to keyboard")).clicked() {
                let colors = dev.snap.colors.clone();
                self.worker.run("write colors", move |kb| {
                    kb.set_key_colors(&colors)?;
                    Ok(Update::None)
                });
            }
            if ui.button(t("Read from keyboard")).clicked() {
                self.worker.run("read colors", |kb| Ok(Update::Colors(kb.key_colors()?)));
            }
            if ui.button(t("Activate custom per-key effect")).clicked() {
                let e = LightEffect { mode: LightMode::UserPicture, brightness: 4, ..LightEffect::default() };
                self.light = e;
                self.worker.run("set light", move |kb| {
                    kb.set_light(&e)?;
                    Ok(Update::Light(Some(e)))
                });
            }
        });
    }

    // ------------------------------------------------------------ macros

    fn macros_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let slots = dev.snap.model.macro_slots;
        let used = dev.snap.used_slots.clone();
        let lang = self.lang;
        let t = |s| tr(lang, s);
        egui::Panel::left("macro_slots").resizable(false).default_size(130.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in 0..slots {
                    let text = editors::slot_label(s, &used, lang);
                    if ui.selectable_label(self.macro_slot == s, text).clicked() {
                        self.macro_slot = s;
                        self.recorder = None;
                        if !self.macros.contains_key(&s) {
                            self.worker.run("read macro", move |kb| Ok(Update::Macro(s, kb.read_macro(s)?)));
                        }
                    }
                }
            });
        });
        let slot = self.macro_slot;
        if !self.macros.contains_key(&slot) && !self.worker.busy() {
            self.macros.insert(slot, Macro { repeat_count: 1, events: vec![] });
            self.worker.run("read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
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
                    ui.colored_label(Color32::LIGHT_RED, format!("⏺ {}", t("Recording: type on this window…")));
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
            ui.label(format!("{}: {used_bytes} / 256", t("Bytes used")));
            if !fits {
                ui.colored_label(Color32::RED, t("Macro too long"));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(ui.available_height() - 40.0).show(ui, |ui| {
            editors::macro_events_ui(ui, lang, m);
        });
        ui.separator();
        let m2 = m.clone();
        ui.horizontal(|ui| {
            if ui.add_enabled(fits, egui::Button::new(t("Write to keyboard"))).clicked() {
                self.worker.run("write macro", move |kb| {
                    kb.write_macro(slot, &m2)?;
                    Ok(Update::Macro(slot, kb.read_macro(slot)?))
                });
            }
            if ui.button(t("Read from keyboard")).clicked() {
                self.worker.run("read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
            }
        });
    }

    // ------------------------------------------------------------ settings

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
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
                        epomaker_core::settings::ChargeState::Discharging => t("discharging"),
                        epomaker_core::settings::ChargeState::Charging => t("charging"),
                        epomaker_core::settings::ChargeState::Full => t("full"),
                    };
                    ui.label(format!("{}% ({state})", s.battery.percent));
                    if ui.small_button("⟳").clicked() {
                        self.worker.run("battery", |kb| Ok(Update::Battery(kb.battery()?)));
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
                        self.worker.run("set profile", move |kb| {
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
                            self.worker.run("set rate", move |kb| {
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
                self.worker.run("set options", move |kb| {
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
                    self.worker.run("set debounce", move |kb| {
                        kb.set_debounce(d)?;
                        Ok(Update::None)
                    });
                }
            });
            if ui.checkbox(&mut s.auto_os, t("Automatic OS detection")).changed() {
                let v = s.auto_os;
                self.worker.run("set auto os", move |kb| {
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
                    self.worker.run("set sleep", move |kb| {
                        kb.set_sleep_times(&sl)?;
                        Ok(Update::None)
                    });
                }
            }
            ui.separator();
            if ui.button(RichText::new(t("Factory reset…")).color(Color32::LIGHT_RED)).clicked() {
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
                    if ui.button(RichText::new(t("Reset")).color(Color32::LIGHT_RED)).clicked() {
                        self.confirm_reset = false;
                        self.macros.clear();
                        self.worker.run("factory reset", |kb| {
                            kb.factory_reset()?;
                            Ok(Update::Connected(Box::new(snapshot(kb)?)))
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
        ui.label(t("Save the whole configuration of the keyboard to a file, or load one."));
        ui.horizontal(|ui| {
            ui.label(t("File"));
            ui.add(egui::TextEdit::singleline(&mut self.backup_path).desired_width(420.0));
        });
        ui.horizontal(|ui| {
            if ui.button(t("Save backup")).clicked() {
                let path = self.backup_path.clone();
                let done = format!("{}: {path}", t("Saved"));
                self.worker.run("backup", move |kb| {
                    let b = Backup::capture(kb)?;
                    std::fs::write(&path, serde_json::to_string_pretty(&b)?)?;
                    Ok(Update::Message(done))
                });
            }
            if ui.button(t("Load backup")).clicked() {
                let path = self.backup_path.clone();
                let done = format!("{}: {path}", t("Restored"));
                self.macros.clear();
                self.worker.run("restore", move |kb| {
                    let b: Backup = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                    b.restore(kb)?;
                    Ok(Update::Many(vec![Update::Connected(Box::new(snapshot(kb)?)), Update::Message(done)]))
                });
            }
        });
    }
}

impl App {
    fn save(&mut self) {
        #[cfg(feature = "autoshot")]
        if self.autoshot.is_some() {
            return;
        }
        self.saved.verify = Some(self.verify);
        persist::save(&self.saved);
    }

    // ------------------------------------------------------------ diagnostics

    fn start_selftest(&mut self) {
        self.selftest = Some(SelfTestRun { device: None, results: vec![], running: true });
        self.worker.run("self-test", |kb| Ok(Update::SelfTestDevice(Box::new(selftest::summary(kb)))));
        for step in Step::ALL {
            self.worker.run("self-test", move |kb| Ok(Update::SelfTestStep(Box::new(selftest::run_step(kb, step)))));
        }
    }

    fn run_probe(&self) {
        self.worker.run_raw(
            "probe",
            Box::new(|_: &mut Option<Keyboard>| {
                let mut lines = Vec::new();
                epomaker_core::probe::run(|l| lines.push(l))?;
                Ok(Update::Probe(lines))
            }),
        );
    }

    fn report(&self) -> Option<String> {
        let st = self.selftest.as_ref()?;
        Some(selftest::markdown(st.device.as_ref()?, &st.results, APP))
    }

    fn diagnostics_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.dev.is_none() {
                self.probe_panel(ui);
                return;
            }
            ui.heading(t("Hardware self-test"));
            ui.label(t(SELFTEST_INTRO));
            ui.add_space(6.0);
            if ui.checkbox(&mut self.verify, t("Verify every write (read it back)")).changed() {
                let v = self.verify;
                self.worker.run("verify", move |kb| {
                    kb.set_verify(v);
                    Ok(Update::None)
                });
                self.save();
            }
            ui.label(RichText::new(t(VERIFY_HINT)).small().weak());
            ui.add_space(8.0);
            let running = self.selftest.as_ref().is_some_and(|s| s.running);
            let report = self.report();
            ui.horizontal(|ui| {
                if ui.add_enabled(!running && !self.worker.busy(), egui::Button::new(t("Run self-test"))).clicked() {
                    self.start_selftest();
                }
                if ui.add_enabled(!running && report.is_some(), egui::Button::new(t("Copy report"))).clicked() {
                    if let Some(md) = report.clone() {
                        ui.ctx().copy_text(md);
                        self.status = t("Report copied to the clipboard").to_string();
                    }
                }
            });
            let Some(st) = &self.selftest else { return };
            ui.add_space(8.0);
            for (i, step) in Step::ALL.iter().enumerate() {
                ui.separator();
                ui.horizontal(|ui| {
                    let r = st.results.get(i);
                    let (text, color) = match r.map(|r| r.outcome) {
                        Some(Outcome::Passed) => (t("passed"), Color32::from_rgb(90, 190, 90)),
                        Some(Outcome::Failed) => (t("FAILED"), Color32::LIGHT_RED),
                        Some(Outcome::Skipped) => (t("skipped"), Color32::GRAY),
                        None if st.running && i == st.results.len() => (t("running…"), ui.visuals().text_color()),
                        None => (t("pending"), ui.visuals().weak_text_color()),
                    };
                    ui.add_sized([90.0, 18.0], egui::Label::new(RichText::new(text).color(color).strong()));
                    ui.label(RichText::new(t(step.title())).strong());
                    match r.and_then(|r| r.restored) {
                        Some(true) => {
                            ui.label(RichText::new(format!("({})", t("restored"))).weak());
                        }
                        Some(false) => {
                            ui.colored_label(Color32::LIGHT_RED, format!("({})", t("NOT restored")));
                        }
                        None => {}
                    }
                    if r.is_none() && st.running && i == st.results.len() {
                        ui.spinner();
                    }
                });
                if let Some(r) = st.results.get(i) {
                    ui.indent(i, |ui| {
                        for d in &r.details {
                            ui.add(egui::Label::new(RichText::new(d).small()).wrap());
                        }
                        if let Some(e) = &r.error {
                            ui.add(egui::Label::new(RichText::new(e).small().color(color_for(r.outcome))).wrap());
                        }
                    });
                }
            }
            ui.separator();
            if let Some(md) = report {
                egui::CollapsingHeader::new(t("Report preview")).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut md.as_str()).code_editor().desired_width(f32::INFINITY));
                });
            }
        });
    }

    /// Without a keyboard: the read-only USB probe, for keyboards that are not detected.
    fn probe_panel(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        ui.heading(t("Read-only probe"));
        ui.label(t(PROBE_INTRO));
        ui.add_space(8.0);
        let output = self.probe.as_ref().map(|lines| {
            format!(
                "{APP} ({} {})\n\n```text\n{}\n```\n",
                std::env::consts::OS,
                std::env::consts::ARCH,
                lines.join("\n")
            )
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.worker.busy(), egui::Button::new(t("Run probe"))).clicked() {
                self.run_probe();
            }
            if ui.add_enabled(output.is_some(), egui::Button::new(t("Copy output"))).clicked() {
                if let Some(o) = output.clone() {
                    ui.ctx().copy_text(o);
                    self.status = t("Output copied to the clipboard").to_string();
                }
            }
        });
        if let Some(lines) = &self.probe {
            ui.add_space(8.0);
            let text = lines.join("\n");
            ui.add(egui::TextEdit::multiline(&mut text.as_str()).code_editor().desired_width(f32::INFINITY));
        }
    }
}

const SELFTEST_INTRO: &str = "Checks that this app understands your keyboard. It only reads, except for three small \
changes that are undone right away: the backlight brightness, one rarely used key on profile 3 and an unused macro \
slot. The original state is restored even if a step fails. Then copy the report into a GitHub issue.";
const VERIFY_HINT: &str =
    "Every change made in this app is read back from the keyboard; a mismatch is shown as an error.";
const PROBE_INTRO: &str = "No supported keyboard is connected. The probe lists the USB interfaces of EPOMAKER \
keyboards and sends only read commands. Share its output in a GitHub issue if your keyboard is not detected.";

fn color_for(o: Outcome) -> Color32 {
    match o {
        Outcome::Failed => Color32::LIGHT_RED,
        _ => Color32::GRAY,
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

fn describe(a: KeyAction) -> String {
    match a {
        KeyAction::Key { .. } | KeyAction::Disabled | KeyAction::Raw(_) => editors::short(a),
        KeyAction::Mouse(m) => m.label().into(),
        KeyAction::Function(f) => f.label().into(),
        KeyAction::Macro { index, mode } => format!("Macro {index} ({})", mode.label()),
    }
}

fn default_backup_path() -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("epomaker-backup.json").to_string_lossy().into_owned()
}

#[cfg(feature = "autoshot")]
impl App {
    fn autoshot(&mut self, ctx: &egui::Context) {
        let Some(mut shot) = self.autoshot.take() else { return };
        ctx.request_repaint();
        // one shot per tab, then the Diagnostics tab once more without a keyboard
        let unplugged_step = shot.step == Tab::ALL.len();
        if (self.dev.is_none() && !unplugged_step) || self.worker.busy() {
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
            let path = shot.dir.join(format!("{}-{:?}.png", shot.step, self.lang).to_lowercase());
            let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create png"));
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
        if shot.step > Tab::ALL.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if shot.step == Tab::ALL.len() {
            if self.dev.is_some() {
                // what an unplugged keyboard looks like, with the read-only probe
                self.dev = None;
                self.connected = None;
                self.unplugged = true;
                self.status = self.t("Keyboard disconnected").to_string();
                self.worker.run_raw(
                    "unplug",
                    Box::new(|k: &mut Option<Keyboard>| {
                        *k = None;
                        Ok(Update::None)
                    }),
                );
                self.run_probe();
            }
            self.tab = Tab::Diagnostics;
        } else {
            self.tab = Tab::ALL[shot.step];
        }
        if self.tab == Tab::Diagnostics && self.dev.is_some() && self.selftest.is_none() {
            self.start_selftest();
        }
        if self.tab == Tab::Keymap && self.selected.is_none() {
            if let Some(d) = &self.dev {
                // show a remapped key and its editor
                let i = d.snap.model.keys.iter().find(|k| k.label == "A").and_then(|k| k.matrix_index).unwrap_or(0);
                let caps = d
                    .snap
                    .model
                    .keys
                    .iter()
                    .find(|k| k.label.starts_with("Caps"))
                    .and_then(|k| k.matrix_index)
                    .unwrap_or(0);
                let layer = self.layer;
                self.worker.run("demo remap", move |kb| {
                    kb.set_key(layer, caps, KeyAction::Disabled)?;
                    kb.set_key(
                        layer,
                        i,
                        KeyAction::Key { modifier: epomaker_core::keymap::Modifier::Ctrl, key: 0x06, key2: 0 },
                    )?;
                    Ok(Update::Layer(layer, kb.read_layer(layer)?))
                });
                self.selected = Some(i);
                let def = d.snap.model.default_entry(i).unwrap_or_default();
                self.editor = Some(ActionEditor::new(
                    KeyAction::Key { modifier: epomaker_core::keymap::Modifier::Ctrl, key: 0x06, key2: 0 },
                    def,
                ));
            }
        }
        if self.tab == Tab::Macros {
            self.macros.entry(0).or_insert(Macro {
                repeat_count: 1,
                events: vec![
                    MacroEvent::KeyDown(0xE1),
                    MacroEvent::Delay(15),
                    MacroEvent::KeyDown(0x0B),
                    MacroEvent::Delay(40),
                    MacroEvent::KeyUp(0x0B),
                    MacroEvent::KeyUp(0xE1),
                ],
            });
        }
        shot.frames += 1;
        if !shot.waiting && shot.frames > 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            shot.waiting = true;
        }
        self.autoshot = Some(shot);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(feature = "autoshot")]
        self.autoshot(&ui.ctx().clone());
        if let Some(list) = self.hotplug.as_ref().and_then(|h| h.poll()) {
            match list {
                Ok(list) => self.devices_changed(list),
                Err(e) => log::warn!("USB scan: {e}"),
            }
        }
        for (label, r) in self.worker.poll() {
            match r {
                Ok(u) => {
                    self.error = None;
                    self.apply(u);
                }
                Err(e) => {
                    match label.as_str() {
                        "connect" => self.connecting = None,
                        "self-test" => {
                            if let Some(st) = &mut self.selftest {
                                st.running = false;
                            }
                        }
                        _ => {}
                    }
                    self.error = Some(format!("{label}: {e}"));
                }
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
                    ui.colored_label(Color32::LIGHT_RED, format!("{}: {e}", self.t("Error")));
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
                let enabled = self.dev.is_some() || tab == Tab::Diagnostics;
                if ui.add_enabled(enabled, egui::Button::selectable(self.tab == tab, text)).clicked() {
                    self.tab = tab;
                }
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.dev.is_none() && self.tab != Tab::Diagnostics {
                ui.centered_and_justified(|ui| {
                    if self.worker.busy() {
                        ui.spinner();
                    } else if self.unplugged {
                        ui.label(
                            RichText::new(format!(
                                "{}\n\n{}",
                                self.t("Keyboard disconnected"),
                                self.t("It will reconnect automatically when plugged in again.")
                            ))
                            .size(18.0),
                        );
                    } else {
                        ui.label(RichText::new(self.t("No keyboard connected")).size(18.0));
                    }
                });
                return;
            }
            match self.tab {
                Tab::Lighting => self.lighting_tab(ui),
                Tab::Keymap => self.keymap_tab(ui),
                Tab::Colors => self.colors_tab(ui),
                Tab::Macros => self.macros_tab(ui),
                Tab::Settings => self.settings_tab(ui),
                Tab::Backup => self.backup_tab(ui),
                Tab::Diagnostics => self.diagnostics_tab(ui),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_strings_are_translated() {
        let mut strings = vec![SELFTEST_INTRO, VERIFY_HINT, PROBE_INTRO, Tab::Diagnostics.label()];
        strings.extend(Step::ALL.iter().map(|s| s.title()));
        for s in strings {
            assert_ne!(tr(Lang::Es, s), s, "missing Spanish for {s:?}");
        }
    }
}
