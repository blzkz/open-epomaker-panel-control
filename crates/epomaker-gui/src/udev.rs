//! Linux: explains and installs the udev rule that lets desktop users open the keyboard.

use crate::i18n::{tr, Lang};
use eframe::egui::{self, RichText};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

pub const RULE: &str = include_str!("../../../packaging/linux/70-epomaker.rules");
const RULE_PATH: &str = "/etc/udev/rules.d/70-epomaker.rules";
/// Runs as root through pkexec; the rule arrives on stdin.
const SCRIPT: &str = "set -e; umask 022; cat > /etc/udev/rules.d/70-epomaker.rules; \
                      udevadm control --reload-rules; udevadm trigger --subsystem-match=hidraw; \
                      udevadm settle --timeout=5 || true";

/// Whether a connection error means that the hidraw node is not accessible.
pub fn is_permission_error(msg: &str) -> bool {
    // epomaker-core appends a "(permission denied? install the udev rule…)" hint to every
    // open failure on Linux; only the error itself counts
    let m = msg.to_lowercase();
    let m = m.split(" (permission denied?").next().unwrap_or_default();
    m.contains("permission denied") || m.contains("eacces") || m.contains("operation not permitted")
}

fn manual_commands() -> String {
    format!(
        "sudo tee {RULE_PATH} <<'EOF'\n{}EOF\nsudo udevadm control --reload-rules\nsudo udevadm trigger --subsystem-match=hidraw",
        RULE.lines().filter(|l| !l.starts_with('#')).map(|l| format!("{l}\n")).collect::<String>()
    )
}

fn install() -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .args(["/bin/sh", "-c", SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("pkexec: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(RULE.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(0) => Ok(()),
        // pkexec: 126 = the user dismissed the dialog, 127 = not authorized
        Some(126 | 127) => Err("authorization was cancelled or denied".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

#[derive(Default)]
pub struct UdevHelper {
    pub open: bool,
    running: Option<Receiver<Result<(), String>>>,
    result: Option<Result<(), String>>,
}

impl UdevHelper {
    pub fn show_for(&mut self, error: &str) {
        if is_permission_error(error) {
            self.open = true;
            self.result = None;
        }
    }

    /// Draws the dialog. Returns true once the rule was installed (time to reconnect).
    pub fn ui(&mut self, ctx: &egui::Context, lang: Lang) -> bool {
        let t = |s| tr(lang, s);
        let mut installed = false;
        if let Some(rx) = &self.running {
            if let Ok(r) = rx.try_recv() {
                installed = r.is_ok();
                self.result = Some(r);
                self.running = None;
            }
        }
        if !self.open {
            return installed;
        }
        let modal = egui::Modal::new(egui::Id::new("udev_dialog")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.heading(t("Permission needed to access the keyboard"));
            ui.add_space(6.0);
            ui.label(t("Linux only lets root open the keyboard's configuration interface until a udev rule grants \
                 access to the logged-in user. The rule only matches EPOMAKER / RongYuan devices (USB vendor 3151)."));
            ui.add_space(6.0);
            ui.label(
                RichText::new(RULE.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n")).monospace(),
            );
            ui.add_space(6.0);
            ui.label(format!("{} {RULE_PATH}", t("It will be installed as")));
            ui.add_space(8.0);
            if let Some(Err(e)) = &self.result {
                ui.colored_label(ui.visuals().error_fg_color, format!("{}: {e}", t("Could not install the rule")));
            }
            ui.horizontal(|ui| {
                let running = self.running.is_some();
                let b = egui::Button::new(RichText::new(t("Install rule (asks for your password)")).strong());
                if ui.add_enabled(!running, b).clicked() {
                    let (tx, rx) = channel();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(install());
                        ctx.request_repaint();
                    });
                    self.running = Some(rx);
                    self.result = None;
                }
                if running {
                    ui.spinner();
                }
                if ui.button(t("Copy commands")).on_hover_text(t("To run them yourself in a terminal")).clicked() {
                    ui.ctx().copy_text(manual_commands());
                }
                if ui.button(t("Close")).clicked() {
                    self.open = false;
                }
            });
        });
        if modal.should_close() && self.running.is_none() {
            self.open = false;
        }
        if installed {
            self.open = false;
        }
        installed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_permission_errors() {
        assert!(is_permission_error("HID error: Failed to open a device with path '/dev/hidraw3': Permission denied"));
        assert!(!is_permission_error("no supported keyboard found"));
        assert!(!is_permission_error(
            "HID error: Failed to open a device with path '/dev/hidraw3': Device or resource busy \
             (permission denied? install the udev rule: packaging/linux/70-epomaker.rules)"
        ));
        assert!(RULE.contains("3151") && manual_commands().contains("uaccess"));
    }
}
