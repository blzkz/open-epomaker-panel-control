//! Diagnostics tab: hardware self-test with a report for GitHub issues, the
//! verify-every-write switch, and the read-only probe when no keyboard is connected.

use super::{App, ToastKind, Update};
use crate::i18n::tr;
use crate::worker::JobInfo;
use eframe::egui::{self, Color32, RichText};
use epomaker_core::selftest::{self, DeviceSummary, Outcome, Step, StepResult};
use epomaker_core::Keyboard;

const APP: &str = concat!("epomaker-panel ", env!("CARGO_PKG_VERSION"));

/// Progress of a self-test (one worker job per step).
pub(super) struct SelfTestRun {
    pub device: Option<DeviceSummary>,
    pub results: Vec<StepResult>,
    pub running: bool,
}

pub(super) const SELFTEST_INTRO: &str =
    "Checks that this app understands your keyboard. It only reads, except for three small \
changes that are undone right away: the backlight brightness, one rarely used key on profile 3 and an unused macro \
slot. The original state is restored even if a step fails. Then copy the report into a GitHub issue.";
pub(super) const VERIFY_HINT: &str =
    "Every change made in this app is read back from the keyboard; a mismatch is shown as an error.";
pub(super) const PROBE_INTRO: &str =
    "No supported keyboard is connected. The probe lists the USB interfaces of EPOMAKER \
keyboards and sends only read commands. Share its output in a GitHub issue if your keyboard is not detected.";

impl App {
    pub(super) fn start_selftest(&mut self) {
        // live effects would interleave frames with the test's reads
        self.worker.set_live(None, None);
        self.selftest = Some(SelfTestRun { device: None, results: vec![], running: true });
        self.worker.run("Self-test", |kb| Ok(Update::SelfTestDevice(Box::new(selftest::summary(kb)))));
        for step in Step::ALL {
            self.worker.run("Self-test", move |kb| Ok(Update::SelfTestStep(Box::new(selftest::run_step(kb, step)))));
        }
    }

    pub(super) fn run_probe(&self) {
        self.worker.run_raw(
            JobInfo::quiet("Probe"),
            Box::new(|_: &mut Option<Keyboard>, _| {
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

    pub(super) fn diagnostics_tab(&mut self, ui: &mut egui::Ui) {
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
                self.worker.run("Verify writes", move |kb| {
                    kb.set_verify(v);
                    Ok(Update::None)
                });
            }
            ui.label(RichText::new(t(VERIFY_HINT)).small().weak());
            ui.add_space(8.0);
            let running = self.selftest.as_ref().is_some_and(|s| s.running);
            let report = self.report();
            let mut copied = false;
            ui.horizontal(|ui| {
                if ui.add_enabled(!running && !self.worker.busy(), egui::Button::new(t("Run self-test"))).clicked() {
                    self.start_selftest();
                }
                if ui.add_enabled(!running && report.is_some(), egui::Button::new(t("Copy report"))).clicked() {
                    if let Some(md) = report.clone() {
                        ui.ctx().copy_text(md);
                        copied = true;
                    }
                }
            });
            if copied {
                let text = t("Report copied to the clipboard").to_string();
                self.toast(ToastKind::Success, text);
            }
            let Some(st) = &self.selftest else { return };
            ui.add_space(8.0);
            for (i, step) in Step::ALL.iter().enumerate() {
                ui.separator();
                ui.horizontal(|ui| {
                    let r = st.results.get(i);
                    let current = st.running && i == st.results.len();
                    let (text, color) = match r.map(|r| r.outcome) {
                        Some(Outcome::Passed) => (t("passed"), Color32::from_rgb(90, 190, 90)),
                        Some(Outcome::Failed) => (t("FAILED"), ui.visuals().error_fg_color),
                        Some(Outcome::Skipped) => (t("skipped"), Color32::GRAY),
                        None if current => (t("running…"), ui.visuals().text_color()),
                        None => (t("pending"), ui.visuals().weak_text_color()),
                    };
                    ui.add_sized([90.0, 18.0], egui::Label::new(RichText::new(text).color(color).strong()));
                    ui.label(RichText::new(t(step.title())).strong());
                    match r.and_then(|r| r.restored) {
                        Some(true) => {
                            ui.label(RichText::new(format!("({})", t("restored"))).weak());
                        }
                        Some(false) => {
                            ui.colored_label(ui.visuals().error_fg_color, format!("({})", t("NOT restored")));
                        }
                        None => {}
                    }
                    if r.is_none() && current {
                        ui.spinner();
                    }
                });
                if let Some(r) = st.results.get(i) {
                    ui.indent(i, |ui| {
                        for d in &r.details {
                            ui.add(egui::Label::new(RichText::new(d).small()).wrap());
                        }
                        if let Some(e) = &r.error {
                            let c =
                                if r.outcome == Outcome::Failed { ui.visuals().error_fg_color } else { Color32::GRAY };
                            ui.add(egui::Label::new(RichText::new(e).small().color(c)).wrap());
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
        let mut copied = false;
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.worker.busy(), egui::Button::new(t("Run probe"))).clicked() {
                self.run_probe();
            }
            if ui.add_enabled(output.is_some(), egui::Button::new(t("Copy output"))).clicked() {
                if let Some(o) = output.clone() {
                    ui.ctx().copy_text(o);
                    copied = true;
                }
            }
        });
        if copied {
            let text = t("Output copied to the clipboard").to_string();
            self.toast(ToastKind::Success, text);
        }
        if let Some(lines) = &self.probe {
            ui.add_space(8.0);
            let text = lines.join("\n");
            ui.add(egui::TextEdit::multiline(&mut text.as_str()).code_editor().desired_width(f32::INFINITY));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    #[test]
    fn diagnostics_strings_are_translated() {
        let mut strings = vec![SELFTEST_INTRO, VERIFY_HINT, PROBE_INTRO, "Diagnostics"];
        strings.extend(Step::ALL.iter().map(|s| s.title()));
        for s in strings {
            assert_ne!(tr(Lang::Es, s), s, "missing Spanish for {s:?}");
        }
    }
}
