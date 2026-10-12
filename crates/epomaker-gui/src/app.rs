//! Main window.

mod agent;
mod colors;
mod diagnostics;
mod lighting;
mod tester;

use crate::editors::{self, ActionEditor, Recorder};
use crate::i18n::{tr, Lang};
use crate::keyboard_view::{self, KeyStyle};
use crate::legends::{self, Legend};
use crate::theme::{self, icon, Accent};
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
    Tester,
    Agent,
    Settings,
    Backup,
    Diagnostics,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Lighting => "Lighting",
            Tab::Keymap => "Key mapping",
            Tab::Colors => "Per-key colors",
            Tab::Macros => "Macros",
            Tab::Tester => "Key tester",
            Tab::Agent => "Tray agent",
            Tab::Settings => "Settings",
            Tab::Backup => "Backup",
            Tab::Diagnostics => "Diagnostics",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Lighting => icon::LIGHTBULB,
            Tab::Keymap => icon::KEYBOARD,
            Tab::Colors => icon::PALETTE,
            Tab::Macros => icon::LIGHTNING,
            Tab::Tester => icon::CURSOR_CLICK,
            Tab::Agent => icon::TRAY,
            Tab::Settings => icon::SLIDERS_HORIZONTAL,
            Tab::Backup => icon::FLOPPY_DISK,
            Tab::Diagnostics => icon::STETHOSCOPE,
        }
    }

    /// One line under the page title.
    fn subtitle(self) -> &'static str {
        match self {
            Tab::Lighting => "Backlight effect, speed, brightness and color.",
            Tab::Keymap => "Click a key to choose what it sends, for each profile and Fn layer.",
            Tab::Colors => "Paint the custom picture key by key.",
            Tab::Macros => "Record or edit key sequences, then assign them to keys.",
            Tab::Tester => {
                "Press keys while this window is focused. It shows what the computer receives, after your remapping."
            }
            Tab::Agent => {
                "Runs in the background: switches profiles when you change application, keeps the PC effects running and warns when the battery is low."
            }
            Tab::Settings => "Polling rate, keyboard options, debounce and sleep timers.",
            Tab::Backup => "Save the whole configuration of the keyboard to a file, or load one.",
            Tab::Diagnostics => "Check that this panel works with your keyboard, and gather data for a bug report.",
        }
    }
}

/// A section of the sidebar.
struct NavGroup {
    label: &'static str,
    tabs: &'static [Tab],
}

const NAV: [NavGroup; 3] = [
    NavGroup { label: "Device", tabs: &[Tab::Lighting, Tab::Colors, Tab::Keymap, Tab::Macros] },
    NavGroup { label: "Tools", tabs: &[Tab::Tester, Tab::Agent, Tab::Backup] },
    NavGroup { label: "System", tabs: &[Tab::Settings, Tab::Diagnostics] },
];

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
    System,
    Light,
    #[default]
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

const STYLE_VERSION: u8 = 1;

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
    /// Read back every write from the keyboard.
    verify: bool,
    accent: Accent,
    legend: Legend,
    /// 0: saved before the 0.4 restyle (the theme default changed to dark).
    #[serde(default)]
    style_version: u8,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            lang: None,
            theme: ThemeChoice::Dark,
            zoom: 1.0,
            last_device: None,
            recent_colors: vec![],
            verify: true,
            light_auto: true,
            accent: Accent::default(),
            legend: Legend::default(),
            style_version: STYLE_VERSION,
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
    accent: Accent,
    legend: Legend,
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
    macro_file: String,
    last_heartbeat: Option<std::time::Instant>,
    tester: tester::TesterState,
    // diagnostics
    verify: bool,
    selftest: Option<diagnostics::SelfTestRun>,
    probe: Option<Vec<String>>,
    // hotplug
    hotplug: Option<crate::worker::Hotplug>,
    /// Target of the connection in progress / of the open connection.
    connecting: Option<Target>,
    connected: Option<Target>,
    /// The connected keyboard disappeared from USB.
    unplugged: bool,
    agent: agent::AgentState,
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
        let mut prefs: Prefs = match cc.storage {
            Some(s) if !fresh => eframe::get_value(s, eframe::APP_KEY).unwrap_or_default(),
            _ => Prefs::default(),
        };
        if prefs.style_version < STYLE_VERSION {
            prefs.theme = ThemeChoice::Dark;
        }
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, prefs.accent);
        legends::set(prefs.legend);
        crate::rawkeys::set_context(&cc.egui_ctx);
        let zoom = if ZOOMS.iter().any(|z| (z - prefs.zoom).abs() < 0.01) { prefs.zoom } else { 1.0 };
        let theme = if fresh { ThemeChoice::Dark } else { prefs.theme };
        cc.egui_ctx.set_theme(theme.egui());
        cc.egui_ctx.set_zoom_factor(zoom);
        let mut colors = ColorsState::default();
        colors.recent = prefs.recent_colors.iter().copied().take(colors::RECENT_MAX).collect();
        let mut app = App {
            lang: prefs.lang.unwrap_or_else(Lang::detect),
            theme,
            accent: prefs.accent,
            legend: prefs.legend,
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
            macro_file: default_macro_path(),
            last_heartbeat: None,
            tester: Default::default(),
            verify: prefs.verify,
            selftest: None,
            probe: None,
            hotplug: None,
            connecting: None,
            connected: None,
            unplugged: false,
            agent: Default::default(),
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
        app.hotplug = Some(crate::worker::Hotplug::start(cc.egui_ctx.clone(), std::time::Duration::from_secs(2)));
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

    /// Tells a running tray agent that the panel is open, so it releases the keyboard.
    fn heartbeat(&mut self, ctx: &egui::Context) {
        #[cfg(feature = "autoshot")]
        if self.autoshot.is_some() {
            return;
        }
        let due = self.last_heartbeat.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(4));
        if due {
            epomaker_host::agent_config::panel_heartbeat();
            self.last_heartbeat = Some(std::time::Instant::now());
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(4));
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

    /// Handles a new USB device list from the hotplug thread: notices when the open
    /// keyboard is gone and connects to one that just appeared.
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
            self.worker.set_live(None, None);
            let text = self.t("Keyboard disconnected").to_string();
            self.toast(ToastKind::Error, text);
            self.worker.run_raw(
                JobInfo::quiet("Disconnect"),
                Box::new(|k: &mut Option<Keyboard>, _| {
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
            let saved = self.saved_target.clone();
            let pick = appeared
                .iter()
                .find(|c| saved.as_ref() == Some(&SavedTarget::of(&Target::Hid((*c).clone()))))
                .unwrap_or(&appeared[0]);
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
        self.colors.reset();
        self.selftest = None;
        self.connecting = Some(target.clone());
        self.worker.set_live(None, None);
        let verify = self.verify;
        self.saved_target = Some(SavedTarget::of(&target));
        self.worker.run_raw(
            JobInfo { label: "Connect", notify: false, blocking: true },
            Box::new(move |slot: &mut Option<Keyboard>, progress| {
                *slot = None;
                let mut kb = match target {
                    Target::Demo(id) => Keyboard::emulated(id)?,
                    Target::Hid(c) => {
                        if epomaker_host::agent_config::agent_running() {
                            // the tray agent polls every 250 ms: let it notice the panel and let go
                            epomaker_host::agent_config::panel_heartbeat();
                            std::thread::sleep(std::time::Duration::from_millis(600));
                        }
                        let dongle = Database::get().devices.iter().any(|d| d.pid == c.pid && d.dongle)
                            && !Database::get().devices.iter().any(|d| d.pid == c.pid && !d.dongle);
                        Keyboard::connect(Box::new(HidTransport::open(&c)?), dongle)?
                    }
                };
                kb.set_verify(verify);
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
                let name = s.model.display_name(&s.info.display_name);
                self.status = format!("{name} — {}{demo}", s.describe);
                let text = format!("{}: {name}{demo}", self.t("Connected"));
                self.toast(ToastKind::Success, text);
                let layers = s.layers.iter().cloned().collect();
                self.dev = Some(DeviceState { snap: *s, layers });
                self.unplugged = false;
                // a connection made by connect() (factory reset and restore also send Connected)
                if let Some(t) = self.connecting.take() {
                    self.connected = Some(t);
                }
            }
            Update::Disconnected => {
                self.dev = None;
                self.connected = None;
            }
            Update::SelfTestDevice(d) => {
                if let Some(st) = &mut self.selftest {
                    st.device = Some(*d);
                }
            }
            Update::SelfTestStep(r) => {
                if let Some(st) = &mut self.selftest {
                    st.results.push(*r);
                    st.running = st.results.len() < epomaker_core::selftest::Step::ALL.len();
                }
            }
            Update::Probe(lines) => self.probe = Some(lines),
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
                    if info.label == "Connect" {
                        self.connecting = None;
                        #[cfg(target_os = "linux")]
                        self.udev.show_for(&e.to_string());
                    }
                    if info.label == "Self-test" {
                        if let Some(st) = &mut self.selftest {
                            st.running = false;
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ top bar

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let lang = self.lang;
        let t = |s| tr(lang, s);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            // device badge, name and connection
            let (rect, _) = ui.allocate_exact_size(egui::vec2(46.0, 46.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 11.0, p.accent.gamma_multiply(0.15));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon::KEYBOARD,
                egui::FontId::proportional(26.0),
                p.accent,
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(2.0);
                let (name, line, dot) = match &self.dev {
                    Some(d) => {
                        let s = &d.snap;
                        let link = if s.emulated {
                            t("Demo, no hardware")
                        } else if s.info.dongle {
                            t("2.4 GHz receiver")
                        } else {
                            "USB"
                        };
                        let legend = t(legends::current().label());
                        let line = format!("{} · {link} · FW {:04x} · {legend}", s.info.company, s.firmware);
                        (
                            s.model.display_name(&s.info.display_name).to_uppercase(),
                            line,
                            if s.emulated { p.warn } else { p.ok },
                        )
                    }
                    None if self.worker.busy() => (t("Connecting…").to_string(), String::new(), p.warn),
                    None => (
                        t("No keyboard connected").to_string(),
                        t("Plug in a keyboard, or pick a demo model on the right.").to_string(),
                        p.text_dim,
                    ),
                };
                ui.label(RichText::new(name).size(22.0).family(theme::display()).color(p.text_strong));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    theme::dot(ui, dot);
                    ui.label(RichText::new(line).small().color(p.text_dim));
                });
            });
            if let Some(d) = &self.dev {
                ui.add_space(8.0);
                let b = d.snap.battery;
                let glyph = match (b.state, b.percent) {
                    (ChargeState::Charging, _) => icon::BATTERY_CHARGING,
                    (_, 75..) => icon::BATTERY_FULL,
                    (_, 50..) => icon::BATTERY_HIGH,
                    (_, 25..) => icon::BATTERY_MEDIUM,
                    (_, 10..) => icon::BATTERY_LOW,
                    _ => icon::BATTERY_WARNING,
                };
                let color = match b.percent {
                    0..=15 => p.error,
                    16..=30 => p.warn,
                    _ => p.ok,
                };
                let state = match b.state {
                    ChargeState::Discharging => t("discharging"),
                    ChargeState::Charging => t("charging"),
                    ChargeState::Full => t("full"),
                };
                let r = theme::pill(ui, &format!("{glyph} {}%", b.percent), color)
                    .interact(egui::Sense::click())
                    .on_hover_text(format!("{} ({state}). {}", t("Battery"), t("Click to read it again.")));
                if r.clicked() {
                    self.worker.run("Read battery", |kb| Ok(Update::Battery(kb.battery()?)));
                }
                // active profile
                ui.add_space(4.0);
                ui.label(RichText::new(t("Profile")).color(p.text_dim));
                let mut profile = d.snap.profile;
                let items: Vec<(u8, String)> = (0..3u8).map(|i| (i, format!("{}", i + 1))).collect();
                let enabled = !self.worker.busy();
                let changed = ui.add_enabled_ui(enabled, |ui| theme::segmented(ui, &mut profile, &items)).inner;
                if changed {
                    self.set_profile(profile);
                }
            }
            if self.worker.busy() {
                ui.add_space(8.0);
                let what = self.t(self.worker.current()).to_string();
                match self.worker.progress() {
                    Some(f) => {
                        ui.add(egui::ProgressBar::new(f).desired_width(120.0).desired_height(6.0).fill(p.accent));
                    }
                    None => {
                        ui.add(egui::Spinner::new().color(p.accent));
                    }
                }
                ui.label(RichText::new(format!("{what}…")).color(p.text_dim));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                self.preferences_menu(ui);
                ui.add_space(6.0);
                self.device_picker(ui);
            });
        });
    }

    fn set_profile(&mut self, profile: u8) {
        if let Some(d) = &mut self.dev {
            d.snap.profile = profile;
        }
        self.worker.run("Set profile", move |kb| {
            kb.set_profile(profile)?;
            Ok(Update::Profile(kb.profile()?))
        });
    }

    /// Device list, rescan and connect / disconnect (right side of the header).
    fn device_picker(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let busy = self.worker.busy();
        if self.dev.is_some() {
            let b = egui::Button::new(format!("{} {}", icon::PLUGS, tr(lang, "Disconnect")));
            if ui.add_enabled(!busy, b).clicked() {
                self.worker.run_raw(
                    JobInfo::quiet("Disconnect"),
                    Box::new(|k: &mut Option<Keyboard>, _| {
                        *k = None;
                        Ok(Update::Disconnected)
                    }),
                );
            }
        }
        let differs = self.dev.is_none() || self.connected != self.target;
        if differs && theme::primary_button(ui, !busy && self.target.is_some(), tr(lang, "Connect")).clicked() {
            if let Some(t) = self.target.clone() {
                self.connect(t);
            }
        }
        if theme::icon_button(ui, icon::ARROWS_CLOCKWISE, tr(lang, "Look for keyboards again")).clicked() {
            self.refresh_devices();
        }
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
        let label = |t: &Option<Target>| match t {
            Some(Target::Hid(c)) => {
                // Windows reports a generic "(Standard system devices)" product name
                let product = c.product.trim();
                let name =
                    if product.is_empty() || product.starts_with('(') { tr(lang, "USB keyboard") } else { product };
                format!("{} {name} · {:04x}:{:04x}", icon::USB, c.vid, c.pid)
            }
            Some(Target::Demo(id)) => {
                let d = Database::get().device_by_id(*id);
                let (name, key) = d.map(|d| (d.display_name.as_str(), d.name.as_str())).unwrap_or_default();
                format!("{} {name} ({key}) · {}", icon::FLASK, tr(lang, "demo, no hardware"))
            }
            None => tr(lang, "No keyboard connected").to_string(),
        };
        let current = label(&self.target);
        let mut new_target = None;
        ui.add_enabled_ui(!self.worker.blocked(), |ui| {
            ComboBox::from_id_salt("device").width(260.0).truncate().selected_text(current).show_ui(ui, |ui| {
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
    }

    fn preferences_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let button = RichText::new(icon::GEAR_SIX).size(19.0);
        ui.menu_button(button, |ui| {
            ui.set_min_width(220.0);
            theme::section_label(ui, t("Language"));
            ui.radio_value(&mut self.lang, Lang::En, "English");
            ui.radio_value(&mut self.lang, Lang::Es, "Español");
            ui.separator();
            theme::section_label(ui, t("Theme"));
            for th in ThemeChoice::ALL {
                if ui.radio_value(&mut self.theme, th, t(th.label())).changed() {
                    ui.ctx().set_theme(th.egui());
                }
            }
            ui.separator();
            theme::section_label(ui, t("Accent color"));
            ui.horizontal(|ui| {
                for a in Accent::ALL {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                    ui.painter().circle_filled(rect.center(), 9.0, a.color());
                    if self.accent == a {
                        ui.painter().circle_stroke(
                            rect.center(),
                            11.5,
                            egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                        );
                    }
                    if resp.on_hover_text(t(a.label())).clicked() {
                        self.accent = a;
                        theme::apply(ui.ctx(), a);
                    }
                }
            });
            ui.separator();
            theme::section_label(ui, t("Keycap legends"));
            let detected = legends::detect().map(|l| t(l.label()).to_string());
            for l in Legend::ALL {
                let mut text = t(l.label()).to_string();
                if l == Legend::Auto {
                    text = format!("{text} ({})", detected.as_deref().unwrap_or(t("not detected")));
                }
                if ui.radio_value(&mut self.legend, l, text).changed() {
                    legends::set(l);
                }
            }
            ui.separator();
            theme::section_label(ui, t("Interface size"));
            ui.horizontal_wrapped(|ui| {
                for z in ZOOMS {
                    if ui.selectable_label(self.zoom == z, format!("{:.0}%", z * 100.0)).clicked() {
                        self.zoom = z;
                        ui.ctx().set_zoom_factor(z);
                    }
                }
            });
        });
    }

    /// Central area when no keyboard is connected.
    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.28).max(20.0));
            if self.worker.busy() {
                ui.add(egui::Spinner::new().size(36.0).color(p.accent));
                return;
            }
            let glyph = if self.unplugged { icon::PLUGS } else { icon::KEYBOARD };
            ui.label(RichText::new(glyph).size(64.0).color(p.text_dim.gamma_multiply(0.6)));
            ui.add_space(8.0);
            let title = if self.unplugged { "Keyboard disconnected" } else { "No keyboard connected" };
            ui.label(
                RichText::new(self.t(title).to_uppercase()).size(24.0).family(theme::display()).color(p.text_strong),
            );
            let hint = if self.unplugged {
                "It will reconnect automatically when plugged in again."
            } else {
                "Plug in a keyboard, or pick a demo model on the right."
            };
            ui.label(RichText::new(self.t(hint)).color(p.text_dim));
            ui.add_space(14.0);
            if !self.candidates.is_empty() || self.target.is_some() {
                let enabled = self.target.is_some();
                if theme::primary_button(ui, enabled, self.t("Connect")).clicked() {
                    if let Some(t) = self.target.clone() {
                        self.connect(t);
                    }
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
        let mut items: Vec<(Layer, String)> = (0..3u8)
            .map(|p| {
                let mark = if p == active { " •" } else { "" };
                (Layer::Profile(p), format!("{} {}{mark}", tr(lang, "Profile"), p + 1))
            })
            .collect();
        for f in 0..fn_layers {
            let text = if fn_layers > 1 {
                format!("{} {}", tr(lang, "Fn layer"), f + 1)
            } else {
                tr(lang, "Fn layer").to_string()
            };
            items.push((Layer::Fn(f), text));
        }
        if theme::segmented(ui, &mut self.layer, &items) {
            self.editor = None;
        }
    }

    fn keymap_tab(&mut self, ui: &mut egui::Ui) {
        let busy = self.worker.busy();
        ui.horizontal(|ui| {
            self.layer_selector(ui);
            let layer = self.layer;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if ui.button(format!("{} {}", icon::DOWNLOAD_SIMPLE, self.t("Read from keyboard"))).clicked() {
                        self.worker.run("Read layer", move |kb| Ok(Update::Layer(layer, kb.read_layer(layer)?)));
                    }
                    if ui
                        .button(format!("{} {}", icon::ARROW_COUNTER_CLOCKWISE, self.t("Reset layer to factory")))
                        .clicked()
                    {
                        self.worker.run_notify("Reset layer", move |kb| {
                            kb.reset_layer(layer)?;
                            Ok(Update::Layer(layer, kb.read_layer(layer)?))
                        });
                    }
                });
            });
        });
        ui.add_space(8.0);
        let Some(dev) = &self.dev else { return };
        let model = dev.snap.model;
        let Some(matrix) = dev.layers.get(&self.layer).cloned() else { return };
        let selected = self.selected;
        let lang = self.lang;
        let pal = theme::palette(ui);
        let max_h = (ui.available_height() * 0.5).max(180.0);
        let resp = keyboard_view::show(ui, model, max_h, |k| {
            let entry = matrix.entries.get(k.idx).copied().unwrap_or_default();
            let default = model.default_entry(k.idx).unwrap_or_default();
            let changed = entry != default;
            let action = KeyAction::from_bytes(entry);
            KeyStyle {
                fill: if changed { pal.accent_fill } else { pal.control },
                text: if changed { editors::short(action) } else { k.label.clone() },
                text_color: if changed { pal.on_accent } else { pal.text },
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
        ui.add_space(12.0);
        let used = dev.snap.used_slots.clone();
        let layer = self.layer;
        egui::ScrollArea::vertical().id_salt("key_editor").show(ui, |ui| {
            theme::card(ui, "", |ui| match (self.selected, &mut self.editor) {
                (Some(i), Some(ed)) => {
                    let label = model
                        .key_for_matrix_index(i)
                        .map(|k| legends::label(k.code, &k.label).replace('\n', " "))
                        .unwrap_or_default();
                    let entry = matrix.entries.get(i).copied().unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(label.to_uppercase())
                                .size(20.0)
                                .family(theme::display())
                                .color(pal.text_strong),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                tr(lang, "now"),
                                editors::describe(KeyAction::from_bytes(entry), lang)
                            ))
                            .color(pal.text_dim),
                        );
                    });
                    ui.add_space(4.0);
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
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon::CURSOR_CLICK).size(20.0).color(pal.accent));
                        ui.label(RichText::new(tr(lang, "Select a key on the keyboard.")).color(pal.text_dim));
                    });
                }
            });
        });
    }

    // ------------------------------------------------------------ macros

    fn macros_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let slots = dev.snap.model.macro_slots;
        let used = dev.snap.used_slots.clone();
        let lang = self.lang;
        let busy = self.worker.busy();
        let t = |s| tr(lang, s);
        let pal = theme::palette(ui);
        let height = ui.available_height();
        let mut note: Option<Result<String, String>> = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.allocate_ui_with_layout(egui::vec2(180.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                theme::card(ui, t("Slots"), |ui| {
                    egui::ScrollArea::vertical().id_salt("macro_slots").auto_shrink([false, false]).show(ui, |ui| {
                        for s in 0..slots {
                            let text = editors::slot_label(s, &used, lang);
                            let b = egui::Button::selectable(self.macro_slot == s, text)
                                .min_size(egui::vec2(ui.available_width(), 28.0));
                            if ui.add(b).clicked() {
                                self.macro_slot = s;
                                self.recorder = None;
                                if !self.macros.contains_key(&s) {
                                    self.worker.run("Read macro", move |kb| Ok(Update::Macro(s, kb.read_macro(s)?)));
                                }
                            }
                        }
                    });
                });
            });
            let slot = self.macro_slot;
            if !self.macros.contains_key(&slot) && !busy {
                self.macros.insert(slot, Macro { repeat_count: 1, events: vec![] });
                self.worker.run("Read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
            }
            let Some(m) = self.macros.get_mut(&slot) else { return };
            ui.vertical(|ui| {
                let fits = m.encode().is_ok();
                theme::card(ui, "", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} {}", t("Macro"), slot).to_uppercase())
                                .size(20.0)
                                .family(theme::display())
                                .color(pal.text_strong),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::DragValue::new(&mut m.repeat_count).range(1..=65535));
                            ui.label(t("Repeat count"));
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        match &mut self.recorder {
                            Some(rec) => {
                                rec.capture(ui, m);
                                if theme::danger_button(ui, true, &format!("{} {}", icon::STOP, t("Stop recording")))
                                    .clicked()
                                {
                                    self.recorder = None;
                                }
                            }
                            None => {
                                let text = format!("{} {}", icon::RECORD, t("Record"));
                                if theme::primary_button(ui, true, &text).clicked() {
                                    self.recorder = Some(Recorder::new(ui.input(|i| i.time)));
                                }
                            }
                        }
                        if ui.button(t("Add key press")).clicked() {
                            m.events.extend([
                                MacroEvent::KeyDown(0x04),
                                MacroEvent::Delay(20),
                                MacroEvent::KeyUp(0x04),
                            ]);
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
                        if ui.button(t("Add mouse move")).clicked() {
                            m.events.extend([MacroEvent::MouseMove { dx: 20, dy: 0 }, MacroEvent::Delay(10)]);
                        }
                        if ui.button(format!("{} {}", icon::TRASH, t("Clear"))).clicked() {
                            m.events.clear();
                        }
                    });
                    if self.recorder.is_some() {
                        ui.colored_label(
                            pal.error,
                            format!("{} {}", icon::RECORD, t("Recording: type on this window…")),
                        );
                    }
                    let used_bytes =
                        m.encode().map(|b| b.iter().rposition(|x| *x != 0).map(|i| i + 1).unwrap_or(0)).unwrap_or(256);
                    ui.horizontal(|ui| {
                        let fill = if fits { pal.accent } else { pal.error };
                        ui.add(
                            egui::ProgressBar::new(used_bytes.min(256) as f32 / 256.0)
                                .desired_width(160.0)
                                .desired_height(6.0)
                                .fill(fill),
                        );
                        ui.label(RichText::new(format!("{}: {used_bytes} / 256", t("Bytes used"))).small().weak());
                        if !fits {
                            ui.colored_label(pal.error, t("Macro too long"));
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("macro_events")
                        .max_height((ui.available_height() - 150.0).max(120.0))
                        .show(ui, |ui| {
                            editors::macro_events_ui(ui, lang, m);
                        });
                    ui.separator();
                    let m2 = m.clone();
                    ui.horizontal(|ui| {
                        let text = format!("{} {}", icon::UPLOAD_SIMPLE, t("Write to keyboard"));
                        if theme::primary_button(ui, fits && !busy, &text).clicked() {
                            self.worker.run_notify("Write macro", move |kb| {
                                kb.write_macro(slot, &m2)?;
                                Ok(Update::Macro(slot, kb.read_macro(slot)?))
                            });
                        }
                        let read = egui::Button::new(format!("{} {}", icon::DOWNLOAD_SIMPLE, t("Read from keyboard")));
                        if ui.add_enabled(!busy, read).clicked() {
                            self.worker.run("Read macro", move |kb| Ok(Update::Macro(slot, kb.read_macro(slot)?)));
                        }
                    });
                });
                ui.add_space(12.0);
                // macro library: share macros as files
                theme::card(ui, t("File"), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(format!("{} {}", icon::FOLDER_OPEN, t("Import"))).clicked() {
                            note = Some(
                                match epomaker_core::macros::MacroFile::load(std::path::Path::new(&self.macro_file)) {
                                    Ok(f) => {
                                        *m = f.r#macro;
                                        Ok(format!("{}: {}", t("Loaded"), f.name))
                                    }
                                    Err(e) => Err(e.to_string()),
                                },
                            );
                        }
                        if ui.button(format!("{} {}", icon::FLOPPY_DISK, t("Export"))).clicked() {
                            let path = std::path::PathBuf::from(&self.macro_file);
                            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                            note = Some(
                                epomaker_core::macros::MacroFile::new(name, m.clone())
                                    .save(&path)
                                    .map(|_| format!("{}: {}", t("Saved"), path.display()))
                                    .map_err(|e| e.to_string()),
                            );
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.macro_file).desired_width(f32::INFINITY));
                    });
                });
            });
        });
        match note {
            Some(Ok(msg)) => self.toast(ToastKind::Success, msg),
            Some(Err(e)) => self.toast(ToastKind::Error, e),
            None => {}
        }
    }

    // ------------------------------------------------------------ settings

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let busy = self.worker.busy();
        let Some(dev) = &mut self.dev else { return };
        let s = &mut dev.snap;
        let model = s.model;
        let mut reset_clicked = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |cols| {
                let (left, right) = cols.split_at_mut(1);
                let (left, right) = (&mut left[0], &mut right[0]);
                theme::card(left, t("Device"), |ui| {
                    egui::Grid::new("info").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                        ui.weak(t("Model"));
                        ui.label(format!(
                            "{} ({}, id {})",
                            model.display_name(&s.info.display_name),
                            s.info.name,
                            s.info.id
                        ));
                        ui.end_row();
                        ui.weak(t("Connection"));
                        ui.label(RichText::new(&s.describe).small());
                        ui.end_row();
                        ui.weak(t("Firmware"));
                        ui.label(format!("{:#06x}", s.firmware));
                        ui.end_row();
                        ui.weak(t("Battery"));
                        ui.horizontal(|ui| {
                            let state = match s.battery.state {
                                ChargeState::Discharging => t("discharging"),
                                ChargeState::Charging => t("charging"),
                                ChargeState::Full => t("full"),
                            };
                            ui.label(format!("{}% ({state})", s.battery.percent));
                            if ui.small_button(icon::ARROWS_CLOCKWISE).on_hover_text(t("Read battery")).clicked() {
                                self.worker.run("Read battery", |kb| Ok(Update::Battery(kb.battery()?)));
                            }
                        });
                        ui.end_row();
                    });
                });
                left.add_space(12.0);
                let p = s.profile as usize;
                let title = format!("{} {}", t("Profile"), p + 1);
                theme::card(left, &title, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t("Polling rate")).color(ui.visuals().strong_text_color()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    let o = &mut s.options[p];
                    let mut changed = false;
                    changed |= theme::toggle_row(ui, &mut o.win_key_lock, t("Disable Windows key"), "").changed();
                    changed |= theme::toggle_row(ui, &mut o.mac_mode, t("Mac mode"), "").changed();
                    changed |= theme::toggle_row(ui, &mut o.wasd_arrows_swap, t("Swap WASD and arrows"), "").changed();
                    changed |= theme::toggle_row(ui, &mut o.fn_key_swap, t("Swap F-keys and media keys"), "").changed();
                    changed |= theme::toggle_row(ui, &mut o.led_off, t("Backlight off"), "").changed();
                    changed |= theme::toggle_row(ui, &mut o.power_save, t("Power saving"), "").changed();
                    if changed {
                        let (o, pp) = (*o, p as u8);
                        self.worker.run("Set options", move |kb| {
                            kb.set_options(pp, &o)?;
                            Ok(Update::None)
                        });
                    }
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(t("Options apply to the active profile; switch profiles in the header."))
                            .small()
                            .weak(),
                    );
                });

                theme::card(right, t("Typing"), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t("Debounce (ms)")).color(ui.visuals().strong_text_color()));
                        ui.add_space(8.0);
                        let r = ui.add(Slider::new(&mut s.debounce, 1..=50));
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            let d = s.debounce;
                            self.worker.run("Set debounce", move |kb| {
                                kb.set_debounce(d)?;
                                Ok(Update::None)
                            });
                        }
                    });
                    ui.add_space(4.0);
                    if theme::toggle_row(ui, &mut s.auto_os, t("Automatic OS detection"), "").changed() {
                        let v = s.auto_os;
                        self.worker.run("Set auto OS", move |kb| {
                            kb.set_auto_os(v)?;
                            Ok(Update::None)
                        });
                    }
                });
                if model.sleep_bt.is_some() || model.sleep_dongle.is_some() {
                    right.add_space(12.0);
                    theme::card(right, t("Sleep timers (minutes)"), |ui| {
                        let mut sl: SleepTimes = s.sleep;
                        let mut changed = false;
                        ui.spacing_mut().slider_width = 110.0;
                        egui::Grid::new("sleep").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                            if let Some(l) = model.sleep_bt {
                                changed |= minutes(
                                    ui,
                                    t("Bluetooth: light off"),
                                    &mut sl.bt_light_off,
                                    l.min,
                                    l.max,
                                    true,
                                    lang,
                                );
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
                                changed |= minutes(
                                    ui,
                                    t("2.4 GHz: light off"),
                                    &mut sl.dongle_light_off,
                                    l.min,
                                    l.max,
                                    true,
                                    lang,
                                );
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
                    });
                }
                right.add_space(12.0);
                theme::card(right, t("Factory reset"), |ui| {
                    ui.label(
                        RichText::new(t("Erases every profile, key mapping, macro and lighting setting."))
                            .small()
                            .weak(),
                    );
                    ui.add_space(4.0);
                    reset_clicked = theme::danger_button(ui, !busy, t("Factory reset…")).clicked();
                });
            });
        });
        if reset_clicked {
            self.confirm_reset = true;
        }
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
        theme::card(ui, t("Backup file"), |ui| {
            ui.horizontal(|ui| {
                ui.label(icon::FILE_TEXT);
                ui.add(egui::TextEdit::singleline(&mut self.backup_path).desired_width(f32::INFINITY));
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, !busy, &format!("{} {}", icon::FLOPPY_DISK, t("Save backup"))).clicked() {
                    let path = self.backup_path.clone();
                    let done = format!("{}: {path}", t("Saved"));
                    self.worker.run_with(JobInfo::quiet("Save backup"), move |kb, _| {
                        let b = Backup::capture(kb)?;
                        std::fs::write(&path, serde_json::to_string_pretty(&b)?)?;
                        Ok(Update::Message(done))
                    });
                }
                let load = egui::Button::new(format!("{} {}", icon::FOLDER_OPEN, t("Load backup")));
                if ui.add_enabled(!busy, load).clicked() {
                    let path = self.backup_path.clone();
                    let done = format!("{}: {path}", t("Restored"));
                    self.macros.clear();
                    let info = JobInfo { label: "Load backup", notify: false, blocking: true };
                    self.worker.run_with(info, move |kb, progress| {
                        let b: Backup = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                        b.restore(kb)?;
                        Ok(Update::Many(vec![
                            Update::Connected(Box::new(snapshot(kb, progress)?)),
                            Update::Message(done),
                        ]))
                    });
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(t(
                    "Includes every profile, the Fn layers, macros, lighting, per-key colors and settings.",
                ))
                .small()
                .weak(),
            );
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

fn default_macro_path() -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("epomaker-macro.json").to_string_lossy().into_owned()
}

fn default_backup_path() -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("epomaker-backup.json").to_string_lossy().into_owned()
}

#[cfg(feature = "autoshot")]
impl App {
    const SHOTS: [&'static str; 14] = [
        "lighting",
        "lighting-ripple",
        "keymap",
        "keymap-search",
        "colors",
        "macros",
        "lighting-music",
        "tester",
        "agent",
        "settings",
        "backup",
        "diagnostics",
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
            "lighting-music" => {
                self.tab = Tab::Lighting;
                self.light = LightEffect { mode: LightMode::MusicFollow3, brightness: 4, ..LightEffect::default() };
                self.send_light();
            }
            "tester" => {
                self.worker.set_live(None, None);
                self.tab = Tab::Tester;
                let t = ctx.input(|i| i.time);
                for (dt, code, down) in [(0.0, 0x04, true), (0.08, 0x04, false), (0.1, 0x16, true), (0.18, 0x16, false)]
                {
                    self.tester.record_for_demo(t + dt, code, down);
                }
                self.tester.record_for_demo(t + 0.2, 0x07, true);
                self.tester.record_for_demo(t + 0.25, 0x07, false);
                self.tester.record_for_demo(t + 0.26, 0x07, true);
            }
            "agent" => self.tab = Tab::Agent,
            "diagnostics" => {
                self.tab = Tab::Diagnostics;
                if self.selftest.is_none() {
                    self.start_selftest();
                }
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
        self.heartbeat(&ctx);
        if let Some(list) = self.hotplug.as_ref().and_then(|h| h.poll()) {
            match list {
                Ok(list) => self.devices_changed(list),
                Err(e) => log::warn!("USB scan: {e}"),
            }
        }
        self.poll_worker();
        #[cfg(target_os = "linux")]
        if self.udev.ui(&ctx, self.lang) {
            self.toast(ToastKind::Success, self.t("Rule installed. Reconnecting…").to_string());
            self.refresh_devices();
            if let Some(t @ Target::Hid(_)) = self.target.clone() {
                self.connect(t);
            }
        }
        if self.tab != Tab::Tester {
            // keys pressed elsewhere must not show up when the tester opens
            crate::rawkeys::take();
        }
        let p = theme::palette(ui);
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(p.header)
                    .inner_margin(egui::Margin { left: 18, right: 14, top: 10, bottom: 10 })
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show_separator_line(false)
            .show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(14, 4)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(e) = &self.error {
                        ui.label(RichText::new(format!("{} {e}", icon::WARNING)).small().color(p.error));
                    } else {
                        ui.label(RichText::new(&self.status).small().color(p.text_dim));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = format!(
                            "v{} · {}",
                            env!("CARGO_PKG_VERSION"),
                            self.t("Experimental: this software is not affiliated with EPOMAKER.")
                        );
                        ui.label(RichText::new(text).small().color(p.text_dim));
                    });
                });
            });
        egui::Panel::left("tabs")
            .resizable(false)
            .exact_size(212.0)
            .frame(egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin {
                left: 10,
                right: 10,
                top: 16,
                bottom: 10,
            }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for group in NAV {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        theme::section_label(ui, self.t(group.label));
                    });
                    ui.add_space(2.0);
                    for &tab in group.tabs {
                        let enabled = self.dev.is_some() || tab == Tab::Diagnostics;
                        let label = self.t(tab.label()).to_string();
                        if theme::nav_item(ui, tab.icon(), &label, self.tab == tab, enabled).clicked() {
                            self.tab = tab;
                        }
                    }
                    ui.add_space(14.0);
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin {
                left: 26,
                right: 22,
                top: 18,
                bottom: 12,
            }))
            .show(ui, |ui| {
                if self.dev.is_none() && self.tab != Tab::Diagnostics {
                    self.empty_state(ui);
                    return;
                }
                if self.worker.blocked() {
                    ui.disable();
                }
                theme::page_title(ui, self.t(self.tab.label()), self.t(self.tab.subtitle()));
                match self.tab {
                    Tab::Lighting => self.lighting_tab(ui),
                    Tab::Keymap => self.keymap_tab(ui),
                    Tab::Colors => self.colors_tab(ui),
                    Tab::Macros => self.macros_tab(ui),
                    Tab::Tester => self.tester_tab(ui),
                    Tab::Agent => self.agent_tab(ui),
                    Tab::Settings => self.settings_tab(ui),
                    Tab::Backup => self.backup_tab(ui),
                    Tab::Diagnostics => self.diagnostics_tab(ui),
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
            verify: self.verify,
            accent: self.accent,
            legend: self.legend,
            style_version: STYLE_VERSION,
        };
        eframe::set_value(storage, eframe::APP_KEY, &prefs);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.last_heartbeat.is_some() {
            epomaker_host::agent_config::panel_closed();
        }
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
            verify: false,
            accent: Accent::Green,
            legend: Legend::Es,
            style_version: STYLE_VERSION,
        };
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.lang, Some(Lang::Es));
        assert!(back.theme == ThemeChoice::Light && back.zoom == 1.25 && !back.light_auto);
        assert!(back.last_device == p.last_device && back.recent_colors == p.recent_colors);
        let old: Prefs = serde_json::from_str(r#"{"lang":"En"}"#).unwrap();
        assert!(old.light_auto && old.zoom == 1.0 && old.theme == ThemeChoice::Dark);
        assert!(old.style_version == 0 && old.accent == Accent::Cyan && old.legend == Legend::Auto);
        assert!(back.accent == Accent::Green && back.legend == Legend::Es);
        assert!(old.verify, "verify is on unless turned off");
        assert!(!back.verify);
    }
}
