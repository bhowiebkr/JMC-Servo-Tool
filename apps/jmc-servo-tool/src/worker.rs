//! The drive thread. It owns the serial link, so the window never waits on the drive.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use jmc_core::drive::{Drive, Persist, WriteOptions};
use jmc_core::guard::{self, WriteReport, WriteRequest};
use jmc_core::link::{self, Link, LinkStats, PortInfo, ProbeHit, SerialSettings};
use jmc_core::monitor::{Sample, Status};
use jmc_core::params::Schema;
use jmc_core::rtu;
use jmc_core::sim::SimDrive;
use jmc_core::snapshot::Snapshot;

#[derive(Debug, Clone)]
pub enum Target {
    Serial(SerialSettings),
    /// The simulated drive, optionally loaded from a snapshot or JMC export.
    Sim(Option<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct WriteJob {
    pub axis: String,
    /// `CODE=VALUE` items.
    pub items: Vec<String>,
    pub baselines_dir: PathBuf,
    pub backup_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    ListPorts,
    Connect { target: Target, slave: u8, max_read: u16 },
    /// Try the usual baud rates and framings; stay connected on the one that answers.
    Probe { port: String, slave: u8, max_read: u16 },
    Disconnect,
    ReadAll,
    /// RAM only. The window never asks for an EEPROM save.
    Write(WriteJob),
    Status,
    ResetAlarm,
    /// Start sampling the monitor registers at this interval, or stop with `None`.
    Monitor(Option<Duration>),
}

#[derive(Debug)]
pub enum Event {
    Ports(Vec<PortInfo>),
    Connected { label: String },
    Probed(Box<ProbeHit>),
    Disconnected,
    Progress { done: usize, total: usize },
    ReadAll { values: BTreeMap<String, u16>, errors: BTreeMap<String, String>, label: String },
    Written(Box<WriteReport>),
    Status(Status),
    AlarmReset,
    Sample { t: f64, sample: Sample },
    MonitorStopped { reason: Option<String> },
    Stats(LinkStats),
    Error { what: &'static str, message: String },
}

pub struct Worker {
    tx: Sender<Cmd>,
    rx: Receiver<Event>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Worker {
        let (tx, cmd_rx) = mpsc::channel();
        let (ev_tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("drive".into())
            .spawn(move || run(&cmd_rx, &Out { tx: ev_tx, ctx }))
            .expect("start the drive thread");
        Worker { tx, rx }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}

struct Out {
    tx: Sender<Event>,
    ctx: egui::Context,
}

impl Out {
    fn send(&self, e: Event) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }

    fn error(&self, what: &'static str, message: impl ToString) {
        self.send(Event::Error { what, message: message.to_string() });
    }
}

struct Monitor {
    interval: Duration,
    start: Instant,
    next: Instant,
    failures: u32,
}

/// Give up after this many failed samples in a row.
const MAX_SAMPLE_FAILURES: u32 = 5;

fn run(rx: &Receiver<Cmd>, out: &Out) {
    let schema = Schema::model512();
    let mut drive: Option<Drive> = None;
    let mut monitor: Option<Monitor> = None;
    loop {
        let cmd = match &monitor {
            Some(m) => match rx.recv_timeout(m.next.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            },
        };
        let Some(cmd) = cmd else {
            sample(&mut drive, &mut monitor, out);
            continue;
        };
        match cmd {
            Cmd::ListPorts => match link::list_ports() {
                Ok(p) => out.send(Event::Ports(p)),
                Err(e) => out.error("ports", e),
            },
            Cmd::Connect { target, slave, max_read } => {
                monitor = None;
                drive = None;
                match connect(schema, &target, slave, max_read) {
                    Ok(d) => {
                        out.send(Event::Connected { label: d.link.label.clone() });
                        drive = Some(d);
                    }
                    Err(e) => out.error("connect", e),
                }
            }
            Cmd::Probe { port, slave, max_read } => {
                monitor = None;
                drive = None;
                match link::probe(&port, slave, &link::probe_candidates()) {
                    Ok((mut l, hit)) => {
                        l.max_read = max_read.clamp(1, rtu::MAX_READ);
                        out.send(Event::Probed(Box::new(hit)));
                        out.send(Event::Connected { label: l.label.clone() });
                        drive = Some(Drive::new(l, schema));
                    }
                    Err(e) => out.error("probe", format!("{port}: no drive answered: {e}")),
                }
            }
            Cmd::Disconnect => {
                monitor = None;
                drive = None;
                out.send(Event::Disconnected);
            }
            Cmd::ReadAll => {
                let Some(d) = drive.as_mut() else {
                    out.error("read", "not connected");
                    continue;
                };
                let r = d.read_all(|done, total| out.send(Event::Progress { done, total }));
                match r {
                    Ok(all) => {
                        out.send(Event::ReadAll { values: all.values, errors: all.errors, label: d.link.label.clone() })
                    }
                    Err(e) => out.error("read", e),
                }
                out.send(Event::Stats(d.link.stats));
            }
            Cmd::Write(job) => {
                let Some(d) = drive.as_mut() else {
                    out.error("write", "not connected");
                    continue;
                };
                match write(d, schema, &job, out) {
                    Ok(r) => out.send(Event::Written(Box::new(r))),
                    Err(e) => out.error("write", e),
                }
                out.send(Event::Stats(d.link.stats));
            }
            Cmd::Status => {
                let Some(d) = drive.as_mut() else {
                    out.error("status", "not connected");
                    continue;
                };
                match d.status() {
                    Ok(s) => out.send(Event::Status(s)),
                    Err(e) => out.error("status", e),
                }
            }
            Cmd::ResetAlarm => {
                let Some(d) = drive.as_mut() else {
                    out.error("reset", "not connected");
                    continue;
                };
                match d.reset_alarm() {
                    Ok(()) => {
                        out.send(Event::AlarmReset);
                        match d.status() {
                            Ok(s) => out.send(Event::Status(s)),
                            Err(e) => out.error("status", e),
                        }
                    }
                    Err(e) => out.error("reset", e),
                }
            }
            Cmd::Monitor(Some(interval)) => {
                let Some(d) = drive.as_mut() else {
                    out.error("monitor", "not connected");
                    continue;
                };
                d.forget_monitor_layout();
                let now = Instant::now();
                monitor = Some(Monitor { interval, start: now, next: now, failures: 0 });
            }
            Cmd::Monitor(None) => {
                if monitor.take().is_some() {
                    out.send(Event::MonitorStopped { reason: None });
                }
            }
        }
    }
}

fn connect(schema: &'static Schema, target: &Target, slave: u8, max_read: u16) -> jmc_core::Result<Drive> {
    let mut link = match target {
        Target::Serial(s) => Link::open(s, slave)?,
        Target::Sim(from) => {
            let sim = SimDrive::new(schema);
            sim.state().slave = slave;
            if let Some(f) = from {
                let s = Snapshot::load_any(schema, f)?;
                for (code, raw) in s.raw(schema)? {
                    sim.set_param(schema.get(&code)?.address, raw);
                }
            }
            let mut l = Link::new(Box::new(sim), slave);
            l.label = match from {
                Some(f) => format!("simulated drive ({})", f.file_name().unwrap_or_default().to_string_lossy()),
                None => "simulated drive".into(),
            };
            l
        }
    };
    link.max_read = max_read.clamp(1, rtu::MAX_READ);
    let mut d = Drive::new(link, schema);
    // An open port says nothing about the drive: make sure something answers.
    let p0 = schema.get("P00-00")?;
    d.read_param(p0).map_err(|e| match e {
        jmc_core::Error::Timeout { .. } => jmc_core::Error::Serial(format!(
            "{}: the port opened but no drive answered ({e}). Check the cable and station number, or use Find settings",
            d.link.label
        )),
        other => other,
    })?;
    Ok(d)
}

fn write(d: &mut Drive, schema: &'static Schema, job: &WriteJob, out: &Out) -> jmc_core::Result<WriteReport> {
    let opts = WriteOptions { persist: Persist::Ram, allow_protected: false };
    let items = guard::parse_assignments(schema, &job.items, opts)?;
    let baselines = guard::baselines_for(&job.baselines_dir, &job.axis)?;
    let req = WriteRequest {
        axis: job.axis.clone(),
        items,
        opts,
        baselines: Some(&baselines),
        backup_dir: job.backup_dir.clone(),
        description: job.items.join(" "),
    };
    guard::guarded_write(d, &req, |done, total| out.send(Event::Progress { done, total }))
}

fn sample(drive: &mut Option<Drive>, monitor: &mut Option<Monitor>, out: &Out) {
    let (Some(d), Some(m)) = (drive.as_mut(), monitor.as_mut()) else {
        *monitor = None;
        return;
    };
    match d.sample() {
        Ok(s) => {
            m.failures = 0;
            out.send(Event::Sample { t: m.start.elapsed().as_secs_f64(), sample: s });
        }
        Err(e) => {
            m.failures += 1;
            if m.failures >= MAX_SAMPLE_FAILURES {
                *monitor = None;
                out.send(Event::MonitorStopped { reason: Some(format!("{MAX_SAMPLE_FAILURES} reads in a row failed: {e}")) });
                return;
            }
        }
    }
    let now = Instant::now();
    m.next += m.interval;
    if m.next < now {
        // Fell behind (slow link): don't burst to catch up.
        m.next = now + m.interval;
    }
}
