//! High level API over a connected keyboard.

use crate::devices::{database_id, Database, DeviceInfo, ModelInfo};
use crate::keymap::{encode_read_matrix, encode_set_key, encode_write_matrix, KeyAction, KeyMatrix, Layer};
use crate::lighting::{LightEffect, Rgb};
use crate::macros::{self, Macro, MACRO_SLOT_LEN};
use crate::protocol::{cmd, query};
use crate::settings::{self, Battery, KeyboardOptions, ReportRate, SleepTimes};
use crate::transport::{sleep, Link, Pacing, Transport};
use crate::Error;

pub struct Keyboard {
    link: Link<Box<dyn Transport>>,
    pub info: DeviceInfo,
    pub model: &'static ModelInfo,
    /// Raw id reported by the firmware.
    pub raw_id: u32,
}

impl Keyboard {
    /// Identifies the keyboard behind `transport` with `GET_DEVICE_ID` and binds it to
    /// the matching database entry. `via_dongle` marks 2.4 GHz receivers.
    pub fn connect(transport: Box<dyn Transport>, via_dongle: bool) -> Result<Keyboard, Error> {
        let pacing = if transport.needs_pacing() { Pacing::default() } else { Pacing::none() };
        let mut link = Link { transport, pacing, trace: false };
        let resp = link.query(&query(cmd::GET_DEVICE_ID, 0, 0))?;
        if resp[0] != cmd::GET_DEVICE_ID {
            log::warn!("unexpected GET_DEVICE_ID echo {:02x}", resp[0]);
        }
        let raw_id = settings::decode_device_id(&resp);
        let id = database_id(raw_id, via_dongle);
        let db = Database::get();
        let info = db.device_by_id(id).cloned().ok_or(Error::UnknownDevice(id))?;
        let model = db.model(&info.name).ok_or(Error::UnknownDevice(id))?;
        Ok(Keyboard { link, info, model, raw_id })
    }

    /// Opens the first supported keyboard found on USB.
    #[cfg(feature = "hid")]
    pub fn open_first() -> Result<Keyboard, Error> {
        let candidates = crate::hid::enumerate()?;
        let mut last = Error::NotFound;
        for c in candidates {
            let dongle = Database::get().devices.iter().any(|d| d.pid == c.pid && d.dongle)
                && !Database::get().devices.iter().any(|d| d.pid == c.pid && !d.dongle);
            match crate::hid::HidTransport::open(&c).and_then(|t| Keyboard::connect(Box::new(t), dongle)) {
                Ok(k) => return Ok(k),
                Err(e) => {
                    log::warn!("{}: {e}", c.path);
                    last = e;
                }
            }
        }
        Err(last)
    }

    /// A keyboard backed by the firmware emulator (no hardware needed).
    pub fn emulated(id: i64) -> Result<Keyboard, Error> {
        let emu = crate::emulator::Emulator::new(id)?;
        Keyboard::connect(Box::new(emu), id < 0)
    }

    pub fn set_trace(&mut self, on: bool) {
        self.link.trace = on;
    }

    pub fn describe(&self) -> String {
        self.link.transport.describe()
    }

    pub fn is_emulated(&self) -> bool {
        !self.link.transport.needs_pacing()
    }

    // ---- device information ----

    pub fn firmware_version(&mut self) -> Result<u16, Error> {
        Ok(settings::decode_firmware(&self.link.query(&query(cmd::GET_FIRMWARE, 0, 0))?))
    }

    pub fn battery(&mut self) -> Result<Battery, Error> {
        Ok(Battery::decode(&self.link.query(&query(cmd::GET_BATTERY, 0, 0))?))
    }

    // ---- profiles ----

    pub fn profile(&mut self) -> Result<u8, Error> {
        Ok(settings::decode_profile(&self.link.query(&query(cmd::GET_PROFILE, 0, 0))?))
    }

    pub fn set_profile(&mut self, profile: u8) -> Result<(), Error> {
        if profile >= settings::PROFILES {
            return Err(Error::InvalidArgument(format!("profile {profile} (0..=2)")));
        }
        self.link.write(&settings::encode_set_profile(profile))?;
        sleep(self.link.pacing.after_light);
        Ok(())
    }

    // ---- lighting ----

    pub fn light(&mut self) -> Result<Option<LightEffect>, Error> {
        Ok(LightEffect::decode(&self.link.query(&LightEffect::query())?))
    }

    pub fn set_light(&mut self, effect: &LightEffect) -> Result<(), Error> {
        self.link.write(&effect.encode())?;
        sleep(self.link.pacing.after_light);
        Ok(())
    }

    /// Per-key colors of the custom picture (index = matrix index).
    pub fn key_colors(&mut self) -> Result<Vec<Rgb>, Error> {
        let data = self.link.query_all(&settings::encode_read_colors(self.model.pics_count()))?;
        Ok(settings::decode_colors(&data))
    }

    /// Sets one key color in custom-picture slot `slot` (0..=2).
    pub fn set_key_color(&mut self, slot: u8, matrix_index: usize, color: Rgb) -> Result<(), Error> {
        self.check_index(matrix_index)?;
        self.link.write(&settings::encode_set_key_color(slot, matrix_index as u8, color))
    }

    /// Writes all key colors at once (custom picture slot 0).
    pub fn set_key_colors(&mut self, colors: &[Rgb]) -> Result<(), Error> {
        let frames = settings::encode_set_all_colors(colors, self.model.pics_count());
        // the vendor app waits 20 ms (instead of 10) between these packets
        let saved = self.link.pacing.before_write;
        if !self.is_emulated() {
            self.link.pacing.before_write = std::time::Duration::from_millis(20);
        }
        let result = self.link.write_all(&frames);
        self.link.pacing.before_write = saved;
        result?;
        sleep(self.link.pacing.after_big_command);
        Ok(())
    }

    // ---- host-streamed effects ----

    /// Sends one frame of the "music follow" effects (32 band levels, 0..=6). The keyboard
    /// must be on [`LightMode::MusicFollow2`](crate::lighting::LightMode) or `MusicFollow3`
    /// and frames must keep coming (~every 30 ms) for it to animate.
    pub fn send_music_levels(&mut self, levels: &[u8]) -> Result<(), Error> {
        self.link.write(&crate::live::encode_music_frame(levels))
    }

    /// Sends the current screen color for the "screen color" effect (~every 40 ms).
    pub fn send_screen_color(&mut self, color: Rgb) -> Result<(), Error> {
        self.link.write(&crate::live::encode_screen_color(color))
    }

    // ---- per-profile settings ----

    pub fn report_rate(&mut self, profile: u8) -> Result<Option<ReportRate>, Error> {
        Ok(ReportRate::decode(&self.link.query(&ReportRate::query(profile))?))
    }

    pub fn set_report_rate(&mut self, profile: u8, rate: ReportRate) -> Result<(), Error> {
        self.link.write(&rate.encode(profile))?;
        sleep(self.link.pacing.after_big_command);
        Ok(())
    }

    pub fn options(&mut self, profile: u8) -> Result<KeyboardOptions, Error> {
        Ok(KeyboardOptions::decode(&self.link.query(&KeyboardOptions::query(profile))?))
    }

    pub fn set_options(&mut self, profile: u8, options: &KeyboardOptions) -> Result<(), Error> {
        self.link.write(&options.encode(profile))
    }

    pub fn debounce(&mut self) -> Result<u8, Error> {
        Ok(settings::decode_debounce(&self.link.query(&query(cmd::GET_DEBOUNCE, 0, 0))?))
    }

    pub fn set_debounce(&mut self, ms: u8) -> Result<(), Error> {
        self.link.write(&settings::encode_set_debounce(ms))?;
        sleep(self.link.pacing.after_light);
        Ok(())
    }

    pub fn sleep_times(&mut self) -> Result<SleepTimes, Error> {
        let r = self.link.query(&query(cmd::GET_SLEEP_TIME, 0, 0))?;
        Ok(SleepTimes::decode(&r, self.model.sleep_bt, self.model.sleep_dongle))
    }

    pub fn set_sleep_times(&mut self, times: &SleepTimes) -> Result<(), Error> {
        self.link.write(&times.encode())?;
        sleep(self.link.pacing.after_light);
        Ok(())
    }

    pub fn auto_os(&mut self) -> Result<bool, Error> {
        Ok(self.link.query(&query(cmd::GET_AUTO_OS, 0, 0))?[1] != 0)
    }

    pub fn set_auto_os(&mut self, enabled: bool) -> Result<(), Error> {
        self.link.write(&settings::encode_set_auto_os(enabled))
    }

    // ---- key mapping ----

    pub fn read_layer(&mut self, layer: Layer) -> Result<KeyMatrix, Error> {
        self.check_layer(layer)?;
        let mut data = self.link.query_all(&encode_read_matrix(layer))?;
        data.truncate(self.model.default_matrix.len());
        Ok(KeyMatrix::from_bytes(&data))
    }

    /// Remaps a single key (what the vendor app does for this family).
    pub fn set_key(&mut self, layer: Layer, matrix_index: usize, action: KeyAction) -> Result<(), Error> {
        self.check_layer(layer)?;
        self.check_index(matrix_index)?;
        self.link.write(&encode_set_key(layer, matrix_index as u8, action))
    }

    /// Writes a whole layer.
    pub fn write_layer(&mut self, layer: Layer, matrix: &KeyMatrix) -> Result<(), Error> {
        self.check_layer(layer)?;
        let mut bytes = matrix.to_bytes();
        bytes.resize(self.model.default_matrix.len(), 0);
        self.link.write_all(&encode_write_matrix(layer, &bytes))?;
        sleep(self.link.pacing.after_big_command);
        Ok(())
    }

    /// Restores the factory mapping of one layer, key by key.
    pub fn reset_layer(&mut self, layer: Layer) -> Result<(), Error> {
        let current = self.read_layer(layer)?;
        for i in 0..self.model.matrix_len() {
            let def = self.model.default_entry(i).unwrap_or_default();
            if current.entries.get(i) != Some(&def) {
                self.set_key(layer, i, KeyAction::Raw(def))?;
            }
        }
        Ok(())
    }

    // ---- macros ----

    pub fn read_macro(&mut self, slot: u8) -> Result<Macro, Error> {
        self.check_slot(slot)?;
        let data = self.link.query_all(&macros::encode_read(slot))?;
        Ok(Macro::decode(&data))
    }

    pub fn read_macro_raw(&mut self, slot: u8) -> Result<Vec<u8>, Error> {
        self.check_slot(slot)?;
        self.link.query_all(&macros::encode_read(slot))
    }

    pub fn write_macro(&mut self, slot: u8, m: &Macro) -> Result<(), Error> {
        self.check_slot(slot)?;
        let data = m.encode()?;
        self.write_macro_raw(slot, &data)
    }

    pub fn write_macro_raw(&mut self, slot: u8, data: &[u8; MACRO_SLOT_LEN]) -> Result<(), Error> {
        self.link.write_all(&macros::encode_write(slot, data)?)?;
        sleep(self.link.pacing.after_big_command);
        Ok(())
    }

    /// Macro slots referenced by any layer (to find a free one).
    pub fn used_macro_slots(&mut self) -> Result<Vec<u8>, Error> {
        let mut used = Vec::new();
        for layer in self.layers() {
            used.extend(self.read_layer(layer)?.macro_indices());
        }
        used.sort();
        used.dedup();
        Ok(used)
    }

    /// All layers of this model: base layers of the 3 profiles and the Fn layers.
    pub fn layers(&self) -> Vec<Layer> {
        let mut v: Vec<Layer> = (0..settings::PROFILES).map(Layer::Profile).collect();
        v.extend((0..self.info.fn_layers).map(Layer::Fn));
        v
    }

    // ---- maintenance ----

    pub fn factory_reset(&mut self) -> Result<(), Error> {
        self.link.write(&settings::encode_factory_reset())?;
        sleep(self.link.pacing.after_big_command);
        Ok(())
    }

    fn check_index(&self, i: usize) -> Result<(), Error> {
        if i >= self.model.matrix_len() || i > u8::MAX as usize {
            return Err(Error::InvalidArgument(format!("matrix index {i}")));
        }
        Ok(())
    }

    fn check_slot(&self, slot: u8) -> Result<(), Error> {
        if slot >= self.model.macro_slots {
            return Err(Error::InvalidArgument(format!("macro slot {slot} (0..{})", self.model.macro_slots)));
        }
        Ok(())
    }

    fn check_layer(&self, layer: Layer) -> Result<(), Error> {
        match layer {
            Layer::Profile(p) if p < settings::PROFILES => Ok(()),
            Layer::Fn(f) if f < self.info.fn_layers.max(1) => Ok(()),
            _ => Err(Error::InvalidArgument(format!("layer {layer:?}"))),
        }
    }
}
