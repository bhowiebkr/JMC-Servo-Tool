//! The safe write sequence, shared by the CLI and the GUI: check every value first, read the whole
//! drive, check it is the right axis, save a backup, then write and read back one at a time.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::drive::{Drive, Persist, WriteFailure, WriteOptions, WriteOutcome};
use crate::error::{Error, Result};
use crate::params::{ParamDef, Schema};
use crate::snapshot::{self, Snapshot};

/// Parse `CODE=VALUE` items and check each one against the write rules. Nothing touches the drive.
pub fn parse_assignments(schema: &'static Schema, items: &[String], opts: WriteOptions) -> Result<Vec<(&'static ParamDef, u16)>> {
    let mut plan: Vec<(&'static ParamDef, u16)> = Vec::new();
    for a in items {
        let (code, value) =
            a.split_once('=').ok_or_else(|| Error::Value(format!("{a:?}: use CODE=VALUE, e.g. P02-00=520.0")))?;
        let def = schema.get(code)?;
        check_new(&plan, def)?;
        let raw = def.parse_raw(value)?;
        Drive::check_write(def, opts)?;
        plan.push((def, raw));
    }
    Ok(plan)
}

fn check_new(plan: &[(&'static ParamDef, u16)], def: &ParamDef) -> Result<()> {
    if plan.iter().any(|(p, _)| p.code == def.code) {
        return Err(Error::Refused(format!("{} is given twice", def.code)));
    }
    Ok(())
}

/// Load the baselines under `dir` and make sure at least one is for `axis`.
pub fn baselines_for(dir: &Path, axis: &str) -> Result<Vec<(PathBuf, Snapshot)>> {
    let b = snapshot::load_baselines(dir)?;
    if !b.iter().any(|(_, s)| s.axis.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(axis))) {
        return Err(Error::Refused(format!("no baseline for axis {} under {}", axis.to_ascii_uppercase(), dir.display())));
    }
    Ok(b)
}

/// `%LOCALAPPDATA%\JMC-Servo-Tool\backups`.
pub fn default_backup_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("JMC-Servo-Tool")
        .join("backups")
}

/// `dir/stem.json`, or `dir/stem-2.json` and so on if that exists: a backup is never replaced.
pub fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let mut path = dir.join(format!("{stem}.json"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.json"));
        n += 1;
    }
    path
}

pub struct WriteRequest<'a> {
    /// The axis the user says this drive is.
    pub axis: String,
    /// From `parse_assignments`.
    pub items: Vec<(&'static ParamDef, u16)>,
    pub opts: WriteOptions,
    /// Baselines to check the axis against. `None` skips the axis check.
    pub baselines: Option<&'a [(PathBuf, Snapshot)]>,
    pub backup_dir: PathBuf,
    /// What is being written, for the backup's note, e.g. `P02-00=520.0`.
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentifySummary {
    /// Axes tied for the best match.
    #[serde(rename = "match")]
    pub tied: Vec<String>,
    /// How many baselines were compared.
    pub baselines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteReport {
    pub ok: bool,
    pub axis: String,
    pub identify: Option<IdentifySummary>,
    /// The snapshot saved before anything was written.
    pub backup: PathBuf,
    pub results: Vec<WriteOutcome>,
    /// The item that failed. Its outcome is in `results` when the drive read back a different value.
    pub failed: Option<String>,
    /// Items after the one that failed.
    pub not_attempted: Vec<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// Run the whole sequence. Errors before the backup is saved mean nothing was written; after that,
/// the report says what was written and what was not.
pub fn guarded_write(d: &mut Drive, req: &WriteRequest, progress: impl FnMut(usize, usize)) -> Result<WriteReport> {
    let axis = req.axis.trim().to_ascii_uppercase();
    if axis.is_empty() {
        return Err(Error::Refused("no axis given".into()));
    }
    let mut seen: Vec<(&'static ParamDef, u16)> = Vec::new();
    for &(def, raw) in &req.items {
        check_new(&seen, def)?;
        Drive::check_write(def, req.opts)?;
        seen.push((def, raw));
    }
    let schema = d.schema();
    let mut warnings = Vec::new();
    // One full read is both the axis check and the backup.
    let before = d.read_all(progress)?;
    let identify = match req.baselines {
        Some(b) => {
            let id = snapshot::identify(schema, &before.values, b);
            for s in &id.skipped {
                warnings.push(format!("baseline {} not used: {}", s.file.display(), s.reason));
            }
            if let Some(why) = id.refusal(&axis) {
                return Err(Error::Refused(format!("{why}; nothing written")));
            }
            Some(IdentifySummary { tied: id.tied, baselines: id.candidates.len() })
        }
        None => None,
    };
    // Range checks now that the current values are known. An unchanged value may sit outside the
    // table's range; it is still checked before it goes to EEPROM. A refused write saves no backup.
    let mut servo_off = None;
    for (def, raw) in &req.items {
        if before.values.get(&def.code) != Some(raw) || req.opts.persist == Persist::Eeprom {
            def.check_range(*raw)?;
            if def.needs_servo_off() {
                servo_off.get_or_insert(def);
            }
        }
    }
    if let Some(def) = servo_off
        && d.status()?.servo_on
    {
        return Err(Error::Refused(format!(
            "{} ({}) changes only with the servo off, and the servo is on: switch the servo off first; nothing written",
            def.code, def.name
        )));
    }
    let mut backup = Snapshot::from_raw(schema, &d.link.label, &before.values);
    backup.axis = Some(axis.clone());
    backup.unread = before.errors.clone();
    let checked = if identify.is_some() { "axis checked" } else { "axis not checked" };
    backup.note = Some(format!("before writing {} ({checked})", req.description));
    std::fs::create_dir_all(&req.backup_dir)
        .map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("creating {}: {e}", req.backup_dir.display()))))?;
    let backup_path = unique_path(&req.backup_dir, &format!("{axis}-{}", snapshot::stamp_for_file(&backup.taken_utc)));
    backup.save(&backup_path)?;

    let mut results = Vec::new();
    let mut error = None;
    let mut failed = None;
    let mut attempted = 0;
    for (def, raw) in &req.items {
        attempted += 1;
        match d.write_param_detailed(def, *raw, req.opts) {
            Ok(o) => results.push(o),
            Err(WriteFailure { error: e, outcome }) => {
                error = Some(format!("{}: {e}", def.code));
                failed = Some(def.code.clone());
                results.extend(outcome.map(|o| *o));
                break;
            }
        }
    }
    let ok = error.is_none();
    if req.opts.persist == Persist::Eeprom {
        warnings.push("EEPROM save uses address bit 15, unconfirmed: power-cycle the drive and read back to be sure".to_string());
    } else if results.iter().any(|o| o.written) {
        warnings.push("written to RAM only: the drive forgets these at power-off".to_string());
    }
    let not_attempted = req.items.iter().skip(attempted).map(|(p, _)| p.code.clone()).collect();
    Ok(WriteReport { ok, axis, identify, backup: backup_path, results, failed, not_attempted, error, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Link;
    use crate::sim::SimDrive;

    fn sim_drive() -> (Drive, SimDrive) {
        let schema = Schema::model512();
        let sim = SimDrive::new(schema);
        let mut link = Link::new(Box::new(sim.clone()), 1);
        link.label = "simulated drive".into();
        (Drive::new(link, schema), sim)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jmc-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parse_rejects_bad_items() {
        let s = Schema::model512();
        let opts = WriteOptions { persist: Persist::Ram, allow_protected: false };
        let e = parse_assignments(s, &["P02-00".into()], opts).unwrap_err().to_string();
        assert!(e.contains("CODE=VALUE"), "{e}");
        let e = parse_assignments(s, &["P02-00=520".into(), "p2-0=500".into()], opts).unwrap_err().to_string();
        assert!(e.contains("twice"), "{e}");
        let e = parse_assignments(s, &["P00-21=3".into()], opts).unwrap_err().to_string();
        assert!(e.contains("serial settings"), "{e}");
        // A power cycle clears RAM: such a write needs an EEPROM save.
        let e = parse_assignments(s, &["P03-09=5000".into()], opts).unwrap_err().to_string();
        assert!(e.contains("power cycle"), "{e}");
        let eeprom = WriteOptions { persist: Persist::Eeprom, allow_protected: false };
        assert!(parse_assignments(s, &["P03-09=5000".into()], eeprom).is_ok());
        let ok = parse_assignments(s, &["P02-00=520.0".into()], opts).unwrap();
        assert_eq!(ok[0].0.code, "P02-00");
    }

    #[test]
    fn writes_with_backup_and_refuses_wrong_axis() {
        let schema = Schema::model512();
        let dir = temp_dir("write");
        let (mut d, sim) = sim_drive();
        // A baseline for X taken from this drive, and one for Z that differs.
        let all = d.read_all(|_, _| {}).unwrap();
        let mut x = Snapshot::from_raw(schema, "test", &all.values);
        x.axis = Some("X".into());
        let mut zvals = all.values.clone();
        for code in ["P02-00", "P02-01", "P02-02"] {
            *zvals.get_mut(code).unwrap() += 1;
        }
        let mut z = Snapshot::from_raw(schema, "test", &zvals);
        z.axis = Some("Z".into());
        let baselines = vec![(dir.join("x.json"), x), (dir.join("z.json"), z)];
        let opts = WriteOptions { persist: Persist::Ram, allow_protected: false };
        let items = parse_assignments(schema, &["P02-00=123.4".into()], opts).unwrap();

        let mut req = WriteRequest {
            axis: "z".into(),
            items: items.clone(),
            opts,
            baselines: Some(&baselines),
            backup_dir: dir.join("backups"),
            description: "P02-00=123.4".into(),
        };
        let e = guarded_write(&mut d, &req, |_, _| {}).unwrap_err().to_string();
        assert!(e.contains("best match is X"), "{e}");
        assert!(!dir.join("backups").exists(), "a refused write must not save a backup");

        // Out of range: refused before the backup.
        req.axis = "x".into();
        let def = schema.get("P02-00").unwrap();
        assert!(def.max < 65535);
        req.items = vec![(def, (def.max + 1) as u16)];
        assert!(matches!(guarded_write(&mut d, &req, |_, _| {}), Err(Error::Value(_))));
        assert!(!dir.join("backups").exists(), "a refused write must not save a backup");

        req.items = items;
        let r = guarded_write(&mut d, &req, |_, _| {}).unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.axis, "X");
        assert!(r.backup.exists());
        assert_eq!(r.results.len(), 1);
        assert_eq!(sim.state().ram.get(&def.address).copied(), Some(1234));
        assert!(r.warnings.iter().any(|w| w.contains("RAM only")));

        // A read-back mismatch: the outcome shows the value that the drive kept, and the rest is not tried.
        let p1 = schema.get("P02-01").unwrap();
        sim.state().ignore_writes.push(p1.address);
        req.items = parse_assignments(schema, &["P02-01=99.0".into(), "P02-03=40.0".into()], opts).unwrap();
        let r = guarded_write(&mut d, &req, |_, _| {}).unwrap();
        assert!(!r.ok);
        assert_eq!(r.failed.as_deref(), Some("P02-01"));
        let o = r.results.last().unwrap();
        assert_eq!(o.code, "P02-01");
        assert!(o.written);
        assert_eq!(o.after, o.before);
        assert_eq!(r.not_attempted, vec!["P02-03".to_string()]);

        // With baselines for one axis only, the check cannot tell axes apart: nothing is written.
        let backups = || std::fs::read_dir(dir.join("backups")).unwrap().count();
        let n = backups();
        let only_x = vec![baselines[0].clone()];
        req.baselines = Some(&only_x);
        req.items = parse_assignments(schema, &["P02-00=123.5".into()], opts).unwrap();
        let e = guarded_write(&mut d, &req, |_, _| {}).unwrap_err().to_string();
        assert!(e.contains("cannot tell axis X"), "{e}");
        assert_eq!(backups(), n, "a refused write must not save a backup");

        // A parameter that changes only with the servo off: refused before the backup while the servo is on.
        req.baselines = Some(&baselines);
        let p = schema
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
        req.items = vec![(p, v)];
        d.link.write_one(crate::monitor::SERVO_ENABLE, 1).unwrap();
        let e = guarded_write(&mut d, &req, |_, _| {}).unwrap_err().to_string();
        assert!(e.contains("servo off"), "{e}");
        assert_eq!(backups(), n, "a refused write must not save a backup");
        d.link.write_one(crate::monitor::SERVO_ENABLE, 0).unwrap();
        assert!(guarded_write(&mut d, &req, |_, _| {}).unwrap().ok);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
