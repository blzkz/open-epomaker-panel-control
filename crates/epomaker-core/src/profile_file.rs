//! Backup / restore of a keyboard's full configuration as JSON.

use crate::keymap::{KeyMatrix, Layer};
use crate::lighting::{LightEffect, Rgb};
use crate::macros::MACRO_SLOT_LEN;
use crate::settings::{KeyboardOptions, ReportRate, SleepTimes, PROFILES};
use crate::{Error, Keyboard};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileBackup {
    pub report_rate: Option<ReportRate>,
    pub options: KeyboardOptions,
    pub layer: KeyMatrix,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Backup {
    pub format: u32,
    pub model: String,
    pub device_id: i64,
    pub active_profile: u8,
    pub light: Option<LightEffect>,
    pub debounce: u8,
    pub sleep: SleepTimes,
    pub profiles: Vec<ProfileBackup>,
    pub fn_layers: Vec<KeyMatrix>,
    /// Non-empty macro slots, raw 256-byte encoding as hex.
    pub macros: Vec<(u8, String)>,
    pub key_colors: Vec<Rgb>,
}

impl Backup {
    pub fn capture(kb: &mut Keyboard) -> Result<Backup, Error> {
        let active_profile = kb.profile()?;
        let mut profiles = Vec::new();
        for p in 0..PROFILES {
            profiles.push(ProfileBackup {
                report_rate: kb.report_rate(p)?,
                options: kb.options(p)?,
                layer: kb.read_layer(Layer::Profile(p))?,
            });
        }
        let mut fn_layers = Vec::new();
        for f in 0..kb.info.fn_layers {
            fn_layers.push(kb.read_layer(Layer::Fn(f))?);
        }
        let mut macros = Vec::new();
        for slot in kb.used_macro_slots()? {
            let raw = kb.read_macro_raw(slot)?;
            macros.push((slot, raw.iter().map(|b| format!("{b:02x}")).collect()));
        }
        Ok(Backup {
            format: 1,
            model: kb.model.name.clone(),
            device_id: kb.info.id,
            active_profile,
            light: kb.light()?,
            debounce: kb.debounce()?,
            sleep: kb.sleep_times()?,
            profiles,
            fn_layers,
            macros,
            key_colors: kb.key_colors()?,
        })
    }

    pub fn restore(&self, kb: &mut Keyboard) -> Result<(), Error> {
        if self.model != kb.model.name {
            return Err(Error::InvalidArgument(format!("backup is for {}, keyboard is {}", self.model, kb.model.name)));
        }
        for (slot, hex) in &self.macros {
            let bytes = parse_hex(hex)?;
            let mut data = [0u8; MACRO_SLOT_LEN];
            let n = bytes.len().min(MACRO_SLOT_LEN);
            data[..n].copy_from_slice(&bytes[..n]);
            kb.write_macro_raw(*slot, &data)?;
        }
        for (p, prof) in self.profiles.iter().enumerate() {
            let p = p as u8;
            if let Some(rate) = prof.report_rate {
                kb.set_report_rate(p, rate)?;
            }
            kb.set_options(p, &prof.options)?;
            kb.write_layer(Layer::Profile(p), &prof.layer)?;
        }
        for (f, layer) in self.fn_layers.iter().enumerate() {
            kb.write_layer(Layer::Fn(f as u8), layer)?;
        }
        kb.set_debounce(self.debounce)?;
        kb.set_sleep_times(&self.sleep)?;
        if !self.key_colors.is_empty() {
            kb.set_key_colors(&self.key_colors)?;
        }
        kb.set_profile(self.active_profile)?;
        if let Some(light) = &self.light {
            kb.set_light(light)?;
        }
        Ok(())
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>, Error> {
    (0..s.len() / 2)
        .map(|i| {
            u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| Error::InvalidArgument("bad hex in backup".into()))
        })
        .collect()
}
