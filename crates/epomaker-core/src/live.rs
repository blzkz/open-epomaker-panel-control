//! Effects driven by the host in real time: "music follow" (`SET_AUDIO`, 0x0D) and
//! "screen color" (`SET_SCREEN_COLOR`, 0x0E).
//!
//! The firmware only renders these effects while the computer keeps streaming data:
//! 32 band levels (0..=6) every ~30 ms for the music modes, the average screen color every
//! ~40 ms for the screen mode. The vendor app computes the bands from a Web Audio
//! `AnalyserNode`; [`SpectrumAnalyzer`] reproduces it and [`music_levels`] is a port of its
//! band selection (`MusicFollowStore.music3Delt`).

use crate::lighting::Rgb;
use crate::protocol::{report_from, Checksum, Frame};
use std::f32::consts::PI;

pub const SET_AUDIO: u8 = 0x0D;
pub const SET_SCREEN_COLOR: u8 = 0x0E;
/// Bands per music frame.
pub const MUSIC_BANDS: usize = 32;
/// Highest band level for keyboards (the vendor uses 9 for mice).
pub const MUSIC_MAX_LEVEL: u8 = 6;
/// Interval between frames used by the vendor app.
pub const MUSIC_INTERVAL_MS: u64 = 30;
pub const SCREEN_INTERVAL_MS: u64 = 40;

/// One music frame: `0D 00 00 00 00 00 00 CK` followed by 32 levels (wired keyboards).
pub fn encode_music_frame(levels: &[u8]) -> Frame {
    let mut r = report_from(&[SET_AUDIO]);
    for (i, l) in levels.iter().take(MUSIC_BANDS).enumerate() {
        r[8 + i] = *l;
    }
    Frame::new(r, Checksum::Bit7)
}

/// One screen-color frame: `0E RR GG BB FF`.
pub fn encode_screen_color(c: Rgb) -> Frame {
    Frame::new(report_from(&[SET_SCREEN_COLOR, c.r(), c.g(), c.b(), 0xFF]), Checksum::Bit7)
}

/// Port of the vendor's band selection: the first 100 FFT bins are normalized against
/// the floor of bins 20..40 and quantized to `0..=max_level`; bins 27..59 become the bands.
pub fn music_levels(spectrum: &[u8], max_level: u8) -> [u8; MUSIC_BANDS] {
    let mut out = [0u8; MUSIC_BANDS];
    if spectrum.len() < 59 {
        return out;
    }
    let bins = &spectrum[..spectrum.len().min(100)];
    let floor = bins[20..40.min(bins.len())].iter().copied().min().unwrap_or(0) as f64;
    let rel: Vec<f64> = bins.iter().map(|b| *b as f64 - floor).collect();
    let max = rel.iter().copied().fold(f64::MIN, f64::max);
    let step = max / max_level as f64;
    for (i, o) in out.iter_mut().enumerate() {
        let q = js_round(rel[27 + i] / step);
        // 0/0 (silence) is NaN in the vendor code and ends up as 0 on the wire
        *o = if q.is_nan() || q < 0.0 { 0 } else { q.min(max_level as f64) as u8 };
    }
    out
}

/// JavaScript `Math.round` (halves round up, also for negatives).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Reimplementation of the Web Audio `AnalyserNode` frequency analysis
/// (Blackman window, FFT, time smoothing, dB scaling to bytes).
pub struct SpectrumAnalyzer {
    fft_size: usize,
    smoothing: f32,
    min_db: f32,
    max_db: f32,
    ring: Vec<f32>,
    pos: usize,
    smoothed: Vec<f32>,
    window: Vec<f32>,
}

impl Default for SpectrumAnalyzer {
    /// The settings used by the vendor app: 2048 points, smoothing 0.4, -100..-30 dB.
    fn default() -> Self {
        SpectrumAnalyzer::new(2048, 0.4, -100.0, -30.0)
    }
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, smoothing: f32, min_db: f32, max_db: f32) -> Self {
        assert!(fft_size.is_power_of_two());
        let n = fft_size as f32;
        let window = (0..fft_size)
            .map(|i| {
                let x = i as f32 / n;
                0.42 - 0.5 * (2.0 * PI * x).cos() + 0.08 * (4.0 * PI * x).cos()
            })
            .collect();
        SpectrumAnalyzer {
            fft_size,
            smoothing,
            min_db,
            max_db,
            ring: vec![0.0; fft_size],
            pos: 0,
            smoothed: vec![0.0; fft_size / 2],
            window,
        }
    }

    /// Feeds mono samples in `-1.0..=1.0`.
    pub fn push(&mut self, samples: &[f32]) {
        for s in samples {
            self.ring[self.pos] = *s;
            self.pos = (self.pos + 1) % self.fft_size;
        }
    }

    /// Byte spectrum (`fft_size / 2` bins), like `getByteFrequencyData`.
    pub fn byte_frequency_data(&mut self) -> Vec<u8> {
        let n = self.fft_size;
        let mut re: Vec<f32> = (0..n).map(|i| self.ring[(self.pos + i) % n] * self.window[i]).collect();
        let mut im = vec![0.0f32; n];
        fft(&mut re, &mut im);
        let range = self.max_db - self.min_db;
        (0..n / 2)
            .map(|k| {
                let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / n as f32;
                let s = self.smoothing * self.smoothed[k] + (1.0 - self.smoothing) * mag;
                self.smoothed[k] = if s.is_finite() { s } else { 0.0 };
                let db = 20.0 * self.smoothed[k].max(1e-12).log10();
                (255.0 / range * (db - self.min_db)).floor().clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    /// Spectrum → the 32 levels sent to the keyboard.
    pub fn levels(&mut self) -> [u8; MUSIC_BANDS] {
        let spectrum = self.byte_frequency_data();
        music_levels(&spectrum, MUSIC_MAX_LEVEL)
    }
}

/// In-place iterative radix-2 FFT.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Average color of a set of RGB pixels.
pub fn average_color(pixels: impl Iterator<Item = [u8; 3]>) -> Rgb {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for p in pixels {
        r += p[0] as u64;
        g += p[1] as u64;
        b += p[2] as u64;
        n += 1;
    }
    if n == 0 {
        return Rgb(0);
    }
    Rgb::new((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_peaks_in_the_right_bin() {
        let mut a = SpectrumAnalyzer::default();
        // 48 kHz, 1 kHz tone -> bin 1000 / (48000 / 2048) ≈ 42.7
        let s: Vec<f32> = (0..4096).map(|i| (2.0 * PI * 1000.0 * i as f32 / 48000.0).sin() * 0.5).collect();
        a.push(&s);
        let spec = a.byte_frequency_data();
        let peak = spec.iter().enumerate().max_by_key(|(_, v)| **v).unwrap().0;
        assert!((42..=43).contains(&peak), "peak at {peak}");
        assert!(spec[400] < spec[peak]);
    }

    #[test]
    fn silence_gives_zero_levels() {
        let mut a = SpectrumAnalyzer::default();
        a.push(&[0.0; 2048]);
        assert_eq!(a.levels(), [0; MUSIC_BANDS]);
    }
}
