//! Raw feature-report transport.

use crate::protocol::{Frame, Report};
use crate::Error;
use std::time::Duration;

/// Something that can exchange 64-byte feature reports with a keyboard.
pub trait Transport: Send {
    /// Sends one report (checksum already applied).
    fn send(&mut self, report: &Report) -> Result<(), Error>;
    /// Reads the pending response.
    fn receive(&mut self) -> Result<Report, Error>;
    /// Human readable description (device path…).
    fn describe(&self) -> String;
    /// Whether delays between reports are required (real hardware) or not (emulator).
    fn needs_pacing(&self) -> bool {
        true
    }
}

impl Transport for Box<dyn Transport> {
    fn send(&mut self, report: &Report) -> Result<(), Error> {
        (**self).send(report)
    }
    fn receive(&mut self) -> Result<Report, Error> {
        (**self).receive()
    }
    fn describe(&self) -> String {
        (**self).describe()
    }
    fn needs_pacing(&self) -> bool {
        (**self).needs_pacing()
    }
}

/// Timing used by the vendor app between reports. The firmware drops reports that
/// arrive too quickly, so we keep the same pacing on real hardware.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    pub before_write: Duration,
    pub before_read: Duration,
    pub after_light: Duration,
    pub after_big_command: Duration,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing {
            before_write: Duration::from_millis(10),
            before_read: Duration::from_millis(5),
            after_light: Duration::from_millis(500),
            after_big_command: Duration::from_millis(1000),
        }
    }
}

impl Pacing {
    pub fn none() -> Self {
        Pacing {
            before_write: Duration::ZERO,
            before_read: Duration::ZERO,
            after_light: Duration::ZERO,
            after_big_command: Duration::ZERO,
        }
    }
}

/// Helper used by [`crate::Keyboard`]: frames + pacing over any transport.
pub(crate) struct Link<T: Transport> {
    pub transport: T,
    pub pacing: Pacing,
    /// Every frame written, for diagnostics (`--trace`).
    pub trace: bool,
}

impl<T: Transport> Link<T> {
    pub fn write(&mut self, frame: &Frame) -> Result<(), Error> {
        sleep(self.pacing.before_write);
        let bytes = frame.wire_bytes();
        if self.trace {
            log::info!("-> {}", hex(&bytes));
        }
        self.transport.send(&bytes)
    }

    pub fn read(&mut self) -> Result<Report, Error> {
        sleep(self.pacing.before_read);
        let r = self.transport.receive()?;
        if self.trace {
            log::info!("<- {}", hex(&r));
        }
        Ok(r)
    }

    /// Write then read one response.
    pub fn query(&mut self, frame: &Frame) -> Result<Report, Error> {
        self.write(frame)?;
        self.read()
    }

    pub fn write_all(&mut self, frames: &[Frame]) -> Result<(), Error> {
        frames.iter().try_for_each(|f| self.write(f))
    }

    pub fn query_all(&mut self, frames: &[Frame]) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(frames.len() * 64);
        for f in frames {
            out.extend_from_slice(&self.query(f)?);
        }
        Ok(out)
    }
}

pub(crate) fn sleep(d: Duration) {
    if !d.is_zero() {
        std::thread::sleep(d);
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let trimmed = bytes.iter().rposition(|b| *b != 0).map(|i| &bytes[..=i]).unwrap_or(&[]);
    trimmed.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}
