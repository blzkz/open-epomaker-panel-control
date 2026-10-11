//! Keeps the host-driven effects alive.
//!
//! [`LiveSession`] does one step per call so it can live inside another loop (tray agent,
//! GUI worker); [`run`] is the blocking convenience used by the CLI.

use crate::{audio, cpu, screen, HostError};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::live::{SpectrumAnalyzer, MUSIC_BANDS, MUSIC_INTERVAL_MS, SCREEN_INTERVAL_MS};
use epomaker_core::Keyboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Frames are re-sent at least this often even when nothing changes.
const KEEPALIVE: Duration = Duration::from_millis(250);

/// Which host-driven effect to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LiveEffect {
    /// "Music follow" with one of the vendor variants (`MusicFollow3` or `MusicFollow2`).
    Music(LightMode),
    Screen,
    /// Software effect: CPU usage as a green-to-red bar on the number row (custom per-key mode).
    CpuLoad,
}

impl LiveEffect {
    /// The lighting mode the keyboard must be in for this effect.
    pub fn light_mode(self) -> LightMode {
        match self {
            LiveEffect::Music(m) => m,
            LiveEffect::Screen => LightMode::ScreenColor,
            LiveEffect::CpuLoad => LightMode::UserPicture,
        }
    }

    pub fn interval(self) -> Duration {
        Duration::from_millis(match self {
            LiveEffect::Music(_) => MUSIC_INTERVAL_MS,
            LiveEffect::Screen => SCREEN_INTERVAL_MS,
            LiveEffect::CpuLoad => 500,
        })
    }
}

/// What was sent in one step.
#[derive(Clone, Copy, Debug)]
pub enum Frame {
    Music([u8; MUSIC_BANDS]),
    Screen(Rgb),
    /// CPU usage, 0.0..=1.0.
    Cpu(f32),
}

enum Source {
    Audio(audio::AudioCapture, Box<SpectrumAnalyzer>),
    Screen(Box<dyn screen::ScreenSampler>),
    Cpu { meter: cpu::CpuMeter, bar: Vec<usize>, shown: Vec<Rgb> },
}

/// Keys of the CPU bar: the number row, left to right.
const CPU_BAR: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="];
const CPU_IDLE: Rgb = Rgb(0x0a0a14);

fn cpu_color(position: f32) -> Rgb {
    // green -> yellow -> red along the bar
    let p = position.clamp(0.0, 1.0);
    let (r, g) = if p < 0.5 { (p * 2.0, 1.0) } else { (1.0, (1.0 - p) * 2.0) };
    Rgb::new((r * 255.0) as u8, (g * 200.0) as u8, 0)
}

/// A running host-driven effect.
pub struct LiveSession {
    pub effect: LiveEffect,
    source: Source,
    last: Option<(Frame, Instant)>,
}

impl LiveSession {
    /// Starts capturing (audio or screen). Call [`LiveSession::activate`] to switch the
    /// keyboard to the matching lighting mode, then [`LiveSession::step`] every
    /// [`LiveEffect::interval`].
    pub fn start(effect: LiveEffect) -> Result<LiveSession, HostError> {
        let source = match effect {
            LiveEffect::Music(_) => {
                let capture = audio::start()?;
                log::info!("audio capture via {} at {} Hz", capture.backend, capture.sample_rate);
                Source::Audio(capture, Box::default())
            }
            LiveEffect::Screen => Source::Screen(screen::sampler()?),
            LiveEffect::CpuLoad => Source::Cpu { meter: cpu::CpuMeter::new(), bar: vec![], shown: vec![] },
        };
        Ok(LiveSession { effect, source, last: None })
    }

    /// Puts the keyboard in the effect's lighting mode (keeping `base`'s settings if it matches).
    pub fn activate(&mut self, kb: &mut Keyboard, base: Option<LightEffect>) -> Result<(), HostError> {
        let mode = self.effect.light_mode();
        let light =
            base.filter(|b| b.mode == mode).unwrap_or(LightEffect { mode, brightness: 4, ..LightEffect::default() });
        if let Source::Cpu { bar, shown, .. } = &mut self.source {
            *bar = CPU_BAR
                .iter()
                .filter_map(|l| kb.model.keys.iter().find(|k| k.label == *l).and_then(|k| k.matrix_index))
                .collect();
            if bar.is_empty() {
                return Err(HostError::Other("this keyboard has no number row in its layout".into()));
            }
            kb.set_key_colors(&vec![CPU_IDLE; kb.model.matrix_len()])?;
            *shown = vec![CPU_IDLE; bar.len()];
        }
        kb.set_light(&LightEffect { option: 0, ..light })?;
        Ok(())
    }

    /// Samples once and sends a frame if it changed (or the keep-alive is due).
    pub fn step(&mut self, kb: &mut Keyboard) -> Result<Frame, HostError> {
        let frame = match &mut self.source {
            Source::Audio(capture, analyzer) => {
                analyzer.push(&capture.drain());
                Frame::Music(analyzer.levels())
            }
            Source::Screen(sampler) => Frame::Screen(sampler.sample()?),
            Source::Cpu { meter, bar, shown } => {
                let usage = meter.usage()?;
                let lit = (usage * bar.len() as f32).round() as usize;
                for (i, idx) in bar.iter().enumerate() {
                    let c = if i < lit.max(1) { cpu_color(i as f32 / (bar.len() - 1).max(1) as f32) } else { CPU_IDLE };
                    if shown[i] != c {
                        kb.set_key_color(0, *idx, c)?;
                        shown[i] = c;
                    }
                }
                return Ok(Frame::Cpu(usage));
            }
        };
        let changed = match (&self.last, frame) {
            (Some((Frame::Music(a), t)), Frame::Music(b)) => *a != b || t.elapsed() >= KEEPALIVE,
            (Some((Frame::Screen(a), t)), Frame::Screen(b)) => *a != b || t.elapsed() >= KEEPALIVE,
            _ => true,
        };
        if changed {
            match frame {
                Frame::Music(levels) => kb.send_music_levels(&levels)?,
                Frame::Screen(c) => kb.send_screen_color(c)?,
                Frame::Cpu(_) => {}
            }
            self.last = Some((frame, Instant::now()));
        }
        Ok(frame)
    }
}

/// Switches the keyboard to the effect and streams until `stop` becomes true.
/// `on_frame` receives each music frame or screen color, e.g. for a level meter.
pub fn run(
    kb: &mut Keyboard,
    effect: LiveEffect,
    stop: &AtomicBool,
    base: Option<LightEffect>,
    mut on_frame: impl FnMut(Frame),
) -> Result<(), HostError> {
    let mut session = LiveSession::start(effect)?;
    session.activate(kb, base)?;
    while !stop.load(Ordering::Relaxed) {
        let tick = Instant::now();
        on_frame(session.step(kb)?);
        let next = tick + effect.interval();
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
    }
    Ok(())
}
