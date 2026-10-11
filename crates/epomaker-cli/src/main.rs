//! `epomaker` — command line configuration tool for EPOMAKER keyboards.

use clap::{Parser, Subcommand};
use epomaker_core::keycodes;
use epomaker_core::keymap::{KeyAction, Layer, MacroMode, Modifier, MouseAction, SpecialFn};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use epomaker_core::macros::{Macro, MacroEvent};
use epomaker_core::profile_file::Backup;
use epomaker_core::settings::{ReportRate, SleepTimes};
use epomaker_core::{Error, Keyboard};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "epomaker", version, about = "Configure EPOMAKER keyboards (TH80 family) without the vendor software")]
struct Cli {
    /// Use an emulated keyboard instead of real hardware.
    #[arg(long, global = true)]
    demo: bool,
    /// Model id emulated by --demo (1118 = TH80, 1091 = TH80 SOC, 886 = TH80 SE, 928 = TH80 X, 741 = TH80 UK,
    /// 834 = TH96, 988 = TH68, 789 = TH21, 1081 = EP84).
    #[arg(long, global = true, default_value_t = 1118)]
    demo_model: i64,
    /// Open this HID path (see `probe`) instead of auto-detecting.
    #[arg(long, global = true)]
    device: Option<String>,
    /// Print every report sent/received.
    #[arg(long, global = true)]
    trace: bool,
    /// Read every write back and fail if the keyboard did not store it.
    #[arg(long, global = true)]
    verify: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List detected keyboards.
    List,
    /// Read-only diagnostics: sends only GET commands and prints the raw answers.
    /// Safe first step with a keyboard that was never tested with this tool.
    Probe,
    /// Hardware self-test: read-only checks plus small writes that are undone right
    /// away (brightness, one key of profile 3, an unused macro slot). Prints a Markdown
    /// report to paste into a GitHub issue.
    Selftest {
        #[arg(long)]
        yes: bool,
    },
    /// Show model, firmware and current settings.
    Info,
    /// Backlight effect.
    Light {
        #[command(subcommand)]
        cmd: LightCmd,
    },
    /// Active onboard profile (0..=2).
    Profile { profile: Option<u8> },
    /// USB polling rate in Hz (125, 250, 500, 1000).
    Rate {
        hz: Option<u16>,
        #[arg(long, default_value_t = 0)]
        profile: u8,
    },
    /// Keyboard options (per profile).
    Options {
        #[arg(long, default_value_t = 0)]
        profile: u8,
        /// Disable the Windows key.
        #[arg(long)]
        win_lock: Option<bool>,
        /// Mac layout.
        #[arg(long)]
        mac: Option<bool>,
        /// Swap WASD and arrows.
        #[arg(long)]
        wasd_swap: Option<bool>,
        /// Swap F-keys and multimedia keys on the Fn layer.
        #[arg(long)]
        fn_swap: Option<bool>,
        /// Power saving (wireless).
        #[arg(long)]
        power_save: Option<bool>,
    },
    /// Key debounce in ms.
    Debounce { ms: Option<u8> },
    /// Wireless sleep timers in seconds.
    Sleep {
        #[arg(long)]
        bt_light: Option<u16>,
        #[arg(long)]
        dongle_light: Option<u16>,
        #[arg(long)]
        bt_deep: Option<u16>,
        #[arg(long)]
        dongle_deep: Option<u16>,
    },
    /// Key remapping.
    Keymap {
        #[command(subcommand)]
        cmd: KeymapCmd,
    },
    /// Macros.
    Macro {
        #[command(subcommand)]
        cmd: MacroCmd,
    },
    /// Per-key colors (custom light effect).
    Color {
        #[command(subcommand)]
        cmd: ColorCmd,
    },
    /// Save the whole configuration to a JSON file.
    Backup { file: String },
    /// Restore a configuration saved with `backup`.
    Restore { file: String },
    /// Factory reset (all profiles, keymaps, macros, lighting).
    FactoryReset {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum LightCmd {
    /// Show the current effect.
    Get,
    /// List the effects supported by the keyboard.
    Modes,
    /// Set an effect, e.g. `light set wave --speed 3 --option left --color ff0080`.
    Set {
        mode: String,
        #[arg(long, default_value_t = 2)]
        speed: u8,
        #[arg(long, default_value_t = 4)]
        brightness: u8,
        #[arg(long)]
        option: Option<String>,
        /// RRGGBB
        #[arg(long, default_value = "ffffff")]
        color: String,
        /// Rainbow colors.
        #[arg(long)]
        dazzle: bool,
    },
}

#[derive(Subcommand)]
enum KeymapCmd {
    /// Show the keys that differ from the factory layout (or all with --all).
    Show {
        #[arg(long, default_value = "p0")]
        layer: String,
        #[arg(long)]
        all: bool,
    },
    /// Remap a key: `keymap set Caps disable`, `keymap set A key:ctrl+c`, `keymap set F1 fn:mute`.
    Set {
        /// Key label as printed on the keycap, or `#<matrix index>`.
        key: String,
        /// disable | default | key:[mod+]KEY[+KEY2] | mouse:NAME | fn:NAME | macro:SLOT[:repeat|toggle|hold] | raw:XXXXXXXX
        action: String,
        #[arg(long, default_value = "p0")]
        layer: String,
    },
    /// Restore the factory mapping of a layer.
    Reset {
        #[arg(long, default_value = "p0")]
        layer: String,
    },
    /// List action names accepted by `set`.
    Actions,
}

#[derive(Subcommand)]
enum MacroCmd {
    /// Show a macro slot.
    Show { slot: u8 },
    /// Store a macro: `macro set 0 "down:LeftShift down:A delay:30 up:A up:LeftShift"`.
    Set {
        slot: u8,
        events: String,
        #[arg(long, default_value_t = 1)]
        repeat: u16,
    },
    /// Slots referenced by key bindings.
    Used,
}

#[derive(Subcommand)]
enum ColorCmd {
    /// Show per-key colors.
    Show,
    /// Set one key: `color set Esc ff0000`.
    Set {
        key: String,
        color: String,
        /// Custom picture slot (0..=2).
        #[arg(long, default_value_t = 0)]
        slot: u8,
    },
    /// Set every key to one color.
    Fill { color: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let level = if cli.trace { "info" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).format_timestamp_millis().init();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open(cli: &Cli) -> Result<Keyboard, Error> {
    let mut kb = if cli.demo {
        Keyboard::emulated(cli.demo_model)?
    } else if let Some(path) = &cli.device {
        // any interface of the vendor is accepted, so other interfaces can be tried by hand
        let c = epomaker_core::hid::enumerate_vendor(0x3151)?
            .into_iter()
            .find(|c| &c.path == path)
            .ok_or(Error::NotFound)?;
        Keyboard::connect(Box::new(epomaker_core::hid::HidTransport::open(&c)?), false)?
    } else {
        Keyboard::open_first()?
    };
    kb.set_trace(cli.trace);
    kb.set_verify(cli.verify);
    Ok(kb)
}

fn run(cli: Cli) -> Result<(), Error> {
    if let Cmd::List = cli.command {
        let list = epomaker_core::hid::enumerate()?;
        if list.is_empty() {
            println!("No supported keyboard found.");
            #[cfg(target_os = "linux")]
            println!("If one is connected, check permissions (packaging/linux/70-epomaker.rules).");
        }
        for c in list {
            println!(
                "{:04x}:{:04x}  usage {:04x}/{:04x}  if{}  {} {}  {}",
                c.vid, c.pid, c.usage_page, c.usage, c.interface, c.manufacturer, c.product, c.path
            );
        }
        return Ok(());
    }
    if let Cmd::Probe = cli.command {
        epomaker_core::probe::run(|l| println!("{l}"))?;
        println!("\nPlease share this output when reporting results for a new keyboard.");
        return Ok(());
    }
    if let Cmd::Selftest { yes: false } = cli.command {
        return Err(Error::InvalidArgument(
            "the self-test briefly changes the brightness, one key of profile 3 and an unused macro slot, \
             then restores them; pass --yes to run it"
                .into(),
        ));
    }
    let mut kb = open(&cli)?;
    match cli.command {
        Cmd::List | Cmd::Probe => unreachable!(),
        Cmd::Selftest { .. } => selftest(&mut kb)?,
        Cmd::Info => info(&mut kb)?,
        Cmd::Light { cmd } => light(&mut kb, cmd)?,
        Cmd::Profile { profile } => match profile {
            Some(p) => kb.set_profile(p)?,
            None => println!("{}", kb.profile()?),
        },
        Cmd::Rate { hz, profile } => match hz {
            Some(hz) => {
                let r = ReportRate::from_hz(hz).ok_or_else(|| Error::InvalidArgument(format!("{hz} Hz")))?;
                kb.set_report_rate(profile, r)?
            }
            None => {
                println!("{}", kb.report_rate(profile)?.map(|r| format!("{} Hz", r.hz())).unwrap_or("unknown".into()))
            }
        },
        Cmd::Options { profile, win_lock, mac, wasd_swap, fn_swap, power_save } => {
            let mut o = kb.options(profile)?;
            let changed = [win_lock, mac, wasd_swap, fn_swap, power_save].iter().any(Option::is_some);
            if let Some(v) = win_lock {
                o.win_key_lock = v;
            }
            if let Some(v) = mac {
                o.mac_mode = v;
            }
            if let Some(v) = wasd_swap {
                o.wasd_arrows_swap = v;
            }
            if let Some(v) = fn_swap {
                o.fn_key_swap = v;
            }
            if let Some(v) = power_save {
                o.power_save = v;
            }
            if changed {
                kb.set_options(profile, &o)?;
            }
            println!("{o:#?}");
        }
        Cmd::Debounce { ms } => match ms {
            Some(ms) => kb.set_debounce(ms)?,
            None => println!("{} ms", kb.debounce()?),
        },
        Cmd::Sleep { bt_light, dongle_light, bt_deep, dongle_deep } => {
            let mut s: SleepTimes = kb.sleep_times()?;
            let changed = [bt_light, dongle_light, bt_deep, dongle_deep].iter().any(Option::is_some);
            s.bt_light_off = bt_light.unwrap_or(s.bt_light_off);
            s.dongle_light_off = dongle_light.unwrap_or(s.dongle_light_off);
            s.bt_deep_sleep = bt_deep.unwrap_or(s.bt_deep_sleep);
            s.dongle_deep_sleep = dongle_deep.unwrap_or(s.dongle_deep_sleep);
            if changed {
                kb.set_sleep_times(&s)?;
            }
            println!("{s:#?}");
        }
        Cmd::Keymap { cmd } => keymap(&mut kb, cmd)?,
        Cmd::Macro { cmd } => macros(&mut kb, cmd)?,
        Cmd::Color { cmd } => colors(&mut kb, cmd)?,
        Cmd::Backup { file } => {
            let b = Backup::capture(&mut kb)?;
            std::fs::write(&file, serde_json::to_string_pretty(&b)?)?;
            println!("Saved to {file}");
        }
        Cmd::Restore { file } => {
            let b: Backup = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
            b.restore(&mut kb)?;
            println!("Restored {file}");
        }
        Cmd::FactoryReset { yes } => {
            if !yes {
                return Err(Error::InvalidArgument("this erases every setting; pass --yes to confirm".into()));
            }
            kb.factory_reset()?;
            println!("Keyboard reset to factory settings");
        }
    }
    Ok(())
}

fn selftest(kb: &mut Keyboard) -> Result<(), Error> {
    use epomaker_core::selftest::{self, Outcome, Step};
    eprintln!("Self-test of {} ({})", kb.info.display_name, kb.describe());
    let device = selftest::summary(kb);
    let mut steps = Vec::new();
    for step in Step::ALL {
        eprint!("  {:<32} ", step.title());
        let r = selftest::run_step(kb, step);
        let restored = match r.restored {
            Some(true) => ", restored",
            Some(false) => ", NOT RESTORED",
            None => "",
        };
        match (&r.outcome, &r.error) {
            (Outcome::Passed, _) => eprintln!("ok{restored}"),
            (Outcome::Skipped, e) => eprintln!("skipped: {}", e.as_deref().unwrap_or_default()),
            (Outcome::Failed, e) => eprintln!("FAILED{restored}: {}", e.as_deref().unwrap_or_default()),
        }
        steps.push(r);
    }
    let report = selftest::SelfTestReport { device, steps };
    eprintln!("\nPaste the report below into a GitHub issue:\n");
    println!("{}", report.to_markdown(concat!("epomaker ", env!("CARGO_PKG_VERSION"))));
    if report.passed() {
        Ok(())
    } else {
        Err(Error::Protocol("self-test failed (see the report)".into()))
    }
}

fn info(kb: &mut Keyboard) -> Result<(), Error> {
    println!("Model:     {} ({}, id {})", kb.info.display_name, kb.info.name, kb.info.id);
    println!("Device:    {}", kb.describe());
    println!("Firmware:  {:#06x}", kb.firmware_version()?);
    let b = kb.battery()?;
    println!("Battery:   {}% ({:?})", b.percent, b.state);
    let p = kb.profile()?;
    println!("Profile:   {p}");
    println!("Light:     {}", kb.light()?.map(|e| describe_effect(&e)).unwrap_or("unknown".into()));
    println!("Rate:      {}", kb.report_rate(p)?.map(|r| format!("{} Hz", r.hz())).unwrap_or("unknown".into()));
    println!("Debounce:  {} ms", kb.debounce()?);
    println!("Options:   {:?}", kb.options(p)?);
    println!("Sleep:     {:?}", kb.sleep_times()?);
    Ok(())
}

fn describe_effect(e: &LightEffect) -> String {
    let mut s = format!("{} brightness {}", e.mode.label(), e.brightness);
    if e.mode.uses_speed() {
        s += &format!(" speed {}", e.speed);
    }
    if let Some(o) = e.mode.options().get(e.option as usize) {
        s += &format!(" option {o}");
    }
    if e.mode.uses_color() {
        s += &if e.dazzle { " rainbow".to_string() } else { format!(" color #{:06x}", e.color.0) };
    }
    s
}

fn parse_color(s: &str) -> Result<Rgb, Error> {
    u32::from_str_radix(s.trim_start_matches('#'), 16)
        .ok()
        .filter(|v| *v <= 0xFFFFFF)
        .map(Rgb)
        .ok_or_else(|| Error::InvalidArgument(format!("color {s} (use RRGGBB)")))
}

fn light(kb: &mut Keyboard, cmd: LightCmd) -> Result<(), Error> {
    match cmd {
        LightCmd::Get => println!("{}", kb.light()?.map(|e| describe_effect(&e)).unwrap_or("unknown".into())),
        LightCmd::Modes => {
            for c in &kb.model.light_modes {
                println!(
                    "{:<14} {:<30} options: {}{}",
                    format!("{:?}", c.mode).to_lowercase(),
                    c.mode.label(),
                    if c.options.is_empty() { "-".into() } else { c.options.join(", ") },
                    if c.rgb { "  [color]" } else { "" }
                );
            }
        }
        LightCmd::Set { mode, speed, brightness, option, color, dazzle } => {
            let m = LightMode::ALL
                .into_iter()
                .find(|m| format!("{m:?}").eq_ignore_ascii_case(&mode) || m.vendor_name().eq_ignore_ascii_case(&mode))
                .ok_or_else(|| Error::InvalidArgument(format!("unknown mode {mode} (see `light modes`)")))?;
            if m != LightMode::Off && kb.model.caps(m).is_none() {
                return Err(Error::InvalidArgument(format!("{mode} is not supported by this keyboard")));
            }
            let option =
                match option {
                    None => 0,
                    Some(o) => m.options().iter().position(|x| x.eq_ignore_ascii_case(&o)).ok_or_else(|| {
                        Error::InvalidArgument(format!("option {o}; valid: {}", m.options().join(", ")))
                    })? as u8,
                };
            let e = LightEffect { mode: m, speed, brightness, option, color: parse_color(&color)?, dazzle };
            kb.set_light(&e)?;
            println!("{}", describe_effect(&e));
        }
    }
    Ok(())
}

fn parse_layer(s: &str) -> Result<Layer, Error> {
    let bad = || Error::InvalidArgument(format!("layer {s} (use p0, p1, p2 or fn0)"));
    if let Some(n) = s.strip_prefix("fn") {
        return n.parse().map(Layer::Fn).map_err(|_| bad());
    }
    s.trim_start_matches('p').parse().map(Layer::Profile).map_err(|_| bad())
}

fn find_key(kb: &Keyboard, key: &str) -> Result<usize, Error> {
    if let Some(i) = key.strip_prefix('#') {
        return i.parse().map_err(|_| Error::InvalidArgument(key.into()));
    }
    let norm = |s: &str| s.to_lowercase().replace([' ', '\n', '_'], "");
    let want = norm(key);
    let exact = kb.model.keys.iter().find(|k| norm(&k.label) == want);
    let prefixed: Vec<_> = kb.model.keys.iter().filter(|k| norm(&k.label).starts_with(&want)).collect();
    let found = exact.or(if prefixed.len() == 1 { Some(prefixed[0]) } else { None });
    found.and_then(|k| k.matrix_index).ok_or_else(|| {
        let labels: Vec<String> = kb.model.keys.iter().map(|k| k.label.replace('\n', " ")).collect();
        Error::InvalidArgument(format!("no key labelled {key}; keys: {}", labels.join(", ")))
    })
}

fn key_label(kb: &Keyboard, index: usize) -> String {
    kb.model.key_for_matrix_index(index).map(|k| k.label.replace('\n', " ")).unwrap_or(format!("#{index}"))
}

fn describe_action(a: KeyAction) -> String {
    match a {
        KeyAction::Disabled => "disabled".into(),
        KeyAction::Key { modifier, key, key2 } => {
            let mut s = String::new();
            if modifier != Modifier::None {
                s += modifier.label();
                s += "+";
            }
            s += &keycodes::display(key);
            if key2 != 0 {
                s += "+";
                s += &keycodes::display(key2);
            }
            s
        }
        KeyAction::Mouse(m) => m.label().into(),
        KeyAction::Function(f) => f.label().into(),
        KeyAction::Macro { index, mode } => format!("macro {index} ({})", mode.label()),
        KeyAction::Raw(b) => format!("raw {:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3]),
    }
}

fn slug(s: &str) -> String {
    s.to_lowercase().replace(['/', ' ', ':'], "-")
}

fn parse_action(kb: &Keyboard, index: usize, s: &str) -> Result<KeyAction, Error> {
    let bad = || Error::InvalidArgument(format!("action {s}"));
    let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
    Ok(match kind.to_lowercase().as_str() {
        "disable" | "disabled" => KeyAction::Disabled,
        "default" => KeyAction::Raw(kb.model.default_entry(index).ok_or_else(bad)?),
        "key" => {
            let parts: Vec<&str> = rest.split('+').collect();
            let mut modifier = Modifier::None;
            let mut keys = Vec::new();
            for p in parts {
                if let Some(m) = Modifier::ALL
                    .into_iter()
                    .find(|m| m != &Modifier::None && m.label().split('/').any(|l| l.eq_ignore_ascii_case(p)))
                {
                    modifier = m;
                } else {
                    keys.push(keycodes::parse(p).ok_or_else(|| Error::InvalidArgument(format!("unknown key {p}")))?);
                }
            }
            match keys.as_slice() {
                [k] => KeyAction::Key { modifier, key: *k, key2: 0 },
                [k, k2] => KeyAction::Key { modifier, key: *k, key2: *k2 },
                _ => return Err(bad()),
            }
        }
        "mouse" => KeyAction::Mouse(
            MouseAction::ALL.into_iter().find(|m| slug(m.label()).ends_with(&slug(rest))).ok_or_else(bad)?,
        ),
        "fn" => {
            KeyAction::Function(SpecialFn::ALL.into_iter().find(|f| slug(f.label()) == slug(rest)).ok_or_else(bad)?)
        }
        "macro" => {
            let (slot, mode) = rest.split_once(':').unwrap_or((rest, "repeat"));
            let mode = match mode {
                "toggle" => MacroMode::OnOff,
                "hold" => MacroMode::WhileHeld,
                _ => MacroMode::RepeatTimes,
            };
            KeyAction::Macro { index: slot.parse().map_err(|_| bad())?, mode }
        }
        "raw" => {
            let v = u32::from_str_radix(rest, 16).map_err(|_| bad())?;
            KeyAction::Raw(v.to_be_bytes())
        }
        _ => return Err(bad()),
    })
}

fn keymap(kb: &mut Keyboard, cmd: KeymapCmd) -> Result<(), Error> {
    match cmd {
        KeymapCmd::Show { layer, all } => {
            let l = kb.read_layer(parse_layer(&layer)?)?;
            for (i, e) in l.entries.iter().enumerate() {
                let def = kb.model.default_entry(i).unwrap_or_default();
                if (all && def != [0; 4]) || *e != def {
                    let mark = if *e != def { "*" } else { " " };
                    println!("{mark} {:<14} -> {}", key_label(kb, i), describe_action(KeyAction::from_bytes(*e)));
                }
            }
        }
        KeymapCmd::Set { key, action, layer } => {
            let i = find_key(kb, &key)?;
            let a = parse_action(kb, i, &action)?;
            kb.set_key(parse_layer(&layer)?, i, a)?;
            println!("{} -> {}", key_label(kb, i), describe_action(a));
        }
        KeymapCmd::Reset { layer } => kb.reset_layer(parse_layer(&layer)?)?,
        KeymapCmd::Actions => {
            println!("disable | default | raw:XXXXXXXX");
            println!(
                "key:[ctrl|shift|alt|win+]KEY[+KEY2]   keys: {}",
                keycodes::all().map(|(_, n)| n).collect::<Vec<_>>().join(", ")
            );
            println!("mouse: {}", MouseAction::ALL.iter().map(|m| slug(m.label())).collect::<Vec<_>>().join(", "));
            println!("fn: {}", SpecialFn::ALL.iter().map(|f| slug(f.label())).collect::<Vec<_>>().join(", "));
            println!("macro:SLOT[:repeat|toggle|hold]");
        }
    }
    Ok(())
}

fn macros(kb: &mut Keyboard, cmd: MacroCmd) -> Result<(), Error> {
    match cmd {
        MacroCmd::Show { slot } => {
            let m = kb.read_macro(slot)?;
            println!("repeat: {}", m.repeat_count);
            for e in m.events {
                println!(
                    "  {}",
                    match e {
                        MacroEvent::KeyDown(k) => format!("down:{}", keycodes::display(k)),
                        MacroEvent::KeyUp(k) => format!("up:{}", keycodes::display(k)),
                        MacroEvent::MouseDown(b) => format!("down:{}", slug(b.label())),
                        MacroEvent::MouseUp(b) => format!("up:{}", slug(b.label())),
                        MacroEvent::MouseMove { dx, dy } => format!("move:{dx},{dy}"),
                        MacroEvent::Delay(d) => format!("delay:{d}"),
                    }
                );
            }
        }
        MacroCmd::Set { slot, events, repeat } => {
            let mut ev = Vec::new();
            for tok in events.split_whitespace() {
                let (k, v) = tok.split_once(':').ok_or_else(|| Error::InvalidArgument(tok.into()))?;
                let mouse = MouseAction::ALL.into_iter().find(|m| slug(m.label()) == slug(v));
                ev.push(match (k, mouse) {
                    ("delay", _) => MacroEvent::Delay(v.parse().map_err(|_| Error::InvalidArgument(tok.into()))?),
                    ("down", Some(m)) => MacroEvent::MouseDown(m),
                    ("up", Some(m)) => MacroEvent::MouseUp(m),
                    ("down", None) => {
                        MacroEvent::KeyDown(keycodes::parse(v).ok_or_else(|| Error::InvalidArgument(tok.into()))?)
                    }
                    ("up", None) => {
                        MacroEvent::KeyUp(keycodes::parse(v).ok_or_else(|| Error::InvalidArgument(tok.into()))?)
                    }
                    _ => return Err(Error::InvalidArgument(tok.into())),
                });
            }
            kb.write_macro(slot, &Macro { repeat_count: repeat, events: ev })?;
            println!("Macro {slot} stored. Bind it with: epomaker keymap set <KEY> macro:{slot}");
        }
        MacroCmd::Used => println!("{:?}", kb.used_macro_slots()?),
    }
    Ok(())
}

fn colors(kb: &mut Keyboard, cmd: ColorCmd) -> Result<(), Error> {
    match cmd {
        ColorCmd::Show => {
            let c = kb.key_colors()?;
            for k in &kb.model.keys {
                if let Some(i) = k.matrix_index {
                    println!("{:<14} #{:06x}", k.label.replace('\n', " "), c.get(i).copied().unwrap_or_default().0);
                }
            }
        }
        ColorCmd::Set { key, color, slot } => {
            let i = find_key(kb, &key)?;
            kb.set_key_color(slot, i, parse_color(&color)?)?;
        }
        ColorCmd::Fill { color } => {
            let c = parse_color(&color)?;
            kb.set_key_colors(&vec![c; kb.model.matrix_len()])?;
        }
    }
    Ok(())
}
