//! Database of supported keyboards (generated from the vendor software by
//! `tools/re-harness/dump_devices.js`, stored in our own schema).

use crate::lighting::{LightMode, LightModeCaps};
use crate::settings::SleepLimits;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SOURCES: [&str; 5] = [
    include_str!("../data/th80_family.json"),
    include_str!("../data/th96.json"),
    include_str!("../data/th68_soc.json"),
    include_str!("../data/th21.json"),
    include_str!("../data/ep84_soc.json"),
];

/// One (model, connection) entry as known by the vendor software.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Firmware model id. Negative ids are the same model behind a 2.4 GHz dongle.
    pub id: i64,
    /// Model key, e.g. `yc500_th80`.
    pub name: String,
    pub display_name: String,
    pub company: String,
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub layers: u8,
    pub fn_layers: u8,
    pub dongle: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VisualKey {
    pub label: String,
    /// HID usage, or a 32-bit big-endian matrix entry for special keys.
    pub code: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Position of this key in the firmware key matrix.
    pub matrix_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    /// Factory key matrix (4 bytes per entry).
    pub default_matrix: Vec<u8>,
    pub macro_slots: u8,
    pub light_modes: Vec<LightModeCaps>,
    pub report_rates: Vec<u16>,
    pub default_debounce: Option<u8>,
    pub sleep_bt: Option<SleepLimits>,
    pub sleep_dongle: Option<SleepLimits>,
    pub has_screen: bool,
    pub keys: Vec<VisualKey>,
}

impl ModelInfo {
    /// ISO layout (extra key next to the left Shift). The vendor data names these models
    /// after the UK version (`yc500_th80_uk`), but every ISO national version (Spanish,
    /// German…) reports the same id: only the printed legends differ.
    pub fn is_iso(&self) -> bool {
        self.keys.iter().any(|k| k.code == 0x64 && k.matrix_index.is_some())
    }

    /// Name for people: "TH80 ISO" for ISO boards.
    pub fn display_name(&self, base: &str) -> String {
        if self.is_iso() {
            format!("{base} ISO")
        } else {
            base.to_string()
        }
    }

    pub fn matrix_len(&self) -> usize {
        self.default_matrix.len() / 4
    }

    /// Number of 64-byte packets needed for the per-key color table.
    pub fn pics_count(&self) -> usize {
        self.matrix_len() * 3 / 64
    }

    pub fn default_entry(&self, index: usize) -> Option<[u8; 4]> {
        self.default_matrix.get(4 * index..4 * index + 4).map(|s| [s[0], s[1], s[2], s[3]])
    }

    pub fn key_for_matrix_index(&self, index: usize) -> Option<&VisualKey> {
        self.keys.iter().find(|k| k.matrix_index == Some(index))
    }

    pub fn caps(&self, mode: LightMode) -> Option<&LightModeCaps> {
        self.light_modes.iter().find(|c| c.mode == mode)
    }
}

pub struct Database {
    pub devices: Vec<DeviceInfo>,
    pub models: BTreeMap<String, ModelInfo>,
}

impl Database {
    pub fn get() -> &'static Database {
        static DB: OnceLock<Database> = OnceLock::new();
        DB.get_or_init(|| Database::parse(&SOURCES))
    }

    fn parse(sources: &[&str]) -> Database {
        let mut devices = Vec::new();
        let mut models = BTreeMap::new();
        for src in sources {
            let v: Value = serde_json::from_str(src).expect("embedded device database is valid JSON");
            for d in v["devices"].as_array().into_iter().flatten() {
                devices.push(DeviceInfo {
                    id: d["id"].as_i64().unwrap_or_default(),
                    name: str_of(&d["name"]),
                    display_name: str_of(&d["display_name"]),
                    company: str_of(&d["company"]),
                    vid: d["vid"].as_u64().unwrap_or_default() as u16,
                    pid: d["pid"].as_u64().unwrap_or_default() as u16,
                    usage_page: d["usage_page"].as_u64().unwrap_or_default() as u16,
                    usage: d["usage"].as_u64().unwrap_or_default() as u16,
                    layers: d["layers"].as_u64().unwrap_or(1) as u8,
                    fn_layers: d["fn_layers"].as_u64().unwrap_or(0) as u8,
                    dongle: d["dongle"].as_bool().unwrap_or(false),
                });
            }
            for (name, m) in v["models"].as_object().into_iter().flatten() {
                models.insert(name.clone(), parse_model(name, m));
            }
        }
        Database { devices, models }
    }

    pub fn device_by_id(&self, id: i64) -> Option<&DeviceInfo> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn model(&self, name: &str) -> Option<&ModelInfo> {
        self.models.get(name)
    }

    /// Distinct (vid, pid, usage_page, usage) tuples worth probing.
    pub fn usb_ids(&self) -> Vec<(u16, u16, u16, u16)> {
        let mut v: Vec<_> = self.devices.iter().map(|d| (d.vid, d.pid, d.usage_page, d.usage)).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Converts the id reported by `GET_DEVICE_ID` into the database id.
/// Through a dongle the vendor driver marks the id with bit 31 and the app negates it.
pub fn database_id(raw: u32, via_dongle: bool) -> i64 {
    if via_dongle || raw & 0x8000_0000 != 0 {
        -((raw | 0x8000_0000) as i64)
    } else {
        raw as i64
    }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn limits(v: &Value) -> Option<SleepLimits> {
    Some(SleepLimits {
        min: v.get("min")?.as_u64()? as u16,
        max: v.get("max")?.as_u64()? as u16,
        min_deep: v.get("min_deep")?.as_u64()? as u16,
        max_deep: v.get("max_deep")?.as_u64()? as u16,
    })
}

fn parse_model(name: &str, m: &Value) -> ModelInfo {
    let default_matrix: Vec<u8> =
        m["matrix"].as_array().into_iter().flatten().map(|b| b.as_u64().unwrap_or(0) as u8).collect();
    let light_modes = m["light_modes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let mode = LightMode::from_vendor_name(t["type"].as_str()?)?;
            Some(LightModeCaps {
                mode,
                options: t["options"]
                    .as_array()
                    .map(|o| o.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                max_speed: t["maxSpeed"].as_u64().map(|v| v as u8),
                max_brightness: t["maxValue"].as_u64().map(|v| v as u8),
                rgb: t["rgb"].as_bool().unwrap_or(false),
                dazzle: t["dazzle"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    let other = &m["other"];
    let mut keys: Vec<VisualKey> = m["layout"]["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| VisualKey {
            label: pretty_label(&str_of(&k["label"])),
            code: k["code"].as_u64().unwrap_or(0) as u32,
            x: k["x"].as_f64().unwrap_or(0.0) as f32,
            y: k["y"].as_f64().unwrap_or(0.0) as f32,
            w: k["w"].as_f64().unwrap_or(0.0) as f32,
            h: k["h"].as_f64().unwrap_or(0.0) as f32,
            matrix_index: None,
        })
        .collect();
    assign_matrix_indices(&mut keys, &default_matrix);
    ModelInfo {
        name: name.to_string(),
        default_matrix,
        macro_slots: m["macro_max"].as_u64().unwrap_or(50) as u8,
        light_modes,
        report_rates: m["report_rates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_u64())
            .map(|r| r as u16)
            .collect(),
        default_debounce: other["deBounce"].as_u64().map(|v| v as u8),
        sleep_bt: limits(&other["sleep_bt"]),
        sleep_dongle: limits(&other["sleep_24"]),
        has_screen: other.get("LED").is_some(),
        keys,
    }
}

/// Replaces symbols that common UI fonts lack and normalizes the vendor's spelling.
fn pretty_label(label: &str) -> String {
    match label {
        "↑" => "Up",
        "↓" => "Down",
        "←" => "Left",
        "→" => "Right",
        "U-" => "Vol-",
        "U+" => "Vol+",
        "win" => "Win",
        "space" => "Space",
        "r-Alt" | "r_Alt" => "R-Alt",
        "r-Ctrl" | "r_Ctrl" => "R-Ctrl",
        "r-Win" | "r_Win" => "R-Win",
        "r_Shift" | "r-Shift" => "R-Shift",
        "(space)" => "Space",
        "L_\\" => "\\",
        "BackSpace" => "Backspace",
        "FN" | "fn" => "Fn",
        "play" => "Play",
        // keypad keys: "num_7" -> "7"
        other => other.strip_prefix("num_").unwrap_or(other),
    }
    .to_string()
}

/// Matrix entry that a layout key code refers to (vendor `findIndexInDefaultMatrix`).
pub fn code_to_entry(code: u32) -> [u8; 4] {
    if code <= 0xff {
        [0, 0, code as u8, 0]
    } else {
        code.to_be_bytes()
    }
}

fn assign_matrix_indices(keys: &mut [VisualKey], matrix: &[u8]) {
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    for k in keys.iter_mut() {
        let nth = *seen.entry(k.code).and_modify(|n| *n += 1).or_insert(0);
        let want = code_to_entry(k.code);
        k.matrix_index =
            matrix.as_chunks::<4>().0.iter().enumerate().filter(|(_, e)| **e == want).map(|(i, _)| i).nth(nth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_loads_th80() {
        let db = Database::get();
        let th80 = db.device_by_id(1118).expect("TH80 wired");
        assert_eq!((th80.vid, th80.pid), (0x3151, 0x4010));
        let model = db.model(&th80.name).unwrap();
        assert_eq!(model.matrix_len(), 128);
        assert_eq!(model.pics_count(), 6);
        assert!(model.keys.iter().all(|k| k.matrix_index.is_some()));
        assert!(model.caps(LightMode::Wave).unwrap().options.len() == 4);
    }

    #[test]
    fn database_loads_other_models() {
        let db = Database::get();
        for (id, name, pid) in [
            (834, "yc500_th96", 0x4010),
            (988, "yc500_th68_soc", 0x4015),
            (789, "yc500_th21", 0x4010),
            (1081, "yc500_ep84_soc", 0x4015),
        ] {
            let dev = db.device_by_id(id).unwrap_or_else(|| panic!("{name} wired"));
            assert_eq!((dev.name.as_str(), dev.vid, dev.pid), (name, 0x3151, pid));
            assert!(db.device_by_id(database_id(id as u32, true)).is_some(), "{name} dongle");
            let model = db.model(name).unwrap();
            assert_eq!(model.matrix_len(), 128, "{name}");
            assert_eq!(model.pics_count(), 6, "{name}");
            assert!(!model.has_screen, "{name}");
            assert!(!model.keys.is_empty() && model.keys.iter().all(|k| k.matrix_index.is_some()), "{name}");
        }
    }

    #[test]
    fn dongle_ids_are_negated() {
        assert_eq!(database_id(1118, true), -2147484766);
        assert_eq!(database_id(1118, false), 1118);
        assert!(Database::get().device_by_id(database_id(1118, true)).is_some());
    }
}
