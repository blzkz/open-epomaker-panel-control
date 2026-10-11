//! Agent settings, stored as JSON in the user's configuration directory.

use epomaker_host::stream::LiveEffect;
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
