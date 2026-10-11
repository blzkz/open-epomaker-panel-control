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
use std::sync::{Arc, Mutex};

pub type Job = Box<dyn FnOnce(&mut Option<Keyboard>, &Progress) -> Result<Update, Error> + Send>;

/// Progress of the running job, for writes that take many packets.
#[derive(Default)]
pub struct Progress {
    done: AtomicUsize,
    total: AtomicUsize,
}

impl Progress {
    pub fn set(&self, done: usize, total: usize) {
        self.total.store(total, Ordering::SeqCst);
        self.done.store(done, Ordering::SeqCst);
    }

    /// Fraction done, when the running job reports progress.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::SeqCst);
        (total > 0).then(|| self.done.load(Ordering::SeqCst) as f32 / total as f32)
    }
}

/// How the UI reports a job: `label` names it in messages, `notify` asks for a
/// confirmation toast on success (errors are always shown) and `blocking` locks the
/// editors until the job is done (for jobs that reload the whole keyboard state).
#[derive(Clone, Copy)]
pub struct JobInfo {
    pub label: &'static str,
    pub notify: bool,
    pub blocking: bool,
}

impl JobInfo {
    pub fn quiet(label: &'static str) -> JobInfo {
        JobInfo { label, notify: false, blocking: false }
    }
    pub fn notify(label: &'static str) -> JobInfo {
        JobInfo { label, notify: true, blocking: false }
    }
    pub fn blocking(label: &'static str) -> JobInfo {
        JobInfo { label, notify: true, blocking: true }
    }
}

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
    tx: Sender<(JobInfo, Job)>,
    rx: Receiver<(JobInfo, Result<Update, Error>)>,
    pending: Arc<AtomicUsize>,
    blocking: AtomicUsize,
    progress: Arc<Progress>,
    current: Arc<Mutex<&'static str>>,
}

impl Worker {
    pub fn new(ctx: eframe::egui::Context) -> Worker {
        let (tx, job_rx) = channel::<(JobInfo, Job)>();
        let (res_tx, rx) = channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let progress = Arc::new(Progress::default());
        let current = Arc::new(Mutex::new(""));
        let (p, prog, cur) = (pending.clone(), progress.clone(), current.clone());
        std::thread::Builder::new()
            .name("keyboard".into())
            .spawn(move || {
                let mut kb: Option<Keyboard> = None;
                while let Ok((info, job)) = job_rx.recv() {
                    *cur.lock().unwrap() = info.label;
                    prog.set(0, 0);
                    let r = job(&mut kb, &prog);
                    prog.set(0, 0);
                    p.fetch_sub(1, Ordering::SeqCst);
                    if res_tx.send((info, r)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn worker");
        Worker { tx, rx, pending, blocking: AtomicUsize::new(0), progress, current }
    }

    /// Queues `job` quietly: only errors are reported.
    pub fn run(&self, label: &'static str, job: impl FnOnce(&mut Keyboard) -> Result<Update, Error> + Send + 'static) {
        self.run_with(JobInfo::quiet(label), move |kb, _| job(kb));
    }

    /// Queues `job` and confirms its success with a toast.
    pub fn run_notify(
        &self,
        label: &'static str,
        job: impl FnOnce(&mut Keyboard) -> Result<Update, Error> + Send + 'static,
    ) {
        self.run_with(JobInfo::notify(label), move |kb, _| job(kb));
    }

    /// Queues a job that reports its progress.
    pub fn run_with(
        &self,
        info: JobInfo,
        job: impl FnOnce(&mut Keyboard, &Progress) -> Result<Update, Error> + Send + 'static,
    ) {
        self.run_raw(
            info,
            Box::new(move |kb: &mut Option<Keyboard>, p: &Progress| match kb.as_mut() {
                Some(k) => job(k, p),
                None => Err(Error::NotFound),
            }),
        );
    }

    pub fn run_raw(&self, info: JobInfo, job: Job) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if info.blocking {
            self.blocking.fetch_add(1, Ordering::SeqCst);
        }
        if self.tx.send((info, job)).is_err() {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            if info.blocking {
                self.blocking.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    /// A job that reloads the whole keyboard state is queued or running.
    pub fn blocked(&self) -> bool {
        self.blocking.load(Ordering::SeqCst) > 0
    }

    /// Label of the job running now.
    pub fn current(&self) -> &'static str {
        *self.current.lock().unwrap()
    }

    /// Fraction done of the running job, if it reports progress.
    pub fn progress(&self) -> Option<f32> {
        self.progress.fraction()
    }

    pub fn poll(&self) -> Vec<(JobInfo, Result<Update, Error>)> {
        let results: Vec<_> = self.rx.try_iter().collect();
        for (info, _) in &results {
            if info.blocking {
                self.blocking.fetch_sub(1, Ordering::SeqCst);
            }
        }
        results
    }
}

/// Reads the full state of a freshly connected keyboard.
pub fn snapshot(kb: &mut Keyboard, progress: &Progress) -> Result<Snapshot, Error> {
    let all_layers = kb.layers();
    let steps = 2 + PROFILES as usize + all_layers.len();
    progress.set(0, steps);
    let profile = kb.profile()?;
    let mut rates = Vec::new();
    let mut options = Vec::new();
    for p in 0..PROFILES {
        rates.push(kb.report_rate(p)?);
        options.push(kb.options(p)?);
        progress.set(1 + p as usize, steps);
    }
    let mut layers = Vec::new();
    for (i, layer) in all_layers.into_iter().enumerate() {
        layers.push((layer, kb.read_layer(layer)?));
        progress.set(1 + PROFILES as usize + i, steps);
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
