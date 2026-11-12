//! The safe write sequence, shared by the CLI and the GUI: check every value first, read the whole
//! drive, check it is the right axis, save a backup, then write and read back one at a time.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::drive::{Drive, Persist, WriteOptions, WriteOutcome};
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
    // One full read is both the axis check and the backup.
    let before = d.read_all(progress)?;
    let identify = match req.baselines {
        Some(b) => {
            let id = snapshot::identify(schema, &before.values, b);
            if !id.consistent_with(&axis) {
                let best = if id.tied.is_empty() { "nothing".to_string() } else { id.tied.join("/") };
                return Err(Error::Refused(format!(
                    "this drive does not look like axis {axis}: best match is {best}; nothing written"
                )));
            }
            Some(IdentifySummary { tied: id.tied, baselines: id.candidates.len() })
        }
        None => None,
    };
    let mut backup = Snapshot::from_raw(schema, &d.link.label, &before.values);
    backup.axis = Some(axis.clone());
    backup.unread = before.errors.clone();
    let checked = if identify.is_some() { "axis checked" } else { "axis not checked" };
    backup.note = Some(format!("before writing {} ({checked})", req.description));
    std::fs::create_dir_all(&req.backup_dir)
        .map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("creating {}: {e}", req.backup_dir.display()))))?;
    let backup_path = unique_path(&req.backup_dir, &format!("{axis}-{}", snapshot::stamp_for_file(&backup.taken_utc)));
    backup.save(&backup_path)?;
    // Range checks now that the current values are known: an unchanged value may sit outside the table's range.
    for (def, raw) in &req.items {
        if before.values.get(&def.code) != Some(raw) {
            def.check_range(*raw)?;
        }
    }

    let mut results = Vec::new();
    let mut error = None;
    for (def, raw) in &req.items {
        match d.write_param(def, *raw, req.opts) {
            Ok(o) => results.push(o),
            Err(e) => {
                error = Some(format!("{}: {e}", def.code));
                break;
            }
        }
    }
    let ok = error.is_none();
    let mut warnings = Vec::new();
    if req.opts.persist == Persist::Eeprom {
        warnings.push("EEPROM save uses address bit 15, unconfirmed: power-cycle the drive and read back to be sure".to_string());
    } else if results.iter().any(|o| o.written) {
        warnings.push("written to RAM only: the drive forgets these at power-off".to_string());
    }
    let not_attempted =
        req.items.iter().skip(results.len() + usize::from(!ok)).map(|(p, _)| p.code.clone()).collect();
    Ok(WriteReport { ok, axis, identify, backup: backup_path, results, not_attempted, error, warnings })
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

        req.axis = "x".into();
        let r = guarded_write(&mut d, &req, |_, _| {}).unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.axis, "X");
        assert!(r.backup.exists());
        assert_eq!(r.results.len(), 1);
        let def = schema.get("P02-00").unwrap();
        assert_eq!(sim.state().ram.get(&def.address).copied(), Some(1234));
        assert!(r.warnings.iter().any(|w| w.contains("RAM only")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
