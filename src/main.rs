mod discover;
mod headphones;
mod protocol;
mod rfcomm;

use clap::{Parser, Subcommand};
use headphones::{AncMode, Headphones, PlaybackAction, DEFAULT_CHANNEL};

// ── ANSI helpers ────────────────────────────────────────────────────────

const B: &str = "\x1b[1m";
const D: &str = "\x1b[2m";
const G: &str = "\x1b[32m";
const Y: &str = "\x1b[33m";
const C: &str = "\x1b[36m";
const R: &str = "\x1b[31m";
const Z: &str = "\x1b[0m";

fn ok(msg: &str) { println!("{G}✓{Z} {msg}"); }
fn err(msg: &str) { eprintln!("{R}✗{Z} {msg}"); }

fn bar(pct: i32) -> String {
    let w = 15;
    let f = (pct as usize * w / 100).min(w);
    format!("{}{}", "█".repeat(f), "░".repeat(w - f))
}

// ── CLI ─────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "sonyctl", about = "Control Sony WH-1000XM series headphones over Bluetooth")]
struct Cli {
    /// Bluetooth MAC (auto-detected if omitted)
    #[arg(long)]
    mac: Option<String>,
    /// RFCOMM channel
    #[arg(long, default_value_t = DEFAULT_CHANNEL)]
    channel: u8,
    /// Show protocol debug output
    #[arg(short, long)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show battery, ANC, EQ, volume, speak-to-chat
    Status,
    /// Show model, firmware version, codec
    Info,
    /// Show battery percentage
    Battery,
    /// Enable noise cancelling
    Anc,
    /// Enable ambient sound mode
    Ambient {
        /// Ambient level 1-20
        #[arg(default_value_t = 10)]
        level: u8,
        /// Focus on voice
        #[arg(long)]
        voice: bool,
    },
    /// Disable ANC / ambient sound
    AncOff,
    /// DSEE upsampling
    Dsee {
        /// on/off (omit to show current)
        state: Option<String>,
    },
    /// Auto power-off timer
    AutoOff {
        /// 0=off, 5, 30, 60, 180 minutes (omit to show current)
        minutes: Option<u16>,
    },
    /// Voice guidance notifications
    VoiceGuidance {
        /// on/off (omit to show current)
        state: Option<String>,
    },
    /// Equalizer control
    Eq {
        #[command(subcommand)]
        action: EqCmd,
    },
    /// Get or set volume
    Volume {
        /// Volume 0-30 (omit to show current)
        level: Option<u8>,
    },
    /// Speak-to-Chat control
    Stc {
        /// on/off (omit to show current)
        state: Option<String>,
    },
    /// Play media
    Play,
    /// Pause media
    Pause,
    /// Next track
    Next,
    /// Previous track
    Prev,
    /// Show connected and paired devices
    Devices,
    /// Get or toggle multipoint
    Multipoint {
        /// on/off (omit to show current)
        state: Option<String>,
    },
    /// Power off headphones
    PowerOff,
}

#[derive(Subcommand)]
enum EqCmd {
    /// Show current EQ
    Get,
    /// Reset to flat
    Flat,
    /// Set custom EQ values (-10 to +10 each)
    Set { values: Vec<i8> },
}

fn is_on(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "on" | "1" | "true" | "yes")
}

fn open(cli: &Cli) -> Headphones {
    let mac = cli.mac.clone().or_else(discover::find_sony_mac).unwrap_or_else(|| {
        err("No Sony headphones found. Pair & connect first, or use --mac.");
        std::process::exit(1);
    });
    let mut hp = Headphones::connect(&mac, cli.channel, cli.verbose).unwrap_or_else(|e| {
        err(&format!("Connection failed: {e}"));
        std::process::exit(1);
    });
    hp.init().unwrap_or_else(|e| {
        err(&format!("Init failed: {e}"));
        std::process::exit(1);
    });
    hp
}

fn main() {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Status => {
            let mut hp = open(&cli);
            let bat = hp.get_battery().unwrap_or(-1);
            let anc = hp.get_anc().ok().flatten();
            let eq = hp.get_eq().ok().flatten();
            let vol = hp.get_volume().ok().flatten();
            let stc = hp.get_speak_to_chat().ok().flatten();

            println!("\n{B}WH-1000XM6{Z} {D}{}{Z}\n", hp.mac);

            println!("  {C}Battery{Z}");
            if bat >= 0 {
                let col = if bat > 30 { G } else if bat > 10 { Y } else { R };
                println!("    {col}{} {}%{Z}", bar(bat), bat);
            } else {
                println!("    {D}—{Z}");
            }

            println!("\n  {C}ANC / Ambient{Z}");
            if let Some(anc) = &anc {
                let m = match anc.mode {
                    AncMode::NoiseCancelling => format!("{G}Noise Cancelling{Z}"),
                    AncMode::Ambient => format!("{Y}Ambient Sound (level {}){Z}", anc.level),
                    AncMode::Off => format!("{D}Off{Z}"),
                };
                println!("    Mode   {m}");
                if anc.voice {
                    println!("    Voice  {G}Focus on Voice{Z}");
                }
            } else {
                println!("    {D}Could not read{Z}");
            }

            println!("\n  {C}Volume{Z}");
            if let Some(v) = vol {
                println!("    {v}/30");
            } else {
                println!("    {D}—{Z}");
            }

            println!("\n  {C}Equalizer{Z}");
            if let Some(eq) = &eq {
                let flat = eq.bands.iter().all(|&b| b == 0);
                if flat {
                    println!("    {D}Flat{Z}");
                } else {
                    let labels_10 = ["Bass", "400Hz", "1kHz", "2.5kHz", "6.3kHz",
                                     "16kHz", "B7", "B8", "B9", "B10"];
                    let labels_6 = ["Bass", "400Hz", "1kHz", "2.5kHz", "6.3kHz", "16kHz"];
                    let labels: &[&str] = if eq.bands.len() == 10 { &labels_10 } else { &labels_6 };
                    for (lbl, &v) in labels.iter().zip(eq.bands.iter()) {
                        println!("    {:<7} {:+}", lbl, v);
                    }
                }
            } else {
                println!("    {D}Could not read{Z}");
            }

            println!("\n  {C}Speak-to-Chat{Z}");
            match stc {
                Some(true) => println!("    🟢 On"),
                Some(false) => println!("    {D}Off{Z}"),
                None => println!("    {D}—{Z}"),
            }
            println!();
        }

        Cmd::Info => {
            let mut hp = open(&cli);
            let info = hp.get_info().unwrap_or_default();
            println!("{B}{}{Z}  FW {}  {C}{}{Z}",
                if info.model.is_empty() { "?" } else { &info.model },
                if info.firmware.is_empty() { "?" } else { &info.firmware },
                if info.codec.is_empty() { "?" } else { &info.codec },
            );
        }

        Cmd::Battery => {
            let mut hp = open(&cli);
            let bat = hp.get_battery().unwrap_or(-1);
            if bat >= 0 { println!("{}%", bat); } else { println!("—"); }
        }

        Cmd::Anc => {
            let mut hp = open(&cli);
            hp.set_nc().unwrap();
            ok("Noise cancelling enabled");
        }

        Cmd::Ambient { level, voice } => {
            let mut hp = open(&cli);
            hp.set_ambient(*level, *voice).unwrap();
            let extra = if *voice { " + voice focus" } else { "" };
            ok(&format!("Ambient sound level {level}{extra}"));
        }

        Cmd::AncOff => {
            let mut hp = open(&cli);
            hp.set_anc_off().unwrap();
            ok("ANC / ambient disabled");
        }

        Cmd::Dsee { state } => {
            let mut hp = open(&cli);
            match state {
                None => match hp.get_dsee().ok().flatten() {
                    Some(on) => println!("DSEE: {}", if on { "on" } else { "off" }),
                    None => err("Could not read DSEE status"),
                },
                Some(s) => {
                    let on = is_on(s);
                    hp.set_dsee(on).unwrap();
                    ok(&format!("DSEE {}", if on { "enabled" } else { "disabled" }));
                }
            }
        }

        Cmd::AutoOff { minutes } => {
            let mut hp = open(&cli);
            match minutes {
                None => match hp.get_auto_off().ok().flatten() {
                    Some(0) => println!("Auto power-off: off"),
                    Some(m) => println!("Auto power-off: {} min", m),
                    None => err("Could not read auto power-off"),
                },
                Some(m) => match hp.set_auto_off(*m) {
                    Ok(()) => ok(&format!("Auto power-off: {}", if *m == 0 { "off".into() } else { format!("{m} min") })),
                    Err(e) => err(&format!("{e}")),
                }
            }
        }

        Cmd::VoiceGuidance { state } => {
            let mut hp = open(&cli);
            match state {
                None => match hp.get_voice_guidance().ok().flatten() {
                    Some(on) => println!("Voice guidance: {}", if on { "on" } else { "off" }),
                    None => err("Could not read voice guidance status"),
                },
                Some(s) => {
                    let on = is_on(s);
                    hp.set_voice_guidance(on).unwrap();
                    ok(&format!("Voice guidance {}", if on { "enabled" } else { "disabled" }));
                }
            }
        }

        Cmd::Eq { action } => {
            let mut hp = open(&cli);
            match action {
                EqCmd::Get => match hp.get_eq().ok().flatten() {
                    Some(eq) => {
                        let labels_10 = ["Bass", "400Hz", "1kHz", "2.5kHz", "6.3kHz",
                                         "16kHz", "B7", "B8", "B9", "B10"];
                        let labels_6 = ["Bass", "400Hz", "1kHz", "2.5kHz", "6.3kHz", "16kHz"];
                        let labels: &[&str] = if eq.bands.len() == 10 { &labels_10 } else { &labels_6 };
                        for (lbl, &v) in labels.iter().zip(eq.bands.iter()) {
                            println!("{:<8} {:+}", lbl, v);
                        }
                    }
                    None => err("Could not read EQ"),
                },
                EqCmd::Flat => {
                    let eq = hp.get_eq().ok().flatten();
                    let n = eq.map(|e| e.bands.len()).unwrap_or(10);
                    hp.set_eq(&vec![0i8; n]).unwrap();
                    ok("EQ reset to flat");
                }
                EqCmd::Set { values } => {
                    hp.set_eq(values).unwrap();
                    let desc: String = values.iter().map(|v| format!("{:+}", v)).collect::<Vec<_>>().join(" ");
                    ok(&format!("EQ set: {desc}"));
                }
            }
        }

        Cmd::Volume { level } => {
            let mut hp = open(&cli);
            match level {
                None => match hp.get_volume().ok().flatten() {
                    Some(v) => println!("{v}/30"),
                    None => println!("—"),
                },
                Some(l) => {
                    hp.set_volume(*l).unwrap();
                    ok(&format!("Volume set to {l}/30"));
                }
            }
        }

        Cmd::Stc { state } => {
            let mut hp = open(&cli);
            match state {
                None => match hp.get_speak_to_chat().ok().flatten() {
                    Some(on) => println!("Speak-to-Chat: {}", if on { "on" } else { "off" }),
                    None => err("Could not read speak-to-chat status"),
                },
                Some(s) => {
                    let on = is_on(s);
                    hp.set_speak_to_chat(on).unwrap();
                    ok(&format!("Speak-to-Chat {}", if on { "enabled" } else { "disabled" }));
                }
            }
        }

        Cmd::Play => { open(&cli).playback(PlaybackAction::Play).unwrap(); ok("Play"); }
        Cmd::Pause => { open(&cli).playback(PlaybackAction::Pause).unwrap(); ok("Pause"); }
        Cmd::Next => { open(&cli).playback(PlaybackAction::Next).unwrap(); ok("Next"); }
        Cmd::Prev => { open(&cli).playback(PlaybackAction::Prev).unwrap(); ok("Prev"); }

        Cmd::Devices => {
            let mut hp = open(&cli);
            let (connected, paired) = hp.get_devices().unwrap_or_default();
            if !connected.is_empty() {
                println!("{C}Connected{Z}");
                for d in &connected {
                    println!("  {G}●{Z} {B}{}{Z}  {D}{}{Z}", d.name, d.mac);
                }
            }
            if !paired.is_empty() {
                println!("{C}Paired{Z}");
                for d in &paired {
                    println!("  {D}○ {}  {}{Z}", d.name, d.mac);
                }
            }
            if connected.is_empty() && paired.is_empty() {
                println!("{D}No devices{Z}");
            }
        }

        Cmd::Multipoint { state } => {
            let mut hp = open(&cli);
            match state {
                None => match hp.get_multipoint().ok().flatten() {
                    Some(on) => println!("Multipoint: {}", if on { "on" } else { "off" }),
                    None => err("Could not read multipoint status"),
                },
                Some(s) => {
                    let on = is_on(s);
                    match hp.set_multipoint(on) {
                        Ok(true) => ok(&format!("Multipoint {}", if on { "enabled" } else { "disabled" })),
                        Ok(false) => err("Headphones refused — multipoint can only be toggled from the phone (primary device)"),
                        Err(e) => err(&format!("{e}")),
                    }
                }
            }
        }

        Cmd::PowerOff => {
            let mut hp = open(&cli);
            hp.power_off().unwrap();
            ok("Power off sent");
        }
    }
}

