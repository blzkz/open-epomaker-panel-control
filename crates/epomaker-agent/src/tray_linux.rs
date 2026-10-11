//! Tray icon for Linux desktops through the StatusNotifierItem D-Bus protocol
//! (KDE Plasma, and GNOME with the AppIndicator extension, XFCE, Cinnamon…).

use crate::controller::{Command, Controller, Shared, Status};
use crate::i18n::t;
use crate::menu::{header, live_index, LIVE_CHOICES};
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use std::sync::mpsc::Sender;

struct AgentTray {
    status: Shared,
    tx: Sender<Command>,
    icon: Vec<ksni::Icon>,
}

impl AgentTray {
    fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}

impl ksni::Tray for AgentTray {
    fn id(&self) -> String {
        "open-epomaker-agent".into()
    }

    fn title(&self) -> String {
        "Open Epomaker".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let s = self.status.lock().unwrap().clone();
        ksni::ToolTip { title: "Open Epomaker".into(), description: header(&s), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::OpenPanel);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let s: Status = self.status.lock().unwrap().clone();
        let connected = s.model.is_some() && !s.paused;
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem { label: header(&s), enabled: false, ..Default::default() }.into(),
            MenuItem::Separator,
            SubMenu {
                label: t("Profile").into(),
                enabled: connected,
                submenu: vec![RadioGroup {
                    selected: s.profile.map(|p| p as usize).unwrap_or(usize::MAX),
                    select: Box::new(|tray: &mut Self, i| tray.send(Command::SetProfile(i as u8))),
                    options: (1..=3)
                        .map(|p| RadioItem { label: format!("{} {p}", t("Profile")), ..Default::default() })
                        .collect(),
                }
                .into()],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: t("Live effect").into(),
                enabled: connected && s.live_supported,
                submenu: vec![RadioGroup {
                    selected: live_index(s.live),
                    select: Box::new(|tray: &mut Self, i| tray.send(Command::SetLive(LIVE_CHOICES[i].1))),
                    options: LIVE_CHOICES
                        .iter()
                        .map(|(label, _)| RadioItem { label: t(label).into(), ..Default::default() })
                        .collect(),
                }
                .into()],
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: if s.per_app_available {
                    t("Per-application profiles").into()
                } else {
                    format!("{} ({})", t("Per-application profiles"), t("not available on this desktop"))
                },
                enabled: s.per_app_available,
                checked: s.per_app,
                activate: Box::new(move |tray: &mut Self| tray.send(Command::SetPerApp(!s.per_app))),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t("Edit rules…").into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::EditRules)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: t("Open control panel").into(),
                enabled: !s.paused,
                activate: Box::new(|tray: &mut Self| tray.send(Command::OpenPanel)),
                ..Default::default()
            }
            .into(),
        ];
        let autostart = s.autostart;
        items.push(
            CheckmarkItem {
                label: t("Start with the system").into(),
                checked: autostart,
                activate: Box::new(move |tray: &mut Self| tray.send(Command::SetAutostart(!autostart))),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: t("Quit").into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

pub fn run(controller: impl FnOnce(Box<dyn Fn() + Send>) -> Controller, status: Shared) {
    let (tx, rx) = std::sync::mpsc::channel();
    crate::quit_on_signal(tx.clone());
    let icon = crate::icon::rgba();
    let argb: Vec<u8> = icon.pixels.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    let tray = AgentTray {
        status: status.clone(),
        tx,
        icon: vec![ksni::Icon { width: icon.width as i32, height: icon.height as i32, data: argb }],
    };
    let handle = match tray.spawn() {
        Ok(h) => Some(h),
        Err(e) => {
            log::error!("cannot show the tray icon ({e}); running without it");
            None
        }
    };
    let h = handle.clone();
    let notify: Box<dyn Fn() + Send> = Box::new(move || {
        if let Some(h) = &h {
            h.update(|_| {});
        }
    });
    controller(notify).run(rx);
    if let Some(h) = handle {
        h.shutdown().wait();
    }
}
