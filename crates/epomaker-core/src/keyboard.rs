//! High level API over a connected keyboard.

use crate::devices::{database_id, Database, DeviceInfo, ModelInfo};
use crate::keymap::{
    encode_read_matrix, encode_set_key, encode_write_matrix, KeyAction, KeyMatrix, Layer, MATRIX_WRITE_CHUNK,
    MATRIX_WRITE_PACKETS,
};
use crate::lighting::{LightEffect, Rgb};
use crate::macros::{self, Macro, MACRO_SLOT_LEN};
use crate::protocol::{cmd, query, report_from, Checksum, Frame, Report};
use crate::settings::{self, Battery, KeyboardOptions, ReportRate, SleepTimes};
use crate::transport::{hex, sleep, Link, Pacing, Transport};
use crate::Error;
use std::fmt::Debug;

pub struct Keyboard {
    link: Link<Box<dyn Transport>>,
    pub info: DeviceInfo,
    pub model: &'static ModelInfo,
    /// Raw id reported by the firmware.
    pub raw_id: u32,
    /// Read every write back and fail with [`Error::VerifyMismatch`] when it did not stick.
    verify: bool,
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
        Ok(Keyboard { link, info, model, raw_id, verify: false })
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

    /// Delays between reports and retries of failed transfers (see [`Pacing`]).
    pub fn pacing(&self) -> Pacing {
        self.link.pacing
    }

    pub fn set_pacing(&mut self, pacing: Pacing) {
        self.link.pacing = pacing;
    }

    pub fn set_trace(&mut self, on: bool) {
        self.link.trace = on;
    }

    /// Verify-after-write: when on, every setter reads the value back and returns
    /// [`Error::VerifyMismatch`] if the keyboard reports something else. Off by default
    /// (it roughly doubles the time of each write).
    pub fn set_verify(&mut self, on: bool) {
        self.verify = on;
    }

    pub fn verify(&self) -> bool {
        self.verify
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
        if self.verify {
            let got = self.profile()?;
            check("profile", &profile, &got)?;
        }
        Ok(())
    }

    // ---- lighting ----

    pub fn light(&mut self) -> Result<Option<LightEffect>, Error> {
        Ok(LightEffect::decode(&self.link.query(&LightEffect::query())?))
    }

    pub fn set_light(&mut self, effect: &LightEffect) -> Result<(), Error> {
        let frame = effect.encode();
        self.link.write_light(&frame)?;
        sleep(self.link.pacing.after_light);
        if self.verify {
            // compare what the decoder makes of both, so fields the effect ignores do not count
            let sent = frame.wire_bytes();
            let got = self.link.query(&LightEffect::query())?;
            if LightEffect::decode(&sent) != LightEffect::decode(&got) {
                return Err(mismatch(
                    "lighting",
                    format!("{:?} [{}]", LightEffect::decode(&sent), hex(&sent[1..8])),
                    format!("{:?} [{}]", LightEffect::decode(&got), hex(&got[1..8])),
                ));
            }
        }
        Ok(())
    }

    /// The 7 bytes of `GET_LED` that describe the current effect, exactly as reported.
    pub fn light_raw(&mut self) -> Result<[u8; 7], Error> {
        let r = self.link.query(&LightEffect::query())?;
        let mut out = [0u8; 7];
        out.copy_from_slice(&r[1..8]);
        Ok(out)
    }

    /// Writes back bytes read with [`Keyboard::light_raw`] (`SET_LED` uses the same layout).
    pub fn set_light_raw(&mut self, raw: &[u8; 7]) -> Result<(), Error> {
        let mut r = report_from(&[cmd::SET_LED]);
        r[1..8].copy_from_slice(raw);
        self.link.write_light(&Frame::new(r, Checksum::Bit8))?;
        sleep(self.link.pacing.after_light);
        if self.verify {
            let got = self.light_raw()?;
            check_bytes("lighting (raw)", raw, &got)?;
        }
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
        self.link.write(&settings::encode_set_key_color(slot, matrix_index as u8, color))?;
        // only slot 0 can be read back (the read command of other slots is unknown)
        if self.verify && slot == 0 {
            let got = self.key_colors()?.get(matrix_index).copied();
            check(&format!("color of key #{matrix_index}"), &Some(color), &got)?;
        }
        Ok(())
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
        if self.verify {
            let got = self.key_colors()?;
            let n = colors.len().min(self.model.matrix_len()).min(200);
            for (i, c) in colors.iter().enumerate().take(n) {
                check(&format!("color of key #{i}"), &Some(*c), &got.get(i).copied())?;
            }
        }
        Ok(())
    }

    // ---- host-streamed effects ----

    /// Sends one frame of the "music follow" effects (32 band levels, 0..=6). The keyboard
    /// must be on [`LightMode::MusicFollow2`](crate::lighting::LightMode) or `MusicFollow3`
    /// and frames must keep coming (~every 30 ms) for it to animate.
    pub fn send_music_levels(&mut self, levels: &[u8]) -> Result<(), Error> {
        self.link.write_once(&crate::live::encode_music_frame(levels))
    }

    /// Sends the current screen color for the "screen color" effect (~every 40 ms).
    pub fn send_screen_color(&mut self, color: Rgb) -> Result<(), Error> {
        self.link.write_once(&crate::live::encode_screen_color(color))
    }

    // ---- per-profile settings ----

    pub fn report_rate(&mut self, profile: u8) -> Result<Option<ReportRate>, Error> {
        Ok(ReportRate::decode(&self.link.query(&ReportRate::query(profile))?))
    }

    pub fn set_report_rate(&mut self, profile: u8, rate: ReportRate) -> Result<(), Error> {
        self.on_profile(profile, |kb| kb.set_report_rate_active(profile, rate))
    }

    fn set_report_rate_active(&mut self, profile: u8, rate: ReportRate) -> Result<(), Error> {
        self.link.write(&rate.encode(profile))?;
        sleep(self.link.pacing.after_big_command);
        if self.verify {
            let got = self.report_rate(profile)?;
            check(&format!("report rate of profile {profile}"), &Some(rate), &got)?;
        }
        Ok(())
    }

    pub fn options(&mut self, profile: u8) -> Result<KeyboardOptions, Error> {
        Ok(KeyboardOptions::decode(&self.link.query(&KeyboardOptions::query(profile))?))
    }

    pub fn set_options(&mut self, profile: u8, options: &KeyboardOptions) -> Result<(), Error> {
        self.on_profile(profile, |kb| kb.set_options_active(profile, options))
    }

    fn set_options_active(&mut self, profile: u8, options: &KeyboardOptions) -> Result<(), Error> {
        let frame = options.encode(profile);
        self.link.write(&frame)?;
        if self.verify {
            // the read uses the write's layout; decoding both normalises unused bits
            let expected = KeyboardOptions::decode(&frame.wire_bytes());
            let got = self.options(profile)?;
            check(&format!("options of profile {profile}"), &expected, &got)?;
        }
        Ok(())
    }

    pub fn debounce(&mut self) -> Result<u8, Error> {
        Ok(settings::decode_debounce(&self.link.query(&query(cmd::GET_DEBOUNCE, 0, 0))?))
    }

    pub fn set_debounce(&mut self, ms: u8) -> Result<(), Error> {
        self.link.write(&settings::encode_set_debounce(ms))?;
        sleep(self.link.pacing.after_light);
        if self.verify {
            let got = self.debounce()?;
            check("debounce", &ms, &got)?;
        }
        Ok(())
    }

    pub fn sleep_times(&mut self) -> Result<SleepTimes, Error> {
        let r = self.link.query(&query(cmd::GET_SLEEP_TIME, 0, 0))?;
        Ok(SleepTimes::decode(&r, self.model.sleep_bt, self.model.sleep_dongle))
    }

    pub fn set_sleep_times(&mut self, times: &SleepTimes) -> Result<(), Error> {
        let frame = times.encode();
        self.link.write(&frame)?;
        sleep(self.link.pacing.after_light);
        if self.verify {
            // raw comparison: decoding clamps to the model's limits
            let got = self.link.query(&query(cmd::GET_SLEEP_TIME, 0, 0))?;
            check_bytes("sleep timers", &frame.data[8..16], &got[1..9])?;
        }
        Ok(())
    }

    pub fn auto_os(&mut self) -> Result<bool, Error> {
        Ok(self.link.query(&query(cmd::GET_AUTO_OS, 0, 0))?[1] != 0)
    }

    pub fn set_auto_os(&mut self, enabled: bool) -> Result<(), Error> {
        self.link.write(&settings::encode_set_auto_os(enabled))?;
        if self.verify {
            let got = self.auto_os()?;
            check("automatic OS detection", &enabled, &got)?;
        }
        Ok(())
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
        self.on_layer(layer, |kb| {
            kb.write_key(layer, matrix_index, action)?;
            if kb.verify {
                let got = kb.read_layer(layer)?;
                check_entry(layer, matrix_index, action.to_bytes(), got.entries.get(matrix_index).copied())?;
            }
            Ok(())
        })
    }

    fn write_key(&mut self, layer: Layer, matrix_index: usize, action: KeyAction) -> Result<(), Error> {
        self.check_layer(layer)?;
        self.check_index(matrix_index)?;
        self.link.write(&encode_set_key(layer, matrix_index as u8, action))?;
        sleep(self.link.pacing.after_key);
        Ok(())
    }

    /// Runs `f` with `profile` active on the keyboard. The firmware applies key, polling
    /// rate and option writes to the active profile only (the vendor app always sends them
    /// for the profile it has just activated), so another profile is activated for the
    /// write and the previous one restored afterwards.
    fn on_profile<R>(&mut self, profile: u8, f: impl FnOnce(&mut Self) -> Result<R, Error>) -> Result<R, Error> {
        if profile >= settings::PROFILES {
            return Err(Error::InvalidArgument(format!("profile {profile} (0..=2)")));
        }
        let active = self.profile()?;
        if active == profile {
            return f(self);
        }
        log::info!("activating profile {} for the write (profile {} was active)", profile + 1, active + 1);
        self.link.write(&settings::encode_set_profile(profile))?;
        sleep(self.link.pacing.after_light);
        let result = f(self);
        let back = self.link.write(&settings::encode_set_profile(active));
        sleep(self.link.pacing.after_light);
        let r = result?;
        back?;
        Ok(r)
    }

    fn on_layer<R>(&mut self, layer: Layer, f: impl FnOnce(&mut Self) -> Result<R, Error>) -> Result<R, Error> {
        match layer {
            Layer::Profile(p) => self.on_profile(p, f),
            Layer::Fn(_) => f(self),
        }
    }

    /// Writes a whole layer.
    pub fn write_layer(&mut self, layer: Layer, matrix: &KeyMatrix) -> Result<(), Error> {
        self.on_layer(layer, |kb| kb.write_layer_active(layer, matrix))
    }

    fn write_layer_active(&mut self, layer: Layer, matrix: &KeyMatrix) -> Result<(), Error> {
        self.check_layer(layer)?;
        let mut bytes = matrix.to_bytes();
        bytes.resize(self.model.default_matrix.len(), 0);
        self.link.write_all(&encode_write_matrix(layer, &bytes))?;
        sleep(self.link.pacing.after_big_command);
        if self.verify {
            // the bulk write never carries the last bytes of a full 512-byte layer
            let sent = (MATRIX_WRITE_CHUNK * MATRIX_WRITE_PACKETS).min(bytes.len()) / 4;
            let expected = KeyMatrix::from_bytes(&bytes);
            self.verify_layer(layer, &expected, sent)?;
        }
        Ok(())
    }

    /// Restores the factory mapping of one layer, key by key.
    pub fn reset_layer(&mut self, layer: Layer) -> Result<(), Error> {
        self.on_layer(layer, |kb| kb.reset_layer_active(layer))
    }

    fn reset_layer_active(&mut self, layer: Layer) -> Result<(), Error> {
        let current = self.read_layer(layer)?;
        for i in 0..self.model.matrix_len() {
            let def = self.model.default_entry(i).unwrap_or_default();
            if current.entries.get(i) != Some(&def) {
                self.write_key(layer, i, KeyAction::Raw(def))?;
            }
        }
        if self.verify {
            let factory = KeyMatrix::from_bytes(&self.model.default_matrix);
            self.verify_layer(layer, &factory, self.model.matrix_len())?;
        }
        Ok(())
    }

    /// Compares the first `entries` keys of `layer` with `expected`.
    fn verify_layer(&mut self, layer: Layer, expected: &KeyMatrix, entries: usize) -> Result<(), Error> {
        let got = self.read_layer(layer)?;
        for (i, e) in expected.entries.iter().enumerate().take(entries) {
            check_entry(layer, i, *e, got.entries.get(i).copied())?;
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
        let frames = macros::encode_write(slot, data)?;
        self.link.write_all(&frames)?;
        sleep(self.link.pacing.after_big_command);
        if self.verify {
            // only the chunks actually sent are known; the firmware may keep older bytes after them
            let sent = (frames.len() * macros::WRITE_CHUNK).min(MACRO_SLOT_LEN);
            self.verify_macro(slot, &data[..sent])?;
        }
        Ok(())
    }

    /// Empties a macro slot. An all-zero macro produces no packets with the vendor's
    /// write routine, so this sends the first chunk explicitly, zeroed.
    pub fn clear_macro(&mut self, slot: u8) -> Result<(), Error> {
        self.check_slot(slot)?;
        self.link.write(&macros::encode_clear(slot)?)?;
        sleep(self.link.pacing.after_big_command);
        if self.verify {
            self.verify_macro(slot, &[0; macros::WRITE_CHUNK])?;
        }
        Ok(())
    }

    fn verify_macro(&mut self, slot: u8, expected: &[u8]) -> Result<(), Error> {
        let got = self.read_macro_raw(slot)?;
        let got = &got[..expected.len().min(got.len())];
        check_bytes(&format!("macro {slot}"), expected, got)
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

    /// First macro slot not referenced by any layer.
    pub fn first_free_macro_slot(&mut self) -> Result<Option<u8>, Error> {
        let used = self.used_macro_slots()?;
        Ok((0..self.model.macro_slots).find(|s| !used.contains(s)))
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

    /// Sends one read-only query and returns the raw answer (diagnostics).
    pub fn raw_query(&mut self, command: u8, arg1: u8, arg2: u8) -> Result<Report, Error> {
        self.link.query(&query(command, arg1, arg2))
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

fn mismatch(what: &str, expected: String, got: String) -> Error {
    Error::VerifyMismatch { what: what.to_string(), expected, got }
}

fn check<T: PartialEq + Debug>(what: &str, expected: &T, got: &T) -> Result<(), Error> {
    if expected == got {
        Ok(())
    } else {
        Err(mismatch(what, format!("{expected:?}"), format!("{got:?}")))
    }
}

fn check_bytes(what: &str, expected: &[u8], got: &[u8]) -> Result<(), Error> {
    if expected == got {
        return Ok(());
    }
    let first = expected.iter().zip(got).position(|(a, b)| a != b).unwrap_or(expected.len().min(got.len()));
    let full = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ");
    Err(mismatch(&format!("{what} (first difference at byte {first})"), full(expected), full(got)))
}

fn check_entry(layer: Layer, index: usize, expected: [u8; 4], got: Option<[u8; 4]>) -> Result<(), Error> {
    if got == Some(expected) {
        return Ok(());
    }
    let entry = |e: [u8; 4]| format!("{:02x}{:02x}{:02x}{:02x}", e[0], e[1], e[2], e[3]);
    Err(mismatch(
        &format!("key #{index} on {layer:?}"),
        entry(expected),
        got.map(entry).unwrap_or_else(|| "nothing".into()),
    ))
}
