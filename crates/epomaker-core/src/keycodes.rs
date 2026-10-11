//! Names of USB HID keyboard usages (page 0x07), as used in key entries and macros.

const NAMES: &[(u8, &str)] = &[
    (0x04, "A"),
    (0x05, "B"),
    (0x06, "C"),
    (0x07, "D"),
    (0x08, "E"),
    (0x09, "F"),
    (0x0A, "G"),
    (0x0B, "H"),
    (0x0C, "I"),
    (0x0D, "J"),
    (0x0E, "K"),
    (0x0F, "L"),
    (0x10, "M"),
    (0x11, "N"),
    (0x12, "O"),
    (0x13, "P"),
    (0x14, "Q"),
    (0x15, "R"),
    (0x16, "S"),
    (0x17, "T"),
    (0x18, "U"),
    (0x19, "V"),
    (0x1A, "W"),
    (0x1B, "X"),
    (0x1C, "Y"),
    (0x1D, "Z"),
    (0x1E, "1"),
    (0x1F, "2"),
    (0x20, "3"),
    (0x21, "4"),
    (0x22, "5"),
    (0x23, "6"),
    (0x24, "7"),
    (0x25, "8"),
    (0x26, "9"),
    (0x27, "0"),
    (0x28, "Enter"),
    (0x29, "Esc"),
    (0x2A, "Backspace"),
    (0x2B, "Tab"),
    (0x2C, "Space"),
    (0x2D, "-"),
    (0x2E, "="),
    (0x2F, "["),
    (0x30, "]"),
    (0x31, "\\"),
    (0x32, "#~ (ISO)"),
    (0x33, ";"),
    (0x34, "'"),
    (0x35, "`"),
    (0x36, ","),
    (0x37, "."),
    (0x38, "/"),
    (0x39, "Caps Lock"),
    (0x3A, "F1"),
    (0x3B, "F2"),
    (0x3C, "F3"),
    (0x3D, "F4"),
    (0x3E, "F5"),
    (0x3F, "F6"),
    (0x40, "F7"),
    (0x41, "F8"),
    (0x42, "F9"),
    (0x43, "F10"),
    (0x44, "F11"),
    (0x45, "F12"),
    (0x46, "Print Screen"),
    (0x47, "Scroll Lock"),
    (0x48, "Pause"),
    (0x49, "Insert"),
    (0x4A, "Home"),
    (0x4B, "Page Up"),
    (0x4C, "Delete"),
    (0x4D, "End"),
    (0x4E, "Page Down"),
    (0x4F, "Right"),
    (0x50, "Left"),
    (0x51, "Down"),
    (0x52, "Up"),
    (0x53, "Num Lock"),
    (0x54, "Num /"),
    (0x55, "Num *"),
    (0x56, "Num -"),
    (0x57, "Num +"),
    (0x58, "Num Enter"),
    (0x59, "Num 1"),
    (0x5A, "Num 2"),
    (0x5B, "Num 3"),
    (0x5C, "Num 4"),
    (0x5D, "Num 5"),
    (0x5E, "Num 6"),
    (0x5F, "Num 7"),
    (0x60, "Num 8"),
    (0x61, "Num 9"),
    (0x62, "Num 0"),
    (0x63, "Num ."),
    (0x64, "\\| (ISO)"),
    (0x65, "Menu"),
    (0x66, "Power"),
    (0x67, "Num ="),
    (0x68, "F13"),
    (0x69, "F14"),
    (0x6A, "F15"),
    (0x6B, "F16"),
    (0x6C, "F17"),
    (0x6D, "F18"),
    (0x6E, "F19"),
    (0x6F, "F20"),
    (0x70, "F21"),
    (0x71, "F22"),
    (0x72, "F23"),
    (0x73, "F24"),
    (0x87, "Ro (JIS)"),
    (0x88, "Kana (JIS)"),
    (0x89, "Yen (JIS)"),
    (0x8A, "Henkan (JIS)"),
    (0x8B, "Muhenkan (JIS)"),
    (0xE0, "Left Ctrl"),
    (0xE1, "Left Shift"),
    (0xE2, "Left Alt"),
    (0xE3, "Left Win"),
    (0xE4, "Right Ctrl"),
    (0xE5, "Right Shift"),
    (0xE6, "Right Alt"),
    (0xE7, "Right Win"),
];

/// Name of a HID keyboard usage, if standard.
pub fn name(code: u8) -> Option<&'static str> {
    NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Name, or `0xNN` when unknown.
pub fn display(code: u8) -> String {
    name(code).map(String::from).unwrap_or_else(|| format!("0x{code:02X}"))
}

/// All named usages, for pickers.
pub fn all() -> impl Iterator<Item = (u8, &'static str)> {
    NAMES.iter().copied()
}

const ALIASES: &[(&str, u8)] = &[
    ("return", 0x28),
    ("escape", 0x29),
    ("bksp", 0x2A),
    ("minus", 0x2D),
    ("equal", 0x2E),
    ("del", 0x4C),
    ("ins", 0x49),
    ("pgup", 0x4B),
    ("pgdn", 0x4E),
    ("prtsc", 0x46),
    ("caps", 0x39),
    ("ctrl", 0xE0),
    ("lctrl", 0xE0),
    ("shift", 0xE1),
    ("lshift", 0xE1),
    ("alt", 0xE2),
    ("lalt", 0xE2),
    ("win", 0xE3),
    ("lwin", 0xE3),
    ("super", 0xE3),
    ("gui", 0xE3),
    ("rctrl", 0xE4),
    ("rshift", 0xE5),
    ("ralt", 0xE6),
    ("altgr", 0xE6),
    ("rwin", 0xE7),
];

fn normalize(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '_' | '-') || s.len() == 1).collect::<String>().to_lowercase()
}

/// Looks up a usage by name (case and spacing insensitive, e.g. `LeftShift`, `page up`),
/// a common alias (`lshift`, `esc`…) or a hex (`0x04`) / decimal number.
pub fn parse(s: &str) -> Option<u8> {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u8::from_str_radix(h, 16).ok();
    }
    let n = normalize(t);
    if let Some((c, _)) = NAMES.iter().find(|(_, name)| normalize(name) == n) {
        return Some(*c);
    }
    if let Some((_, c)) = ALIASES.iter().find(|(a, _)| *a == n) {
        return Some(*c);
    }
    if t.len() > 1 {
        return t.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_names() {
        assert_eq!(parse("A"), Some(4));
        assert_eq!(parse("a"), Some(4));
        assert_eq!(parse("LeftShift"), Some(0xE1));
        assert_eq!(parse("left shift"), Some(0xE1));
        assert_eq!(parse("page_up"), Some(0x4B));
        assert_eq!(parse("-"), Some(0x2D));
        assert_eq!(parse("1"), Some(0x1E));
        assert_eq!(parse("0x29"), Some(0x29));
        assert_eq!(parse("esc"), Some(0x29));
        assert_eq!(parse("caps"), Some(0x39));
    }
}
