//! Keys egui does not report (Caps Lock, Scroll Lock, Pause, Num Lock, Print Screen,
//! Menu), for the key tester. On Windows they come from the window messages; on Linux
//! the tester reads the keyboard's own reports instead (see `KeyReports`).

use eframe::egui;
use std::sync::{Mutex, OnceLock};

/// HID usages of the keys egui cannot report.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const UNSEEN: [u8; 6] = [0x39, 0x47, 0x48, 0x53, 0x46, 0x65];

static QUEUE: Mutex<Vec<(u8, bool)>> = Mutex::new(Vec::new());
static CTX: OnceLock<egui::Context> = OnceLock::new();

/// Lets the message hook wake the UI up.
pub fn set_context(ctx: &egui::Context) {
    let _ = CTX.set(ctx.clone());
}

/// Presses (`true`) and releases seen since the last call, as HID usages.
pub fn take() -> Vec<(u8, bool)> {
    QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

#[allow(dead_code)] // unused where no platform source exists
fn push(events: &[(u8, bool)]) {
    if let Ok(mut q) = QUEUE.lock() {
        q.extend_from_slice(events);
        let n = q.len();
        if n > 64 {
            q.drain(..n - 64);
        }
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Installs the window message hook (Windows only; does nothing elsewhere).
#[cfg(windows)]
pub fn install<T>(builder: &mut winit::event_loop::EventLoopBuilder<T>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    builder.with_msg_hook(|msg| {
        // SAFETY: winit passes a pointer to the MSG being dispatched.
        let msg = unsafe { &*(msg as *const MSG) };
        let pressed = match msg.message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return false,
        };
        // bit 30: the key was already down (auto-repeat)
        if pressed && (msg.lParam >> 30) & 1 == 1 {
            return false;
        }
        let hid = match msg.wParam as u16 {
            0x14 => 0x39, // VK_CAPITAL
            0x91 => 0x47, // VK_SCROLL
            0x13 => 0x48, // VK_PAUSE
            0x90 => 0x53, // VK_NUMLOCK
            0x2C => 0x46, // VK_SNAPSHOT (only the release reaches applications)
            0x5D => 0x65, // VK_APPS
            _ => return false,
        };
        if hid == 0x46 && !pressed {
            push(&[(hid, true), (hid, false)]);
        } else if hid != 0x46 {
            push(&[(hid, pressed)]);
        }
        false
    });
}

#[cfg(not(windows))]
pub fn install<T>(_builder: &mut T) {}
