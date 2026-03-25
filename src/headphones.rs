/// High-level headphone controller — connects, sends commands, parses responses.
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use crate::protocol::{self, DataType, Message, END_MARKER, START_MARKER};
use crate::rfcomm::RfcommSocket;

pub const DEFAULT_CHANNEL: u8 = 9;

pub struct Headphones {
    sock: RfcommSocket,
    seq: u8,
    buf: Vec<u8>,
    pub verbose: bool,
    pub mac: String,
}

impl Headphones {
    pub fn connect(mac: &str, channel: u8, verbose: bool) -> io::Result<Self> {
        let sock = RfcommSocket::connect(mac, channel)?;
        sock.set_timeout(Duration::from_secs(3))?;
        if verbose {
            eprintln!("\x1b[2m[sonyctl] connected to {} ch {}\x1b[0m", mac, channel);
        }
        Ok(Self { sock, seq: 0, buf: Vec::with_capacity(2048), verbose, mac: mac.to_string() })
    }

    fn dbg(&self, msg: &str) {
        if self.verbose {
            eprintln!("\x1b[2m[sonyctl] {}\x1b[0m", msg);
        }
    }

    fn send_raw(&mut self, pkt: &[u8]) -> io::Result<()> {
        self.sock.write_all(pkt)?;
        if self.verbose {
            let hex: String = pkt.iter().map(|b| format!("{:02x}", b)).collect();
            self.dbg(&format!("TX {}", hex));
        }
        Ok(())
    }

    pub fn send(&mut self, payload: &[u8], dt: DataType) -> io::Result<()> {
        let pkt = protocol::build_packet(dt, self.seq, payload);
        self.send_raw(&pkt)
    }

    pub fn send_ack(&mut self, device_seq: u8) -> io::Result<()> {
        let pkt = protocol::build_ack(device_seq);
        self.send_raw(&pkt)
    }

    pub fn recv(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        let deadline = Instant::now() + timeout;
        loop {
            // Try to extract a frame from buffer
            if let Some(si) = self.buf.iter().position(|&b| b == START_MARKER) {
                if let Some(ei) = self.buf[si + 1..].iter().position(|&b| b == END_MARKER) {
                    let ei = si + 1 + ei;
                    let raw_esc: Vec<u8> = self.buf[si + 1..ei].to_vec();
                    self.buf.drain(..=ei);
                    let raw = protocol::unescape(&raw_esc);
                    if let Some(msg) = protocol::parse_packet(&raw) {
                        if self.verbose {
                            let hex: String = msg.payload.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                            self.dbg(&format!("RX dt={} seq={} [{}]", msg.data_type as u8, msg.seq, hex));
                        }
                        return Ok(Some(msg));
                    }
                    continue;
                }
            }
            // Need more data
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let read_timeout = remaining.min(Duration::from_millis(500));
            self.sock.set_timeout(read_timeout)?;
            let mut tmp = [0u8; 2048];
            match self.sock.read(&mut tmp) {
                Ok(0) => return Ok(None),
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut
                    || e.raw_os_error() == Some(11)   // EAGAIN
                    || e.raw_os_error() == Some(110)   // ETIMEDOUT
                    => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Send a command, handle ACKs, collect response payloads.
    pub fn command(&mut self, payload: &[u8], dt: DataType, expect: Option<u8>, timeout: Duration) -> io::Result<Vec<Vec<u8>>> {
        self.send(payload, dt)?;
        let mut results = Vec::new();
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let msg = match self.recv(remaining.min(Duration::from_millis(1500)))? {
                Some(m) => m,
                None => break,
            };
            if msg.data_type == DataType::Ack {
                self.seq = msg.seq;
                continue;
            }
            self.send_ack(msg.seq)?;
            if !msg.payload.is_empty() {
                let done = expect.map_or(false, |e| msg.payload[0] == e);
                results.push(msg.payload);
                if done { break; }
            }
        }
        Ok(results)
    }

    /// Shorthand: command on DataMdr with default timeout.
    pub fn cmd(&mut self, payload: &[u8], expect: Option<u8>) -> io::Result<Vec<Vec<u8>>> {
        self.command(payload, DataType::DataMdr, expect, Duration::from_secs(3))
    }

    pub fn drain(&mut self, timeout: Duration) -> io::Result<()> {
        loop {
            match self.recv(timeout)? {
                None => break,
                Some(msg) => {
                    if msg.data_type == DataType::Ack {
                        self.seq = msg.seq;
                    } else {
                        self.send_ack(msg.seq)?;
                    }
                }
            }
        }
        Ok(())
    }

    // ── High-Level API ──────────────────────────────────────────────────

    pub fn init(&mut self) -> io::Result<()> {
        self.drain(Duration::from_millis(500))?;
        // INIT on dt=12 (COMMAND_1)
        self.cmd(&[0x00, 0x00], Some(0x01))?;
        self.drain(Duration::from_millis(500))?;
        // INIT_2 on dt=14 (COMMAND_2) — needed to unlock dt=14 responses
        self.command(&[0x06, 0x00], DataType::DataMdrNo2, Some(0x07), Duration::from_secs(2))?;
        self.drain(Duration::from_millis(500))?;
        Ok(())
    }

    pub fn get_battery(&mut self) -> io::Result<i32> {
        for pl in self.cmd(&[0x22, 0x00], Some(0x23))? {
            if pl.len() >= 3 && pl[0] == 0x23 && pl[1] == 0x00 {
                return Ok(pl[2] as i32);
            }
        }
        Ok(-1)
    }

    pub fn get_anc(&mut self) -> io::Result<Option<AncStatus>> {
        // XM6 uses sub-type 0x19; fall back to 0x17
        for sub in [0x19, 0x17] {
            for pl in self.cmd(&[0x66, sub], Some(0x67))? {
                if (pl[0] == 0x67 || pl[0] == 0x69) && pl.len() >= 7 {
                    // Skip all-zeros from stale 0x17
                    if sub == 0x17 && pl[3..7].iter().all(|&b| b == 0) {
                        continue;
                    }
                    let enabled = pl[3] != 0;
                    let is_ambient = pl[4] != 0;
                    let voice = pl[5] != 0;
                    let level = if is_ambient { pl[6] } else { 0 };
                    let mode = if !enabled {
                        AncMode::Off
                    } else if is_ambient {
                        AncMode::Ambient
                    } else {
                        AncMode::NoiseCancelling
                    };
                    return Ok(Some(AncStatus { mode, voice, level }));
                }
            }
        }
        Ok(None)
    }

    pub fn set_nc(&mut self) -> io::Result<()> {
        self.cmd(&[0x68, 0x17, 0x01, 0x01, 0x00, 0x00, 0x01], None)?;
        Ok(())
    }

    pub fn set_ambient(&mut self, level: u8, voice: bool) -> io::Result<()> {
        let level = level.clamp(1, 20);
        self.cmd(&[0x68, 0x17, 0x01, 0x01, 0x01, if voice { 0x01 } else { 0x00 }, level], None)?;
        Ok(())
    }

    pub fn set_anc_off(&mut self) -> io::Result<()> {
        self.cmd(&[0x68, 0x17, 0x01, 0x00, 0x00, 0x00, 0x01], None)?;
        Ok(())
    }

    pub fn get_eq(&mut self) -> io::Result<Option<EqStatus>> {
        for pl in self.cmd(&[0x56, 0x00], Some(0x57))? {
            if pl[0] != 0x57 || pl.len() < 4 {
                continue;
            }
            let num_bands = pl[3] as usize;
            if pl.len() < 4 + num_bands {
                continue;
            }
            let bands: Vec<i8> = pl[4..4 + num_bands].iter().map(|&b| b as i8 - 10).collect();
            return Ok(Some(EqStatus { bands }));
        }
        Ok(None)
    }

    pub fn set_eq(&mut self, bands: &[i8]) -> io::Result<()> {
        let mut payload = vec![0x58, 0x00, 0xA0, bands.len() as u8];
        for &b in bands {
            payload.push((b.clamp(-10, 10) + 10) as u8);
        }
        self.cmd(&payload, None)?;
        Ok(())
    }

    pub fn get_volume(&mut self) -> io::Result<Option<u8>> {
        for pl in self.cmd(&[0xA6, 0x20], Some(0xA7))? {
            if pl[0] == 0xA7 && pl[1] == 0x20 && pl.len() >= 3 {
                return Ok(Some(pl[2]));
            }
        }
        Ok(None)
    }

    pub fn set_volume(&mut self, level: u8) -> io::Result<()> {
        let level = level.min(30);
        self.cmd(&[0xA8, 0x20, level], None)?;
        Ok(())
    }

    pub fn get_speak_to_chat(&mut self) -> io::Result<Option<bool>> {
        for pl in self.cmd(&[0xF6, 0x0C], Some(0xF7))? {
            if pl[0] == 0xF7 && pl[1] == 0x0C && pl.len() >= 3 {
                return Ok(Some(pl[2] == 0)); // inverted
            }
        }
        Ok(None)
    }

    pub fn set_speak_to_chat(&mut self, enabled: bool) -> io::Result<()> {
        self.cmd(&[0xF8, 0x0C, if enabled { 0x00 } else { 0x01 }, 0x01], None)?;
        Ok(())
    }

    pub fn get_info(&mut self) -> io::Result<DeviceInfo> {
        let mut info = DeviceInfo::default();

        for pl in self.cmd(&[0x04, 0x01], Some(0x05))? {
            if pl[0] == 0x05 && pl[1] == 0x01 && pl.len() >= 3 {
                let len = pl[2] as usize;
                if pl.len() >= 3 + len {
                    info.model = String::from_utf8_lossy(&pl[3..3 + len]).into();
                }
            }
        }
        for pl in self.cmd(&[0x04, 0x02], Some(0x05))? {
            if pl[0] == 0x05 && pl[1] == 0x02 && pl.len() >= 3 {
                let len = pl[2] as usize;
                if pl.len() >= 3 + len {
                    info.firmware = String::from_utf8_lossy(&pl[3..3 + len]).into();
                }
            }
        }
        let codecs = [
            (0x01, "SBC"), (0x02, "AAC"), (0x10, "LDAC"),
            (0x20, "aptX"), (0x21, "aptX HD"),
        ];
        for pl in self.cmd(&[0x12, 0x02], Some(0x13))? {
            if pl[0] == 0x13 && pl.len() >= 3 {
                info.codec = codecs.iter()
                    .find(|&&(c, _)| c == pl[2])
                    .map(|&(_, name)| name.to_string())
                    .unwrap_or_else(|| format!("0x{:02x}", pl[2]));
            }
        }
        Ok(info)
    }

    pub fn get_dsee(&mut self) -> io::Result<Option<bool>> {
        for pl in self.cmd(&[0xE6, 0x01], Some(0xE7))? {
            if pl[0] == 0xE7 && pl[1] == 0x01 && pl.len() >= 3 {
                return Ok(Some(pl[2] != 0));
            }
        }
        Ok(None)
    }

    pub fn set_dsee(&mut self, enabled: bool) -> io::Result<()> {
        self.cmd(&[0xE8, 0x01, if enabled { 0x01 } else { 0x00 }], None)?;
        Ok(())
    }

    pub fn get_auto_off(&mut self) -> io::Result<Option<u16>> {
        let codes: &[((u8, u8), u16)] = &[
            ((0x11, 0x00), 0), ((0x10, 0x00), 5), ((0x10, 0x01), 30),
            ((0x10, 0x02), 60), ((0x10, 0x03), 180),
        ];
        for pl in self.cmd(&[0x26, 0x05], Some(0x27))? {
            if pl[0] == 0x27 && pl[1] == 0x05 && pl.len() >= 4 {
                let key = (pl[2], pl[3]);
                return Ok(Some(
                    codes.iter().find(|&&(k, _)| k == key).map(|&(_, v)| v).unwrap_or(9999)
                ));
            }
        }
        Ok(None)
    }

    pub fn set_auto_off(&mut self, minutes: u16) -> io::Result<()> {
        let codes: &[(u16, (u8, u8))] = &[
            (0, (0x11, 0x00)), (5, (0x10, 0x00)), (30, (0x10, 0x01)),
            (60, (0x10, 0x02)), (180, (0x10, 0x03)),
        ];
        let (c1, c2) = codes.iter()
            .find(|&&(m, _)| m == minutes)
            .map(|&(_, c)| c)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput,
                "invalid timeout: use 0 (off), 5, 30, 60, or 180"))?;
        self.cmd(&[0x28, 0x05, c1, c2], None)?;
        Ok(())
    }

    pub fn get_voice_guidance(&mut self) -> io::Result<Option<bool>> {
        for pl in self.command(&[0x46, 0x01], DataType::DataMdrNo2, Some(0x47), Duration::from_secs(3))? {
            if pl[0] == 0x47 && pl[1] == 0x01 && pl.len() >= 3 {
                return Ok(Some(pl[2] != 0));
            }
        }
        Ok(None)
    }

    pub fn set_voice_guidance(&mut self, enabled: bool) -> io::Result<()> {
        self.command(
            &[0x48, 0x01, if enabled { 0x01 } else { 0x00 }],
            DataType::DataMdrNo2, None, Duration::from_secs(3),
        )?;
        Ok(())
    }

    pub fn get_multipoint(&mut self) -> io::Result<Option<bool>> {
        for pl in self.cmd(&[0xD6, 0xD2], Some(0xD7))? {
            if pl[0] == 0xD7 && pl[1] == 0xD2 && pl.len() >= 4 {
                return Ok(Some(pl[3] == 0)); // inverted
            }
        }
        Ok(None)
    }

    pub fn set_multipoint(&mut self, enabled: bool) -> io::Result<bool> {
        let responses = self.command(
            &[0xD8, 0xD2, 0x00, if enabled { 0x00 } else { 0x01 }],
            DataType::DataMdr, Some(0xD9), Duration::from_secs(5),
        )?;
        self.cmd(&[0x98, 0x00, 0x07, 0x01], None)?;
        for pl in &responses {
            if pl[0] == 0xD9 && pl[1] == 0xD2 && pl.len() >= 4 {
                let actual = pl[3] == 0; // inverted
                return Ok(actual == enabled);
            }
        }
        Ok(false)
    }

    pub fn get_devices(&mut self) -> io::Result<(Vec<BtDevice>, Vec<BtDevice>)> {
        let mut connected = Vec::new();
        let mut paired = Vec::new();
        // Response may arrive late — don't filter by expect, collect all
        for pl in self.command(&[0x36, 0x02], DataType::DataMdrNo2, None, Duration::from_secs(5))? {
            if pl[0] != 0x37 || pl.len() < 4 {
                continue;
            }
            let num_connected = pl[1] as usize;
            let num_total = pl[2] as usize;
            let mut pos = 3;
            for i in 0..num_total {
                if pos + 17 > pl.len() { break; }
                let mac = String::from_utf8_lossy(&pl[pos..pos + 17]).into();
                pos += 17;
                pos += 4; // skip flags
                if pos >= pl.len() { break; }
                let name_len = pl[pos] as usize;
                pos += 1;
                let name = if pos + name_len <= pl.len() {
                    String::from_utf8_lossy(&pl[pos..pos + name_len]).into()
                } else {
                    "?".into()
                };
                pos += name_len;
                let dev = BtDevice { name, mac };
                if i < num_connected {
                    connected.push(dev);
                } else {
                    paired.push(dev);
                }
            }
        }
        Ok((connected, paired))
    }

    pub fn playback(&mut self, action: PlaybackAction) -> io::Result<()> {
        let code = match action {
            PlaybackAction::Pause => 1,
            PlaybackAction::Next => 2,
            PlaybackAction::Prev => 3,
            PlaybackAction::Play => 7,
        };
        self.cmd(&[0xA4, 0x01, 0x00, code], None)?;
        Ok(())
    }

    pub fn power_off(&mut self) -> io::Result<()> {
        self.cmd(&[0x24, 0x03, 0x01], None)?;
        Ok(())
    }
}

// ── Data types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncMode {
    Off,
    NoiseCancelling,
    Ambient,
}

#[derive(Debug)]
pub struct AncStatus {
    pub mode: AncMode,
    pub voice: bool,
    pub level: u8,
}

#[derive(Debug)]
pub struct EqStatus {
    pub bands: Vec<i8>,
}

#[derive(Default, Debug)]
pub struct DeviceInfo {
    pub model: String,
    pub firmware: String,
    pub codec: String,
}

#[derive(Debug)]
pub struct BtDevice {
    pub name: String,
    pub mac: String,
}

#[derive(Debug, Clone, Copy)]
pub enum PlaybackAction {
    Play,
    Pause,
    Next,
    Prev,
}
