//! Start the agent when the user logs in.
//!
//! Linux: an XDG autostart entry in `~/.config/autostart`. Windows: the per-user `Run` key.

/// Command that starts this agent (the AppImage itself when running from one).
fn command() -> String {
    std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "epomaker-agent".into())
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;

    fn entry() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("autostart").join("open-epomaker-agent.desktop")
    }

    pub fn is_enabled() -> bool {
        entry().exists()
    }

    pub fn set(on: bool) -> std::io::Result<()> {
        let path = entry();
        if !on {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        let exec = super::command();
        let quoted = if exec.contains(' ') { format!("\"{exec}\"") } else { exec };
        std::fs::write(
            path,
            format!(
                "[Desktop Entry]\nType=Application\nName=Open Epomaker Agent\nComment=Keyboard profiles and effects\nExec={quoted}\nIcon=open-epomaker-panel\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
            ),
        )
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "OpenEpomakerAgent";

    pub fn is_enabled() -> bool {
        let (key, value) = (wide(KEY), wide(VALUE));
        let mut size = 0u32;
        // SAFETY: querying the size of a registry value with valid NUL-terminated strings.
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            ) == ERROR_SUCCESS
        }
    }

    pub fn set(on: bool) -> std::io::Result<()> {
        let (key, value) = (wide(KEY), wide(VALUE));
        // SAFETY: registry calls with valid NUL-terminated UTF-16 strings and sizes.
        let status = unsafe {
            if on {
                let data = wide(&format!("\"{}\"", super::command()));
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            } else {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr())
            }
        };
        if status == ERROR_SUCCESS || !on {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(status as i32))
        }
    }
}

pub use imp::{is_enabled, set};
