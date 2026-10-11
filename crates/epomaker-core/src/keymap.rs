//! Key matrix: what every physical key does.
//!
//! The firmware stores a layer as `N` 4-byte entries (column-major over the switch
//! matrix). Each entry encodes one action:
//!
//! | bytes            | action                                                    |
//! |------------------|-----------------------------------------------------------|
//! | `00 00 00 00`    | disabled                                                  |
//! | `00 MM KK K2`    | key `KK` (+ optional second key `K2`) with modifier `MM`  |
//! | `01 00 BB XX`    | mouse button / wheel                                      |
//! | `03 00 LL HH`    | consumer control usage `0xHHLL` (media keys…)             |
//! | `09 TT II 00`    | macro `II`, trigger mode `TT`                             |
//! | `0A ..`, `12 ..` | firmware functions (Fn key, layer toggles, Siri…)         |

use crate::protocol::{cmd, report_from, Checksum, Frame, REPORT_LEN};
use serde::{Deserialize, Serialize};

/// Modifier allowed in byte 1 of a key entry (single modifier, HID usage code).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    #[default]
    None,
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Modifier {
    pub const ALL: [Modifier; 5] = [Modifier::None, Modifier::Ctrl, Modifier::Shift, Modifier::Alt, Modifier::Win];

    pub fn hid(self) -> u8 {
        match self {
            Modifier::None => 0,
            Modifier::Ctrl => 0xE0,
            Modifier::Shift => 0xE1,
            Modifier::Alt => 0xE2,
            Modifier::Win => 0xE3,
        }
    }

    pub fn from_hid(code: u8) -> Option<Modifier> {
        Modifier::ALL.into_iter().find(|m| m.hid() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Modifier::None => "None",
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
            Modifier::Win => "Win/Super",
        }
    }
}

/// Mouse actions available for remapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseAction {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelForward,
    WheelBack,
    WheelLeft,
    WheelRight,
    ScrollUp,
    ScrollDown,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
}

impl MouseAction {
    pub const ALL: [MouseAction; 15] = [
        MouseAction::Left,
        MouseAction::Right,
        MouseAction::Middle,
        MouseAction::Back,
        MouseAction::Forward,
        MouseAction::WheelForward,
        MouseAction::WheelBack,
        MouseAction::WheelLeft,
        MouseAction::WheelRight,
        MouseAction::ScrollUp,
        MouseAction::ScrollDown,
        MouseAction::MoveLeft,
        MouseAction::MoveRight,
        MouseAction::MoveUp,
        MouseAction::MoveDown,
    ];

    pub fn bytes(self) -> [u8; 4] {
        match self {
            MouseAction::Left => [1, 0, 240, 0],
            MouseAction::Right => [1, 0, 241, 0],
            MouseAction::Middle => [1, 0, 242, 0],
            MouseAction::Forward => [1, 0, 243, 0],
            MouseAction::Back => [1, 0, 244, 0],
            MouseAction::WheelLeft => [1, 0, 245, 0],
            MouseAction::WheelRight => [1, 0, 246, 0],
            MouseAction::WheelForward => [1, 0, 247, 0],
            MouseAction::WheelBack => [1, 0, 248, 0],
            MouseAction::ScrollUp => [1, 0, 245, 1],
            MouseAction::ScrollDown => [1, 0, 245, 255],
            MouseAction::MoveLeft => [1, 0, 246, 251],
            MouseAction::MoveRight => [1, 0, 246, 5],
            MouseAction::MoveUp => [1, 0, 247, 251],
            MouseAction::MoveDown => [1, 0, 247, 5],
        }
    }

    pub fn from_bytes(b: [u8; 4]) -> Option<MouseAction> {
        MouseAction::ALL.into_iter().find(|m| m.bytes() == b)
    }

    /// Single-byte code used inside macros (third byte of [`MouseAction::bytes`]).
    pub fn macro_code(self) -> u8 {
        self.bytes()[2]
    }

    pub fn label(self) -> &'static str {
        match self {
            MouseAction::Left => "Mouse left",
            MouseAction::Right => "Mouse right",
            MouseAction::Middle => "Mouse middle",
            MouseAction::Back => "Mouse back",
            MouseAction::Forward => "Mouse forward",
            MouseAction::WheelForward => "Wheel forward",
            MouseAction::WheelBack => "Wheel back",
            MouseAction::WheelLeft => "Wheel left",
            MouseAction::WheelRight => "Wheel right",
            MouseAction::ScrollUp => "Scroll up",
            MouseAction::ScrollDown => "Scroll down",
            MouseAction::MoveLeft => "Move left",
            MouseAction::MoveRight => "Move right",
            MouseAction::MoveUp => "Move up",
            MouseAction::MoveDown => "Move down",
        }
    }
}

/// How a macro bound to a key is triggered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MacroMode {
    /// Play `repeat_count` times on press.
    #[default]
    RepeatTimes,
    /// Toggle: first press starts looping, second press stops.
    OnOff,
    /// Loop while the key is held.
    WhileHeld,
}

impl MacroMode {
    pub const ALL: [MacroMode; 3] = [MacroMode::RepeatTimes, MacroMode::OnOff, MacroMode::WhileHeld];

    pub fn code(self) -> u8 {
        match self {
            MacroMode::RepeatTimes => 0,
            MacroMode::OnOff => 1,
            MacroMode::WhileHeld => 2,
        }
    }

    pub fn from_code(c: u8) -> MacroMode {
        match c {
            1 => MacroMode::OnOff,
            2 => MacroMode::WhileHeld,
            _ => MacroMode::RepeatTimes,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MacroMode::RepeatTimes => "Repeat N times",
            MacroMode::OnOff => "Toggle loop",
            MacroMode::WhileHeld => "Loop while held",
        }
    }
}

/// Built-in firmware/consumer functions (the vendor's "special function" table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecialFn {
    PrevTrack,
    NextTrack,
    Stop,
    PlayPause,
    MediaPlayer,
    Mute,
    VolumeDown,
    VolumeUp,
    Calculator,
    Mail,
    MyComputer,
    WwwSearch,
    WwwHome,
    WwwBack,
    WwwRefresh,
    BrightnessDown,
    BrightnessUp,
    Fn,
    RightFn,
    FnLock,
    Siri,
    ParenLeft,
    ParenRight,
    BraceLeft,
    BraceRight,
    SwitchInputMethod,
    ZoomOut,
    ZoomIn,
    LockScreen,
    VolumeOrBacklight,
    VolumeOrBacklight2,
    VolumeOrBacklight3,
    RapidFire,
}

impl SpecialFn {
    pub const ALL: [SpecialFn; 33] = [
        SpecialFn::PrevTrack,
        SpecialFn::NextTrack,
        SpecialFn::Stop,
        SpecialFn::PlayPause,
        SpecialFn::MediaPlayer,
        SpecialFn::Mute,
        SpecialFn::VolumeDown,
        SpecialFn::VolumeUp,
        SpecialFn::Calculator,
        SpecialFn::Mail,
        SpecialFn::MyComputer,
        SpecialFn::WwwSearch,
        SpecialFn::WwwHome,
        SpecialFn::WwwBack,
        SpecialFn::WwwRefresh,
        SpecialFn::BrightnessDown,
        SpecialFn::BrightnessUp,
        SpecialFn::Fn,
        SpecialFn::RightFn,
        SpecialFn::FnLock,
        SpecialFn::Siri,
        SpecialFn::ParenLeft,
        SpecialFn::ParenRight,
        SpecialFn::BraceLeft,
        SpecialFn::BraceRight,
        SpecialFn::SwitchInputMethod,
        SpecialFn::ZoomOut,
        SpecialFn::ZoomIn,
        SpecialFn::LockScreen,
        SpecialFn::VolumeOrBacklight,
        SpecialFn::VolumeOrBacklight2,
        SpecialFn::VolumeOrBacklight3,
        SpecialFn::RapidFire,
    ];

    pub fn bytes(self) -> [u8; 4] {
        match self {
            SpecialFn::PrevTrack => [3, 0, 182, 0],
            SpecialFn::NextTrack => [3, 0, 181, 0],
            SpecialFn::Stop => [3, 0, 183, 0],
            SpecialFn::PlayPause => [3, 0, 205, 0],
            SpecialFn::MediaPlayer => [3, 0, 131, 1],
            SpecialFn::Mute => [3, 0, 226, 0],
            SpecialFn::VolumeDown => [3, 0, 234, 0],
            SpecialFn::VolumeUp => [3, 0, 233, 0],
            SpecialFn::Calculator => [3, 0, 146, 1],
            SpecialFn::Mail => [3, 0, 138, 1],
            SpecialFn::MyComputer => [3, 0, 148, 1],
            SpecialFn::WwwSearch => [3, 0, 33, 2],
            SpecialFn::WwwHome => [3, 0, 35, 2],
            SpecialFn::WwwBack => [3, 0, 36, 2],
            SpecialFn::WwwRefresh => [3, 0, 39, 2],
            SpecialFn::BrightnessDown => [3, 0, 112, 0],
            SpecialFn::BrightnessUp => [3, 0, 111, 0],
            SpecialFn::Fn => [10, 1, 0, 0],
            SpecialFn::RightFn => [10, 1, 1, 0],
            SpecialFn::FnLock => [10, 13, 0, 0],
            SpecialFn::Siri => [18, 0, 227, 44],
            SpecialFn::ParenLeft => [0, 0, 229, 38],
            SpecialFn::ParenRight => [0, 0, 229, 39],
            SpecialFn::BraceLeft => [0, 225, 47, 0],
            SpecialFn::BraceRight => [0, 225, 48, 0],
            SpecialFn::SwitchInputMethod => [0, 224, 44, 0],
            SpecialFn::ZoomOut => [0, 0, 227, 45],
            SpecialFn::ZoomIn => [0, 0, 227, 46],
            SpecialFn::LockScreen => [0, 0, 227, 15],
            SpecialFn::VolumeOrBacklight => [10, 15, 0, 0],
            SpecialFn::VolumeOrBacklight2 => [10, 13, 0, 0],
            SpecialFn::VolumeOrBacklight3 => [10, 14, 0, 0],
            SpecialFn::RapidFire => [11, 0, 0, 0],
        }
    }

    /// Name used by the (Chinese) vendor software.
    pub fn vendor_name(self) -> &'static str {
        match self {
            SpecialFn::PrevTrack => "上一曲",
            SpecialFn::NextTrack => "下一曲",
            SpecialFn::Stop => "停止",
            SpecialFn::PlayPause => "播放/暂停",
            SpecialFn::MediaPlayer => "播放器",
            SpecialFn::Mute => "静音",
            SpecialFn::VolumeDown => "音量减",
            SpecialFn::VolumeUp => "音量大",
            SpecialFn::Calculator => "计算器",
            SpecialFn::Mail => "邮件",
            SpecialFn::MyComputer => "我的电脑",
            SpecialFn::WwwSearch => "搜索",
            SpecialFn::WwwHome => "主页",
            SpecialFn::WwwBack => "返回",
            SpecialFn::WwwRefresh => "刷新",
            SpecialFn::BrightnessDown => "亮度减",
            SpecialFn::BrightnessUp => "亮度加",
            SpecialFn::Fn => "fn",
            SpecialFn::RightFn => "r_FN",
            SpecialFn::FnLock => "fn锁屏",
            SpecialFn::Siri => "呼出siri",
            SpecialFn::ParenLeft => "(",
            SpecialFn::ParenRight => ")",
            SpecialFn::BraceLeft => "{",
            SpecialFn::BraceRight => "}",
            SpecialFn::SwitchInputMethod => "切换输入法",
            SpecialFn::ZoomOut => "缩小",
            SpecialFn::ZoomIn => "放大",
            SpecialFn::LockScreen => "锁屏",
            SpecialFn::VolumeOrBacklight => "音量<->键盘亮度",
            SpecialFn::VolumeOrBacklight2 => "音量<->键盘亮度2",
            SpecialFn::VolumeOrBacklight3 => "音量<->键盘亮度3",
            SpecialFn::RapidFire => "火力",
        }
    }

    pub fn from_vendor_name(name: &str) -> Option<SpecialFn> {
        SpecialFn::ALL.into_iter().find(|f| f.vendor_name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            SpecialFn::PrevTrack => "Previous track",
            SpecialFn::NextTrack => "Next track",
            SpecialFn::Stop => "Stop",
            SpecialFn::PlayPause => "Play/Pause",
            SpecialFn::MediaPlayer => "Media player",
            SpecialFn::Mute => "Mute",
            SpecialFn::VolumeDown => "Volume down",
            SpecialFn::VolumeUp => "Volume up",
            SpecialFn::Calculator => "Calculator",
            SpecialFn::Mail => "Mail",
            SpecialFn::MyComputer => "My computer",
            SpecialFn::WwwSearch => "Web search",
            SpecialFn::WwwHome => "Web home",
            SpecialFn::WwwBack => "Web back",
            SpecialFn::WwwRefresh => "Web refresh",
            SpecialFn::BrightnessDown => "Screen brightness down",
            SpecialFn::BrightnessUp => "Screen brightness up",
            SpecialFn::Fn => "Fn",
            SpecialFn::RightFn => "Right Fn",
            SpecialFn::FnLock => "Fn lock",
            SpecialFn::Siri => "Siri",
            SpecialFn::ParenLeft => "(",
            SpecialFn::ParenRight => ")",
            SpecialFn::BraceLeft => "{",
            SpecialFn::BraceRight => "}",
            SpecialFn::SwitchInputMethod => "Switch input method",
            SpecialFn::ZoomOut => "Zoom out",
            SpecialFn::ZoomIn => "Zoom in",
            SpecialFn::LockScreen => "Lock screen",
            SpecialFn::VolumeOrBacklight => "Knob: volume/backlight",
            SpecialFn::VolumeOrBacklight2 => "Knob: volume/backlight 2",
            SpecialFn::VolumeOrBacklight3 => "Knob: volume/backlight 3",
            SpecialFn::RapidFire => "Rapid fire",
        }
    }

    /// Entries whose first byte the vendor app classifies as a "function" when reading.
    fn is_function_class(first: u8) -> bool {
        matches!(first, 3 | 8 | 10 | 13 | 14 | 18 | 19)
    }

    pub fn from_bytes(b: [u8; 4]) -> Option<SpecialFn> {
        SpecialFn::ALL.into_iter().find(|f| f.bytes() == b)
    }
}

/// What a single key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAction {
    Disabled,
    /// A keyboard key (HID usage), optionally with a modifier and a second key.
    Key {
        modifier: Modifier,
        key: u8,
        key2: u8,
    },
    Mouse(MouseAction),
    Function(SpecialFn),
    Macro {
        index: u8,
        mode: MacroMode,
    },
    /// Anything this implementation does not model; written back verbatim.
    Raw([u8; 4]),
}

impl KeyAction {
    pub fn key(code: u8) -> Self {
        KeyAction::Key { modifier: Modifier::None, key: code, key2: 0 }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        match self {
            KeyAction::Disabled => [0, 0, 0, 0],
            KeyAction::Key { modifier, key, key2 } => [0, modifier.hid(), key, key2],
            KeyAction::Mouse(m) => m.bytes(),
            KeyAction::Function(f) => f.bytes(),
            KeyAction::Macro { index, mode } => [9, mode.code(), index, 0],
            KeyAction::Raw(b) => b,
        }
    }

    /// Classifies an entry following the vendor's `matrixToConfigValues` rules.
    pub fn from_bytes(b: [u8; 4]) -> KeyAction {
        if b == [0, 0, 0, 0] {
            return KeyAction::Disabled;
        }
        if b == [0, 0, 1, 0] {
            return KeyAction::Raw(b);
        }
        match b[0] {
            0 => {
                if let Some(f) = SpecialFn::from_bytes(b) {
                    return KeyAction::Function(f);
                }
                match Modifier::from_hid(b[1]) {
                    Some(modifier) => KeyAction::Key { modifier, key: b[2], key2: b[3] },
                    None => KeyAction::Raw(b),
                }
            }
            1 => MouseAction::from_bytes(b).map(KeyAction::Mouse).unwrap_or(KeyAction::Raw(b)),
            9 => KeyAction::Macro { index: b[2], mode: MacroMode::from_code(b[1]) },
            x if SpecialFn::is_function_class(x) || x == 11 => {
                SpecialFn::from_bytes(b).map(KeyAction::Function).unwrap_or(KeyAction::Raw(b))
            }
            _ => KeyAction::Raw(b),
        }
    }
}

/// A full layer as stored by the firmware.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMatrix {
    pub entries: Vec<[u8; 4]>,
}

impl KeyMatrix {
    pub fn from_bytes(bytes: &[u8]) -> KeyMatrix {
        KeyMatrix { entries: bytes.as_chunks::<4>().0.to_vec() }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.entries.iter().flatten().copied().collect()
    }

    pub fn action(&self, index: usize) -> Option<KeyAction> {
        self.entries.get(index).map(|b| KeyAction::from_bytes(*b))
    }

    pub fn set(&mut self, index: usize, action: KeyAction) {
        if let Some(e) = self.entries.get_mut(index) {
            *e = action.to_bytes();
        }
    }

    /// Macro slots referenced by this layer.
    pub fn macro_indices(&self) -> impl Iterator<Item = u8> + '_ {
        self.entries.iter().filter(|e| e[0] == 9).map(|e| e[2])
    }
}

/// Which layer a key write targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Layer {
    /// Base layer of an onboard profile (0..=2).
    Profile(u8),
    /// Fn layer (0-based).
    Fn(u8),
}

/// Remaps one key (vendor `setKeyConfigSimple` / `setFnKeyConfigSimple`).
pub fn encode_set_key(layer: Layer, matrix_index: u8, action: KeyAction) -> Frame {
    let (command, arg) = match layer {
        Layer::Profile(p) => (cmd::SET_KEY_SIMPLE, p),
        Layer::Fn(f) => (cmd::SET_FN_SIMPLE, f),
    };
    let mut r = report_from(&[command, arg, matrix_index]);
    r[8..12].copy_from_slice(&action.to_bytes());
    Frame::new(r, Checksum::Bit7)
}

/// Bytes of matrix data carried per packet in a full-layer write.
pub const MATRIX_WRITE_CHUNK: usize = 56;
/// Packets in a full-layer write (the last 8 bytes of a 512-byte layer are never sent).
pub const MATRIX_WRITE_PACKETS: usize = 9;
/// Packets in a full-layer read (64 bytes each).
pub const MATRIX_READ_PACKETS: usize = 8;

/// Writes a whole layer at once (vendor `_setKeyConfig` / `_setFnKeyConfig`).
/// The checksum is computed in the header by the caller, the driver adds none.
pub fn encode_write_matrix(layer: Layer, matrix: &[u8]) -> Vec<Frame> {
    let (command, arg) = match layer {
        Layer::Profile(p) => (cmd::SET_KEY_MATRIX, p),
        Layer::Fn(f) => (cmd::SET_FN_MATRIX, f),
    };
    (0..MATRIX_WRITE_PACKETS)
        .map(|i| {
            let mut r = [0u8; REPORT_LEN];
            r[..5].copy_from_slice(&[command, arg, 248, 1, i as u8]);
            Checksum::Bit7.apply(&mut r);
            let start = (i * MATRIX_WRITE_CHUNK).min(matrix.len());
            let end = (start + MATRIX_WRITE_CHUNK).min(matrix.len());
            r[8..8 + (end - start)].copy_from_slice(&matrix[start..end]);
            Frame::new(r, Checksum::None)
        })
        .collect()
}

/// Queries for reading a whole layer; concatenate the 8 responses.
pub fn encode_read_matrix(layer: Layer) -> Vec<Frame> {
    let (command, arg) = match layer {
        Layer::Profile(p) => (cmd::GET_KEY_MATRIX, p),
        Layer::Fn(f) => (cmd::GET_FN_MATRIX, f),
    };
    (0..MATRIX_READ_PACKETS).map(|i| crate::protocol::query(command, arg, i as u8)).collect()
}
