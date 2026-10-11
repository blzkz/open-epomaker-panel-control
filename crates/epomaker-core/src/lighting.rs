//! Backlight effects (`SET_LED` / `GET_LED`, commands `0x07` / `0x87`).
//!
//! Layout of the report (checksum [`Checksum::Bit8`] at byte 8):
//!
//! | byte | meaning                                                        |
//! |------|----------------------------------------------------------------|
//! | 0    | `0x07`                                                         |
//! | 1    | effect id ([`LightMode::code`])                               |
//! | 2    | `4 - speed` (speed 0..=4, 4 = fastest)                         |
//! | 3    | brightness 0..=4                                               |
//! | 4    | `option << 4 \| color_mode` (color_mode 7 = fixed, 8 = rainbow) |
//! | 5..8 | red, green, blue                                               |

use crate::protocol::{cmd, report_from, Checksum, Frame, Report};
use serde::{Deserialize, Serialize};

pub const MAX_SPEED: u8 = 4;
pub const MAX_BRIGHTNESS: u8 = 4;

/// Low nibble of byte 4: a single fixed color.
const COLOR_FIXED: u8 = 7;
/// Low nibble of byte 4: "dazzle" (cycling rainbow colors).
const COLOR_DAZZLE: u8 = 8;

/// Firmware stores pure white as this value (the vendor app swaps it both ways).
const WHITE: u32 = 0xFF_FFFF;
const WHITE_ON_WIRE: u32 = 0xFA_FFFA;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LightMode {
    Off,
    AlwaysOn,
    Breath,
    Neon,
    Wave,
    Ripple,
    Raindrop,
    Snake,
    PressAction,
    Converge,
    SineWave,
    Kaleidoscope,
    LineWave,
    UserPicture,
    Laser,
    CircleWave,
    Dazzling,
    RainDown,
    Meteor,
    PressActionOff,
    MusicFollow3,
    ScreenColor,
    MusicFollow2,
    Train,
    Fireworks,
    UserColor,
}

impl LightMode {
    pub const ALL: [LightMode; 26] = [
        LightMode::Off,
        LightMode::AlwaysOn,
        LightMode::Breath,
        LightMode::Neon,
        LightMode::Wave,
        LightMode::Ripple,
        LightMode::Raindrop,
        LightMode::Snake,
        LightMode::PressAction,
        LightMode::Converge,
        LightMode::SineWave,
        LightMode::Kaleidoscope,
        LightMode::LineWave,
        LightMode::UserPicture,
        LightMode::Laser,
        LightMode::CircleWave,
        LightMode::Dazzling,
        LightMode::RainDown,
        LightMode::Meteor,
        LightMode::PressActionOff,
        LightMode::MusicFollow3,
        LightMode::ScreenColor,
        LightMode::MusicFollow2,
        LightMode::Train,
        LightMode::Fireworks,
        LightMode::UserColor,
    ];

    /// Effect id sent in byte 1.
    pub fn code(self) -> u8 {
        match self {
            LightMode::UserColor => 66,
            other => LightMode::ALL.iter().position(|m| *m == other).unwrap() as u8,
        }
    }

    pub fn from_code(code: u8) -> Option<LightMode> {
        match code {
            66 => Some(LightMode::UserColor),
            0..=24 => Some(LightMode::ALL[code as usize]),
            _ => None,
        }
    }

    /// Identifier used by the vendor software and in the device database.
    pub fn vendor_name(self) -> &'static str {
        match self {
            LightMode::Off => "LightOff",
            LightMode::AlwaysOn => "LightAlwaysOn",
            LightMode::Breath => "LightBreath",
            LightMode::Neon => "LightNeon",
            LightMode::Wave => "LightWave",
            LightMode::Ripple => "LightRipple",
            LightMode::Raindrop => "LightRaindrop",
            LightMode::Snake => "LightSnake",
            LightMode::PressAction => "LightPressAction",
            LightMode::Converge => "LightConverage",
            LightMode::SineWave => "LightSineWave",
            LightMode::Kaleidoscope => "LightKaleidoscope",
            LightMode::LineWave => "LightLineWave",
            LightMode::UserPicture => "LightUserPicture",
            LightMode::Laser => "LightLaser",
            LightMode::CircleWave => "LightCircleWave",
            LightMode::Dazzling => "LightDazzing",
            LightMode::RainDown => "LightRainDown",
            LightMode::Meteor => "LightMeteor",
            LightMode::PressActionOff => "LightPressActionOff",
            LightMode::MusicFollow3 => "LightMusicFollow3",
            LightMode::ScreenColor => "LightScreenColor",
            LightMode::MusicFollow2 => "LightMusicFollow2",
            LightMode::Train => "LightTrain",
            LightMode::Fireworks => "LightFireWorks",
            LightMode::UserColor => "LightUserColor",
        }
    }

    pub fn from_vendor_name(name: &str) -> Option<LightMode> {
        LightMode::ALL.into_iter().find(|m| m.vendor_name() == name)
    }

    /// Human readable name.
    pub fn label(self) -> &'static str {
        match self {
            LightMode::Off => "Off",
            LightMode::AlwaysOn => "Static",
            LightMode::Breath => "Breathing",
            LightMode::Neon => "Neon (spectrum)",
            LightMode::Wave => "Wave",
            LightMode::Ripple => "Ripple",
            LightMode::Raindrop => "Raindrop",
            LightMode::Snake => "Snake",
            LightMode::PressAction => "Reactive (light on press)",
            LightMode::Converge => "Converge",
            LightMode::SineWave => "Sine wave",
            LightMode::Kaleidoscope => "Kaleidoscope",
            LightMode::LineWave => "Line wave",
            LightMode::UserPicture => "Custom per-key",
            LightMode::Laser => "Laser",
            LightMode::CircleWave => "Circle wave",
            LightMode::Dazzling => "Dazzling",
            LightMode::RainDown => "Rain down",
            LightMode::Meteor => "Meteor",
            LightMode::PressActionOff => "Reactive (light off on press)",
            LightMode::MusicFollow3 => "Music follow 1",
            LightMode::ScreenColor => "Screen color",
            LightMode::MusicFollow2 => "Music follow 2",
            LightMode::Train => "Train",
            LightMode::Fireworks => "Fireworks",
            LightMode::UserColor => "User color",
        }
    }

    /// Names of the selectable options, in wire order (index = value of the high nibble).
    pub fn options(self) -> &'static [&'static str] {
        match self {
            LightMode::Neon => &["Default", "Random"],
            LightMode::Wave => &["right", "left", "down", "up"],
            LightMode::Ripple => &["full", "single"],
            LightMode::Snake => &["z", "return"],
            LightMode::PressAction => &["onToOff", "offToOn"],
            LightMode::Kaleidoscope => &["out", "in"],
            LightMode::LineWave | LightMode::Fireworks => &["right", "left"],
            LightMode::CircleWave => &["anti-clockwise", "clockwise"],
            LightMode::UserPicture => &["1", "2", "3", "4", "5"],
            LightMode::MusicFollow3 | LightMode::MusicFollow2 => &["upright", "separate", "intersect"],
            _ => &[],
        }
    }

    fn is_music(self) -> bool {
        matches!(self, LightMode::MusicFollow2 | LightMode::MusicFollow3)
    }
}

/// An RGB color stored as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u32);

impl Rgb {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }
    pub fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn b(self) -> u8 {
        self.0 as u8
    }
    pub fn to_array(self) -> [u8; 3] {
        [self.r(), self.g(), self.b()]
    }
}

/// The complete state of a backlight effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightEffect {
    pub mode: LightMode,
    /// 0 (slowest) ..= 4 (fastest)
    pub speed: u8,
    /// 0 (dimmest) ..= 4 (brightest)
    pub brightness: u8,
    /// Index into [`LightMode::options`]; ignored when the mode has none.
    pub option: u8,
    /// Fixed color, used when `dazzle` is false and the mode supports color.
    pub color: Rgb,
    /// Rainbow / color-cycling variant of the effect.
    pub dazzle: bool,
}

impl Default for LightEffect {
    fn default() -> Self {
        LightEffect { mode: LightMode::AlwaysOn, speed: 2, brightness: 4, option: 0, color: Rgb(WHITE), dazzle: false }
    }
}

impl LightEffect {
    pub fn off() -> Self {
        LightEffect { mode: LightMode::Off, speed: 0, brightness: 0, option: 0, color: Rgb(0), dazzle: false }
    }

    /// Encodes `SET_LED` exactly as the vendor app (YC200Common.setLightSetting) does.
    pub fn encode(&self) -> Frame {
        let color_mode = if self.dazzle { COLOR_DAZZLE } else { COLOR_FIXED };
        let music_flag = if self.dazzle { 0 } else { 4 };
        let opt = self.option.min(15);
        let speed = self.speed.min(MAX_SPEED);
        let brightness = self.brightness.min(MAX_BRIGHTNESS);

        // (uses_speed, brightness, flags byte)
        let (uses_speed, value, flags) = match self.mode {
            LightMode::Off => (false, 0, COLOR_FIXED),
            LightMode::AlwaysOn => (false, brightness, color_mode),
            LightMode::Neon => (true, brightness, COLOR_FIXED),
            LightMode::Wave => (true, brightness, (opt << 4) | color_mode),
            LightMode::Snake
            | LightMode::Kaleidoscope
            | LightMode::LineWave
            | LightMode::CircleWave
            | LightMode::Fireworks => (true, brightness, (if opt == 0 { 0 } else { 16 }) | color_mode),
            LightMode::UserPicture => (false, brightness, user_picture_flags(opt)),
            LightMode::MusicFollow3 | LightMode::MusicFollow2 => (false, brightness, (opt << 4) | music_flag),
            LightMode::ScreenColor => (false, 4, COLOR_FIXED),
            LightMode::UserColor => (false, 0, COLOR_FIXED),
            _ => (true, brightness, color_mode),
        };
        let speed_byte = MAX_SPEED - if uses_speed { speed } else { 0 };

        let mut r = report_from(&[cmd::SET_LED, self.mode.code(), speed_byte, value, flags]);
        if self.mode.uses_color() {
            let c = if self.color.0 == WHITE { Rgb(WHITE_ON_WIRE) } else { self.color };
            r[5..8].copy_from_slice(&c.to_array());
        }
        if self.mode == LightMode::UserPicture {
            r[5..8].copy_from_slice(&[0, 200, 200]);
        }
        Frame::new(r, Checksum::Bit8)
    }

    /// Decodes a `GET_LED` response the way the vendor app interprets it.
    pub fn decode(resp: &Report) -> Option<LightEffect> {
        let mode = LightMode::from_code(resp[1])?;
        let speed = MAX_SPEED.wrapping_sub(resp[2]);
        let brightness = resp[3];
        let flags = resp[4];
        let option = flags >> 4;
        let color_mode = flags & 0x0f;
        let mut color = Rgb::new(resp[5], resp[6], resp[7]);
        if color.0 == WHITE_ON_WIRE {
            color = Rgb(WHITE);
        }
        // Firmware presets: low nibble 0..=6 selects one of seven predefined colors.
        let preset = |color: Rgb| match color_mode {
            0 => Rgb(0xFF0000),
            1 => Rgb(0xFF8000),
            2 => Rgb(0xFFFF00),
            3 => Rgb(0x00FF00),
            4 => Rgb(0x00FFFF),
            5 => Rgb(0x0000FF),
            6 => Rgb(0xFF00FF),
            _ => color,
        };
        let dazzle = color_mode == COLOR_DAZZLE;
        let mut e = LightEffect { mode, speed, brightness, option: 0, color: Rgb(0), dazzle: false };
        match mode {
            LightMode::Off | LightMode::UserColor => {
                e.speed = 0;
                e.brightness = 0;
            }
            LightMode::AlwaysOn => {
                e.speed = 0;
                e.color = preset(color);
                e.dazzle = dazzle;
            }
            LightMode::Neon => e.option = u8::from(option != 0),
            LightMode::Wave => {
                e.option = if option < 4 { option } else { 0 };
                e.color = preset(color);
                e.dazzle = dazzle;
            }
            LightMode::Ripple
            | LightMode::Snake
            | LightMode::PressAction
            | LightMode::Kaleidoscope
            | LightMode::LineWave
            | LightMode::CircleWave
            | LightMode::Fireworks => {
                e.option = u8::from(option != 0);
                e.color = preset(color);
                e.dazzle = dazzle;
            }
            LightMode::UserPicture => {
                e.speed = 0;
                e.option = match flags {
                    0 => 0,
                    16 => 1,
                    32 => 2,
                    48 => 3,
                    64 => 4,
                    _ => 0,
                };
            }
            LightMode::MusicFollow3 | LightMode::MusicFollow2 => {
                e.speed = 0;
                e.option = if option < 3 { option } else { 0 };
                e.color = Rgb::new(resp[5], resp[6], resp[7]);
                // the music modes use 0 as "rainbow" marker in the low nibble
                e.dazzle = color_mode == 0;
            }
            LightMode::ScreenColor => {
                e.speed = 0;
                e.brightness = 4;
            }
            _ => {
                e.color = preset(color);
                e.dazzle = dazzle;
            }
        }
        Some(e)
    }

    pub fn query() -> Frame {
        crate::protocol::query(cmd::GET_LED, 0, 0)
    }
}

impl LightMode {
    /// Whether the effect carries a fixed color in bytes 5..8.
    pub fn uses_color(self) -> bool {
        !matches!(self, LightMode::Off | LightMode::Neon | LightMode::ScreenColor | LightMode::UserColor)
    }

    /// Whether the effect has an adjustable speed.
    pub fn uses_speed(self) -> bool {
        !matches!(
            self,
            LightMode::Off
                | LightMode::AlwaysOn
                | LightMode::UserPicture
                | LightMode::MusicFollow2
                | LightMode::MusicFollow3
                | LightMode::ScreenColor
                | LightMode::UserColor
        )
    }

    /// Music modes are driven by the host streaming audio levels; they are not autonomous.
    pub fn needs_host(self) -> bool {
        self.is_music() || self == LightMode::ScreenColor
    }
}

fn user_picture_flags(opt: u8) -> u8 {
    // options "1".."5" are stored as multiples of 16 (the vendor's USEROP table)
    (opt.min(4)) * 16
}

/// Brightness/speed capability of one effect on one keyboard model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightModeCaps {
    pub mode: LightMode,
    pub options: Vec<String>,
    pub max_speed: Option<u8>,
    pub max_brightness: Option<u8>,
    pub rgb: bool,
    pub dazzle: bool,
}
