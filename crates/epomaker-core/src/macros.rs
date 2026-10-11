//! Macro storage format.
//!
//! A macro occupies a 256-byte slot: `u16 LE repeat count` followed by a stream of
//! events. A key/button event is `code, attr` where bit 7 of `attr` is "pressed" and
//! bits 0..7 hold a short delay (1..=127 ms) to wait afterwards. When the delay does
//! not fit, `attr & 0x7f == 0` and the delay follows as a `u16 LE`. Mouse moves are
//! `0xF9, attr, dx, dy`.
//!
//! Writes use `SET_MACRO_SIMPLE` (0x16) in 56-byte chunks; reads use `GET_MACRO` (0x8B)
//! in 4 × 64-byte chunks.

use crate::keymap::MouseAction;
use crate::protocol::{cmd, report_from, Checksum, Frame};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MACRO_SLOT_LEN: usize = 256;
/// Number of macro slots available on YC500 keyboards.
pub const MACRO_SLOTS: u8 = 50;
const MOUSE_MOVE: u8 = 0xF9;
/// Bytes of macro data carried by one write packet.
pub const WRITE_CHUNK: usize = 56;
const WRITE_MAX_CHUNKS: usize = 5;
const READ_CHUNKS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroEvent {
    KeyDown(u8),
    KeyUp(u8),
    MouseDown(MouseAction),
    MouseUp(MouseAction),
    MouseMove {
        dx: i8,
        dy: i8,
    },
    /// Pause in milliseconds.
    Delay(u16),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub repeat_count: u16,
    pub events: Vec<MacroEvent>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MacroError {
    #[error("macro does not fit in the {MACRO_SLOT_LEN}-byte slot")]
    TooLong,
    #[error("macro slot {0} out of range")]
    BadSlot(u8),
}

/// A macro saved to a file, independent of any keyboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroFile {
    pub format: u32,
    pub name: String,
    #[serde(flatten)]
    pub r#macro: Macro,
}

impl MacroFile {
    pub fn new(name: impl Into<String>, m: Macro) -> Self {
        MacroFile { format: 1, name: name.into(), r#macro: m }
    }

    pub fn load(path: &std::path::Path) -> Result<MacroFile, crate::Error> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), crate::Error> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

impl Macro {
    /// Serializes to the 256-byte slot layout (vendor `setMacro`).
    pub fn encode(&self) -> Result<[u8; MACRO_SLOT_LEN], MacroError> {
        let mut w = Writer { buf: [0; MACRO_SLOT_LEN], pos: 0 };
        w.u16(self.repeat_count)?;
        let ev = &self.events;
        // true when the previous key event still owes its 2-byte delay field
        let mut pending = false;
        let mut i = 0;
        while i < ev.len() {
            let e = ev[i];
            if !matches!(e, MacroEvent::Delay(_)) && pending {
                w.u16(0)?;
            }
            match e {
                MacroEvent::Delay(ms) => {
                    if !pending {
                        w.u16(0)?;
                    }
                    w.u16(ms)?;
                    pending = false;
                }
                MacroEvent::KeyDown(code) | MacroEvent::KeyUp(code) => {
                    w.u8(code)?;
                    pending = attr(&mut w, ev, &mut i, Some(matches!(e, MacroEvent::KeyDown(_))))?;
                }
                MacroEvent::MouseDown(m) | MacroEvent::MouseUp(m) => {
                    w.u8(m.macro_code())?;
                    pending = attr(&mut w, ev, &mut i, Some(matches!(e, MacroEvent::MouseDown(_))))?;
                }
                MacroEvent::MouseMove { dx, dy } => {
                    w.u8(MOUSE_MOVE)?;
                    pending = attr(&mut w, ev, &mut i, None)?;
                    w.u8(dx as u8)?;
                    w.u8(dy as u8)?;
                }
            }
            i += 1;
        }
        Ok(w.buf)
    }

    /// Parses a 256-byte slot (vendor `buffToMacroEvents`).
    pub fn decode(buf: &[u8]) -> Macro {
        let mut events = Vec::new();
        if buf.len() < 6 {
            return Macro::default();
        }
        let repeat_count = u16::from_le_bytes([buf[0], buf[1]]);
        let mut n = 2usize;
        let mut a = [buf[2], buf[3], buf[4], buf[5]];
        n += 4;
        while n < buf.len() {
            if a[0] == MOUSE_MOVE {
                events.push(MacroEvent::MouseMove { dx: a[2] as i8, dy: a[3] as i8 });
                let delay = if a[1] != 0 {
                    (a[1] >> 1) as u16
                } else {
                    let d = u16::from_le_bytes([byte(buf, n), byte(buf, n + 1)]);
                    n += 2;
                    d
                };
                events.push(MacroEvent::Delay(delay));
            } else {
                let down = a[1] & 0x80 != 0;
                if (4..=239).contains(&a[0]) {
                    events.push(if down { MacroEvent::KeyDown(a[0]) } else { MacroEvent::KeyUp(a[0]) });
                } else if let Some(m) = MouseAction::from_bytes([1, 0, a[0], 0]) {
                    events.push(if down { MacroEvent::MouseDown(m) } else { MacroEvent::MouseUp(m) });
                }
                let delay = if a[1] & 0x7f != 0 {
                    n -= 2;
                    (a[1] & 0x7f) as u16
                } else {
                    u16::from_le_bytes([a[2], a[3]])
                };
                events.push(MacroEvent::Delay(delay));
            }
            a = [byte(buf, n), byte(buf, n + 1), byte(buf, n + 2), byte(buf, n + 3)];
            if a == [0, 0, 0, 0] {
                break;
            }
            n += 4;
        }
        events.retain(|e| *e != MacroEvent::Delay(0));
        Macro { repeat_count, events }
    }
}

fn byte(buf: &[u8], i: usize) -> u8 {
    buf.get(i).copied().unwrap_or(0)
}

/// Writes the attribute byte of a key/button/move event. A following short delay is
/// folded into it (and consumed). Returns whether a 2-byte delay field is still owed.
fn attr(w: &mut Writer, ev: &[MacroEvent], i: &mut usize, down: Option<bool>) -> Result<bool, MacroError> {
    let mut a: u8 = match ev.get(*i + 1) {
        Some(MacroEvent::Delay(ms)) if *ms <= 127 => *ms as u8,
        _ => 0,
    };
    if let Some(down) = down {
        a = if down { a | 0x80 } else { a & 0x7f };
    }
    w.u8(a)?;
    if a & 0x7f > 0 {
        *i += 1;
        Ok(false)
    } else {
        Ok(true)
    }
}

struct Writer {
    buf: [u8; MACRO_SLOT_LEN],
    pos: usize,
}

impl Writer {
    fn u8(&mut self, v: u8) -> Result<(), MacroError> {
        *self.buf.get_mut(self.pos).ok_or(MacroError::TooLong)? = v;
        self.pos += 1;
        Ok(())
    }
    fn u16(&mut self, v: u16) -> Result<(), MacroError> {
        let [lo, hi] = v.to_le_bytes();
        self.u8(lo)?;
        self.u8(hi)
    }
}

/// Frames that store `data` into macro `slot` (vendor `_setMacro` for YC500).
/// Only chunks containing non-zero bytes are sent; the last one is flagged.
pub fn encode_write(slot: u8, data: &[u8; MACRO_SLOT_LEN]) -> Result<Vec<Frame>, MacroError> {
    if slot >= MACRO_SLOTS {
        return Err(MacroError::BadSlot(slot));
    }
    let chunk = |t: usize| -> Vec<u8> {
        let start = (t * WRITE_CHUNK).min(data.len());
        let end = (start + WRITE_CHUNK).min(data.len());
        data[start..end].to_vec()
    };
    let count = (0..WRITE_MAX_CHUNKS).filter(|t| chunk(*t).iter().any(|b| *b != 0)).count();
    Ok((0..count)
        .map(|t| {
            let mut r =
                report_from(&[cmd::SET_MACRO_SIMPLE, slot, t as u8, WRITE_CHUNK as u8, u8::from(t == count - 1)]);
            let c = chunk(t);
            r[8..8 + c.len()].copy_from_slice(&c);
            Frame::new(r, Checksum::Bit7)
        })
        .collect())
}

/// A single zeroed first chunk flagged as the last one: empties `slot`.
/// ([`encode_write`] sends nothing for an all-zero slot, like the vendor app.)
pub fn encode_clear(slot: u8) -> Result<Frame, MacroError> {
    if slot >= MACRO_SLOTS {
        return Err(MacroError::BadSlot(slot));
    }
    Ok(Frame::new(report_from(&[cmd::SET_MACRO_SIMPLE, slot, 0, WRITE_CHUNK as u8, 1]), Checksum::Bit7))
}

/// Queries for reading macro `slot`; concatenate the 4 × 64-byte responses.
pub fn encode_read(slot: u8) -> Vec<Frame> {
    (0..READ_CHUNKS).map(|t| crate::protocol::query(cmd::GET_MACRO, slot, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_simple() {
        let m = Macro {
            repeat_count: 1,
            events: vec![
                MacroEvent::KeyDown(4),
                MacroEvent::Delay(50),
                MacroEvent::KeyUp(4),
                MacroEvent::Delay(200),
                MacroEvent::KeyDown(5),
                MacroEvent::KeyUp(5),
            ],
        };
        let buf = m.encode().unwrap();
        assert_eq!(&buf[..12], &[1, 0, 4, 0xb2, 4, 0, 0xc8, 0, 5, 0x80, 0, 0]);
        let d = Macro::decode(&buf);
        assert_eq!(d.repeat_count, 1);
        assert_eq!(d.events[..4], m.events[..4]);
    }

    #[test]
    fn file_roundtrip() {
        let m = Macro {
            repeat_count: 2,
            events: vec![MacroEvent::KeyDown(4), MacroEvent::Delay(300), MacroEvent::KeyUp(4)],
        };
        let f = MacroFile::new("hello", m.clone());
        let path = std::env::temp_dir().join(format!("epomaker-macro-{}.json", std::process::id()));
        f.save(&path).unwrap();
        let back = MacroFile::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(back.r#macro, m);
        assert_eq!(back.name, "hello");
    }

    #[test]
    fn too_long_is_rejected() {
        let mut m = Macro { repeat_count: 1, events: vec![] };
        for _ in 0..200 {
            m.events.push(MacroEvent::KeyDown(4));
            m.events.push(MacroEvent::Delay(1000));
        }
        assert_eq!(m.encode(), Err(MacroError::TooLong));
    }
}
