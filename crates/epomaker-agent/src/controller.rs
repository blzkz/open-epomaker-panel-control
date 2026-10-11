//! The agent's background loop: owns the keyboard and reacts to commands, focus changes
//! and battery level. The tray front-ends only send [`Command`]s and read [`Status`].

use crate::config::Config;
use crate::i18n::t;
use epomaker_core::settings::{Battery, ChargeState};
use epomaker_core::Keyboard;
use epomaker_host::stream::{LiveEffect, LiveSession};
use epomaker_host::window::ActiveWindow;
use std::process::Child;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum Command {
    SetProfile(u8),
    SetLive(Option<LiveEffect>),
    SetPerApp(bool),
    SetAutostart(bool),
    OpenPanel,
    EditRules,
    Quit,
}

/// What the tray shows.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub model: Option<String>,
    pub battery: Option<Battery>,
    pub profile: Option<u8>,
    pub live: Option<LiveEffect>,
    pub live_supported: bool,
    pub per_app: bool,
    pub per_app_available: bool,
    pub active_app: Option<String>,
    pub autostart: bool,
    pub paused: bool,
    pub error: Option<String>,
    pub quit: bool,
}

pub type Shared = Arc<Mutex<Status>>;

const RECONNECT: Duration = Duration::from_secs(3);
const BATTERY_POLL: Duration = Duration::from_secs(60);
const FOCUS_POLL: Duration = Duration::from_millis(700);

pub struct Controller {
    config: Config,
    status: Shared,
    notify: Box<dyn Fn() + Send>,
    kb: Option<Keyboard>,
    live: Option<LiveSession>,
    window: Option<Box<dyn ActiveWindow>>,
    panel: Option<Child>,
    last_connect: Option<Instant>,
    last_battery: Option<Instant>,
    last_focus: Instant,
    battery_warned: bool,
}

impl Controller {
    pub fn new(config: Config, status: Shared, notify: Box<dyn Fn() + Send>) -> Controller {
        let window = match epomaker_host::window::tracker() {
            Ok(w) => Some(w),
            Err(e) => {
                log::warn!("per-application profiles unavailable: {e}");
                None
            }
        };
        {
            let mut s = status.lock().unwrap();
            s.per_app = config.per_app_profiles;
            s.per_app_available = window.is_some();
            s.autostart = crate::autostart::is_enabled();
            s.live = config.live_effect;
        }
        Controller {
            config,
            status,
            notify,
            kb: None,
            live: None,
            window,
            panel: None,
            last_connect: None,
            last_battery: None,
            last_focus: Instant::now(),
            battery_warned: false,
        }
    }

    fn update(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.status.lock().unwrap());
        (self.notify)();
    }

    pub fn run(mut self, commands: Receiver<Command>) {
        loop {
            let wait = match &self.live {
                Some(l) if self.kb.is_some() => l.effect.interval(),
                _ => Duration::from_millis(250),
            };
            match commands.recv_timeout(wait) {
                Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.poll_panel();
            if self.panel.is_some() {
                continue;
            }
            self.ensure_connected();
            if let Some(mut kb) = self.kb.take() {
                match self.step(&mut kb) {
                    Ok(()) => self.kb = Some(kb),
                    Err(e) => {
                        log::warn!("keyboard: {e}");
                        self.disconnect(Some(e));
                    }
                }
            }
        }
        self.live = None;
        self.update(|s| s.quit = true);
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::SetProfile(p) => {
                if let Some(mut kb) = self.kb.take() {
                    match kb.set_profile(p) {
                        Ok(()) => {
                            self.kb = Some(kb);
                            self.update(|s| s.profile = Some(p));
                        }
                        Err(e) => self.disconnect(Some(e.to_string())),
                    }
                }
            }
            Command::SetLive(effect) => {
                self.config.live_effect = effect;
                self.config.save();
                self.live = None;
                self.update(|s| {
                    s.live = effect;
                    s.error = None;
                });
                if let Some(effect) = effect {
                    self.start_live(effect);
                }
            }
            Command::SetPerApp(on) => {
                self.config.per_app_profiles = on;
                self.config.save();
                self.update(|s| s.per_app = on);
            }
            Command::SetAutostart(on) => {
                if let Err(e) = crate::autostart::set(on) {
                    log::warn!("autostart: {e}");
                }
                let enabled = crate::autostart::is_enabled();
                self.update(|s| s.autostart = enabled);
            }
            Command::OpenPanel => self.open_panel(),
            Command::EditRules => open_in_editor(&crate::config::path()),
            Command::Quit => {}
        }
    }

    fn ensure_connected(&mut self) {
        if self.kb.is_some() || self.last_connect.is_some_and(|t| t.elapsed() < RECONNECT) {
            return;
        }
        self.last_connect = Some(Instant::now());
        let demo = std::env::var("EPOMAKER_AGENT_DEMO").ok().and_then(|v| v.parse::<i64>().ok());
        let opened = match demo {
            Some(id) => Keyboard::emulated(id),
            None => Keyboard::open_first(),
        };
        let Ok(mut kb) = opened else { return };
        let profile = kb.profile().ok();
        let battery = kb.battery().ok();
        let model = kb.info.display_name.clone();
        let live_ok =
            [epomaker_core::lighting::LightMode::MusicFollow3, epomaker_core::lighting::LightMode::ScreenColor]
                .iter()
                .any(|m| kb.model.caps(*m).is_some());
        log::info!("connected to {model}");
        self.kb = Some(kb);
        self.last_battery = Some(Instant::now());
        self.update(|s| {
            s.model = Some(model);
            s.profile = profile;
            s.battery = battery;
            s.live_supported = live_ok;
            s.error = None;
        });
        if let Some(effect) = self.config.live_effect {
            self.start_live(effect);
        }
    }

    fn start_live(&mut self, effect: LiveEffect) {
        let Some(kb) = self.kb.as_mut() else { return };
        let started = LiveSession::start(effect).and_then(|mut s| {
            s.activate(kb, None)?;
            Ok(s)
        });
        match started {
            Ok(s) => self.live = Some(s),
            Err(e) => {
                let msg = e.to_string();
                self.update(|s| {
                    s.live = None;
                    s.error = Some(msg);
                });
            }
        }
    }

    fn step(&mut self, kb: &mut Keyboard) -> Result<(), String> {
        if let Some(live) = &mut self.live {
            if let Err(e) = live.step(kb) {
                let msg = e.to_string();
                self.live = None;
                if matches!(e, epomaker_host::HostError::Keyboard(_)) {
                    return Err(msg);
                }
                self.update(|s| {
                    s.live = None;
                    s.error = Some(msg);
                });
            }
        }
        if self.config.per_app_profiles && self.last_focus.elapsed() >= FOCUS_POLL {
            self.last_focus = Instant::now();
            if let Some(w) = &mut self.window {
                let app = w.current().unwrap_or(None);
                let wanted = self.config.profile_for(app.as_deref());
                let current = self.status.lock().unwrap().profile;
                let changed_app = self.status.lock().unwrap().active_app != app;
                if let Some(p) = wanted.filter(|p| Some(*p) != current) {
                    kb.set_profile(p).map_err(|e| e.to_string())?;
                    log::info!("{app:?} -> profile {}", p + 1);
                    self.update(|s| s.profile = Some(p));
                }
                if changed_app {
                    self.update(|s| s.active_app = app);
                }
            }
        }
        if self.last_battery.is_some_and(|t| t.elapsed() >= BATTERY_POLL) {
            self.last_battery = Some(Instant::now());
            let b = kb.battery().map_err(|e| e.to_string())?;
            let threshold = self.config.battery_warning_percent;
            let low = threshold > 0 && b.state == ChargeState::Discharging && b.percent <= threshold;
            if low && !self.battery_warned {
                self.battery_warned = true;
                notify(t("Keyboard battery low"), &format!("{}%", b.percent));
            } else if !low && (b.state != ChargeState::Discharging || b.percent > threshold.saturating_add(5)) {
                self.battery_warned = false;
            }
            self.update(|s| s.battery = Some(b));
        }
        Ok(())
    }

    fn disconnect(&mut self, error: Option<String>) {
        self.live = None;
        self.kb = None;
        self.update(|s| {
            s.model = None;
            s.battery = None;
            s.profile = None;
            if error.is_some() {
                s.error = error;
            }
        });
    }

    /// Opens the control panel and leaves the keyboard to it until it exits.
    fn open_panel(&mut self) {
        if self.panel.is_some() {
            return;
        }
        let exe = panel_executable();
        match std::process::Command::new(&exe).spawn() {
            Ok(child) => {
                self.live = None;
                self.kb = None;
                self.panel = Some(child);
                self.update(|s| s.paused = true);
            }
            Err(e) => self.update(|s| s.error = Some(format!("{}: {e}", exe.display()))),
        }
    }

    fn poll_panel(&mut self) {
        let done = match &mut self.panel {
            Some(c) => !matches!(c.try_wait(), Ok(None)),
            None => false,
        };
        if done {
            self.panel = None;
            self.last_connect = None;
            self.update(|s| s.paused = false);
        }
    }
}

fn panel_executable() -> std::path::PathBuf {
    let name = if cfg!(windows) { "epomaker-panel.exe" } else { "epomaker-panel" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| name.into())
}

fn open_in_editor(path: &std::path::Path) {
    let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
    if let Err(e) = std::process::Command::new(opener).arg(path).spawn() {
        log::warn!("cannot open {}: {e}", path.display());
    }
}

pub fn notify(summary: &str, body: &str) {
    if let Err(e) = notify_rust::Notification::new().appname("Open Epomaker Panel").summary(summary).body(body).show() {
        log::warn!("notification: {e}");
    }
}
