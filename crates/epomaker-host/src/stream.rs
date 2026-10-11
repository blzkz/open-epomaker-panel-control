//! Loops that keep the host-driven effects alive.

use crate::{audio, screen, HostError};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::live::{SpectrumAnalyzer, MUSIC_BANDS, MUSIC_INTERVAL_MS, SCREEN_INTERVAL_MS};
use epomaker_core::Keyboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Frames are re-sent at least this often even when nothing changes.
const KEEPALIVE: Duration = Duration::from_millis(250);

/// Which host-driven effect to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveEffect {
    /// "Music follow" with one of the vendor variants (`MusicFollow3` or `MusicFollow2`).
    Music(LightMode),
    Screen,
}

impl LiveEffect {
    /// The lighting mode the keyboard must be in for this effect.
    pub fn light_mode(self) -> LightMode {
        match self {
            LiveEffect::Music(m) => m,
            LiveEffect::Screen => LightMode::ScreenColor,
        }
    }
}

/// Switches the keyboard to the matching effect (unless `keep_light`) and streams frames until
/// `stop` becomes true. `on_frame` receives each music frame or screen color, e.g. for a UI.
pub fn run(
    kb: &mut Keyboard,
    effect: LiveEffect,
    stop: &AtomicBool,
    base: Option<LightEffect>,
    mut on_frame: impl FnMut(Frame),
) -> Result<(), HostError> {
    let mode = effect.light_mode();
    let light =
        base.filter(|b| b.mode == mode).unwrap_or(LightEffect { mode, brightness: 4, ..LightEffect::default() });
    kb.set_light(&light)?;
    match effect {
        LiveEffect::Music(_) => music(kb, stop, &mut on_frame),
        LiveEffect::Screen => screen_color(kb, stop, &mut on_frame),
    }
}

/// What was sent in one iteration.
#[derive(Clone, Copy, Debug)]
pub enum Frame {
    Music([u8; MUSIC_BANDS]),
    Screen(Rgb),
}

fn music(kb: &mut Keyboard, stop: &AtomicBool, on_frame: &mut dyn FnMut(Frame)) -> Result<(), HostError> {
    let capture = audio::start()?;
    log::info!("audio capture via {} at {} Hz", capture.backend, capture.sample_rate);
    let mut analyzer = SpectrumAnalyzer::default();
    let mut last = ([0u8; MUSIC_BANDS], Instant::now() - KEEPALIVE);
    while !stop.load(Ordering::Relaxed) {
        let tick = Instant::now();
        analyzer.push(&capture.drain());
        let levels = analyzer.levels();
        if levels != last.0 || last.1.elapsed() >= KEEPALIVE {
            kb.send_music_levels(&levels)?;
            last = (levels, Instant::now());
        }
        on_frame(Frame::Music(levels));
        sleep_until(tick + Duration::from_millis(MUSIC_INTERVAL_MS));
    }
    Ok(())
}

fn screen_color(kb: &mut Keyboard, stop: &AtomicBool, on_frame: &mut dyn FnMut(Frame)) -> Result<(), HostError> {
    let mut sampler = screen::sampler()?;
    let mut last = (Rgb(u32::MAX), Instant::now() - KEEPALIVE);
    while !stop.load(Ordering::Relaxed) {
        let tick = Instant::now();
        let c = sampler.sample()?;
        if c != last.0 || last.1.elapsed() >= KEEPALIVE {
            kb.send_screen_color(c)?;
            last = (c, Instant::now());
        }
        on_frame(Frame::Screen(c));
        sleep_until(tick + Duration::from_millis(SCREEN_INTERVAL_MS));
    }
    Ok(())
}

fn sleep_until(t: Instant) {
    let now = Instant::now();
    if t > now {
        std::thread::sleep(t - now);
    }
}
