//! Read-only diagnostics: a fixed list of `GET` queries whose raw answers are printed
//! so that people with untested hardware can share them (`epomaker probe`, the
//! self-test report). Nothing here writes to the keyboard.

use crate::protocol::{cmd, query, Report};
use crate::transport::{hex, Transport};
use crate::{Error, Keyboard};

/// USB vendor id used by the EPOMAKER / RongYuan keyboards.
pub const VENDOR_ID: u16 = 0x3151;

/// `(name, command, arg1, arg2)` of every probe query. All of them are `GET` commands.
pub const QUERIES: [(&str, u8, u8, u8); 10] = [
    ("device id", cmd::GET_DEVICE_ID, 0, 0),
    ("firmware", cmd::GET_FIRMWARE, 0, 0),
    ("battery", cmd::GET_BATTERY, 0, 0),
    ("profile", cmd::GET_PROFILE, 0, 0),
    ("lighting", cmd::GET_LED, 0, 0),
    ("report rate p0", cmd::GET_REPORT_RATE, 0, 0),
    ("options p0", cmd::GET_KB_OPTION, 0, 0),
    ("debounce", cmd::GET_DEBOUNCE, 0, 0),
    ("sleep", cmd::GET_SLEEP_TIME, 0, 0),
    ("key matrix p0 #0", cmd::GET_KEY_MATRIX, 0, 0),
];

fn line(name: &str, r: Result<Report, Error>) -> String {
    match r {
        Ok(resp) => format!("   {name:<18} -> {}", hex(&resp)),
        Err(e) => format!("   {name:<18} !! {e}"),
    }
}

/// Runs the probe queries over a keyboard that is already connected.
pub fn query_keyboard(kb: &mut Keyboard) -> Vec<String> {
    QUERIES.iter().map(|(name, command, a1, a2)| line(name, kb.raw_query(*command, *a1, *a2))).collect()
}

/// Runs the probe queries over a bare transport (no identification needed).
pub fn query_transport(t: &mut dyn Transport) -> Vec<String> {
    QUERIES
        .iter()
        .map(|(name, command, a1, a2)| {
            let f = query(*command, *a1, *a2);
            std::thread::sleep(std::time::Duration::from_millis(20));
            let r = t.send(&f.wire_bytes()).and_then(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                t.receive()
            });
            line(name, r)
        })
        .collect()
}

/// One line per HID interface of [`VENDOR_ID`], supported or not.
#[cfg(feature = "hid")]
pub fn vendor_interfaces() -> Result<Vec<String>, Error> {
    Ok(crate::hid::enumerate_vendor(VENDOR_ID)?.iter().map(|c| format!("   {}", describe_candidate(c))).collect())
}

#[cfg(feature = "hid")]
pub fn describe_candidate(c: &crate::hid::HidCandidate) -> String {
    format!(
        "{:04x}:{:04x} usage {:04x}/{:04x} if{} \"{} {}\" {}",
        c.vid, c.pid, c.usage_page, c.usage, c.interface, c.manufacturer, c.product, c.path
    )
}

/// The whole `epomaker probe` routine: lists the vendor's interfaces, then opens every
/// configuration interface of a known model and sends the read-only queries.
/// Output goes to `out` line by line (so a hanging query still shows progress).
#[cfg(feature = "hid")]
pub fn run(mut out: impl FnMut(String)) -> Result<(), Error> {
    out(format!("All interfaces of vendor {VENDOR_ID:#06x}:"));
    vendor_interfaces()?.into_iter().for_each(&mut out);
    let list = crate::hid::enumerate()?;
    if list.is_empty() {
        out("No configuration interface of a known model found (usage page 0xFFFF / interface 2).".into());
    }
    for c in list {
        out(format!("== {}", describe_candidate(&c)));
        match crate::hid::HidTransport::open(&c) {
            Ok(mut t) => query_transport(&mut t).into_iter().for_each(&mut out),
            Err(e) => out(format!("   cannot open: {e}")),
        }
    }
    Ok(())
}
