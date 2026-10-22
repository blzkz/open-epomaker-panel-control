//! Tray agent tab: start it, start it with the system, and edit its per-application rules
//! (`agent.json`, which the agent reloads by itself).

use super::App;
use crate::i18n::tr;
use crate::theme;
use eframe::egui::{self, ComboBox, Slider};
use epomaker_core::lighting::LightMode;
use epomaker_host::agent_config::{self, Config, Rule};
use epomaker_host::stream::LiveEffect;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const DETECT_DELAY: Duration = Duration::from_secs(5);

/// Result of "detect application": the focused app after the countdown.
type Detection = Receiver<Result<Option<String>, String>>;

#[derive(Default)]
pub(super) struct AgentState {
    config: Option<Config>,
    dirty: bool,
    detecting: Option<(Instant, Detection)>,
    message: Option<String>,
}

const LIVE: [(&str, Option<LiveEffect>); 5] = [
    ("Off", None),
    ("Music follow 1", Some(LiveEffect::Music(LightMode::MusicFollow3))),
    ("Music follow 2", Some(LiveEffect::Music(LightMode::MusicFollow2))),
    ("Screen color", Some(LiveEffect::Screen)),
    ("CPU load bar on the number row", Some(LiveEffect::CpuLoad)),
];

impl App {
    pub(super) fn agent_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let state = &mut self.agent;
        let config = state.config.get_or_insert_with(Config::load);

        let pal = theme::palette(ui);
        let running = agent_config::agent_running();
        let mut changed = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
        theme::card(ui, t("Status"), |ui| {
        ui.horizontal(|ui| {
            if running {
                theme::pill(ui, &format!("{} {}", theme::icon::CHECK_CIRCLE, t("Running")), pal.ok);
            } else {
                theme::pill(ui, &format!("{} {}", theme::icon::X_CIRCLE, t("Not running")), pal.text_dim);
                if theme::primary_button(ui, true, t("Start the agent")).clicked() {
                    state.message = Some(match start_agent() {
                        Ok(()) => t("Agent started").to_string(),
                        Err(e) => format!("{}: {e}", t("Error")),
                    });
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut auto = epomaker_host::autostart::is_enabled();
                if theme::toggle(ui, &mut auto).changed() {
                    if let Err(e) = epomaker_host::autostart::set(auto) {
                        state.message = Some(e.to_string());
                    }
                }
                ui.label(t("Start with the system"));
            });
        });
        });
        ui.add_space(12.0);

        theme::card(ui, "", |ui| {
        changed |= theme::toggle_row(
            ui,
            &mut config.per_app_profiles,
            t("Per-application profiles"),
            t("Windows: the program file (game.exe). Linux: the window class (firefox, steam_app_570). * matches anything."),
        )
        .changed();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(t("Other applications"));
            let label = |p: Option<u8>| match p {
                None => t("Keep the current profile").to_string(),
                Some(p) => format!("{} {p}", t("Profile")),
            };
            ComboBox::from_id_salt("default_profile").selected_text(label(config.default_profile)).show_ui(ui, |ui| {
                for p in [None, Some(1), Some(2), Some(3)] {
                    changed |= ui.selectable_value(&mut config.default_profile, p, label(p)).changed();
                }
            });
        });
        ui.add_space(4.0);
        let mut remove = None;
        egui::Grid::new("agent_rules").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.strong(t("Application"));
            ui.strong(t("Profile"));
            ui.label("");
            ui.end_row();
            for (i, rule) in config.rules.iter_mut().enumerate() {
                changed |= ui.add_sized([260.0, 20.0], egui::TextEdit::singleline(&mut rule.app)).changed();
                ComboBox::from_id_salt(("rule_profile", i)).selected_text(rule.profile.to_string()).show_ui(ui, |ui| {
                    for p in 1..=3u8 {
                        changed |= ui.selectable_value(&mut rule.profile, p, p.to_string()).changed();
                    }
                });
                if ui.small_button(theme::icon::TRASH).on_hover_text(t("Delete")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            config.rules.remove(i);
            changed = true;
        }
        ui.horizontal(|ui| {
            if ui.button(format!("{} {}", theme::icon::PLUS, t("Add rule"))).clicked() {
                config.rules.push(Rule { app: String::new(), profile: 2 });
                changed = true;
            }
            match &state.detecting {
                Some((start, rx)) => {
                    let left = DETECT_DELAY.saturating_sub(start.elapsed()).as_secs() + 1;
                    ui.spinner();
                    ui.label(format!("{} ({left} s)", t("Switch to the application now…")));
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                    if let Ok(result) = rx.try_recv() {
                        match result {
                            Ok(Some(app)) => {
                                config.rules.push(Rule { app, profile: 2 });
                                changed = true;
                            }
                            Ok(None) => state.message = Some(t("No application was focused").into()),
                            Err(e) => state.message = Some(e),
                        }
                        state.detecting = None;
                    }
                }
                None => {
                    if ui.button(format!("{} {}", theme::icon::CROSSHAIR, t("Detect application (5 s)"))).clicked() {
                        let (tx, rx) = channel();
                        std::thread::spawn(move || {
                            let r = epomaker_host::window::tracker().map_err(|e| e.to_string()).and_then(|mut w| {
                                std::thread::sleep(DETECT_DELAY);
                                w.current().map_err(|e| e.to_string())
                            });
                            let _ = tx.send(r);
                        });
                        state.detecting = Some((Instant::now(), rx));
                    }
                }
            }
        });
        });
        ui.add_space(12.0);
        theme::card(ui, t("Other"), |ui| {
        egui::Grid::new("agent_misc").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(t("Low battery warning"));
            changed |= ui.add(Slider::new(&mut config.battery_warning_percent, 0..=50).suffix(" %")).changed();
            ui.end_row();
            ui.label(t("PC effect at start"));
            let current = LIVE.iter().find(|(_, e)| *e == config.live_effect).map(|(l, _)| t(l)).unwrap_or("?");
            ComboBox::from_id_salt("agent_live").selected_text(current).show_ui(ui, |ui| {
                for (label, effect) in LIVE {
                    changed |= ui.selectable_value(&mut config.live_effect, effect, t(label)).changed();
                }
            });
            ui.end_row();
        });
        });
        if changed {
            state.dirty = true;
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if theme::primary_button(ui, state.dirty, t("Save")).clicked() {
                config.rules.retain(|r| !r.app.trim().is_empty());
                config.save();
                state.dirty = false;
                state.message = Some(t("Saved; the agent applies it within a few seconds.").into());
            }
            if ui.button(t("Reload")).clicked() {
                *config = Config::load();
                state.dirty = false;
            }
            ui.weak(tilde(&agent_config::path()));
        });
        if let Some(m) = &state.message {
            ui.label(m);
        }
        });
    }
}

fn start_agent() -> std::io::Result<()> {
    let cmd = epomaker_host::autostart::agent_command();
    // agent_command() quotes paths with spaces and appends `agent` for AppImages
    let parts = shell_split(&cmd);
    let (exe, args) = parts.split_first().ok_or_else(|| std::io::Error::other("no agent command"))?;
    std::process::Command::new(exe).args(args).spawn().map(|_| ())
}

fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `path` with the home directory shown as `~`.
fn tilde(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}
