//! Captures what the computer is playing (not the microphone), as mono `f32` samples.
//!
//! * Linux: records the monitor of the default output with `parec` (PulseAudio and
//!   PipeWire-Pulse) or, failing that, `pw-record`. No build-time audio libraries needed.
//! * Windows: WASAPI loopback through `cpal`.

use crate::HostError;
use std::sync::mpsc::{sync_channel, Receiver};

/// Sample rate requested on Linux (Windows uses the output device's rate).
pub const SAMPLE_RATE: u32 = 48_000;

/// A running capture. Dropping it stops the recording.
pub struct AudioCapture {
    rx: Receiver<Vec<f32>>,
    pub sample_rate: u32,
    pub backend: &'static str,
    #[cfg(target_os = "linux")]
    child: std::process::Child,
    #[cfg(windows)]
    _stream: cpal::Stream,
}

impl AudioCapture {
    /// Every sample captured since the last call.
    pub fn drain(&self) -> Vec<f32> {
        let mut out = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            out.extend(chunk);
        }
        out
    }
}

#[cfg(target_os = "linux")]
impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts capturing the system audio output.
pub fn start() -> Result<AudioCapture, HostError> {
    imp::start()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc::TrySendError;

    pub fn start() -> Result<AudioCapture, HostError> {
        let rate = SAMPLE_RATE.to_string();
        let candidates: [(&'static str, Vec<&str>); 2] = [
            (
                "parec",
                vec![
                    "--device=@DEFAULT_MONITOR@",
                    "--format=float32le",
                    "--rate",
                    &rate,
                    "--channels=1",
                    "--raw",
                    "--latency-msec=20",
                ],
            ),
            (
                "pw-record",
                vec![
                    "-P",
                    "stream.capture.sink=true",
                    "--format",
                    "f32",
                    "--rate",
                    &rate,
                    "--channels",
                    "1",
                    "--raw",
                    "-",
                ],
            ),
        ];
        let mut last = String::new();
        for (cmd, args) in candidates {
            match Command::new(cmd).args(&args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
                Ok(mut child) => {
                    let mut out = child.stdout.take().expect("piped stdout");
                    let (tx, rx) = sync_channel::<Vec<f32>>(64);
                    std::thread::Builder::new()
                        .name("audio-capture".into())
                        .spawn(move || {
                            let mut buf = [0u8; 4096];
                            let mut rest: Vec<u8> = Vec::new();
                            while let Ok(n) = out.read(&mut buf) {
                                if n == 0 {
                                    break;
                                }
                                rest.extend_from_slice(&buf[..n]);
                                let whole = rest.len() / 4 * 4;
                                let samples: Vec<f32> =
                                    rest[..whole].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
                                rest.drain(..whole);
                                match tx.try_send(samples) {
                                    Ok(()) | Err(TrySendError::Full(_)) => {}
                                    Err(TrySendError::Disconnected(_)) => break,
                                }
                            }
                        })
                        .map_err(|e| HostError::Audio(e.to_string()))?;
                    return Ok(AudioCapture { rx, sample_rate: SAMPLE_RATE, backend: cmd, child });
                }
                Err(e) => last = format!("{cmd}: {e}"),
            }
        }
        Err(HostError::Audio(format!(
            "could not start parec or pw-record ({last}); install pulseaudio-utils / libpulse or pipewire"
        )))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;

    pub fn start() -> Result<AudioCapture, HostError> {
        let err = |e: &dyn std::fmt::Display| HostError::Audio(e.to_string());
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| HostError::Audio("no output device".into()))?;
        let supported = device.default_output_config().map_err(|e| err(&e))?;
        let format = supported.sample_format();
        let config = supported.config();
        let sample_rate = config.sample_rate;
        let channels = config.channels.max(1) as usize;
        let (tx, rx) = sync_channel::<Vec<f32>>(64);
        let send = move |mono: Vec<f32>| {
            let _ = tx.try_send(mono);
        };
        let on_error = |e: cpal::Error| log::warn!("audio capture: {e}");
        // building an input stream on an output device enables WASAPI loopback
        let stream = match format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &_| {
                    send(data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect())
                },
                on_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &_| {
                    send(
                        data.chunks(channels)
                            .map(|f| f.iter().map(|s| *s as f32 / 32768.0).sum::<f32>() / channels as f32)
                            .collect(),
                    )
                },
                on_error,
                None,
            ),
            other => return Err(HostError::Audio(format!("unsupported sample format {other:?}"))),
        }
        .map_err(|e| err(&e))?;
        stream.play().map_err(|e| err(&e))?;
        Ok(AudioCapture { rx, sample_rate, backend: "wasapi-loopback", _stream: stream })
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;
    pub fn start() -> Result<AudioCapture, HostError> {
        Err(HostError::Unsupported("audio capture"))
    }
}
