//! `epomaker-agent`: tray icon that keeps running in the background.
//!
//! * per-application profiles: switches the keyboard's onboard profile when the focused
//!   application matches a rule in `agent.json`;
//! * host-driven lighting effects (music follow, screen color);
//! * low-battery notifications and automatic reconnection.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod controller;
mod i18n;
mod icon;
mod menu;
#[cfg(target_os = "linux")]
mod tray_linux;
#[cfg(windows)]
mod tray_windows;

use controller::{Controller, Status};
use epomaker_host::agent_config as config;
use std::sync::{Arc, Mutex};

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,epomaker_agent=info,epomaker_host=info,zbus=error"),
    )
    .init();
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!(
            "epomaker-agent — tray agent for Open Epomaker Panel\n\nSettings: {}\nEnvironment: EPOMAKER_AGENT_DEMO=<model id> uses an emulated keyboard.",
            config::path().display()
        );
        return;
    }
    // one agent per user: two would fight over the keyboard
    let _lock = match config::lock_instance() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("epomaker-agent is already running ({e})");
            return;
        }
    };
    let config = config::Config::load();
    let status = Arc::new(Mutex::new(Status::default()));
    let s = status.clone();
    let make = move |notify: Box<dyn Fn() + Send>| Controller::new(config, s, notify);
    #[cfg(target_os = "linux")]
    tray_linux::run(make, status);
    #[cfg(windows)]
    tray_windows::run(make, status);
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (make, status);
        eprintln!("the tray agent is only available on Linux and Windows");
    }
}

/// Turns Ctrl+C / SIGTERM into a clean shutdown (restores KWin and stops capture processes).
fn quit_on_signal(tx: std::sync::mpsc::Sender<controller::Command>) {
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = tx.send(controller::Command::Quit);
    }) {
        log::warn!("signal handler: {e}");
    }
}
