//! Small settings: profile, polling rate, keyboard options, debounce, sleep timers,
//! battery, firmware version, device id, per-key colors.

use crate::lighting::Rgb;
use crate::protocol::{cmd, query, report_from, Checksum, Frame, Report};
use serde::{Deserialize, Serialize};

/// Number of onboard profiles.
pub const PROFILES: u8 = 3;

pub fn encode_set_profile(profile: u8) -> Frame {
    Frame::new(report_from(&[cmd::SET_PROFILE, profile]), Checksum::Bit7)
}

pub fn decode_profile(resp: &Report) -> u8 {
    resp[1]
}

/// USB polling rates supported by the firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReportRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
}

impl ReportRate {
    pub const ALL: [ReportRate; 4] = [ReportRate::Hz125, ReportRate::Hz250, ReportRate::Hz500, ReportRate::Hz1000];

    pub fn hz(self) -> u16 {
        match self {
            ReportRate::Hz125 => 125,
            ReportRate::Hz250 => 250,
            ReportRate::Hz500 => 500,
            ReportRate::Hz1000 => 1000,
        }
    }

    pub fn from_hz(hz: u16) -> Option<ReportRate> {
        ReportRate::ALL.into_iter().find(|r| r.hz() == hz)
    }

    fn code(self) -> u8 {
        match self {
            ReportRate::Hz1000 => 1,
            ReportRate::Hz500 => 2,
            ReportRate::Hz250 => 4,
            ReportRate::Hz125 => 8,
        }
    }

    pub fn encode(self, profile: u8) -> Frame {
        Frame::new(report_from(&[cmd::SET_REPORT_RATE, profile, self.code()]), Checksum::Bit7)
    }

    pub fn query(profile: u8) -> Frame {
        query(cmd::GET_REPORT_RATE, profile, 0)
    }

    pub fn decode(resp: &Report) -> Option<ReportRate> {
        ReportRate::ALL.into_iter().find(|r| r.code() == resp[2])
    }
}

/// Toggles stored per profile with `SET_KB_OPTION` (0x06).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardOptions {
    /// Disable the Windows/Super key.
    pub win_key_lock: bool,
    /// Mac mode (system layout).
    pub mac_mode: bool,
    /// Swap WASD and arrow keys.
    pub wasd_arrows_swap: bool,
    /// Backlight off.
    pub led_off: bool,
    /// Side/indicator LEDs off.
    pub side_led_off: bool,
    /// Vendor "keyboard mode" flag (gaming mode).
    pub gaming_mode: bool,
    /// Lock the whole keyboard.
    pub keyboard_lock: bool,
    /// Swap the meaning of Fn-layer keys (F1..F12 vs. multimedia).
    pub fn_key_swap: bool,
    /// Power saving mode (wireless).
    pub power_save: bool,
}

impl KeyboardOptions {
    pub fn encode(&self, profile: u8) -> Frame {
        let flags = u8::from(self.win_key_lock)
            | (u8::from(self.mac_mode) << 2)
            | (u8::from(self.wasd_arrows_swap) << 3)
            | (u8::from(self.led_off) << 4)
            | (u8::from(self.side_led_off) << 5)
            | (u8::from(self.gaming_mode) << 6)
            | (u8::from(self.keyboard_lock) << 7);
        Frame::new(
            report_from(&[cmd::SET_KB_OPTION, profile, flags, u8::from(self.fn_key_swap), u8::from(self.power_save)]),
            Checksum::Bit7,
        )
    }

    pub fn query(profile: u8) -> Frame {
        query(cmd::GET_KB_OPTION, profile, 0)
    }

    pub fn decode(resp: &Report) -> KeyboardOptions {
        let f = resp[2];
        KeyboardOptions {
            win_key_lock: f & 0x01 != 0,
            mac_mode: f & 0x06 != 0,
            wasd_arrows_swap: f & 0x08 != 0,
            led_off: f & 0x10 != 0,
            side_led_off: f & 0x20 != 0,
            gaming_mode: f & 0x40 != 0,
            keyboard_lock: f & 0x80 != 0,
            fn_key_swap: resp[3] & 0x01 != 0,
            power_save: resp[4] != 0,
        }
    }
}

/// Key debounce time (ms).
pub fn encode_set_debounce(ms: u8) -> Frame {
    Frame::new(report_from(&[cmd::SET_DEBOUNCE, 0, ms]), Checksum::Bit7)
}

pub fn decode_debounce(resp: &Report) -> u8 {
    resp[2]
}

/// Inactivity timers for wireless modes, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepTimes {
    /// Bluetooth: backlight off after (0 = never).
    pub bt_light_off: u16,
    /// 2.4 GHz: backlight off after (0 = never).
    pub dongle_light_off: u16,
    /// Bluetooth: deep sleep after.
    pub bt_deep_sleep: u16,
    /// 2.4 GHz: deep sleep after.
    pub dongle_deep_sleep: u16,
}

impl Default for SleepTimes {
    fn default() -> Self {
        SleepTimes { bt_light_off: 120, dongle_light_off: 120, bt_deep_sleep: 1680, dongle_deep_sleep: 1680 }
    }
}

/// Allowed ranges (minutes) per model, from the device database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepLimits {
    pub min: u16,
    pub max: u16,
    pub min_deep: u16,
    pub max_deep: u16,
}

impl SleepTimes {
    /// Note: the write puts the values at bytes 8..16 while the read returns them at 1..9.
    pub fn encode(&self) -> Frame {
        let mut r = report_from(&[cmd::SET_SLEEP_TIME]);
        for (i, v) in [self.bt_light_off, self.dongle_light_off, self.bt_deep_sleep, self.dongle_deep_sleep]
            .into_iter()
            .enumerate()
        {
            r[8 + 2 * i..10 + 2 * i].copy_from_slice(&v.to_le_bytes());
        }
        Frame::new(r, Checksum::Bit7)
    }

    pub fn decode(resp: &Report, bt: Option<SleepLimits>, dongle: Option<SleepLimits>) -> SleepTimes {
        let at = |i: usize| u16::from_le_bytes([resp[i], resp[i + 1]]);
        let mut s =
            SleepTimes { bt_light_off: at(1), dongle_light_off: at(3), bt_deep_sleep: at(5), dongle_deep_sleep: at(7) };
        let clamp = |v: u16, lo: u16, hi: u16| v.clamp(lo * 60, hi * 60);
        match dongle {
            Some(l) => {
                if s.dongle_light_off != 0 {
                    s.dongle_light_off = clamp(s.dongle_light_off, l.min, l.max);
                }
                s.dongle_deep_sleep = clamp(s.dongle_deep_sleep, l.min_deep, l.max_deep);
            }
            None => {
                s.dongle_light_off = 0;
                s.dongle_deep_sleep = 0;
            }
        }
        match bt {
            Some(l) => {
                if s.bt_light_off != 0 {
                    s.bt_light_off = clamp(s.bt_light_off, l.min, l.max);
                }
                s.bt_deep_sleep = clamp(s.bt_deep_sleep, l.min_deep, l.max_deep);
            }
            None => {
                s.bt_light_off = 0;
                s.bt_deep_sleep = 0;
            }
        }
        s
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeState {
    Discharging,
    Charging,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: u8,
    pub state: ChargeState,
}

impl Battery {
    pub fn decode(resp: &Report) -> Battery {
        let state = match resp[2] {
            0 => ChargeState::Discharging,
            1 => ChargeState::Charging,
            _ => ChargeState::Full,
        };
        Battery { percent: resp[1], state }
    }
}

pub fn decode_firmware(resp: &Report) -> u16 {
    u16::from_le_bytes([resp[1], resp[2]])
}

/// Model id reported by the firmware (`GET_DEVICE_ID`, 0x8F): `u32` LE at bytes 1..5.
/// Through a 2.4 GHz dongle the vendor driver sets bit 31.
pub fn decode_device_id(resp: &Report) -> u32 {
    u32::from_le_bytes([resp[1], resp[2], resp[3], resp[4]])
}

pub fn encode_factory_reset() -> Frame {
    Frame::new(report_from(&[cmd::RESET]), Checksum::Bit7)
}

/// "Auto OS detection" toggle.
pub fn encode_set_auto_os(enabled: bool) -> Frame {
    Frame::new(report_from(&[cmd::SET_AUTO_OS, u8::from(enabled)]), Checksum::Bit7)
}

/// Sets the color of one key in custom-picture slot `slot` (0-based).
pub fn encode_set_key_color(slot: u8, matrix_index: u8, color: Rgb) -> Frame {
    let mut r = report_from(&[cmd::SET_USER_PIC_SIMPLE, slot, matrix_index]);
    r[8..11].copy_from_slice(&color.to_array());
    Frame::new(r, Checksum::Bit7)
}

/// Writes every key color at once. `colors[i]` is the color of matrix entry `i`.
/// The vendor app always uses slot 0 for this bulk write.
pub fn encode_set_all_colors(colors: &[Rgb], pics_count: usize) -> Vec<Frame> {
    let mut buf = vec![0u8; 600];
    for (i, c) in colors.iter().enumerate().take(200) {
        buf[3 * i..3 * i + 3].copy_from_slice(&c.to_array());
    }
    let len = ((64 * pics_count) as u16).to_le_bytes();
    (0..7)
        .map(|s| {
            let mut r = report_from(&[cmd::SET_USER_PIC, 0, len[0], len[1], s as u8]);
            r[8..64].copy_from_slice(&buf[56 * s..56 * s + 56]);
            Frame::new(r, Checksum::Bit7)
        })
        .collect()
}

pub fn encode_read_colors(pics_count: usize) -> Vec<Frame> {
    (0..pics_count).map(|i| query(cmd::GET_USER_PIC, 0, i as u8)).collect()
}

pub fn decode_colors(data: &[u8]) -> Vec<Rgb> {
    data.as_chunks::<3>().0.iter().map(|c| Rgb::new(c[0], c[1], c[2])).collect()
}
