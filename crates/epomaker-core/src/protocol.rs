//! Wire format of the RongYuan "YC500" configuration protocol used by EPOMAKER keyboards.
//!
//! Every command is a 64-byte HID *feature report* (report id 0) sent to the vendor
//! interface (usage page `0xFFFF`, usage `2`). Responses are read back with a
//! `GET_FEATURE` on the same interface. See `docs/PROTOCOL.md` for the full description.

/// Payload length of every feature report (the report id byte is not included).
pub const REPORT_LEN: usize = 64;

/// One 64-byte feature report payload.
pub type Report = [u8; REPORT_LEN];

/// Checksum variants applied by the vendor's native driver before sending a report.
///
/// Verified against the disassembly of `iot_driver.exe` (function at `0x4a5340`):
/// the checksum is the bitwise NOT of the byte-sum of the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Checksum {
    /// `report[7] = !sum(report[0..7])` (driver checksum type 0).
    Bit7,
    /// `report[8] = !sum(report[0..8])` (driver checksum type 1).
    Bit8,
    /// Sent as-is (driver checksum type 2).
    None,
}

impl Checksum {
    /// Numeric id used by the vendor driver API.
    pub fn driver_id(self) -> u8 {
        match self {
            Checksum::Bit7 => 0,
            Checksum::Bit8 => 1,
            Checksum::None => 2,
        }
    }

    pub fn apply(self, report: &mut Report) {
        match self {
            Checksum::Bit7 => report[7] = !byte_sum(&report[..7]),
            Checksum::Bit8 => report[8] = !byte_sum(&report[..8]),
            Checksum::None => {}
        }
    }
}

/// Wrapping sum of bytes.
pub fn byte_sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Builds a report from a header/payload prefix, zero-padded to 64 bytes.
pub fn report_from(prefix: &[u8]) -> Report {
    let mut r = [0u8; REPORT_LEN];
    let n = prefix.len().min(REPORT_LEN);
    r[..n].copy_from_slice(&prefix[..n]);
    r
}

/// A report ready to be written, with the checksum the driver must apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub data: Report,
    pub checksum: Checksum,
}

impl Frame {
    pub fn new(data: Report, checksum: Checksum) -> Self {
        Self { data, checksum }
    }

    /// The exact bytes that go on the wire (checksum applied).
    pub fn wire_bytes(&self) -> Report {
        let mut r = self.data;
        self.checksum.apply(&mut r);
        r
    }
}

/// Command identifiers (first byte of every report) for the YC500 family.
/// `GET_*` commands are the matching `SET_*` with the high bit set.
pub mod cmd {
    pub const GET_FIRMWARE: u8 = 0x80;
    pub const GET_DEVICE_ID: u8 = 0x8F;
    pub const RESET: u8 = 0x02;
    pub const GET_BATTERY: u8 = 0x83;
    pub const SET_REPORT_RATE: u8 = 0x04;
    pub const GET_REPORT_RATE: u8 = 0x84;
    pub const SET_PROFILE: u8 = 0x05;
    pub const GET_PROFILE: u8 = 0x85;
    pub const SET_KB_OPTION: u8 = 0x06;
    pub const GET_KB_OPTION: u8 = 0x86;
    pub const SET_LED: u8 = 0x07;
    pub const GET_LED: u8 = 0x87;
    pub const SET_KEY_MATRIX: u8 = 0x09;
    pub const GET_KEY_MATRIX: u8 = 0x89;
    pub const GET_MACRO: u8 = 0x8B;
    pub const SET_USER_PIC: u8 = 0x0C;
    pub const GET_USER_PIC: u8 = 0x8C;
    pub const SET_FN_MATRIX: u8 = 0x10;
    pub const GET_FN_MATRIX: u8 = 0x90;
    pub const SET_DEBOUNCE: u8 = 0x11;
    pub const GET_DEBOUNCE: u8 = 0x91;
    pub const SET_SLEEP_TIME: u8 = 0x12;
    pub const GET_SLEEP_TIME: u8 = 0x92;
    pub const SET_KEY_SIMPLE: u8 = 0x13;
    pub const SET_USER_PIC_SIMPLE: u8 = 0x14;
    pub const SET_FN_SIMPLE: u8 = 0x15;
    pub const SET_MACRO_SIMPLE: u8 = 0x16;
    pub const SET_AUTO_OS: u8 = 0x17;
    pub const GET_AUTO_OS: u8 = 0x97;
}

/// A query that is answered by a single feature report read.
pub fn query(command: u8, arg1: u8, arg2: u8) -> Frame {
    Frame::new(report_from(&[command, arg1, arg2]), Checksum::Bit7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit7_checksum_matches_driver() {
        // report rate 1000 Hz on profile 0, captured from the vendor app
        let f = Frame::new(report_from(&[0x04, 0x00, 0x01]), Checksum::Bit7);
        assert_eq!(f.wire_bytes()[7], 0xfa);
    }

    #[test]
    fn bit8_checksum_matches_driver() {
        let f = Frame::new(report_from(&[0x07, 0x04, 0x02, 0x03, 0x07, 0x00, 0xff, 0x00]), Checksum::Bit8);
        assert_eq!(f.wire_bytes()[8], 0xe9);
    }
}
