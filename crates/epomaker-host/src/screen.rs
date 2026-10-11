//! Average color of the screen, for the "screen color" lighting effect.
//!
//! * Windows: GDI half-tone scaling of the primary screen to one pixel.
//! * Linux X11: samples a few full-width rows of the root window.
//! * Linux Wayland: the xdg-desktop-portal ScreenCast interface (see `screen_portal`).

use crate::HostError;
use epomaker_core::lighting::Rgb;

/// Something that can be asked for the screen color repeatedly.
pub trait ScreenSampler: Send {
    fn sample(&mut self) -> Result<Rgb, HostError>;
}

pub fn sampler() -> Result<Box<dyn ScreenSampler>, HostError> {
    imp::sampler()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use epomaker_core::live::average_color;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
    use x11rb::rust_connection::RustConnection;

    const ROWS: u16 = 24;

    struct X11 {
        conn: RustConnection,
        root: u32,
        width: u16,
        height: u16,
    }

    pub fn sampler() -> Result<Box<dyn ScreenSampler>, HostError> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("EPOMAKER_FORCE_X11_CAPTURE").is_none() {
            return Ok(Box::new(crate::screen_portal::sampler()?));
        }
        let (conn, screen) = x11rb::connect(None).map_err(|e| HostError::Screen(e.to_string()))?;
        let s = &conn.setup().roots[screen];
        let (root, width, height) = (s.root, s.width_in_pixels, s.height_in_pixels);
        Ok(Box::new(X11 { conn, root, width, height }))
    }

    impl ScreenSampler for X11 {
        fn sample(&mut self) -> Result<Rgb, HostError> {
            let err = |e: &dyn std::fmt::Display| HostError::Screen(e.to_string());
            let mut pixels: Vec<[u8; 3]> = Vec::new();
            for r in 0..ROWS {
                let y = ((r as u32 * 2 + 1) * self.height as u32 / (ROWS as u32 * 2)) as i16;
                let img = self
                    .conn
                    .get_image(ImageFormat::Z_PIXMAP, self.root, 0, y, self.width, 1, !0)
                    .map_err(|e| err(&e))?
                    .reply()
                    .map_err(|e| err(&e))?;
                if img.depth < 24 {
                    return Err(HostError::Screen(format!("unsupported X11 depth {}", img.depth)));
                }
                // 32 bits per pixel, little endian BGRX
                pixels.extend(img.data.as_chunks::<4>().0.iter().step_by(4).map(|p| [p[2], p[1], p[0]]));
            }
            Ok(average_color(pixels.into_iter()))
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel, ReleaseDC, SelectObject,
        SetStretchBltMode, StretchBlt, HALFTONE, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    struct Gdi;

    pub fn sampler() -> Result<Box<dyn ScreenSampler>, HostError> {
        Ok(Box::new(Gdi))
    }

    impl ScreenSampler for Gdi {
        fn sample(&mut self) -> Result<Rgb, HostError> {
            // SAFETY: plain GDI calls on handles created and released within this function.
            unsafe {
                let screen = GetDC(std::ptr::null_mut());
                if screen.is_null() {
                    return Err(HostError::Screen("GetDC failed".into()));
                }
                let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
                let mem = CreateCompatibleDC(screen);
                let bmp = CreateCompatibleBitmap(screen, 1, 1);
                let old = SelectObject(mem, bmp);
                SetStretchBltMode(mem, HALFTONE);
                let ok = StretchBlt(mem, 0, 0, 1, 1, screen, 0, 0, w, h, SRCCOPY);
                let c = GetPixel(mem, 0, 0);
                SelectObject(mem, old);
                DeleteObject(bmp);
                DeleteDC(mem);
                ReleaseDC(std::ptr::null_mut(), screen);
                if ok == 0 {
                    return Err(HostError::Screen("StretchBlt failed".into()));
                }
                // COLORREF is 0x00BBGGRR
                Ok(Rgb::new((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8))
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;
    pub fn sampler() -> Result<Box<dyn ScreenSampler>, HostError> {
        Err(HostError::Unsupported("screen capture"))
    }
}
