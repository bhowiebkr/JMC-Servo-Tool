//! Parameter reads and safe writes on one drive.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::link::Link;
use crate::monitor::{self, AbsLayout, Sample, Status};
use crate::params::{Access, ParamDef, Schema};
use crate::rtu::SAVE_BIT;
use crate::snapshot::Snapshot;

/// The drive's serial settings. Changing them drops the connection, so they are protected.
pub const LINK_PARAMS: [&str; 6] = ["P00-21", "P00-22", "P00-23", "P00-24", "P00-25", "P00-26"];
/// JMC 2.4.3 waits 25 ms between parameter writes.
pub const WRITE_GAP: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Persist {
    /// Plain write: lost at power-off.
    #[default]
    Ram,
    /// Write with address bit 15, which JMC's tool uses to save to EEPROM (not yet confirmed).
    Eeprom,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WriteOptions {
    pub persist: Persist,
    /// Allow motor data, factory, read-only and serial-link parameters.
    pub allow_protected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteOutcome {
    pub code: String,
    pub name: String,
    pub address: u16,
    pub before: String,
    pub requested: String,
    pub after: String,
    /// False when the value was already set and only RAM was asked for.
    pub written: bool,
    pub persist: Persist,
    pub notes: Vec<String>,
}

/// A write that failed. `outcome` is set when the value was sent and read back, so it shows what
/// the drive holds now.
#[derive(Debug)]
pub struct WriteFailure {
    pub error: Error,
    pub outcome: Option<Box<WriteOutcome>>,
}

impl From<Error> for WriteFailure {
    fn from(error: Error) -> Self {
        WriteFailure { error, outcome: None }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReadAll {
    pub values: BTreeMap<String, u16>,
    /// Parameters the drive refused, with the reason.
    pub errors: BTreeMap<String, String>,
    pub frames: u64,
}

pub struct Drive {
    pub link: Link,
    schema: &'static Schema,
    last_write: Option<Instant>,
    /// From P00-29, read at the first sample of the live monitor.
    abs_layout: Option<AbsLayout>,
}

impl Drive {
    pub fn new(link: Link, schema: &'static Schema) -> Drive {
        Drive { link, schema, last_write: None, abs_layout: None }
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    pub fn read_param(&mut self, def: &ParamDef) -> Result<u16> {
        self.link.read_one(def.address)
    }

    /// Read every parameter in as few frames as `max_read` allows. A block the drive refuses is
    /// read again one parameter at a time, so one hidden parameter can't hide its neighbours.
    /// A timeout aborts the whole read.
    pub fn read_all(&mut self, mut progress: impl FnMut(usize, usize)) -> Result<ReadAll> {
        let mut defs: Vec<&ParamDef> = self.schema.real().collect();
        defs.sort_by_key(|d| d.address);
        let max = usize::from(self.link.max_read.max(1));
        let mut runs: Vec<Vec<&ParamDef>> = Vec::new();
        for d in defs {
            match runs.last_mut() {
                Some(r) if r.len() < max && r[r.len() - 1].address + 1 == d.address => r.push(d),
                _ => runs.push(vec![d]),
            }
        }
        let total: usize = runs.iter().map(Vec::len).sum();
        let start_frames = self.link.stats.requests;
        let mut out = ReadAll::default();
        let mut done = 0;
        for run in runs {
            match self.link.read(run[0].address, run.len() as u16) {
                Ok(v) => {
                    for (d, raw) in run.iter().zip(v) {
                        out.values.insert(d.code.clone(), raw);
                    }
                }
                Err(Error::Exception { .. }) => {
                    for d in &run {
                        match self.link.read_one(d.address) {
                            Ok(raw) => {
                                out.values.insert(d.code.clone(), raw);
                            }
                            Err(e @ Error::Exception { .. }) => {
                                out.errors.insert(d.code.clone(), e.to_string());
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                Err(e) => return Err(e),
            }
            done += run.len();
            progress(done, total);
        }
        out.frames = self.link.stats.requests - start_frames;
        Ok(out)
    }

    pub fn snapshot(&mut self, progress: impl FnMut(usize, usize)) -> Result<Snapshot> {
        let all = self.read_all(progress)?;
        let source = if self.link.label.is_empty() { "drive".to_string() } else { format!("drive on {}", self.link.label) };
        let mut s = Snapshot::from_raw(self.schema, &source, &all.values);
        s.unread = all.errors;
        Ok(s)
    }

    /// The tool's own rules, checked before touching the drive.
    pub fn check_write(def: &ParamDef, opts: WriteOptions) -> Result<()> {
        if def.reserved {
            return Err(Error::Refused(format!("{} is a reserved register with no function", def.code)));
        }
        if !opts.allow_protected {
            Self::check_protected(def)?;
        }
        // A power cycle clears RAM, so this write could never take effect.
        if opts.persist == Persist::Ram && def.needs_power_cycle() {
            return Err(Error::Refused(format!(
                "{} ({}) takes effect only after a power cycle, which clears RAM, so a RAM write does nothing; it needs an EEPROM save",
                def.code, def.name
            )));
        }
        Ok(())
    }

    /// The parameters that need `allow_protected`.
    fn check_protected(def: &ParamDef) -> Result<()> {
        if def.protected() {
            let what = if def.access == Access::Motor { "motor data" } else { "a factory parameter" };
            return Err(Error::Refused(format!("{} ({}) is {what}; it needs allow-protected", def.code, def.name)));
        }
        if LINK_PARAMS.contains(&def.code.as_str()) {
            return Err(Error::Refused(format!(
                "{} ({}) changes the drive's serial settings and would drop the connection; it needs allow-protected",
                def.code, def.name
            )));
        }
        if def.placeholder() {
            return Err(Error::Refused(format!("{} is a spare register with no function; it needs allow-protected", def.code)));
        }
        if def.read_only() {
            return Err(Error::Refused(format!(
                "{} ({}) is marked read-only in the parameter table; it needs allow-protected to try",
                def.code, def.name
            )));
        }
        Ok(())
    }

    /// Write one parameter: check the rules and range, read the old value, write, read back.
    pub fn write_param(&mut self, def: &ParamDef, raw: u16, opts: WriteOptions) -> Result<WriteOutcome> {
        self.write_param_detailed(def, raw, opts).map_err(|f| f.error)
    }

    /// As `write_param`, but a read-back mismatch keeps the outcome with the value read back.
    pub fn write_param_detailed(
        &mut self,
        def: &ParamDef,
        raw: u16,
        opts: WriteOptions,
    ) -> std::result::Result<WriteOutcome, WriteFailure> {
        Self::check_write(def, opts)?;
        let before = self.read_param(def)?;
        // An unchanged value may sit outside the table's range. It is still checked before it goes to EEPROM.
        if before != raw || opts.persist == Persist::Eeprom {
            def.check_range(raw)?;
        }
        let mut notes = Vec::new();
        if def.needs_servo_off() {
            notes.push("only changes while the servo is off".to_string());
        }
        if def.needs_power_cycle() {
            notes.push("takes effect after the drive is powered off and on".to_string());
        }
        let mut outcome = WriteOutcome {
            code: def.code.clone(),
            name: def.name.clone(),
            address: def.address,
            before: def.format(before),
            requested: def.format(raw),
            after: def.format(before),
            written: false,
            persist: opts.persist,
            notes,
        };
        if before == raw && opts.persist == Persist::Ram {
            return Ok(outcome);
        }
        if def.needs_servo_off() && self.status()?.servo_on {
            return Err(WriteFailure::from(Error::Refused(format!(
                "{} ({}) changes only with the servo off, and the servo is on: switch the servo off first",
                def.code, def.name
            ))));
        }
        if let Some(t) = self.last_write {
            let since = t.elapsed();
            if since < WRITE_GAP {
                std::thread::sleep(WRITE_GAP - since);
            }
        }
        let addr = match opts.persist {
            Persist::Ram => def.address,
            Persist::Eeprom => def.address | SAVE_BIT,
        };
        let res = self.link.write_one(addr, raw);
        self.last_write = Some(Instant::now());
        res?;
        let after = self.read_param(def)?;
        outcome.after = def.format(after);
        outcome.written = true;
        if after != raw {
            let error =
                Error::BadReply(format!("{} reads back {} after writing {}", def.code, def.format(after), def.format(raw)));
            return Err(WriteFailure { error, outcome: Some(Box::new(outcome)) });
        }
        Ok(outcome)
    }

    pub fn status(&mut self) -> Result<Status> {
        let s = self.link.read(monitor::STATUS_ADDR, 2)?;
        let a = self.link.read(monitor::ALARM_ADDR, monitor::ALARM_LEN)?;
        Status::decode(&s, &a)
    }

    /// One read of the monitor block. The first sample also reads P00-29, which tells which register
    /// holds the absolute turn count. If that read fails, the factory layout is used, and the next
    /// sample tries again.
    pub fn sample(&mut self) -> Result<Sample> {
        let layout = match self.abs_layout {
            Some(l) => l,
            None => {
                let schema = self.schema;
                match schema.get("P00-29").and_then(|p| self.read_param(p)) {
                    Ok(v) => *self.abs_layout.insert(AbsLayout::from_p00_29(v)),
                    Err(_) => AbsLayout::default(),
                }
            }
        };
        let r = self.link.read(monitor::MONITOR_ADDR, monitor::MONITOR_LEN)?;
        Sample::decode(&r, layout)
    }

    /// Read P00-29 again at the next sample. Call it when a monitor run starts: P00-29 applies after
    /// a power cycle, which can happen while the port stays open.
    pub fn forget_monitor_layout(&mut self) {
        self.abs_layout = None;
    }

    pub fn identity(&mut self) -> Result<Vec<u16>> {
        self.link.read(monitor::IDENTITY_ADDR, 5)
    }

    /// One try only: if the reply is lost, a second reset could clear a new alarm that the user has not seen.
    pub fn reset_alarm(&mut self) -> Result<()> {
        self.link.write_one_once(monitor::ALARM_RESET, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimDrive;

    fn drive() -> (SimDrive, Drive) {
        let schema = Schema::model512();
        let sim = SimDrive::new(schema);
        let mut link = Link::new(Box::new(sim.clone()), 1);
        link.timeout = Duration::from_millis(30);
        (sim, Drive::new(link, schema))
    }

    #[test]
    fn read_all_matches_factory_values() {
        let (sim, mut d) = drive();
        let all = d.read_all(|_, _| {}).unwrap();
        assert_eq!(all.values.len(), 447);
        assert!(all.errors.is_empty());
        let s = d.schema();
        for p in s.real() {
            let want = p.default.and_then(|v| p.to_raw(v).ok()).unwrap_or(0);
            assert_eq!(all.values[&p.code], want, "{}", p.code);
        }
        // Reserved gaps split the blocks: 27 frames at 30 registers per read.
        assert!(all.frames <= 30, "{} frames", all.frames);
        drop(sim);
    }

    #[test]
    fn hidden_parameters_fall_back_to_single_reads() {
        let (sim, mut d) = drive();
        let hidden = d.schema().get("P10-05").unwrap().address;
        sim.state().hidden.push(hidden);
        let all = d.read_all(|_, _| {}).unwrap();
        assert_eq!(all.values.len(), 446);
        assert!(all.errors.contains_key("P10-05"));
    }

    #[test]
    fn writes_are_checked_and_verified() {
        let (sim, mut d) = drive();
        let s = d.schema();
        let gain = s.get("P02-00").unwrap();
        let o = d.write_param(gain, gain.parse("812.5").unwrap(), WriteOptions::default()).unwrap();
        assert!(o.written);
        assert_eq!(o.after, "812.5");
        assert_eq!(sim.state().ram[&gain.address], 8125);
        assert_ne!(sim.state().eeprom[&gain.address], 8125);
        // Unchanged RAM write does nothing.
        let n = sim.state().log.len();
        let o = d.write_param(gain, 8125, WriteOptions::default()).unwrap();
        assert!(!o.written);
        assert_eq!(sim.state().log.len(), n + 1, "only the read");
        // EEPROM write.
        let o = d.write_param(gain, 8125, WriteOptions { persist: Persist::Eeprom, allow_protected: false }).unwrap();
        assert!(o.written);
        sim.power_cycle();
        assert_eq!(d.read_param(gain).unwrap(), 8125);
        // Protected, link and reserved parameters.
        for code in ["P00-00", "P00-24", "P02-53", "P01-00", "P10-00"] {
            let p = s.get(code).unwrap();
            assert!(matches!(d.write_param(p, 1, WriteOptions::default()), Err(Error::Refused(_))), "{code}");
        }
        // Out of range.
        let p = s.real().find(|p| !p.signed && p.max < 65535 && Drive::check_write(p, WriteOptions::default()).is_ok()).unwrap();
        assert!(matches!(d.write_param(p, (p.max + 1) as u16, WriteOptions::default()), Err(Error::Value(_))));
        // The drive refusing a write surfaces as an exception.
        sim.state().reject_writes.push(gain.address);
        assert!(matches!(d.write_param(gain, 9000, WriteOptions::default()), Err(Error::Exception { code: 3, .. })));
        // A read-back mismatch keeps the value that the drive holds.
        sim.state().reject_writes.clear();
        sim.state().ignore_writes.push(gain.address);
        let f = d.write_param_detailed(gain, 9000, WriteOptions::default()).unwrap_err();
        assert!(matches!(f.error, Error::BadReply(_)), "{}", f.error);
        let o = f.outcome.unwrap();
        assert!(o.written);
        assert_eq!((o.requested.as_str(), o.after.as_str()), ("900.0", "812.5"));
        // An unchanged value outside the range is not saved to EEPROM.
        let p = s.real().find(|p| !p.signed && p.max < 65535 && Drive::check_write(p, WriteOptions::default()).is_ok()).unwrap();
        sim.set_param(p.address, (p.max + 1) as u16);
        let eeprom = WriteOptions { persist: Persist::Eeprom, allow_protected: false };
        assert!(matches!(d.write_param(p, (p.max + 1) as u16, eeprom), Err(Error::Value(_))));
        assert!(!d.write_param(p, (p.max + 1) as u16, WriteOptions::default()).unwrap().written);
    }

    #[test]
    fn status_and_monitor() {
        let (sim, mut d) = drive();
        assert!(!d.status().unwrap().servo_on);
        d.link.write_one(monitor::SERVO_ENABLE, 1).unwrap();
        assert!(d.status().unwrap().servo_on);
        sim.state().live.insert(2020, 0x8000);
        sim.state().live.insert(2026, 0x0101);
        let st = d.status().unwrap();
        assert!(st.fault);
        assert_eq!(st.alarms, vec![0x0101]);
        d.reset_alarm().unwrap();
        assert!(!d.status().unwrap().fault);
        // The reset is not sent again when its reply is lost.
        let n = sim.state().log.len();
        sim.push_fault(crate::sim::Fault::DropReply);
        assert!(d.reset_alarm().is_err());
        assert_eq!(sim.state().log.len(), n + 1);
        assert_eq!(d.sample().unwrap().bus_v, 310);
        // P00-29 tells which register holds the turns. The factory value 1 puts them in 2126.
        sim.state().live.insert(2125, 7);
        sim.state().live.insert(2126, 3);
        assert_eq!(d.sample().unwrap().abs_turns, 3);
        let p = d.schema().get("P00-29").unwrap();
        sim.set_param(p.address, 0);
        assert_eq!(d.sample().unwrap().abs_turns, 3, "P00-29 is read once for each monitor run");
        d.forget_monitor_layout();
        assert_eq!(d.sample().unwrap().abs_turns, 7);
    }

    #[test]
    fn servo_off_parameters_need_the_servo_off() {
        let (_sim, mut d) = drive();
        let s = d.schema();
        let p = s
            .real()
            .find(|p| {
                p.needs_servo_off()
                    && !p.needs_power_cycle()
                    && p.max > p.min
                    && Drive::check_write(p, WriteOptions::default()).is_ok()
            })
            .unwrap();
        let now = d.read_param(p).unwrap();
        let v = if p.value_of(now) < p.max { now.wrapping_add(1) } else { now.wrapping_sub(1) };
        d.link.write_one(monitor::SERVO_ENABLE, 1).unwrap();
        let e = d.write_param(p, v, WriteOptions::default()).unwrap_err();
        assert!(matches!(e, Error::Refused(_)) && e.to_string().contains("servo off"), "{e}");
        assert_eq!(d.read_param(p).unwrap(), now);
        d.link.write_one(monitor::SERVO_ENABLE, 0).unwrap();
        assert!(d.write_param(p, v, WriteOptions::default()).unwrap().written);
    }
}
