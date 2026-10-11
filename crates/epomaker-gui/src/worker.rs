//! Background thread that owns the keyboard. HID transactions block (the firmware
//! needs pauses between reports), so the UI never talks to the device directly.

use epomaker_core::devices::{DeviceInfo, ModelInfo};
use epomaker_core::keymap::{KeyMatrix, Layer};
use epomaker_core::lighting::{LightEffect, Rgb};
use epomaker_core::macros::Macro;
use epomaker_core::settings::{Battery, KeyboardOptions, ReportRate, SleepTimes, PROFILES};
use epomaker_core::{Error, Keyboard};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub type Job = Box<dyn FnOnce(&mut Option<Keyboard>) -> Result<Update, Error> + Send>;

/// Everything read from the keyboard right after connecting.
#[derive(Clone)]
pub struct Snapshot {
    pub info: DeviceInfo,
    pub model: &'static ModelInfo,
    pub describe: String,
    pub emulated: bool,
    pub firmware: u16,
    pub battery: Battery,
    pub profile: u8,
    pub light: Option<LightEffect>,
    pub rates: Vec<Option<ReportRate>>,
    pub options: Vec<KeyboardOptions>,
    pub debounce: u8,
    pub sleep: SleepTimes,
    pub auto_os: bool,
    pub layers: Vec<(Layer, KeyMatrix)>,
    pub colors: Vec<Rgb>,
    pub used_slots: Vec<u8>,
}

pub enum Update {
    None,
    Connected(Box<Snapshot>),
    Disconnected,
    Light(Option<LightEffect>),
    Profile(u8),
    Layer(Layer, KeyMatrix),
    Colors(Vec<Rgb>),
    Macro(u8, Macro),
    Battery(Battery),
    Message(String),
    Many(Vec<Update>),
}

pub struct Worker {
    tx: Sender<(String, Job)>,
    rx: Receiver<(String, Result<Update, Error>)>,
    pending: Arc<AtomicUsize>,
}

impl Worker {
    pub fn new(ctx: eframe::egui::Context) -> Worker {
        let (tx, job_rx) = channel::<(String, Job)>();
        let (res_tx, rx) = channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let p = pending.clone();
        std::thread::Builder::new()
            .name("keyboard".into())
            .spawn(move || {
                let mut kb: Option<Keyboard> = None;
                while let Ok((label, job)) = job_rx.recv() {
                    let r = job(&mut kb);
                    p.fetch_sub(1, Ordering::SeqCst);
                    if res_tx.send((label, r)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn worker");
        Worker { tx, rx, pending }
    }

    /// Queues `job`; `label` is shown while it runs and in error messages.
    pub fn run(
        &self,
        label: impl Into<String>,
        job: impl FnOnce(&mut Keyboard) -> Result<Update, Error> + Send + 'static,
    ) {
        self.run_raw(
            label,
            Box::new(move |kb: &mut Option<Keyboard>| match kb.as_mut() {
                Some(k) => job(k),
                None => Err(Error::NotFound),
            }),
        );
    }

    pub fn run_raw(&self, label: impl Into<String>, job: Job) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.tx.send((label.into(), job)).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    pub fn poll(&self) -> Vec<(String, Result<Update, Error>)> {
        self.rx.try_iter().collect()
    }
}

/// Reads the full state of a freshly connected keyboard.
pub fn snapshot(kb: &mut Keyboard) -> Result<Snapshot, Error> {
    let profile = kb.profile()?;
    let mut rates = Vec::new();
    let mut options = Vec::new();
    for p in 0..PROFILES {
        rates.push(kb.report_rate(p)?);
        options.push(kb.options(p)?);
    }
    let mut layers = Vec::new();
    for layer in kb.layers() {
        layers.push((layer, kb.read_layer(layer)?));
    }
    let used_slots = layers.iter().flat_map(|(_, m)| m.macro_indices().collect::<Vec<_>>()).collect::<Vec<_>>();
    let mut used_slots = used_slots;
    used_slots.sort();
    used_slots.dedup();
    Ok(Snapshot {
        info: kb.info.clone(),
        model: kb.model,
        describe: kb.describe(),
        emulated: kb.is_emulated(),
        firmware: kb.firmware_version()?,
        battery: kb.battery()?,
        profile,
        light: kb.light()?,
        rates,
        options,
        debounce: kb.debounce()?,
        sleep: kb.sleep_times()?,
        auto_os: kb.auto_os().unwrap_or(false),
        layers,
        colors: kb.key_colors()?,
        used_slots,
    })
}
