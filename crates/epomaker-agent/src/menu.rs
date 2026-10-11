//! Menu contents shared by the tray front-ends.

use crate::controller::Status;
use crate::i18n::t;
use epomaker_core::lighting::LightMode;
use epomaker_host::stream::LiveEffect;

pub const LIVE_CHOICES: [(&str, Option<LiveEffect>); 5] = [
    ("Off", None),
    ("Music follow 1", Some(LiveEffect::Music(LightMode::MusicFollow3))),
    ("Music follow 2", Some(LiveEffect::Music(LightMode::MusicFollow2))),
    ("Screen color", Some(LiveEffect::Screen)),
    ("CPU load", Some(LiveEffect::CpuLoad)),
];

pub fn live_index(live: Option<LiveEffect>) -> usize {
    LIVE_CHOICES.iter().position(|(_, e)| *e == live).unwrap_or(0)
}

/// One-line summary: model, battery, profile, active app, last error.
pub fn header(s: &Status) -> String {
    if s.paused {
        return t("Paused while the control panel is open").into();
    }
    let Some(model) = &s.model else {
        return match &s.error {
            Some(e) => format!("{} ({e})", t("No keyboard connected")),
            None => t("No keyboard connected").into(),
        };
    };
    let mut parts = vec![model.clone()];
    if let Some(p) = s.profile {
        parts.push(format!("{} {}", t("Profile"), p + 1));
    }
    if let Some(b) = s.battery {
        parts.push(format!("{} {}%", t("Battery"), b.percent));
    }
    if s.per_app {
        if let Some(app) = &s.active_app {
            parts.push(format!("{}: {app}", t("Active app")));
        }
    }
    let mut line = parts.join(" · ");
    if let Some(e) = &s.error {
        line.push_str(&format!(" — {e}"));
    }
    line
}
