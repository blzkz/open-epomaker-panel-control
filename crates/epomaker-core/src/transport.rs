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
    /// Closes and opens the device again (after a transfer error). Optional.
    fn reopen(&mut self) -> Result<(), Error> {
        Ok(())
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
    fn reopen(&mut self) -> Result<(), Error> {
        (**self).reopen()
    }
}

/// Timing used by the vendor app between reports. The firmware drops reports that
/// arrive too quickly, so we keep the same pacing on real hardware.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    pub before_write: Duration,
    pub before_read: Duration,
    /// Extra wait before a lighting write (`writeFeatureCmd(buf, 1, 500)` in the vendor app).
    pub before_light: Duration,
    pub after_light: Duration,
    pub after_big_command: Duration,
    /// Waits before each new attempt when a transfer fails. The firmware stalls USB
    /// requests for a moment while it saves settings (Windows reports
    /// `ERROR_GEN_FAILURE`, os error 31), so failed transfers are tried again.
    pub retry_backoff: &'static [Duration],
}

/// About 4 s in total, enough for the firmware to finish saving to flash.
const BACKOFF: [Duration; 4] =
    [Duration::from_millis(150), Duration::from_millis(400), Duration::from_millis(1000), Duration::from_millis(2500)];

impl Default for Pacing {
    fn default() -> Self {
        Pacing {
            before_write: Duration::from_millis(10),
            before_read: Duration::from_millis(5),
            before_light: Duration::from_millis(500),
            after_light: Duration::from_millis(500),
            after_big_command: Duration::from_millis(1000),
            retry_backoff: &BACKOFF,
        }
        .scaled(scale_from_env())
    }
}

impl Pacing {
    pub fn none() -> Self {
        Pacing {
            before_write: Duration::ZERO,
            before_read: Duration::ZERO,
            before_light: Duration::ZERO,
            after_light: Duration::ZERO,
            after_big_command: Duration::ZERO,
            retry_backoff: &[],
        }
    }

    /// Every delay (not the retry waits) multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        let f = |d: Duration| d.mul_f32(factor.clamp(0.1, 20.0));
        Pacing {
            before_write: f(self.before_write),
            before_read: f(self.before_read),
            before_light: f(self.before_light),
            after_light: f(self.after_light),
            after_big_command: f(self.after_big_command),
            retry_backoff: self.retry_backoff,
        }
    }
}

/// `EPOMAKER_PACING=2` doubles every delay between reports (troubleshooting aid).
fn scale_from_env() -> f32 {
    std::env::var("EPOMAKER_PACING").ok().and_then(|v| v.trim().parse().ok()).filter(|f: &f32| *f > 0.0).unwrap_or(1.0)
}

/// Helper used by [`crate::Keyboard`]: frames + pacing over any transport.
pub(crate) struct Link<T: Transport> {
    pub transport: T,
    pub pacing: Pacing,
    /// Every frame written, for diagnostics (`--trace`).
    pub trace: bool,
}

impl<T: Transport> Link<T> {
    /// Runs `op`, trying again after each [`Pacing::retry_backoff`] wait when the
    /// transfer itself fails. From the second retry on the device is also reopened.
    fn retrying<R>(&mut self, what: &str, mut op: impl FnMut(&mut Self) -> Result<R, Error>) -> Result<R, Error> {
        let mut attempt = 0;
        loop {
            match op(self) {
                Err(Error::Hid(e)) if attempt < self.pacing.retry_backoff.len() => {
                    let wait = self.pacing.retry_backoff[attempt];
                    attempt += 1;
                    log::warn!("{what} failed ({e}); attempt {} in {} ms", attempt + 1, wait.as_millis());
                    sleep(wait);
                    if attempt >= 2 {
                        if let Err(e) = self.transport.reopen() {
                            log::warn!("reopening the device: {e}");
                        }
                    }
                }
                other => return other,
            }
        }
    }

    fn send(&mut self, bytes: &Report) -> Result<(), Error> {
        if self.trace {
            log::info!("-> {}", hex(bytes));
        }
        self.transport.send(bytes)
    }

    pub fn write(&mut self, frame: &Frame) -> Result<(), Error> {
        sleep(self.pacing.before_write);
        let bytes = frame.wire_bytes();
        self.retrying("write", |l| l.send(&bytes))
    }

    /// One attempt only, for frames that are sent continuously (a lost one does not matter).
    pub fn write_once(&mut self, frame: &Frame) -> Result<(), Error> {
        sleep(self.pacing.before_write);
        self.send(&frame.wire_bytes())
    }

    /// A lighting write, with the extra wait the vendor app uses.
    pub fn write_light(&mut self, frame: &Frame) -> Result<(), Error> {
        sleep(self.pacing.before_light);
        self.write(frame)
    }

    fn receive(&mut self) -> Result<Report, Error> {
        sleep(self.pacing.before_read);
        let r = self.transport.receive()?;
        if self.trace {
            log::info!("<- {}", hex(&r));
        }
        Ok(r)
    }

    /// Write then read one response (both again if either fails).
    pub fn query(&mut self, frame: &Frame) -> Result<Report, Error> {
        let bytes = frame.wire_bytes();
        self.retrying("query", |l| {
            sleep(l.pacing.before_write);
            l.send(&bytes)?;
            l.receive()
        })
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
