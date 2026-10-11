//! Name of the application in the foreground, for per-application profiles.
//!
//! * Windows: executable file name of the foreground window's process (e.g. `game.exe`).
//! * Linux X11: `WM_CLASS` of `_NET_ACTIVE_WINDOW` (e.g. `firefox`).
//! * Linux Wayland: not available in general (no standard protocol exposes it).

use crate::HostError;

pub trait ActiveWindow: Send {
    /// Lower-case application identifier, `None` if nothing is focused.
    fn current(&mut self) -> Result<Option<String>, HostError>;
}

pub fn tracker() -> Result<Box<dyn ActiveWindow>, HostError> {
    imp::tracker()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    use x11rb::rust_connection::RustConnection;

    struct X11 {
        conn: RustConnection,
        root: u32,
        active: u32,
    }

    pub fn tracker() -> Result<Box<dyn ActiveWindow>, HostError> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("EPOMAKER_FORCE_X11_CAPTURE").is_none() {
            return Err(HostError::Unsupported("active window detection on Wayland (use an X11 session)"));
        }
        let err = |e: &dyn std::fmt::Display| HostError::Window(e.to_string());
        let (conn, screen) = x11rb::connect(None).map_err(|e| err(&e))?;
        let root = conn.setup().roots[screen].root;
        let active =
            conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").map_err(|e| err(&e))?.reply().map_err(|e| err(&e))?.atom;
        Ok(Box::new(X11 { conn, root, active }))
    }

    impl ActiveWindow for X11 {
        fn current(&mut self) -> Result<Option<String>, HostError> {
            let err = |e: &dyn std::fmt::Display| HostError::Window(e.to_string());
            let reply = self
                .conn
                .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)
                .map_err(|e| err(&e))?
                .reply()
                .map_err(|e| err(&e))?;
            let Some(win) = reply.value32().and_then(|mut v| v.next()).filter(|w| *w != 0) else {
                return Ok(None);
            };
            let class = self
                .conn
                .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
                .map_err(|e| err(&e))?
                .reply()
                .map_err(|e| err(&e))?;
            // WM_CLASS = instance\0class\0 ; the class is the stable application name
            let parts: Vec<&[u8]> = class.value.split(|b| *b == 0).filter(|p| !p.is_empty()).collect();
            Ok(parts.last().map(|p| String::from_utf8_lossy(p).to_lowercase()))
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    struct Win;

    pub fn tracker() -> Result<Box<dyn ActiveWindow>, HostError> {
        Ok(Box::new(Win))
    }

    impl ActiveWindow for Win {
        fn current(&mut self) -> Result<Option<String>, HostError> {
            // SAFETY: standard Win32 calls; the process handle is closed before returning.
            unsafe {
                let hwnd = GetForegroundWindow();
                if hwnd.is_null() {
                    return Ok(None);
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if process.is_null() {
                    return Ok(None);
                }
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
                CloseHandle(process);
                if ok == 0 {
                    return Ok(None);
                }
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                Ok(path.rsplit(['\\', '/']).next().map(|s| s.to_lowercase()))
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;
    pub fn tracker() -> Result<Box<dyn ActiveWindow>, HostError> {
        Err(HostError::Unsupported("active window detection"))
    }
}
