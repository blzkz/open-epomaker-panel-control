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
