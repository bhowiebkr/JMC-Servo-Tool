//! `jmc`: command-line access to JMC servo drives (model 512). Every command prints JSON, so
//! scripts and AI agents can drive it; errors go to stderr as `{"error": "..."}` with exit code 1.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

use jmc_core::drive::{Drive, LINK_PARAMS, Persist, WriteOptions};
use jmc_core::link::{self, Framing, Link, SerialSettings};
use jmc_core::params::{Access, ParamDef, Schema, format_scaled};
use jmc_core::sim::SimDrive;
use jmc_core::snapshot::{self, Snapshot};
use jmc_core::{guard, jmcxml, rtu};

#[derive(Parser)]
#[command(name = "jmc", version, about = "JMC servo drive tool (model 512): read, write, snapshot and monitor drives. Prints JSON.")]
struct Cli {
    #[command(flatten)]
    conn: Conn,
    /// Print JSON on one line.
    #[arg(long, global = true)]
    compact: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Conn {
    /// Serial port, e.g. COM5 (see `jmc ports`).
    #[arg(long, global = true, env = "JMC_PORT")]
    port: Option<String>,
    #[arg(long, global = true, default_value_t = link::DEFAULT_BAUD)]
    baud: u32,
    /// 8E1, 8N2, 8N1 or 8O1.
    #[arg(long, global = true, default_value = "8E1")]
    framing: Framing,
    /// Modbus station address (P00-23).
    #[arg(long, global = true, default_value_t = 1)]
    slave: u8,
    #[arg(long, global = true, default_value_t = 300)]
    timeout_ms: u64,
    /// Most registers per read frame (the drive's real limit is not yet measured).
    #[arg(long, global = true, default_value_t = link::DEFAULT_MAX_READ)]
    max_read: u16,
    /// Use a simulated drive with factory values instead of a port.
    #[arg(long, global = true)]
    sim: bool,
    /// Use a simulated drive loaded from this snapshot or JMC export.
    #[arg(long, global = true, value_name = "FILE")]
    sim_from: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// List serial ports.
    Ports,
    /// Find the baud rate and framing the drive on --port answers on.
    Probe,
    /// List parameters from the built-in table (no drive needed).
    Params {
        /// Filter by code or name (case-insensitive).
        query: Option<String>,
        #[arg(long)]
        group: Option<u8>,
        /// Include the reserved registers (no function, no value).
        #[arg(long)]
        all: bool,
    },
    /// One parameter's full definition: range, choices, brief description.
    Info { code: String },
    /// Read parameters from the drive.
    Read {
        #[arg(required = true)]
        codes: Vec<String>,
    },
    /// Read every parameter and save a snapshot (JSON).
    Snapshot {
        /// Snapshot file to write (JSON).
        #[arg(long)]
        out: PathBuf,
        /// Axis this drive drives: X, YL, YR or Z.
        #[arg(long)]
        axis: Option<String>,
        /// Free text stored in the snapshot.
        #[arg(long)]
        note: Option<String>,
        /// Replace an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Write parameters. RAM only unless --save. Checks the drive matches --axis first.
    Write(WriteArgs),
    /// Stream live samples (position, following error, speed, torque, bus voltage) as JSON lines.
    Monitor {
        /// Samples to take; 0 runs until stopped.
        #[arg(long, default_value_t = 10)]
        count: u64,
        #[arg(long, default_value_t = 100)]
        interval_ms: u64,
    },
    /// Servo on, fault and warning flags, active alarm codes.
    Status,
    /// Clear the drive's alarm. If the servo is still enabled it switches back on.
    ResetAlarm {
        /// Confirm the reset.
        #[arg(long)]
        yes: bool,
    },
    /// Read registers by address (decimal, or hex with 0x).
    RawRead {
        addr: String,
        #[arg(default_value_t = 1)]
        count: u16,
    },
    /// Find the largest block the drive answers in one read frame.
    BenchReadSize {
        #[arg(long, default_value_t = rtu::MAX_READ)]
        max: u16,
    },
    /// Turn a JMC 2.4.3 export (.xml) into a snapshot.
    ImportXml {
        /// The JMC export.
        file: PathBuf,
        /// Axis the export came from: X, YL, YR or Z.
        #[arg(long)]
        axis: Option<String>,
        /// Free text stored in the snapshot.
        #[arg(long)]
        note: Option<String>,
        /// When JMC read the drive, e.g. 2023-07-25T14:21:38Z (default: the file's modified time).
        #[arg(long)]
        taken: Option<String>,
        /// Save here instead of printing the snapshot.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Write a snapshot as a JMC 2.4.3 export, byte-compatible with JMC's own files.
    ExportXml {
        /// Snapshot file (JSON).
        snapshot: PathBuf,
        /// Export file to write (.xml).
        #[arg(long)]
        out: PathBuf,
        /// Replace an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Compare two snapshots or JMC exports.
    Diff { a: PathBuf, b: PathBuf },
    /// Which axis does the drive (or --file) match? Compares against saved baselines.
    Identify {
        /// Folder of snapshots with an axis set, searched recursively.
        #[arg(long, env = "JMC_BASELINES")]
        baselines: PathBuf,
        /// Identify this snapshot or export instead of reading the drive.
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Args)]
struct WriteArgs {
    /// CODE=VALUE pairs in display units, e.g. P02-00=520.0
    #[arg(required = true, value_name = "CODE=VALUE")]
    assignments: Vec<String>,
    /// The axis this drive drives: X, YL, YR or Z.
    #[arg(long)]
    axis: String,
    /// Also save to EEPROM (address bit 15, not yet confirmed on these drives). Needs --yes.
    #[arg(long)]
    save: bool,
    /// Confirm an EEPROM save.
    #[arg(long)]
    yes: bool,
    /// Allow motor data, factory, read-only, spare and serial-link parameters.
    #[arg(long)]
    allow_protected: bool,
    /// Folder of baseline snapshots, used to check the drive is the named axis.
    #[arg(long, env = "JMC_BASELINES")]
    baselines: Option<PathBuf>,
    /// Skip the axis check.
    #[arg(long)]
    no_identify_check: bool,
    /// Where the snapshot taken before writing goes (default %LOCALAPPDATA%\JMC-Servo-Tool\backups).
    #[arg(long, env = "JMC_BACKUP_DIR")]
    backup_dir: Option<PathBuf>,
}

/// A command's JSON result and whether it fully succeeded.
struct Out {
    value: Value,
    ok: bool,
}

impl From<Value> for Out {
    fn from(value: Value) -> Out {
        Out { value, ok: true }
    }
}

fn main() {
    let cli = Cli::parse();
    let compact = cli.compact;
    match run(cli) {
        Ok(out) => {
            if !out.value.is_null() {
                print_json(&out.value, compact);
            }
            if !out.ok {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", json!({ "error": format!("{e:#}") }));
            std::process::exit(1);
        }
    }
}

fn print_json(v: &Value, compact: bool) {
    let text = if compact { serde_json::to_string(v) } else { serde_json::to_string_pretty(v) };
    println!("{}", text.unwrap_or_else(|e| json!({ "error": e.to_string() }).to_string()));
}

fn run(cli: Cli) -> Result<Out> {
    let schema = Schema::model512();
    let c = &cli.conn;
    Ok(match cli.cmd {
        Cmd::Ports => json!(link::list_ports()?).into(),
        Cmd::Probe => {
            let port = c.port.clone().context("probe needs --port (see `jmc ports`)")?;
            let (link, hit) = link::probe(&port, c.slave, &link::probe_candidates())?;
            json!({ "found": hit, "link": link.stats }).into()
        }
        Cmd::Params { query, group, all } => {
            let q = query.as_deref().map(str::to_lowercase);
            let rows: Vec<Value> = schema
                .params
                .iter()
                .filter(|p| all || !p.reserved)
                .filter(|p| group.is_none_or(|g| p.group == g))
                .filter(|p| q.as_deref().is_none_or(|q| p.code.to_lowercase().contains(q) || p.name.to_lowercase().contains(q)))
                .map(param_row)
                .collect();
            json!(rows).into()
        }
        Cmd::Info { code } => info(schema.get(&code)?).into(),
        Cmd::Read { codes } => {
            let defs = codes.iter().map(|q| schema.get(q)).collect::<Result<Vec<_>, _>>()?;
            let mut d = open_drive(c)?;
            let mut rows = Vec::new();
            for def in defs {
                let raw = d.read_param(def)?;
                rows.push(value_row(def, raw));
            }
            json!(rows).into()
        }
        Cmd::Snapshot { out, axis, note, force } => {
            refuse_overwrite(&out, force)?;
            let mut d = open_drive(c)?;
            let t = Instant::now();
            let mut s = d.snapshot(|_, _| {})?;
            s.axis = axis.map(|a| a.to_ascii_uppercase());
            s.note = note;
            s.save(&out)?;
            json!({
                "out": out,
                "axis": s.axis,
                "values": s.values.len(),
                "unread": s.unread,
                "seconds": (t.elapsed().as_secs_f64() * 100.0).round() / 100.0,
                "link": d.link.stats,
            })
            .into()
        }
        Cmd::Write(w) => write(c, schema, w)?,
        Cmd::Monitor { count, interval_ms } => {
            monitor(c, count, Duration::from_millis(interval_ms))?;
            Value::Null.into()
        }
        Cmd::Status => {
            let mut d = open_drive(c)?;
            json!(d.status()?).into()
        }
        Cmd::ResetAlarm { yes } => {
            if !yes {
                bail!("reset-alarm needs --yes: if the servo is still enabled it switches back on");
            }
            let mut d = open_drive(c)?;
            d.reset_alarm()?;
            std::thread::sleep(Duration::from_millis(100));
            json!({ "reset": true, "status": d.status()? }).into()
        }
        Cmd::RawRead { addr, count } => {
            let a = parse_addr(&addr)?;
            let mut d = open_drive(c)?;
            let values = d.link.read(a, count)?;
            let rows: Vec<Value> = values
                .iter()
                .enumerate()
                .map(|(k, &v)| {
                    let at = a.wrapping_add(k as u16);
                    json!({
                        "addr": at,
                        "hex": format!("0x{v:04X}"),
                        "u16": v,
                        "i16": v as i16,
                        "param": schema.by_address(at).map(|p| &p.code),
                    })
                })
                .collect();
            json!(rows).into()
        }
        Cmd::BenchReadSize { max } => bench_read_size(c, schema, max)?.into(),
        Cmd::ImportXml { file, axis, note, taken, out, force } => {
            let mut s = load_any(schema, &file)?;
            s.axis = axis.map(|a| a.to_ascii_uppercase());
            s.note = note;
            if let Some(t) = taken {
                s.taken_utc = parse_utc(&t)?;
            }
            match out {
                Some(path) => {
                    refuse_overwrite(&path, force)?;
                    s.save(&path)?;
                    json!({
                        "out": path,
                        "axis": s.axis,
                        "values": s.values.len(),
                        "unread": s.unread,
                        "taken_utc": s.taken_utc,
                    })
                    .into()
                }
                None => json!(s).into(),
            }
        }
        Cmd::ExportXml { snapshot, out, force } => {
            refuse_overwrite(&out, force)?;
            let s = load_any(schema, &snapshot)?;
            let text = jmcxml::write(schema, &s.values)?;
            let missing: Vec<&str> = schema.real().map(|p| p.code.as_str()).filter(|c| !s.values.contains_key(*c)).collect();
            let tmp = out.with_extension("xml.tmp");
            std::fs::write(&tmp, text.as_bytes()).with_context(|| format!("writing {}", tmp.display()))?;
            std::fs::rename(&tmp, &out).with_context(|| format!("writing {}", out.display()))?;
            let mut v = json!({ "out": out, "values": s.values.len(), "missing": missing });
            if !missing.is_empty() {
                v["warning"] = json!("missing parameters get the factory value from the table");
            }
            v.into()
        }
        Cmd::Diff { a, b } => {
            let (sa, sb) = (load_any(schema, &a)?, load_any(schema, &b)?);
            let rows = snapshot::diff(schema, &sa.values, &sb.values);
            json!({
                "a": { "file": a, "axis": sa.axis, "taken_utc": sa.taken_utc },
                "b": { "file": b, "axis": sb.axis, "taken_utc": sb.taken_utc },
                "differing": rows.len(),
                "rows": rows,
            })
            .into()
        }
        Cmd::Identify { baselines, file } => {
            let base = snapshot::load_baselines(&baselines)?;
            if base.is_empty() {
                bail!("no snapshots with an axis under {}", baselines.display());
            }
            let current = match file {
                Some(f) => load_any(schema, &f)?.raw(schema)?,
                None => open_drive(c)?.read_all(|_, _| {})?.values,
            };
            json!(snapshot::identify(schema, &current, &base)).into()
        }
    })
}

fn open_drive(c: &Conn) -> Result<Drive> {
    let schema = Schema::model512();
    let mut link = if c.sim || c.sim_from.is_some() {
        let sim = SimDrive::new(schema);
        sim.state().slave = c.slave;
        if let Some(f) = &c.sim_from {
            let s = load_any(schema, f)?;
            for (code, raw) in s.raw(schema)? {
                sim.set_param(schema.get(&code)?.address, raw);
            }
        }
        let mut l = Link::new(Box::new(sim), c.slave);
        l.label = "simulated drive".into();
        l
    } else {
        let port = c.port.clone().context("no port: pass --port COM5 or set JMC_PORT (see `jmc ports`), or use --sim")?;
        Link::open(&SerialSettings { port, baud: c.baud, framing: c.framing }, c.slave)?
    };
    link.timeout = Duration::from_millis(c.timeout_ms);
    link.max_read = c.max_read.clamp(1, rtu::MAX_READ);
    Ok(Drive::new(link, schema))
}

/// A snapshot (.json) or a JMC export (.xml).
fn load_any(schema: &Schema, path: &Path) -> Result<Snapshot> {
    Ok(Snapshot::load_any(schema, path)?)
}

fn refuse_overwrite(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!("{} exists; pass --force to replace it", path.display());
    }
    Ok(())
}

/// `2023-07-25T14:21:38Z`, or a date alone (midnight).
fn parse_utc(s: &str) -> Result<String> {
    let t = s.trim().trim_end_matches(['Z', 'z']);
    let (date, time) = t.split_once(['T', 't', ' ']).unwrap_or((t, "00:00:00"));
    let nums = |text: &str, sep: char, n: usize| -> Option<Vec<u32>> {
        let v: Vec<u32> = text.split(sep).map(|x| x.parse().ok()).collect::<Option<_>>()?;
        (v.len() == n).then_some(v)
    };
    let (Some(d), Some(h)) = (nums(date, '-', 3), nums(time, ':', 3)) else {
        bail!("time {s:?}: use 2023-07-25T14:21:38Z or 2023-07-25");
    };
    if !(1..=12).contains(&d[1]) || !(1..=31).contains(&d[2]) || h[0] > 23 || h[1] > 59 || h[2] > 59 {
        bail!("time {s:?} is not a valid date and time");
    }
    Ok(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", d[0], d[1], d[2], h[0], h[1], h[2]))
}

fn parse_addr(s: &str) -> Result<u16> {
    let t = s.trim();
    let v = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u16::from_str_radix(h, 16),
        None => t.parse(),
    };
    v.map_err(|_| anyhow!("address {s:?}: use a number like 2106 or 0x083A"))
}

fn flags(p: &ParamDef) -> Vec<&'static str> {
    let mut f = Vec::new();
    if p.reserved {
        f.push("reserved");
    } else if p.placeholder() {
        f.push("spare");
    }
    if p.protected() {
        f.push(if p.access == Access::Motor { "motor-data" } else { "factory" });
    }
    if LINK_PARAMS.contains(&p.code.as_str()) {
        f.push("serial-link");
    }
    if p.read_only() {
        f.push("read-only");
    }
    if p.needs_servo_off() {
        f.push("servo-off");
    }
    if p.needs_power_cycle() {
        f.push("power-cycle");
    }
    if p.signed {
        f.push("signed");
    }
    f
}

fn param_row(p: &ParamDef) -> Value {
    json!({
        "code": p.code,
        "name": p.name,
        "unit": p.unit,
        "default": p.default.map(|v| p.format_value(v)),
        "min": format_scaled(p.min, p.decimals),
        "max": format_scaled(p.max, p.decimals),
        "flags": flags(p),
    })
}

fn info(p: &ParamDef) -> Value {
    let mut v = param_row(p);
    v["address"] = json!(p.address);
    v["address_hex"] = json!(format!("0x{:04X}", p.address));
    v["decimals"] = json!(p.decimals);
    v["range"] = json!(p.range_text());
    v["applies"] = json!(p.applies);
    v["servo_off"] = json!(p.servo_off);
    v["access"] = json!(p.access);
    v["choices"] = json!(p.choices);
    v["brief"] = json!(p.brief);
    v
}

fn value_row(p: &ParamDef, raw: u16) -> Value {
    json!({
        "code": p.code,
        "name": p.name,
        "value": p.format(raw),
        "unit": p.unit,
        "raw": raw,
        "label": p.choice_label(raw),
    })
}

fn write(c: &Conn, schema: &'static Schema, w: WriteArgs) -> Result<Out> {
    let axis = w.axis.to_ascii_uppercase();
    let opts = WriteOptions { persist: if w.save { Persist::Eeprom } else { Persist::Ram }, allow_protected: w.allow_protected };
    if w.save && !w.yes {
        bail!("--save writes EEPROM through address bit 15, which is not yet confirmed on these drives; add --yes to go ahead");
    }
    // Check everything before touching the drive.
    let items = guard::parse_assignments(schema, &w.assignments, opts)?;
    let baselines = if w.no_identify_check {
        None
    } else {
        let dir = w.baselines.as_deref().with_context(|| {
            format!("write checks the drive is axis {axis} against saved baselines: pass --baselines DIR (or set JMC_BASELINES), or --no-identify-check")
        })?;
        Some(guard::baselines_for(dir, &axis)?)
    };
    let mut d = open_drive(c)?;
    let req = guard::WriteRequest {
        axis,
        items,
        opts,
        baselines: baselines.as_deref(),
        backup_dir: w.backup_dir.clone().unwrap_or_else(guard::default_backup_dir),
        description: w.assignments.join(" "),
    };
    let report = guard::guarded_write(&mut d, &req, |_, _| {})?;
    let ok = report.ok;
    let mut value = serde_json::to_value(&report)?;
    value["link"] = json!(d.link.stats);
    Ok(Out { value, ok })
}

fn monitor(c: &Conn, count: u64, interval: Duration) -> Result<()> {
    let mut d = open_drive(c)?;
    let start = Instant::now();
    let mut out = std::io::stdout().lock();
    let mut taken = 0u64;
    let mut failures = 0;
    loop {
        let t = Instant::now();
        let ms = start.elapsed().as_millis() as u64;
        let line = match d.sample() {
            Ok(s) => {
                failures = 0;
                json!({ "t_ms": ms, "sample": s })
            }
            Err(e) => {
                failures += 1;
                json!({ "t_ms": ms, "error": e.to_string() })
            }
        };
        // A closed pipe (e.g. `| head`) just ends the stream.
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            return Ok(());
        }
        if failures >= 5 {
            bail!("5 failed samples in a row; stopping");
        }
        taken += 1;
        if count != 0 && taken >= count {
            return Ok(());
        }
        std::thread::sleep(interval.saturating_sub(t.elapsed()));
    }
}

/// Read ever larger blocks from the longest run of parameter addresses until the drive refuses.
fn bench_read_size(c: &Conn, schema: &Schema, max: u16) -> Result<Value> {
    let mut addrs: Vec<u16> = schema.params.iter().map(|p| p.address).collect();
    addrs.sort_unstable();
    addrs.dedup();
    let (mut best_start, mut best_len, mut start, mut len) = (0u16, 0u16, 0u16, 0u16);
    for (k, &a) in addrs.iter().enumerate() {
        if k > 0 && addrs[k - 1] + 1 == a {
            len += 1;
        } else {
            start = a;
            len = 1;
        }
        if len > best_len {
            (best_start, best_len) = (start, len);
        }
    }
    let limit = max.clamp(1, rtu::MAX_READ).min(best_len);
    let mut sizes: Vec<u16> = [1, 2, 4, 8, 16, 24, 30, 32, 40, 48, 50, 60, 64, 80, 100, 120, 125].into_iter().filter(|&n| n <= limit).collect();
    if sizes.last() != Some(&limit) {
        sizes.push(limit);
    }
    let mut d = open_drive(c)?;
    d.link.retries = 1;
    let mut tries = Vec::new();
    let mut largest_ok = 0;
    for n in sizes {
        d.link.max_read = n;
        let t = Instant::now();
        let res = d.link.read(best_start, n);
        let ms = (t.elapsed().as_secs_f64() * 10_000.0).round() / 10.0;
        match res {
            Ok(_) => {
                largest_ok = n;
                tries.push(json!({ "count": n, "ok": true, "ms": ms }));
            }
            Err(e) => {
                tries.push(json!({ "count": n, "ok": false, "ms": ms, "error": e.to_string() }));
                break;
            }
        }
    }
    let mut v = json!({
        "start": best_start,
        "run_length": best_len,
        "largest_ok": largest_ok,
        "tries": tries,
        "link": d.link.stats,
    });
    if largest_ok == limit && limit < rtu::MAX_READ {
        v["note"] = json!(format!("the longest run of parameter addresses is {best_len} registers, so larger reads were not tried"));
    }
    Ok(v)
}
