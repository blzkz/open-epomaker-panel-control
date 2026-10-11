//! Software model of a YC500 keyboard's firmware.
//!
//! Used by the test-suite and by the `--demo` mode of the CLI/GUI so the whole
//! application can be exercised without hardware. It validates checksums the same
//! way the protocol requires and answers queries with the layouts the vendor
//! software expects.

use crate::devices::{Database, ModelInfo};
use crate::protocol::{byte_sum, cmd, Report, REPORT_LEN};
use crate::transport::Transport;
use crate::Error;
use std::sync::{Arc, Mutex};

const PROFILES: usize = 3;
const PIC_SLOTS: usize = 3;
const PIC_LEN: usize = 600;

#[derive(Clone, Debug)]
pub struct EmulatorState {
    pub device_id: u32,
    pub firmware: u16,
    pub profile: u8,
    pub led: [u8; 8],
    pub report_rate: [u8; PROFILES],
    pub options: [[u8; 3]; PROFILES],
    pub debounce: u8,
    pub sleep: [u8; 8],
    pub battery: (u8, u8),
    pub auto_os: bool,
    pub matrices: Vec<Vec<u8>>,
    pub fn_matrices: Vec<Vec<u8>>,
    pub macros: Vec<[u8; 256]>,
    pub pics: Vec<Vec<u8>>,
    /// Every report received, in order (with checksum).
    pub log: Vec<Report>,
    pub checksum_errors: usize,
}

impl EmulatorState {
    pub fn new(device_id: u32, model: &ModelInfo) -> Self {
        let mut s = EmulatorState {
            device_id,
            firmware: 0x0105,
            profile: 0,
            led: [0; 8],
            report_rate: [1; PROFILES],
            options: [[0; 3]; PROFILES],
            debounce: model.default_debounce.unwrap_or(5),
            sleep: [0; 8],
            battery: (87, 0),
            auto_os: false,
            matrices: vec![],
            fn_matrices: vec![],
            macros: vec![],
            pics: vec![],
            log: vec![],
            checksum_errors: 0,
        };
        s.factory_reset(model);
        s
    }

    fn factory_reset(&mut self, model: &ModelInfo) {
        // Wave, speed 2, brightness 4, rainbow
        self.led = [cmd::SET_LED, 4, 2, 4, 0x08, 0xff, 0xff, 0xff];
        self.profile = 0;
        self.report_rate = [1; PROFILES];
        self.options = [[0; 3]; PROFILES];
        let sleep: [u16; 4] = [120, 120, 1680, 1680];
        for (i, v) in sleep.iter().enumerate() {
            self.sleep[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        let mut m = model.default_matrix.clone();
        m.resize(512, 0);
        self.matrices = vec![m.clone(); PROFILES];
        self.fn_matrices = vec![vec![0; 512]; 1];
        self.macros = vec![[0; 256]; 50];
        self.pics = vec![vec![0; PIC_LEN]; PIC_SLOTS];
        // a pleasant default custom picture: a gradient
        for slot in self.pics.iter_mut() {
            for i in 0..model.matrix_len() {
                let t = (i * 255 / model.matrix_len().max(1)) as u8;
                slot[3 * i..3 * i + 3].copy_from_slice(&[255 - t, t / 2, t]);
            }
        }
    }
}

/// Emulated keyboard. Cloning shares the same state (handy for inspecting it in tests).
#[derive(Clone)]
pub struct Emulator {
    pub state: Arc<Mutex<EmulatorState>>,
    model: &'static ModelInfo,
    pending: Report,
}

impl Emulator {
    /// Emulates the model whose database id is `id` (e.g. 1118 for a wired TH80).
    pub fn new(id: i64) -> Result<Emulator, Error> {
        let db = Database::get();
        let dev = db.device_by_id(id).ok_or(Error::UnknownDevice(id))?;
        let model = db.model(&dev.name).ok_or(Error::UnknownDevice(id))?;
        let raw = (id.unsigned_abs() as u32) & 0x7fff_ffff;
        Ok(Emulator { state: Arc::new(Mutex::new(EmulatorState::new(raw, model))), model, pending: [0; REPORT_LEN] })
    }

    fn handle(&mut self, r: &Report) -> Result<(), Error> {
        let mut s = self.state.lock().unwrap();
        s.log.push(*r);
        let c = r[0];
        let ok = match c {
            cmd::SET_LED => r[8] == !byte_sum(&r[..8]),
            _ => r[7] == !byte_sum(&r[..7]),
        };
        if !ok {
            s.checksum_errors += 1;
            return Err(Error::Protocol(format!("emulator: bad checksum in {:02x?}", &r[..9])));
        }
        let mut resp = [0u8; REPORT_LEN];
        resp[0] = c;
        let p = |x: u8| (x as usize).min(PROFILES - 1);
        let data = &r[8..];
        match c {
            cmd::GET_FIRMWARE => resp[1..3].copy_from_slice(&s.firmware.to_le_bytes()),
            cmd::GET_DEVICE_ID => resp[1..5].copy_from_slice(&s.device_id.to_le_bytes()),
            cmd::GET_BATTERY => {
                resp[1] = s.battery.0;
                resp[2] = s.battery.1;
            }
            cmd::RESET => s.factory_reset(self.model),
            cmd::SET_PROFILE => s.profile = r[1].min(PROFILES as u8 - 1),
            cmd::GET_PROFILE => resp[1] = s.profile,
            cmd::SET_REPORT_RATE => s.report_rate[p(r[1])] = r[2],
            cmd::GET_REPORT_RATE => {
                resp[1] = r[1];
                resp[2] = s.report_rate[p(r[1])];
            }
            cmd::SET_KB_OPTION => s.options[p(r[1])] = [r[2], r[3], r[4]],
            cmd::GET_KB_OPTION => {
                resp[1] = r[1];
                resp[2..5].copy_from_slice(&s.options[p(r[1])]);
            }
            cmd::SET_LED => s.led.copy_from_slice(&r[..8]),
            cmd::GET_LED => resp[1..8].copy_from_slice(&s.led[1..8]),
            cmd::SET_DEBOUNCE => s.debounce = r[2],
            cmd::GET_DEBOUNCE => resp[2] = s.debounce,
            cmd::SET_SLEEP_TIME => s.sleep.copy_from_slice(&r[8..16]),
            cmd::GET_SLEEP_TIME => resp[1..9].copy_from_slice(&s.sleep.clone()),
            cmd::SET_AUTO_OS => s.auto_os = r[1] != 0,
            cmd::GET_AUTO_OS => resp[1] = u8::from(s.auto_os),
            cmd::SET_KEY_SIMPLE | cmd::SET_FN_SIMPLE => {
                let idx = r[2] as usize * 4;
                let target = if c == cmd::SET_KEY_SIMPLE {
                    let i = p(r[1]);
                    &mut s.matrices[i]
                } else {
                    let i = (r[1] as usize).min(s.fn_matrices.len() - 1);
                    &mut s.fn_matrices[i]
                };
                if idx + 4 <= target.len() {
                    target[idx..idx + 4].copy_from_slice(&r[8..12]);
                }
            }
            cmd::SET_KEY_MATRIX | cmd::SET_FN_MATRIX => {
                let off = r[4] as usize * 56;
                let target = if c == cmd::SET_KEY_MATRIX {
                    let i = p(r[1]);
                    &mut s.matrices[i]
                } else {
                    let i = (r[1] as usize).min(s.fn_matrices.len() - 1);
                    &mut s.fn_matrices[i]
                };
                let end = (off + 56).min(target.len());
                if off < end {
                    target[off..end].copy_from_slice(&data[..end - off]);
                }
            }
            cmd::GET_KEY_MATRIX | cmd::GET_FN_MATRIX => {
                let off = r[2] as usize * 64;
                let src = if c == cmd::GET_KEY_MATRIX {
                    &s.matrices[p(r[1])]
                } else {
                    &s.fn_matrices[(r[1] as usize).min(s.fn_matrices.len() - 1)]
                };
                if off + 64 <= src.len() {
                    resp.copy_from_slice(&src[off..off + 64]);
                }
            }
            cmd::SET_MACRO_SIMPLE => {
                let slot = r[1] as usize;
                if slot < s.macros.len() {
                    if r[2] == 0 {
                        s.macros[slot] = [0; 256];
                    }
                    let off = r[2] as usize * 56;
                    let end = (off + r[3] as usize).min(256);
                    if off < end {
                        s.macros[slot][off..end].copy_from_slice(&data[..end - off]);
                    }
                }
            }
            cmd::GET_MACRO => {
                let slot = (r[1] as usize).min(s.macros.len() - 1);
                let off = r[2] as usize * 64;
                if off + 64 <= 256 {
                    resp.copy_from_slice(&s.macros[slot][off..off + 64]);
                }
            }
            cmd::SET_USER_PIC_SIMPLE => {
                let slot = (r[1] as usize).min(PIC_SLOTS - 1);
                let i = r[2] as usize * 3;
                if i + 3 <= PIC_LEN {
                    s.pics[slot][i..i + 3].copy_from_slice(&r[8..11]);
                }
            }
            cmd::SET_USER_PIC => {
                let off = r[4] as usize * 56;
                let end = (off + 56).min(PIC_LEN);
                if off < end {
                    s.pics[0][off..end].copy_from_slice(&data[..end - off]);
                }
            }
            cmd::GET_USER_PIC => {
                let slot = (r[1] as usize).min(PIC_SLOTS - 1);
                let off = r[2] as usize * 64;
                if off + 64 <= PIC_LEN {
                    resp.copy_from_slice(&s.pics[slot][off..off + 64]);
                }
            }
            _ => {}
        }
        drop(s);
        self.pending = resp;
        Ok(())
    }
}

impl Transport for Emulator {
    fn send(&mut self, report: &Report) -> Result<(), Error> {
        self.handle(report)
    }

    fn receive(&mut self) -> Result<Report, Error> {
        Ok(self.pending)
    }

    fn describe(&self) -> String {
        format!("emulated {}", self.model.name)
    }

    fn needs_pacing(&self) -> bool {
        false
    }
}
