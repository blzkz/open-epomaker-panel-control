//! Start the tray agent when the user logs in.
//!
//! Linux: an XDG autostart entry in `~/.config/autostart`. Windows: the per-user `Run` key.

/// Command line that starts the agent, found next to the running program (or inside the
/// AppImage it runs from).
pub fn agent_command() -> String {
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        return format!("\"{appimage}\" agent");
    }
    let exe = std::env::current_exe().ok();
    let name = if cfg!(windows) { "epomaker-agent.exe" } else { "epomaker-agent" };
    let path = match exe {
        Some(e) if e.file_stem().is_some_and(|s| s == "epomaker-agent") => e,
        Some(e) => e.parent().map(|d| d.join(name)).filter(|p| p.exists()).unwrap_or_else(|| name.into()),
        None => name.into(),
    };
    let p = path.to_string_lossy();
    if p.contains(' ') {
        format!("\"{p}\"")
    } else {
        p.into_owned()
    }
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
        let quoted = super::agent_command();
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
                let data = wide(&super::agent_command());
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
