//! Key tester: shows what the computer receives from each key (after remapping), and
//! spots switch chatter (a key bouncing right after being released).

use super::App;
use crate::hidmap::hid_from_key;
use crate::i18n::tr;
use crate::keyboard_view::{self, KeyStyle};
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

        let events = ui.input(|i| (i.time, i.events.clone()));
        for e in events.1 {
            if let egui::Event::Key { key, physical_key, pressed, repeat: false, .. } = e {
                if let Some(code) = hid_from_key(physical_key.unwrap_or(key)) {
                    self.tester.record(events.0, code, pressed);
                }
            }
        }
        let state = &self.tester;

        ui.label(RichText::new(t("Key tester")).heading());
        ui.weak(t(
            "Press keys while this window is focused. It shows what the computer receives, after your remapping.",
        ));
        ui.add_space(6.0);
        let testable: Vec<usize> =
            model.keys.iter().filter_map(|k| k.matrix_index).filter(|i| sends(*i).is_some()).collect();
        let tested = testable.iter().filter(|i| sends(**i).is_some_and(|c| state.seen.contains(&c))).count();
        let visuals = ui.visuals().clone();
        let max_h = (ui.available_height() * 0.5).clamp(150.0, 320.0);
        keyboard_view::show(ui, model, max_h, |k| {
            let code = sends(k.idx);
            let (fill, text_color) = match code {
                Some(c) if state.down.contains(&c) => (Color32::from_rgb(40, 200, 90), Color32::BLACK),
                Some(c) if state.chatter.contains_key(&c) => (Color32::from_rgb(220, 120, 30), Color32::BLACK),
                Some(c) if state.seen.contains(&c) => (Color32::from_rgb(25, 90, 50), Color32::WHITE),
                _ => (visuals.widgets.inactive.bg_fill, visuals.text_color()),
            };
            KeyStyle {
                fill,
                text: k.label.clone(),
                text_color,
                highlight: false,
                tooltip: code.map(keycodes::display).unwrap_or_default(),
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(format!("{}: {tested} / {}", t("Keys tested"), testable.len()));
            if ui.button(t("Reset")).clicked() {
                self.tester = TesterState::default();
            }
        });
        let state = &self.tester;
        if !state.chatter.is_empty() {
            let list: Vec<String> =
                state.chatter.iter().map(|(c, n)| format!("{} ×{n}", keycodes::display(*c))).collect();
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{}: {}. {}", t("Possible chatter"), list.join(", "), t("Try a higher debounce in Settings.")),
            );
        }
        ui.add_space(4.0);
        egui::Grid::new("tester_log").striped(true).num_columns(3).show(ui, |ui| {
            for (i, (time, code, pressed)) in state.log.iter().enumerate() {
                ui.monospace(keycodes::display(*code));
                ui.label(if *pressed { t("pressed") } else { t("released") });
                // time since the previous (older) event
                let gap = state.log.get(i + 1).map(|(older, ..)| format!("+{:.0} ms", (time - older) * 1000.0));
                ui.weak(gap.unwrap_or_default());
                ui.end_row();
            }
        });
    }
}
