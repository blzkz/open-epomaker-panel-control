//! Keycap legends for the national layouts keyboards are sold with.
//!
//! The firmware only tells ANSI and ISO boards apart: an ISO Spanish TH80 reports the
//! same model id as the ISO UK one, and the vendor data only has UK legends. The keys are
//! physically identical, so the panel prints the legends of the layout the user picks,
//! by default the keyboard layout of the system.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Legend {
    /// The system keyboard layout, if known; else `Model`.
    #[default]
    Auto,
    /// The legends of the vendor data (US for ANSI boards, UK for ISO ones).
    Model,
    Us,
    Uk,
    Es,
    Latam,
    Pt,
    De,
    Fr,
    It,
}

impl Legend {
    pub const ALL: [Legend; 10] = [
        Legend::Auto,
        Legend::Model,
        Legend::Us,
        Legend::Uk,
        Legend::Es,
        Legend::Latam,
        Legend::Pt,
        Legend::De,
        Legend::Fr,
        Legend::It,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Legend::Auto => "Same as the system",
            Legend::Model => "As in the keyboard data",
            Legend::Us => "English (US)",
            Legend::Uk => "English (UK)",
            Legend::Es => "Spanish (Spain)",
            Legend::Latam => "Spanish (Latin America)",
            Legend::Pt => "Portuguese",
            Legend::De => "German",
            Legend::Fr => "French",
            Legend::It => "Italian",
        }
    }

    /// The legends actually drawn for this choice.
    pub fn resolve(self) -> Legend {
        match self {
            Legend::Auto => detect().unwrap_or(Legend::Model),
            other => other,
        }
    }

    fn index(self) -> u8 {
        Legend::ALL.iter().position(|l| *l == self).unwrap_or(0) as u8
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(1); // Model

/// Selects the legends used by [`label`] (resolving `Auto`). Returns the resolved layout.
pub fn set(choice: Legend) -> Legend {
    let resolved = choice.resolve();
    CURRENT.store(resolved.index(), Ordering::Relaxed);
    resolved
}

pub fn current() -> Legend {
    Legend::ALL.get(CURRENT.load(Ordering::Relaxed) as usize).copied().unwrap_or(Legend::Model)
}

/// Legend of the key with layout code `code`, `fallback` being the vendor label.
pub fn label(code: u32, fallback: &str) -> String {
    legend_of(current(), code).map(String::from).unwrap_or_else(|| fallback.to_string())
}

/// Legend that `layout` prints on the key sending HID usage `code`, when it differs
/// from the US one (letters and digits are left to the vendor data).
pub fn legend_of(layout: Legend, code: u32) -> Option<&'static str> {
    let Ok(code) = u8::try_from(code) else { return None };
    // grave, minus, equal, [, ], \ (ANSI), # (ISO), ;, ', ISO extra key, ',', '.', '/'
    const CODES: [u8; 13] = [0x35, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x64, 0x36, 0x37, 0x38];
    let table: [&str; 13] = match layout {
        Legend::Auto | Legend::Model => return None,
        Legend::Us => ["`", "-", "=", "[", "]", "\\", "\\", ";", "'", "\\", ",", ".", "/"],
        Legend::Uk => ["`", "-", "=", "[", "]", "#", "#", ";", "'", "\\", ",", ".", "/"],
        Legend::Es => ["º", "'", "¡", "`", "+", "Ç", "Ç", "Ñ", "´", "<", ",", ".", "-"],
        Legend::Latam => ["|", "'", "¿", "´", "+", "}", "}", "Ñ", "{", "<", ",", ".", "-"],
        Legend::Pt => ["\\", "'", "«", "+", "´", "~", "~", "Ç", "º", "<", ",", ".", "-"],
        Legend::De => ["^", "ß", "´", "Ü", "+", "#", "#", "Ö", "Ä", "<", ",", ".", "-"],
        Legend::Fr => ["²", ")", "=", "^", "$", "*", "*", "M", "ù", "<", ";", ":", "!"],
        Legend::It => ["\\", "'", "ì", "è", "+", "ù", "ù", "ò", "à", "<", ",", ".", "-"],
    };
    if let Some(i) = CODES.iter().position(|c| *c == code) {
        return Some(table[i]);
    }
    let swapped = match (layout, code) {
        (Legend::De, 0x1C) => "Z",
        (Legend::De, 0x1D) => "Y",
        (Legend::Fr, 0x04) => "Q",
        (Legend::Fr, 0x14) => "A",
        (Legend::Fr, 0x1A) => "Z",
        (Legend::Fr, 0x1D) => "W",
        (Legend::Fr, 0x10) => ",",
        (Legend::Us, 0xE6) => return None,
        // every other layout has AltGr on the right
        (_, 0xE6) => "AltGr",
        _ => return None,
    };
    Some(swapped)
}

/// Maps an XKB layout name ("es", "gb", "latam"…) to legends.
#[cfg_attr(windows, allow(dead_code))]
pub fn from_xkb(name: &str) -> Option<Legend> {
    let name = name.trim().trim_matches(|c| c == '\'' || c == '"').to_lowercase();
    let base = name.split(['(', '+', ':']).next().unwrap_or("");
    Some(match base {
        "us" => Legend::Us,
        "gb" | "uk" | "ie" => Legend::Uk,
        "es" => Legend::Es,
        "latam" => Legend::Latam,
        "pt" => Legend::Pt,
        "de" | "at" => Legend::De,
        "fr" => Legend::Fr,
        "it" => Legend::It,
        _ => return None,
    })
}

/// The keyboard layout of the system, when it is one of the supported ones.
pub fn detect() -> Option<Legend> {
    imp::detect()
}

#[cfg(windows)]
mod imp {
    use super::Legend;

    pub fn detect() -> Option<Legend> {
        // SAFETY: plain query of the current thread's input locale.
        let hkl = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout(0) } as usize;
        let langid = (hkl & 0xFFFF) as u16;
        let (primary, sub) = (langid & 0x3FF, langid >> 10);
        Some(match (primary, sub) {
            (0x09, 0x02) | (0x09, 0x06) => Legend::Uk, // UK, Ireland
            (0x09, _) => Legend::Us,
            (0x0A, 0x01) | (0x0A, 0x03) => Legend::Es, // Spain (traditional and modern sort)
            (0x0A, _) => Legend::Latam,
            (0x16, 0x02) => Legend::Pt,
            (0x07, _) => Legend::De,
            (0x0C, 0x01) => Legend::Fr,
            (0x10, _) => Legend::It,
            _ => return None,
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{from_xkb, Legend};
    use std::process::Command;

    pub fn detect() -> Option<Legend> {
        if let Some(l) = std::env::var("XKB_DEFAULT_LAYOUT").ok().and_then(|v| first(&v)) {
            return Some(l);
        }
        // KDE
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
        let config = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).unwrap_or(home.join(".config"));
        if let Ok(rc) = std::fs::read_to_string(config.join("kxkbrc")) {
            if let Some(l) = rc.lines().find_map(|l| l.strip_prefix("LayoutList=")).and_then(first) {
                return Some(l);
            }
        }
        // GNOME: [('xkb', 'es'), ('xkb', 'us')]
        if let Some(out) = run("gsettings", &["get", "org.gnome.desktop.input-sources", "sources"]) {
            if let Some(l) = out.split("'xkb', ").nth(1).and_then(|s| s.split(')').next()).and_then(from_xkb) {
                return Some(l);
            }
        }
        // system default (also X11 sessions)
        for (cmd, args, key) in
            [("localectl", &["status"][..], "X11 Layout:"), ("setxkbmap", &["-query"][..], "layout:")]
        {
            if let Some(out) = run(cmd, args) {
                if let Some(l) = out.lines().find_map(|l| l.trim().strip_prefix(key)).and_then(first) {
                    return Some(l);
                }
            }
        }
        None
    }

    /// First layout of a comma separated list.
    fn first(list: &str) -> Option<Legend> {
        from_xkb(list.split(',').next()?)
    }

    fn run(cmd: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(cmd).args(args).stderr(std::process::Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_iso_legends() {
        assert_eq!(legend_of(Legend::Es, 0x33), Some("Ñ"));
        assert_eq!(legend_of(Legend::Es, 0x35), Some("º"));
        assert_eq!(legend_of(Legend::Es, 0x64), Some("<"));
        assert_eq!(legend_of(Legend::Es, 0xE6), Some("AltGr"));
        assert_eq!(legend_of(Legend::Es, 0x04), None);
        assert_eq!(legend_of(Legend::Us, 0xE6), None);
        assert_eq!(legend_of(Legend::Model, 0x33), None);
        assert_eq!(legend_of(Legend::Es, 0x0300_E900), None);
    }

    #[test]
    fn xkb_names() {
        assert_eq!(from_xkb("es"), Some(Legend::Es));
        assert_eq!(from_xkb("'latam'"), Some(Legend::Latam));
        assert_eq!(from_xkb("gb(extd)"), Some(Legend::Uk));
        assert_eq!(from_xkb("ru"), None);
    }
}
