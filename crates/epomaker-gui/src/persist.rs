//! Small state kept across restarts (last used keyboard, verify mode), stored as JSON
//! in the user's configuration directory.

use epomaker_core::hid::HidCandidate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A keyboard as remembered between runs. The HID path is not stored: it changes when
/// the keyboard is plugged into another port.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavedDevice {
    Hid { vid: u16, pid: u16, interface: i32, product: String },
    Demo(i64),
}

impl SavedDevice {
    pub fn from_candidate(c: &HidCandidate) -> SavedDevice {
        SavedDevice::Hid { vid: c.vid, pid: c.pid, interface: c.interface, product: c.product.clone() }
    }

    pub fn matches(&self, c: &HidCandidate) -> bool {
        match self {
            SavedDevice::Hid { vid, pid, interface, product } => {
                *vid == c.vid && *pid == c.pid && *interface == c.interface && *product == c.product
            }
            SavedDevice::Demo(_) => false,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Saved {
    pub last_device: Option<SavedDevice>,
    pub verify: Option<bool>,
}

fn path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("open-epomaker-panel").join("state.json"))
}

pub fn load() -> Saved {
    path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(saved: &Saved) {
    let Some(p) = path() else { return };
    let result = p
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| std::fs::write(&p, serde_json::to_string_pretty(saved).unwrap_or_default()));
    if let Err(e) = result {
        log::warn!("cannot save {}: {e}", p.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_device_by_ids_not_path() {
        let c = HidCandidate {
            path: "/dev/hidraw3".into(),
            vid: 0x3151,
            pid: 0x4010,
            usage_page: 0xffff,
            usage: 2,
            interface: 2,
            product: "TH80".into(),
            manufacturer: "EPOMAKER".into(),
        };
        let saved = Saved { last_device: Some(SavedDevice::from_candidate(&c)), verify: Some(false) };
        let back: Saved = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let dev = back.last_device.unwrap();
        assert!(dev.matches(&HidCandidate { path: "/dev/hidraw9".into(), ..c.clone() }));
        assert!(!dev.matches(&HidCandidate { pid: 0x4011, ..c }));
        assert_eq!(back.verify, Some(false));
    }
}
