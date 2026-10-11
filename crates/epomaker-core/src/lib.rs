//! Open-source configuration library for EPOMAKER keyboards built on the RongYuan
//! "YC500" firmware (TH80, TH80 Pro, TH80 SE, TH80 X…).
//!
//! The protocol was reverse engineered from the official Windows driver for
//! interoperability; see `docs/PROTOCOL.md`.
//!
//! ```no_run
//! use epomaker_core::{Keyboard, lighting::{LightEffect, LightMode, Rgb}};
//! let mut kb = Keyboard::open_first()?;
//! kb.set_light(&LightEffect { mode: LightMode::Breath, color: Rgb(0x00ff88), ..Default::default() })?;
//! # Ok::<(), epomaker_core::Error>(())
//! ```

pub mod devices;
pub mod emulator;
#[cfg(feature = "hid")]
pub mod hid;
pub mod keyboard;
pub mod keycodes;
pub mod keymap;
pub mod lighting;
pub mod macros;
pub mod profile_file;
pub mod protocol;
pub mod settings;
pub mod transport;

pub use keyboard::Keyboard;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(String),
    #[error("no supported keyboard found")]
    NotFound,
    #[error("keyboard model id {0} is not in the device database")]
    UnknownDevice(i64),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error(transparent)]
    Macro(#[from] macros::MacroError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
