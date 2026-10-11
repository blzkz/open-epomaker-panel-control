//! Rough on-screen simulation of the firmware lighting effects, used for the preview
//! on the lighting tab. It only has to look like the real thing, not match it.

use eframe::egui::{Color32, Vec2};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use std::f32::consts::TAU;

/// Where a key sits, as needed by the effects.
#[derive(Clone, Copy)]
pub struct Spot {
    pub idx: usize,
    /// Center, 0..=1 on both axes (y down).
    pub pos: Vec2,
    pub row: usize,
    pub rows: usize,
    /// Rank when walking the keys row by row ("Z" order) and back and forth.
    pub order: usize,
    pub serp: usize,
    pub count: usize,
}

/// Unlit key.
pub const DARK: Color32 = Color32::from_rgb(34, 34, 38);

pub struct Sim<'a> {
    pub effect: LightEffect,
    pub max_speed: u8,
    pub max_brightness: u8,
    /// Per-key colors (matrix order) shown by the custom effects.
    pub user_colors: &'a [Rgb],
}

impl Sim<'_> {
    /// Color of the key at `s` at time `t` (seconds).
    pub fn color(&self, s: &Spot, t: f64) -> Color32 {
        let e = &self.effect;
        let sp = (e.speed.min(self.max_speed.max(1)) as f32 + 1.0) / (self.max_speed.max(1) as f32 + 1.0);
        let v = 0.15 + 0.85 * sp;
        let bright = if self.max_brightness == 0 {
            1.0
        } else {
            0.15 + 0.85 * e.brightness.min(self.max_brightness) as f32 / self.max_brightness as f32
        };
        // keep the phase small so that f32 keeps enough precision
        let t = (t % 3600.0) as f32;
        let (x, y) = (s.pos.x, s.pos.y);
        let fixed = rgb(e.color);
        let pick = |hue: f32| if e.dazzle { hsv(hue) } else { fixed };
        let (color, level) = match e.mode {
            LightMode::Off => (DARK, 0.0),
            LightMode::AlwaysOn => (pick(x * 0.9), 1.0),
            LightMode::Breath => {
                let cycle = t * v * 0.5;
                (pick(cycle.floor() * 0.17), 0.5 - 0.5 * (cycle * TAU).cos())
            }
            LightMode::Neon => {
                let h = if e.option == 1 { hash(s.idx as u64) } else { 0.0 };
                (hsv(h + t * v * 0.15), 1.0)
            }
            LightMode::Wave => {
                let c = match e.option {
                    1 => 1.0 - x,
                    2 => y,
                    3 => 1.0 - y,
                    _ => x,
                };
                let phase = c - t * v * 0.6;
                (pick(phase), if e.dazzle { 1.0 } else { 0.55 + 0.45 * (phase * TAU * 1.5).sin() })
            }
            LightMode::Ripple => {
                let cycle = t * v * 0.5;
                let origin = if e.option == 1 { random_point(cycle.floor() as u64) } else { Vec2::new(0.5, 0.5) };
                let f = cycle.fract();
                let d = aspect(s.pos - origin).length();
                let level = (1.0 - (d - f * 1.1).abs() * 7.0).max(0.0) * (1.0 - f);
                (pick(d - t * 0.2), level)
            }
            LightMode::Raindrop => drops(s.idx, t * v * 1.4, 0.12, e.dazzle, fixed),
            LightMode::PressAction => {
                let (c, l) = drops(s.idx, t * v * 2.0, 0.06, e.dazzle, fixed);
                if e.option == 1 {
                    (c, 1.0 - l)
                } else {
                    (c, l)
                }
            }
            LightMode::PressActionOff => {
                let (_, l) = drops(s.idx, t * v * 2.0, 0.06, false, fixed);
                (pick(x), 1.0 - l)
            }
            LightMode::Snake | LightMode::Train => {
                let rank = if e.option == 1 { s.serp } else { s.order } as f32;
                let n = s.count.max(1) as f32;
                let head = (t * v * 0.12).fract() * n;
                let (len, gap) = if e.mode == LightMode::Snake { (9.0, n) } else { (5.0, 12.0) };
                let d = (head - rank).rem_euclid(n) % gap;
                (pick(rank / n + t * 0.05), if d < len { 1.0 - d / len } else { 0.0 })
            }
            LightMode::Converge => {
                let f = (t * v * 0.4).fract();
                let d = (x - 0.5).abs() * 2.0;
                ((pick(f)), (1.0 - (d - (1.0 - f)).abs() * 6.0).max(0.0))
            }
            LightMode::SineWave => {
                let phase = x * 1.2 - t * v * 0.5 + 0.12 * (y * TAU).sin();
                (pick(phase), 0.5 + 0.5 * (phase * TAU).sin())
            }
            LightMode::Kaleidoscope => {
                let d = aspect(s.pos - Vec2::new(0.5, 0.5)).length();
                let dir = if e.option == 1 { 1.0 } else { -1.0 };
                let phase = d * 1.6 + dir * t * v * 0.5;
                (hsv(phase), if e.dazzle { 1.0 } else { 0.5 + 0.5 * (phase * TAU).sin() })
            }
            LightMode::LineWave | LightMode::Laser => {
                let c = if e.option == 1 { 1.0 - x } else { x };
                let cycle = t * v * 0.6;
                let head = cycle.fract() * 1.3;
                let on_row = e.mode == LightMode::LineWave
                    || s.row == (hash(cycle.floor() as u64) * s.rows.max(1) as f32) as usize;
                let d = head - c;
                let level = if on_row && (0.0..0.3).contains(&d) { 1.0 - d / 0.3 } else { 0.0 };
                (pick(c + cycle.floor() * 0.2), level)
            }
            LightMode::CircleWave => {
                let d = aspect(s.pos - Vec2::new(0.5, 0.5));
                let a = d.y.atan2(d.x) / TAU;
                let dir = if e.option == 1 { -1.0 } else { 1.0 };
                let phase = a + dir * t * v * 0.4;
                (pick(phase), if e.dazzle { 1.0 } else { 0.5 + 0.5 * (phase * TAU).sin() })
            }
            LightMode::Dazzling => {
                let step = (t * v * 2.0 + hash(s.idx as u64) * 4.0).floor();
                (hsv(hash(s.idx as u64 * 977 + step as u64)), 1.0)
            }
            LightMode::RainDown | LightMode::Meteor => {
                let col = (x * 18.0).floor();
                let speed = 0.5 + hash(col as u64);
                let c = if e.mode == LightMode::Meteor { (y + (1.0 - x)) * 0.5 } else { y };
                let head = (t * v * 0.5 * speed + hash(col as u64 + 99)).fract() * 1.4;
                let d = head - c;
                let level = if (0.0..0.35).contains(&d) { 1.0 - d / 0.35 } else { 0.0 };
                (pick(col / 18.0), level)
            }
            LightMode::MusicFollow2 | LightMode::MusicFollow3 => {
                let col = (x * 16.0).floor();
                let level = 0.5 + 0.45 * (t * (2.0 + 3.0 * hash(col as u64)) + col).sin();
                let lit = match e.option {
                    1 => (y - 0.5).abs() * 2.0 < level,
                    2 => y < level * 0.5 || y > 1.0 - level * 0.5,
                    _ => 1.0 - y < level,
                };
                (if e.dazzle { hsv(0.33 - 0.33 * (1.0 - y)) } else { fixed }, if lit { 1.0 } else { 0.0 })
            }
            LightMode::ScreenColor => (hsv(t * 0.05), 1.0),
            LightMode::Fireworks => {
                let mut level: f32 = 0.0;
                let mut color = DARK;
                for k in 0..2u64 {
                    let cycle = t * v * 0.5 + k as f32 * 0.5;
                    let id = cycle.floor() as u64 * 2 + k;
                    let f = cycle.fract();
                    let d = aspect(s.pos - random_point(id)).length();
                    let l = (1.0 - (d - f * 0.6).abs() * 9.0).max(0.0) * (1.0 - f);
                    if l > level {
                        level = l;
                        color = pick(hash(id + 5));
                    }
                }
                (color, level)
            }
            LightMode::UserPicture | LightMode::UserColor => {
                (self.user_colors.get(s.idx).map(|c| rgb(*c)).unwrap_or(DARK), 1.0)
            }
        };
        mix(DARK, color, (level * bright).clamp(0.0, 1.0))
    }
}

/// Keys lighting up at random and fading out.
fn drops(idx: usize, time: f32, chance: f32, dazzle: bool, fixed: Color32) -> (Color32, f32) {
    let offset = hash(idx as u64 * 31 + 7);
    let cycle = time + offset;
    let id = idx as u64 * 1000 + cycle.floor() as u64;
    if hash(id) < chance {
        (if dazzle { hsv(hash(id + 1)) } else { fixed }, 1.0 - cycle.fract())
    } else {
        (fixed, 0.0)
    }
}

/// Distances look rounder when x is stretched like a keyboard (about 2.5:1).
fn aspect(d: Vec2) -> Vec2 {
    Vec2::new(d.x * 2.5, d.y)
}

fn random_point(seed: u64) -> Vec2 {
    Vec2::new(0.1 + 0.8 * hash(seed * 2 + 11), 0.1 + 0.8 * hash(seed * 2 + 12))
}

/// Pseudo random number in 0..1.
fn hash(mut x: u64) -> f32 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

pub fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r(), c.g(), c.b())
}

/// Fully saturated color for `hue` (wraps around 0..1).
pub fn hsv(hue: f32) -> Color32 {
    let h = hue.rem_euclid(1.0) * 6.0;
    let f = h.fract();
    let (r, g, b) = match h as u32 {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn mix(a: Color32, b: Color32, f: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_renders() {
        let colors = vec![Rgb(0x123456); 4];
        for mode in LightMode::ALL {
            let effect = LightEffect { mode, dazzle: true, option: 1, ..LightEffect::default() };
            let sim = Sim { effect, max_speed: 4, max_brightness: 4, user_colors: &colors };
            for i in 0..20 {
                let s = Spot {
                    idx: i,
                    pos: Vec2::new(i as f32 / 20.0, (i % 4) as f32 / 4.0),
                    row: i % 4,
                    rows: 4,
                    order: i,
                    serp: i,
                    count: 20,
                };
                sim.color(&s, 1234.5 + i as f64);
            }
        }
        assert_eq!(hsv(0.0), Color32::from_rgb(255, 0, 0));
        assert_eq!(hsv(1.0 / 3.0), Color32::from_rgb(0, 255, 0));
        let off = Sim { effect: LightEffect::off(), max_speed: 4, max_brightness: 4, user_colors: &[] };
        let s = Spot { idx: 0, pos: Vec2::ZERO, row: 0, rows: 1, order: 0, serp: 0, count: 1 };
        assert_eq!(off.color(&s, 0.0), DARK);
    }
}
