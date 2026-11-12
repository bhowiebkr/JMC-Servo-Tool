//! Serial transport and the request/retry loop.

use std::fmt;
use std::io;
use std::str::FromStr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::rtu::{self, FrameResult, Reply, Request, Scan};

/// Byte transport under the Modbus link: the real serial port or the simulated drive.
pub trait Transport: Send {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()>;
    /// Read what is available, waiting up to `wait` for the first byte. `Ok(0)` means nothing came.
    fn read_some(&mut self, buf: &mut [u8], wait: Duration) -> io::Result<usize>;
    fn clear_input(&mut self) -> io::Result<()>;
    /// Time one character takes on the wire.
    fn char_time(&self) -> Duration;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Framing {
    #[serde(rename = "8E1")]
    Even1,
    #[serde(rename = "8N2")]
    None2,
    #[serde(rename = "8N1")]
    None1,
    #[serde(rename = "8O1")]
    Odd1,
}

impl Framing {
    /// Bits per character including start, parity and stop bits.
    pub fn bits(self) -> u32 {
        match self {
            Framing::Even1 | Framing::Odd1 | Framing::None2 => 11,
            Framing::None1 => 10,
        }
    }
}

impl fmt::Display for Framing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Framing::Even1 => "8E1",
            Framing::None2 => "8N2",
            Framing::None1 => "8N1",
            Framing::Odd1 => "8O1",
        })
    }
}

impl FromStr for Framing {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "8E1" => Ok(Framing::Even1),
            "8N2" => Ok(Framing::None2),
            "8N1" => Ok(Framing::None1),
            "8O1" => Ok(Framing::Odd1),
            _ => Err(Error::Value(format!("framing {s:?}: use 8E1, 8N2, 8N1 or 8O1"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialSettings {
    pub port: String,
    pub baud: u32,
    pub framing: Framing,
}

impl fmt::Display for SerialSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.port, self.baud, self.framing)
    }
}

/// JMC's PC port default.
pub const DEFAULT_BAUD: u32 = 57600;

/// Baud rates and framings tried by `probe`, most likely first.
pub fn probe_candidates() -> Vec<(u32, Framing)> {
    let mut v = Vec::new();
    for baud in [57600, 115200, 9600, 38400, 19200] {
        for framing in [Framing::Even1, Framing::None2, Framing::None1, Framing::Odd1] {
            v.push((baud, framing));
        }
    }
    v
}

pub struct Serial {
    port: Box<dyn serialport::SerialPort>,
    settings: SerialSettings,
}

fn to_io(e: serialport::Error) -> io::Error {
    io::Error::other(e.to_string())
}

impl Serial {
    pub fn open(settings: &SerialSettings) -> Result<Serial> {
        use serialport::{DataBits, FlowControl, Parity, StopBits};
        let (parity, stop) = match settings.framing {
            Framing::Even1 => (Parity::Even, StopBits::One),
            Framing::Odd1 => (Parity::Odd, StopBits::One),
            Framing::None1 => (Parity::None, StopBits::One),
            Framing::None2 => (Parity::None, StopBits::Two),
        };
        let port = serialport::new(&settings.port, settings.baud)
            .data_bits(DataBits::Eight)
            .parity(parity)
            .stop_bits(stop)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(20))
            .open()
            .map_err(|e| Error::Serial(format!("{}: {e}", settings.port)))?;
        Ok(Serial { port, settings: settings.clone() })
    }

    pub fn settings(&self) -> &SerialSettings {
        &self.settings
    }
}

impl Transport for Serial {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        io::Write::write_all(&mut self.port, data)?;
        io::Write::flush(&mut self.port)
    }

    fn read_some(&mut self, buf: &mut [u8], wait: Duration) -> io::Result<usize> {
        self.port.set_timeout(wait.max(Duration::from_millis(1))).map_err(to_io)?;
        match io::Read::read(&mut self.port, buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(e),
        }
    }

    fn clear_input(&mut self) -> io::Result<()> {
        self.port.clear(serialport::ClearBuffer::Input).map_err(to_io)
    }

    fn char_time(&self) -> Duration {
        Duration::from_secs_f64(f64::from(self.settings.framing.bits()) / f64::from(self.settings.baud))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct LinkStats {
    pub requests: u64,
    pub replies: u64,
    pub timeouts: u64,
    pub crc_errors: u64,
    pub exceptions: u64,
    pub retries: u64,
    pub junk_bytes: u64,
}

/// One Modbus RTU conversation with one drive.
///
/// After a timeout or a reply with bytes trailing it, the next request first waits and empties
/// the input buffer, so a late reply can never be taken as the answer to the next request
/// (JMC 2.4.3 bug 9). Assumes no local echo, which holds for the RS232 cable.
pub struct Link {
    transport: Box<dyn Transport>,
    pub slave: u8,
    pub timeout: Duration,
    pub retries: u32,
    /// Most registers per read. 30 until the drive's real limit is measured.
    pub max_read: u16,
    pub stats: LinkStats,
    /// Optional description of the port, for logs and snapshots.
    pub label: String,
    dirty: bool,
    last_io: Option<Instant>,
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(300);
pub const DEFAULT_MAX_READ: u16 = 30;
const SETTLE: Duration = Duration::from_millis(50);
const BUSY_WAIT: Duration = Duration::from_millis(20);

impl Link {
    pub fn new(transport: Box<dyn Transport>, slave: u8) -> Link {
        Link {
            transport,
            slave,
            timeout: DEFAULT_TIMEOUT,
            retries: 2,
            max_read: DEFAULT_MAX_READ,
            stats: LinkStats::default(),
            label: String::new(),
            dirty: false,
            last_io: None,
        }
    }

    pub fn open(settings: &SerialSettings, slave: u8) -> Result<Link> {
        let mut link = Link::new(Box::new(Serial::open(settings)?), slave);
        link.label = settings.to_string();
        Ok(link)
    }

    /// Send one request, retrying timeouts and garbled replies. A busy drive (exception 06)
    /// gets one extra try after a short wait.
    pub fn request(&mut self, req: &Request) -> Result<Reply> {
        let mut attempt = 0;
        let mut busy_retried = false;
        loop {
            let err = match self.exchange(req) {
                Ok(r) => return Ok(r),
                Err(e) => e,
            };
            let retry = match &err {
                Error::Timeout { .. } | Error::BadReply(_) => attempt < self.retries,
                Error::Exception { code: 6, .. } if !busy_retried => {
                    busy_retried = true;
                    std::thread::sleep(BUSY_WAIT);
                    true
                }
                _ => false,
            };
            if !retry {
                return Err(err);
            }
            attempt += 1;
            self.stats.retries += 1;
        }
    }

    fn exchange(&mut self, req: &Request) -> Result<Reply> {
        let char_time = self.transport.char_time();
        // Modbus wants 3.5 characters of silence between frames; give it a bit more.
        let gap = (char_time * 4).max(Duration::from_millis(2));
        if self.dirty {
            std::thread::sleep(SETTLE);
            self.transport.clear_input()?;
            self.dirty = false;
        }
        if let Some(t) = self.last_io {
            let since = t.elapsed();
            if since < gap {
                std::thread::sleep(gap - since);
            }
        }
        self.transport.clear_input()?;
        let frame = req.encode(self.slave);
        self.stats.requests += 1;
        self.transport.write_all(&frame)?;
        let send_time = char_time * frame.len() as u32;
        let deadline = Instant::now() + send_time + self.timeout;
        let mut buf: Vec<u8> = Vec::with_capacity(272);
        let mut chunk = [0u8; 272];
        let mut crc_seen = false;
        loop {
            loop {
                match rtu::scan(&buf, self.slave, req) {
                    Scan::NeedMore => break,
                    Scan::Skip { n, crc } => {
                        buf.drain(..n);
                        self.stats.junk_bytes += n as u64;
                        if crc && !crc_seen {
                            crc_seen = true;
                            self.stats.crc_errors += 1;
                        }
                    }
                    Scan::Frame { len, result } => {
                        buf.drain(..len);
                        self.last_io = Some(Instant::now());
                        if !buf.is_empty() {
                            self.stats.junk_bytes += buf.len() as u64;
                            self.dirty = true;
                        }
                        return match result {
                            FrameResult::Reply(r) => {
                                self.stats.replies += 1;
                                Ok(r)
                            }
                            FrameResult::Exception(code) => {
                                self.stats.exceptions += 1;
                                Err(Error::Exception { request: req.describe(), code })
                            }
                            FrameResult::Mismatch(s) => {
                                self.dirty = true;
                                Err(Error::BadReply(format!("{}: {s}", req.describe())))
                            }
                        };
                    }
                }
            }
            let now = Instant::now();
            if now >= deadline {
                self.stats.timeouts += 1;
                self.dirty = true;
                self.last_io = Some(now);
                return Err(Error::Timeout { request: req.describe(), received: buf.len() });
            }
            let wait = (deadline - now).min(Duration::from_millis(20));
            let n = self.transport.read_some(&mut chunk, wait)?;
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// Read `count` registers, split into frames of at most `max_read`.
    pub fn read(&mut self, addr: u16, count: u16) -> Result<Vec<u16>> {
        let mut out = Vec::with_capacity(usize::from(count));
        let step = self.max_read.clamp(1, rtu::MAX_READ);
        let mut done = 0u16;
        while done < count {
            let n = (count - done).min(step);
            let a = addr
                .checked_add(done)
                .ok_or_else(|| Error::Value(format!("address range {addr}+{count} overflows")))?;
            match self.request(&Request::Read { addr: a, count: n })? {
                Reply::Registers(v) => out.extend(v),
                Reply::Written => return Err(Error::BadReply("write reply to a read".into())),
            }
            done += n;
        }
        Ok(out)
    }

    pub fn read_one(&mut self, addr: u16) -> Result<u16> {
        Ok(self.read(addr, 1)?[0])
    }

    pub fn write_one(&mut self, addr: u16, value: u16) -> Result<()> {
        self.request(&Request::WriteOne { addr, value }).map(|_| ())
    }

    /// As `write_one`, but a timeout or a garbled reply is not retried. For commands that must not run twice.
    pub fn write_one_once(&mut self, addr: u16, value: u16) -> Result<()> {
        let retries = std::mem::replace(&mut self.retries, 0);
        let r = self.request(&Request::WriteOne { addr, value });
        self.retries = retries;
        r.map(|_| ())
    }

    pub fn write_many(&mut self, addr: u16, values: &[u16]) -> Result<()> {
        if usize::from(addr) + values.len() > 0x1_0000 {
            return Err(Error::Value(format!("address range {addr}+{} overflows", values.len())));
        }
        for (k, chunk) in values.chunks(123).enumerate() {
            let a = addr + (k * 123) as u16;
            self.request(&Request::WriteMany { addr: a, values: chunk.to_vec() })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeHit {
    pub settings: SerialSettings,
    pub slave: u8,
    /// Register 0 (P00-00, motor code).
    pub reg0: Option<u16>,
    /// Exception code if the drive refused the register 0 read.
    pub exception: Option<u8>,
    /// Registers 2150.. (identity block), if the drive answered.
    pub identity: Option<Vec<u16>>,
    pub tried: usize,
}

/// Find the baud rate and framing a drive answers on. Reads register 0, then the identity block.
pub fn probe(port: &str, slave: u8, candidates: &[(u32, Framing)]) -> Result<(Link, ProbeHit)> {
    let mut last_err = None;
    for (k, &(baud, framing)) in candidates.iter().enumerate() {
        let settings = SerialSettings { port: port.to_string(), baud, framing };
        let mut link = match Link::open(&settings, slave) {
            Ok(l) => l,
            Err(e @ Error::Serial(_)) if k == 0 => return Err(e),
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        link.timeout = Duration::from_millis(150);
        link.retries = 0;
        let (reg0, exception) = match link.read_one(0) {
            Ok(v) => (Some(v), None),
            // An exception still means a drive answered at this speed.
            Err(Error::Exception { code, .. }) => (None, Some(code)),
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        link.timeout = DEFAULT_TIMEOUT;
        link.retries = 2;
        let identity = link.read(2150, 5).ok();
        let hit = ProbeHit { settings, slave, reg0, exception, identity, tried: k + 1 };
        return Ok((link, hit));
    }
    Err(last_err.unwrap_or_else(|| Error::Serial(format!("{port}: no candidates to try"))))
}

#[derive(Debug, Clone, Serialize)]
pub struct PortInfo {
    pub name: String,
    pub kind: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports().map_err(|e| Error::Serial(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(u) => PortInfo {
                name: p.port_name,
                kind: "usb".into(),
                vid: Some(u.vid),
                pid: Some(u.pid),
                manufacturer: u.manufacturer,
                product: u.product,
                serial_number: u.serial_number,
            },
            other => PortInfo {
                name: p.port_name,
                kind: match other {
                    serialport::SerialPortType::PciPort => "pci",
                    serialport::SerialPortType::BluetoothPort => "bluetooth",
                    _ => "unknown",
                }
                .into(),
                vid: None,
                pid: None,
                manufacturer: None,
                product: None,
                serial_number: None,
            },
        })
        .collect())
}
