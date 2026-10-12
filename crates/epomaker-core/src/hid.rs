//! USB HID transport (hidapi). Works on Linux (hidraw) and Windows.

use crate::devices::Database;
use crate::protocol::{Report, REPORT_LEN};
use crate::transport::Transport;
use crate::Error;
use hidapi::{HidApi, HidDevice};

/// A vendor interface found during enumeration.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HidCandidate {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface: i32,
    pub product: String,
    pub manufacturer: String,
}

/// Interface number the vendor driver opens when usage information is not available.
const VENDOR_INTERFACE: i32 = 2;

/// Lists every HID interface that looks like the configuration endpoint of a supported keyboard.
pub fn enumerate() -> Result<Vec<HidCandidate>, Error> {
    let api = HidApi::new().map_err(hid_err)?;
    Ok(scan(&api))
}

/// Every HID interface of a vendor, supported or not (for diagnostics).
pub fn enumerate_vendor(vid: u16) -> Result<Vec<HidCandidate>, Error> {
    let api = HidApi::new().map_err(hid_err)?;
    Ok(api
        .device_list()
        .filter(|i| i.vendor_id() == vid)
        .map(|info| HidCandidate {
            path: info.path().to_string_lossy().into_owned(),
            vid: info.vendor_id(),
            pid: info.product_id(),
            usage_page: info.usage_page(),
            usage: info.usage(),
            interface: info.interface_number(),
            product: info.product_string().unwrap_or_default().to_string(),
            manufacturer: info.manufacturer_string().unwrap_or_default().to_string(),
        })
        .collect())
}

fn scan(api: &HidApi) -> Vec<HidCandidate> {
    let ids = Database::get().usb_ids();
    let mut out: Vec<HidCandidate> = Vec::new();
    for info in api.device_list() {
        let matches = ids.iter().any(|(vid, pid, page, usage)| {
            info.vendor_id() == *vid
                && info.product_id() == *pid
                && ((info.usage_page() == *page && info.usage() == *usage)
                    // some hidraw builds report no usage: fall back to the interface number
                    || (info.usage_page() == 0 && info.interface_number() == VENDOR_INTERFACE))
        });
        if !matches {
            continue;
        }
        let path = info.path().to_string_lossy().into_owned();
        if out.iter().any(|c| c.path == path) {
            continue;
        }
        out.push(HidCandidate {
            path,
            vid: info.vendor_id(),
            pid: info.product_id(),
            usage_page: info.usage_page(),
            usage: info.usage(),
            interface: info.interface_number(),
            product: info.product_string().unwrap_or_default().to_string(),
            manufacturer: info.manufacturer_string().unwrap_or_default().to_string(),
        });
    }
    out
}

pub struct HidTransport {
    device: HidDevice,
    candidate: HidCandidate,
}

impl HidTransport {
    pub fn open(candidate: &HidCandidate) -> Result<HidTransport, Error> {
        let api = HidApi::new().map_err(hid_err)?;
        let path = std::ffi::CString::new(candidate.path.clone()).map_err(|_| Error::Hid("bad device path".into()))?;
        let device = api.open_path(&path).map_err(|e| {
            let mut msg = e.to_string();
            if cfg!(target_os = "linux") {
                msg.push_str(" (permission denied? install the udev rule: packaging/linux/70-epomaker.rules)");
            }
            Error::Hid(msg)
        })?;
        Ok(HidTransport { device, candidate: candidate.clone() })
    }

    pub fn candidate(&self) -> &HidCandidate {
        &self.candidate
    }
}

impl Transport for HidTransport {
    fn send(&mut self, report: &Report) -> Result<(), Error> {
        // report id 0 + 64 bytes, exactly like HidD_SetFeature with a 65-byte buffer
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..].copy_from_slice(report);
        self.device.send_feature_report(&buf).map_err(hid_err)
    }

    fn receive(&mut self) -> Result<Report, Error> {
        let mut buf = [0u8; REPORT_LEN + 1];
        let n = self.device.get_feature_report(&mut buf).map_err(hid_err)?;
        if n == 0 {
            return Err(Error::Hid("empty feature report".into()));
        }
        // hidapi keeps the report id in buf[0] on every platform
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&buf[1..]);
        Ok(r)
    }

    fn reopen(&mut self) -> Result<(), Error> {
        let fresh = HidTransport::open(&self.candidate)?;
        self.device = fresh.device;
        Ok(())
    }

    fn describe(&self) -> String {
        format!(
            "{:04x}:{:04x} if{} {}",
            self.candidate.vid, self.candidate.pid, self.candidate.interface, self.candidate.path
        )
    }
}

fn hid_err(e: hidapi::HidError) -> Error {
    Error::Hid(e.to_string())
}

/// The keyboard's own key reports, read from its boot-protocol interface. Shows keys a
/// windowing toolkit does not report (Caps Lock, Scroll Lock…). Works on Linux with the
/// udev rule installed; Windows does not let applications open keyboards.
pub struct KeyReports {
    device: HidDevice,
    down: std::collections::BTreeSet<u8>,
}

impl KeyReports {
    pub fn open(vid: u16, pid: u16) -> Result<KeyReports, Error> {
        let api = HidApi::new().map_err(hid_err)?;
        let ours = |i: &&hidapi::DeviceInfo| i.vendor_id() == vid && i.product_id() == pid;
        let info = api
            .device_list()
            .filter(ours)
            .find(|i| i.interface_number() == 0 && (i.usage_page(), i.usage()) == (1, 6))
            .or_else(|| api.device_list().filter(ours).find(|i| i.interface_number() == 0))
            .ok_or(Error::NotFound)?;
        let device = info.open_device(&api).map_err(hid_err)?;
        device.set_blocking_mode(false).map_err(hid_err)?;
        Ok(KeyReports { device, down: Default::default() })
    }

    /// Key presses and releases (HID usage, pressed) since the last call.
    pub fn poll(&mut self) -> Result<Vec<(u8, bool)>, Error> {
        let mut out = vec![];
        let mut buf = [0u8; 64];
        loop {
            let n = self.device.read(&mut buf).map_err(hid_err)?;
            if n == 0 {
                break;
            }
            if n == 8 {
                let now = boot_report_keys(&buf[..8]);
                out.extend(now.difference(&self.down).map(|k| (*k, true)));
                out.extend(self.down.difference(&now).map(|k| (*k, false)));
                self.down = now;
            }
        }
        Ok(out)
    }
}

/// Keys held in a boot-protocol keyboard report: modifier bits, then up to 6 usages.
pub fn boot_report_keys(r: &[u8]) -> std::collections::BTreeSet<u8> {
    let mut keys: std::collections::BTreeSet<u8> = (0..8).filter(|b| r[0] & (1 << b) != 0).map(|b| 0xE0 + b).collect();
    // usages 1..=3 are error codes (rollover), not keys
    if !r[2..8].contains(&1) {
        keys.extend(r[2..8].iter().copied().filter(|k| *k > 3));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_reports() {
        let k = boot_report_keys(&[0x02, 0, 0x39, 0x04, 0, 0, 0, 0]);
        assert_eq!(k.into_iter().collect::<Vec<_>>(), vec![0x04, 0x39, 0xE1]);
        assert!(boot_report_keys(&[0, 0, 1, 1, 1, 1, 1, 1]).is_empty());
    }
}
