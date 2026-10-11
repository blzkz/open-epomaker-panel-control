//! Settings of the tray agent (`epomaker-agent`), stored as JSON in the user's
//! configuration directory. Shared with the GUI, which edits the rules.

use crate::stream::LiveEffect;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// "When this application is focused, switch to this onboard profile".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Application id, compared case-insensitively. `*` matches any run of characters
    /// (e.g. `steam_app_*`). On Windows this is the executable name (`game.exe`), on Linux
    /// the window class (`firefox`, `steam`).
    pub app: String,
    /// Onboard profile, 1..=3 as printed on the keyboard.
    pub profile: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub per_app_profiles: bool,
    /// Profile (1..=3) used when no rule matches; `None` keeps whatever is active.
    pub default_profile: Option<u8>,
    pub rules: Vec<Rule>,
    /// Warn when a wireless keyboard drops to this battery level (0 disables).
    pub battery_warning_percent: u8,
    /// Host-driven effect restored when the agent starts.
    pub live_effect: Option<LiveEffect>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            per_app_profiles: false,
            default_profile: None,
            rules: vec![Rule { app: "example-game.exe".into(), profile: 2 }],
            battery_warning_percent: 20,
            live_effect: None,
        }
    }
}

pub fn dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("open-epomaker-panel")
}

pub fn path() -> PathBuf {
    dir().join("agent.json")
}

/// Takes the per-user agent lock (held for the agent's lifetime).
pub fn lock_instance() -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(dir())?;
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir().join("agent.lock"))?;
    file.try_lock().map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(file)
}

fn panel_heartbeat_file() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(dir).join("open-epomaker-panel.alive")
}

/// Seconds after which a silent panel is considered closed (crash, kill).
const HEARTBEAT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

/// Called periodically by the control panel while it has a keyboard open, so the agent
/// lets go of the device.
pub fn panel_heartbeat() {
    let f = panel_heartbeat_file();
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(f, std::process::id().to_string());
}

/// The panel is closing: the agent may take the keyboard back.
pub fn panel_closed() {
    let _ = std::fs::remove_file(panel_heartbeat_file());
}

/// Whether a control panel is open (recent heartbeat).
pub fn panel_open() -> bool {
    std::fs::metadata(panel_heartbeat_file())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < HEARTBEAT_TIMEOUT)
}

/// Whether an agent currently holds the lock.
pub fn agent_running() -> bool {
    match std::fs::OpenOptions::new().write(true).open(dir().join("agent.lock")) {
        Ok(f) => f.try_lock().is_err(),
        Err(_) => false,
    }
}

impl Config {
    /// Loads the configuration, creating a commented default file the first time.
    pub fn load() -> Config {
        match std::fs::read_to_string(path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("{}: {e}; using defaults", path().display());
                Config::default()
            }),
            Err(_) => {
                let c = Config::default();
                c.save();
                c
            }
        }
    }

    /// Last modification time of the settings file.
    pub fn modified() -> Option<std::time::SystemTime> {
        std::fs::metadata(path()).and_then(|m| m.modified()).ok()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            if let Err(e) = std::fs::write(path(), text) {
                log::warn!("cannot save {}: {e}", path().display());
            }
        }
    }

    /// Profile (0-based) wanted for `app`, if any rule or default applies.
    pub fn profile_for(&self, app: Option<&str>) -> Option<u8> {
        let rule = app.and_then(|a| self.rules.iter().find(|r| glob_match(&r.app.to_lowercase(), a)));
        rule.map(|r| r.profile).or(self.default_profile).filter(|p| (1..=3).contains(p)).map(|p| p - 1)
    }
}

/// Case-sensitive glob with `*` wildcards (callers lower-case both sides).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(pos) => rest = &rest[pos + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("steam_app_*", "steam_app_570"));
        assert!(glob_match("*.exe", "game.exe"));
        assert!(glob_match("*code*", "visual studio code"));
        assert!(glob_match("firefox", "firefox"));
        assert!(!glob_match("firefox", "firefox-esr"));
        assert!(!glob_match("steam_app_*", "steam"));
    }

    #[test]
    fn profile_selection() {
        let c = Config {
            per_app_profiles: true,
            default_profile: Some(1),
            rules: vec![Rule { app: "Game*.exe".into(), profile: 3 }],
            ..Config::default()
        };
        assert_eq!(c.profile_for(Some("gamex.exe")), Some(2));
        assert_eq!(c.profile_for(Some("firefox")), Some(0));
        assert_eq!(c.profile_for(None), Some(0));
        let c2 = Config { default_profile: None, ..c };
        assert_eq!(c2.profile_for(Some("firefox")), None);
    }
}
