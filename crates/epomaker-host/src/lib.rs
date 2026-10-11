//! Host-side integrations used by the CLI, the GUI and the tray agent: system audio
//! capture, screen color sampling, foreground application detection, and the loops
//! that stream the "music follow" / "screen color" effects to a keyboard.

pub mod agent_config;
pub mod audio;
pub mod autostart;
pub mod cpu;
pub mod screen;
#[cfg(target_os = "linux")]
pub mod screen_portal;
pub mod stream;
pub mod window;

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("audio capture: {0}")]
    Audio(String),
    #[error("screen capture: {0}")]
    Screen(String),
    #[error("active window: {0}")]
    Window(String),
    #[error("not supported on this system: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Keyboard(#[from] epomaker_core::Error),
}
