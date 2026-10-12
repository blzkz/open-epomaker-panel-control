//! Key tester: shows what the computer receives from each key (after remapping), and
//! spots switch chatter (a key bouncing right after being released).

use super::App;
use crate::hidmap::hid_from_key;
use crate::i18n::tr;
use crate::keyboard_view::{self, KeyStyle};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use epomaker_core::keycodes;
use epomaker_core::keymap::{KeyAction, Layer};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// A press arriving this soon after the same key was released counts as a bounce.
const CHATTER_MS: f64 = 40.0;
const LOG_LEN: usize = 14;

#[derive(Default)]
pub(super) struct TesterState {
    down: BTreeSet<u8>,
    seen: BTreeSet<u8>,
    log: VecDeque<(f64, u8, bool)>,
    last_up: HashMap<u8, f64>,
    chatter: BTreeMap<u8, u32>,
    /// The keyboard's own reports, for keys egui does not see.
    #[cfg(target_os = "linux")]
    reports: Option<Result<epomaker_core::hid::KeyReports, String>>,
}

impl TesterState {
    /// Feeds a synthetic event (used to render documentation screenshots).
    #[cfg(feature = "autoshot")]
    pub(super) fn record_for_demo(&mut self, time: f64, code: u8, pressed: bool) {
        self.record(time, code, pressed);
    }

    fn record(&mut self, time: f64, code: u8, pressed: bool) {
        if pressed {
            if self.down.contains(&code) {
                return; // auto-repeat
            }
            if self.last_up.get(&code).is_some_and(|t| (time - t) * 1000.0 < CHATTER_MS) {
                *self.chatter.entry(code).or_default() += 1;
            }
            self.down.insert(code);
            self.seen.insert(code);
        } else {
            self.down.remove(&code);
            self.last_up.insert(code, time);
        }
        self.log.push_front((time, code, pressed));
        self.log.truncate(LOG_LEN);
    }
}

impl App {
    pub(super) fn tester_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let model = dev.snap.model;
        // what the active profile's layer sends for every matrix entry
        let layer = dev.layers.get(&Layer::Profile(dev.snap.profile)).cloned();
        let sends = |idx: usize| -> Option<u8> {
            let entry =
                layer.as_ref().and_then(|l| l.entries.get(idx).copied()).or_else(|| model.default_entry(idx))?;
            match KeyAction::from_bytes(entry) {
                KeyAction::Key { key, .. } => Some(key),
                _ => None,
            }
        };

        // ISO boards: the key left of Enter sends 0x32, which the OS reports like the
        // ANSI backslash (0x31)
        let has = |code: u8| model.keys.iter().any(|k| k.code == code as u32);
        let iso_hash = !has(0x31) && has(0x32);
        let fix = |c: u8| if c == 0x31 && iso_hash { 0x32 } else { c };
        let events = ui.input(|i| (i.time, i.events.clone()));
        for e in events.1 {
            if let egui::Event::Key { key, physical_key, pressed, repeat: false, .. } = e {
                if let Some(code) = hid_from_key(physical_key.unwrap_or(key)) {
                    self.tester.record(events.0, fix(code), pressed);
                }
            }
        }
        for (code, pressed) in crate::rawkeys::take() {
            self.tester.record(events.0, code, pressed);
        }
        #[cfg(target_os = "linux")]
        if let Some(super::Target::Hid(c)) = &self.connected {
            let (vid, pid) = (c.vid, c.pid);
            let reports = self
                .tester
                .reports
                .get_or_insert_with(|| epomaker_core::hid::KeyReports::open(vid, pid).map_err(|e| e.to_string()));
            if let Ok(r) = reports {
                match r.poll() {
                    Ok(list) => {
                        for (code, pressed) in list.into_iter().filter(|(c, _)| crate::rawkeys::UNSEEN.contains(c)) {
                            self.tester.record(events.0, code, pressed);
                        }
                    }
                    Err(e) => self.tester.reports = Some(Err(e.to_string())),
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
            }
        }
        let state = &self.tester;
        let p = theme::palette(ui);

        let testable: Vec<usize> =
            model.keys.iter().filter_map(|k| k.matrix_index).filter(|i| sends(*i).is_some()).collect();
        let tested = testable.iter().filter(|i| sends(**i).is_some_and(|c| state.seen.contains(&c))).count();
        let max_h = (ui.available_height() * 0.5).clamp(150.0, 320.0);
        keyboard_view::show(ui, model, max_h, |k| {
            let code = sends(k.idx);
            let (fill, text_color) = match code {
                Some(c) if state.down.contains(&c) => (p.ok, Color32::BLACK),
                Some(c) if state.chatter.contains_key(&c) => (p.warn, Color32::BLACK),
                Some(c) if state.seen.contains(&c) => (theme::blend(p.control, p.ok, 0.35), p.text_strong),
                _ => (p.control, p.text),
            };
            KeyStyle {
                fill,
                text: k.label.clone(),
                text_color,
                highlight: false,
                tooltip: code.map(keycodes::display).unwrap_or_default(),
            }
        });
        ui.add_space(12.0);
        let mut reset = false;
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], t("Keys tested"), |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{tested} / {}", testable.len()))
                            .size(28.0)
                            .family(theme::display())
                            .color(p.text_strong),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        reset = ui.button(format!("{} {}", theme::icon::ARROW_COUNTER_CLOCKWISE, t("Reset"))).clicked();
                    });
                });
                let frac = tested as f32 / testable.len().max(1) as f32;
                ui.add(egui::ProgressBar::new(frac).desired_height(6.0).fill(p.ok));
                if !state.chatter.is_empty() {
                    let list: Vec<String> =
                        state.chatter.iter().map(|(c, n)| format!("{} ×{n}", keycodes::display(*c))).collect();
                    ui.add_space(6.0);
                    ui.colored_label(
                        p.warn,
                        format!(
                            "{} {}: {}. {}",
                            theme::icon::WARNING,
                            t("Possible chatter"),
                            list.join(", "),
                            t("Try a higher debounce in Settings.")
                        ),
                    );
                }
                #[cfg(target_os = "linux")]
                if let Some(Err(e)) = &state.reports {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("{}: {e}", t("Caps Lock and similar keys cannot be detected")))
                            .small()
                            .weak(),
                    );
                }
            });
            theme::card(&mut cols[1], t("Last events"), |ui| {
                if state.log.is_empty() {
                    ui.weak(t("Press any key…"));
                }
                egui::Grid::new("tester_log").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (i, (time, code, pressed)) in state.log.iter().take(8).enumerate() {
                        ui.label(RichText::new(keycodes::display(*code)).monospace().color(p.text_strong));
                        let (icon, text, color) = if *pressed {
                            (theme::icon::ARROW_DOWN, t("pressed"), p.ok)
                        } else {
                            (theme::icon::ARROW_UP, t("released"), p.text_dim)
                        };
                        ui.label(RichText::new(format!("{icon} {text}")).color(color));
                        // time since the previous (older) event
                        let gap = state.log.get(i + 1).map(|(older, ..)| format!("+{:.0} ms", (time - older) * 1000.0));
                        ui.weak(gap.unwrap_or_default());
                        ui.end_row();
                    }
                });
            });
        });
        if reset {
            self.tester = TesterState::default();
        }
    }
}
