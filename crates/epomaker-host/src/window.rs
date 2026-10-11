//! Name of the application in the foreground, for per-application profiles.
//!
//! * Windows: executable file name of the foreground window's process (e.g. `game.exe`).
//! * Linux X11: `WM_CLASS` of `_NET_ACTIVE_WINDOW` (e.g. `firefox`).
//! * Linux on KDE Plasma (X11 or Wayland): a small KWin script reports `resourceClass`
//!   over D-Bus.
//! * Other Wayland compositors: not available (no standard protocol exposes it).

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
        let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_uppercase().contains("KDE"));
        if kde {
            match kwin::KWinTracker::start() {
                Ok(t) => return Ok(Box::new(t)),
                Err(e) => log::warn!("KWin active-window script unavailable: {e}"),
            }
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("EPOMAKER_FORCE_X11_CAPTURE").is_none() {
            return Err(HostError::Unsupported(
                "active window detection on this Wayland desktop (KDE Plasma or X11 needed)",
            ));
        }
        let err = |e: &dyn std::fmt::Display| HostError::Window(e.to_string());
        let (conn, screen) = x11rb::connect(None).map_err(|e| err(&e))?;
        let root = conn.setup().roots[screen].root;
        let active =
            conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").map_err(|e| err(&e))?.reply().map_err(|e| err(&e))?.atom;
        Ok(Box::new(X11 { conn, root, active }))
    }

    /// KDE Plasma: KWin runs a tiny script that calls us back on every focus change.
    mod kwin {
        use super::*;
        use std::sync::{Arc, Mutex};

        const IFACE: &str = "io.github.blzkz.OpenEpomaker.ActiveWindow";
        const PLUGIN: &str = "open-epomaker-window-tracker";

        struct Receiver {
            current: Arc<Mutex<Option<String>>>,
        }

        #[zbus::interface(name = "io.github.blzkz.OpenEpomaker.ActiveWindow")]
        impl Receiver {
            fn update(&self, class: String) {
                let class = class.trim().to_lowercase();
                *self.current.lock().unwrap() = if class.is_empty() { None } else { Some(class) };
            }
        }

        pub struct KWinTracker {
            conn: zbus::blocking::Connection,
            plugin: String,
            script: std::path::PathBuf,
            current: Arc<Mutex<Option<String>>>,
        }

        impl KWinTracker {
            pub fn start() -> Result<KWinTracker, HostError> {
                let err = |e: &dyn std::fmt::Display| HostError::Window(e.to_string());
                let pid = std::process::id();
                let service = format!("io.github.blzkz.OpenEpomaker.p{pid}");
                let current = Arc::new(Mutex::new(None));
                let conn = zbus::blocking::connection::Builder::session()
                    .and_then(|b| b.name(service.as_str()))
                    .and_then(|b| b.serve_at("/ActiveWindow", Receiver { current: current.clone() }))
                    .and_then(|b| b.build())
                    .map_err(|e| err(&e))?;
                // a fixed name lets a new tracker replace a script left behind by a crash
                let plugin = PLUGIN.to_string();
                let _ = conn.call_method(
                    Some("org.kde.KWin"),
                    "/Scripting",
                    Some("org.kde.kwin.Scripting"),
                    "unloadScript",
                    &(plugin.as_str(),),
                );
                let dir = std::env::var_os("XDG_RUNTIME_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir);
                let script = dir.join(format!("{plugin}.js"));
                std::fs::write(
                    &script,
                    format!(
                        r#"function report(w) {{
    callDBus("{service}", "/ActiveWindow", "{IFACE}", "Update", w ? String(w.resourceClass || "") : "");
}}
if (workspace.windowActivated) {{ workspace.windowActivated.connect(report); report(workspace.activeWindow); }}
else {{ workspace.clientActivated.connect(report); report(workspace.activeClient); }}
"#
                    ),
                )
                .map_err(|e| err(&e))?;
                let reply = conn
                    .call_method(
                        Some("org.kde.KWin"),
                        "/Scripting",
                        Some("org.kde.kwin.Scripting"),
                        "loadScript",
                        &(script.to_string_lossy().as_ref(), plugin.as_str()),
                    )
                    .map_err(|e| err(&e))?;
                let id: i32 = reply.body().deserialize().map_err(|e| err(&e))?;
                if id < 0 {
                    return Err(HostError::Window("KWin refused the script".into()));
                }
                // Plasma 6 exposes /Scripting/Script<id>, Plasma 5 /<id>
                let started = [format!("/Scripting/Script{id}"), format!("/{id}")].iter().any(|path| {
                    conn.call_method(Some("org.kde.KWin"), path.as_str(), Some("org.kde.kwin.Script"), "run", &())
                        .is_ok()
                });
                let tracker = KWinTracker { conn, plugin, script, current };
                if !started {
                    return Err(HostError::Window("could not start the KWin script".into()));
                }
                Ok(tracker)
            }
        }

        impl Drop for KWinTracker {
            fn drop(&mut self) {
                let _ = self.conn.call_method(
                    Some("org.kde.KWin"),
                    "/Scripting",
                    Some("org.kde.kwin.Scripting"),
                    "unloadScript",
                    &(self.plugin.as_str(),),
                );
                let _ = std::fs::remove_file(&self.script);
            }
        }

        impl ActiveWindow for KWinTracker {
            fn current(&mut self) -> Result<Option<String>, HostError> {
                Ok(self.current.lock().unwrap().clone())
            }
        }
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
