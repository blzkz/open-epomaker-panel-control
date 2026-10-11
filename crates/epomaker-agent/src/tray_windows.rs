//! Tray icon for Windows (notification area).

use crate::controller::{Command, Controller, Shared};
use crate::i18n::t;
use crate::menu::{header, live_index, LIVE_CHOICES};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};

pub fn run(controller: impl FnOnce(Box<dyn Fn() + Send>) -> Controller + Send + 'static, status: Shared) {
    let (tx, rx) = std::sync::mpsc::channel();
    crate::quit_on_signal(tx.clone());
    let dirty = Arc::new(AtomicBool::new(true));
    let d = dirty.clone();
    let notify: Box<dyn Fn() + Send> = Box::new(move || d.store(true, Ordering::Relaxed));
    let worker = std::thread::spawn(move || controller(notify).run(rx));

    let header_item = MenuItem::new("", false, None);
    let profiles: Vec<CheckMenuItem> =
        (1..=3).map(|p| CheckMenuItem::new(format!("{} {p}", t("Profile")), true, false, None)).collect();
    let profile_menu = Submenu::new(t("Profile"), true);
    for p in &profiles {
        let _ = profile_menu.append(p);
    }
    let lives: Vec<CheckMenuItem> =
        LIVE_CHOICES.iter().map(|(l, _)| CheckMenuItem::new(t(l), true, false, None)).collect();
    let live_menu = Submenu::new(t("Live effect"), true);
    for l in &lives {
        let _ = live_menu.append(l);
    }
    let per_app = CheckMenuItem::new(t("Per-application profiles"), true, false, None);
    let edit = MenuItem::new(t("Edit rules…"), true, None);
    let open = MenuItem::new(t("Open control panel"), true, None);
    let autostart = CheckMenuItem::new(t("Start with the system"), true, false, None);
    let quit = MenuItem::new(t("Quit"), true, None);
    let menu = Menu::new();
    let sep = || PredefinedMenuItem::separator();
    let _ = menu.append_items(&[
        &header_item,
        &sep(),
        &profile_menu,
        &live_menu,
        &per_app,
        &edit,
        &sep(),
        &open,
        &autostart,
        &sep(),
        &quit,
    ]);
    let icon = crate::icon::rgba();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Open Epomaker")
        .with_icon(Icon::from_rgba(icon.pixels, icon.width, icon.height).expect("icon"))
        .build();
    let tray = match tray {
        Ok(t) => Some(t),
        Err(e) => {
            log::error!("cannot show the tray icon: {e}");
            None
        }
    };

    let send = |c: Command| {
        let _ = tx.send(c);
    };
    loop {
        // SAFETY: standard Win32 message pump on the thread that owns the tray icon.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let id = ev.id();
            if let Some(i) = profiles.iter().position(|p| p.id() == id) {
                send(Command::SetProfile(i as u8));
            } else if let Some(i) = lives.iter().position(|l| l.id() == id) {
                send(Command::SetLive(LIVE_CHOICES[i].1));
            } else if id == per_app.id() {
                send(Command::SetPerApp(per_app.is_checked()));
            } else if id == autostart.id() {
                send(Command::SetAutostart(autostart.is_checked()));
            } else if id == edit.id() {
                send(Command::EditRules);
            } else if id == open.id() {
                send(Command::OpenPanel);
            } else if id == quit.id() {
                send(Command::Quit);
            }
            dirty.store(true, Ordering::Relaxed);
        }
        if dirty.swap(false, Ordering::Relaxed) {
            let s = status.lock().unwrap().clone();
            if s.quit {
                break;
            }
            let connected = s.model.is_some() && !s.paused;
            let text = header(&s);
            header_item.set_text(&text);
            if let Some(tray) = &tray {
                let _ = tray.set_tooltip(Some(format!("Open Epomaker — {text}")));
            }
            profile_menu.set_enabled(connected);
            for (i, p) in profiles.iter().enumerate() {
                p.set_checked(s.profile == Some(i as u8));
            }
            live_menu.set_enabled(connected && s.live_supported);
            let li = live_index(s.live);
            for (i, l) in lives.iter().enumerate() {
                l.set_checked(i == li);
            }
            per_app.set_enabled(s.per_app_available);
            per_app.set_checked(s.per_app);
            autostart.set_checked(s.autostart);
            open.set_enabled(!s.paused);
        }
        if worker.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
