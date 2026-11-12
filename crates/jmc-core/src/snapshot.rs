//! Saved drive settings (JSON), diffs between them, and matching a drive against saved baselines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::jmcxml::XmlParams;
use crate::params::Schema;

pub const FORMAT: &str = "jmc-servo-tool/snapshot/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    pub taken_utc: String,
    /// Where the values came from: a port description or an imported file name.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Parameter code to display text, e.g. `"P02-00": "500.0"`.
    pub values: BTreeMap<String, String>,
    /// Parameters that could not be read, with the reason.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unread: BTreeMap<String, String>,
}

impl Snapshot {
    pub fn new(model: &str, source: &str) -> Snapshot {
        Snapshot {
            format: FORMAT.into(),
            model: model.into(),
            axis: None,
            taken_utc: now_utc(),
            source: source.into(),
            note: None,
            values: BTreeMap::new(),
            unread: BTreeMap::new(),
        }
    }

    pub fn from_raw(schema: &Schema, source: &str, raw: &BTreeMap<String, u16>) -> Snapshot {
        let mut s = Snapshot::new(&schema.machine, source);
        for (code, &v) in raw {
            if let Ok(d) = schema.get(code) {
                s.values.insert(code.clone(), d.format(v));
            }
        }
        s
    }

    /// From a JMC export. Values keep the file's text (so exporting again gives the same file);
    /// values that do not parse go to `unread`.
    pub fn from_xml(schema: &Schema, xml: &XmlParams, source: &str) -> Snapshot {
        let mut s = Snapshot::new(xml.machine.as_deref().unwrap_or(&schema.machine), source);
        for (code, text) in &xml.values {
            match schema.get(code).and_then(|d| d.parse_raw(text)) {
                Ok(_) => {
                    s.values.insert(code.clone(), text.trim().to_string());
                }
                Err(e) => {
                    s.unread.insert(code.clone(), format!("{text:?}: {e}"));
                }
            }
        }
        s
    }

    pub fn load(path: &Path) -> Result<Snapshot> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))))?;
        let s: Snapshot = serde_json::from_str(&text).map_err(|e| Error::Json(format!("{}: {e}", path.display())))?;
        if !s.format.starts_with("jmc-servo-tool/snapshot/") {
            return Err(Error::Json(format!("{}: not a snapshot (format {:?})", path.display(), s.format)));
        }
        Ok(s)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        // Write next to the target and rename, so a crash never leaves half a file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// A snapshot (.json) or a JMC 2.4.3 export (.xml). An export's time is its file time, since
    /// JMC writes the file when it reads the drive.
    pub fn load_any(schema: &Schema, path: &Path) -> Result<Snapshot> {
        let is_xml = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml"));
        if !is_xml {
            return Snapshot::load(path);
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("reading {}: {e}", path.display()))))?;
        let xml = crate::jmcxml::read(&text).map_err(|e| match e {
            Error::Xml(m) => Error::Xml(format!("{}: {m}", path.display())),
            other => other,
        })?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut s = Snapshot::from_xml(schema, &xml, &format!("JMC export {name}"));
        if let Ok(t) = std::fs::metadata(path).and_then(|m| m.modified())
            && let Ok(d) = t.duration_since(UNIX_EPOCH)
        {
            s.taken_utc = format_utc(d.as_secs());
        }
        Ok(s)
    }

    /// Register values for every parameter that parses.
    pub fn raw(&self, schema: &Schema) -> Result<BTreeMap<String, u16>> {
        let mut out = BTreeMap::new();
        for (code, text) in &self.values {
            let d = schema.get(code)?;
            out.insert(d.code.clone(), d.parse_raw(text)?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffRow {
    pub code: String,
    pub name: String,
    pub unit: String,
    pub a: Option<String>,
    pub b: Option<String>,
}

/// Parameters whose values differ (compared as numbers, so `5` equals `5.0`).
pub fn diff(schema: &Schema, a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    for p in schema.real() {
        let (va, vb) = (a.get(&p.code), b.get(&p.code));
        let same = match (va, vb) {
            (Some(x), Some(y)) => match (p.parse_raw(x), p.parse_raw(y)) {
                (Ok(rx), Ok(ry)) => rx == ry,
                _ => x == y,
            },
            (None, None) => true,
            _ => false,
        };
        if !same {
            rows.push(DiffRow {
                code: p.code.clone(),
                name: p.name.clone(),
                unit: p.unit.clone(),
                a: va.cloned(),
                b: vb.cloned(),
            });
        }
    }
    rows
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub axis: Option<String>,
    pub file: PathBuf,
    pub taken_utc: String,
    pub differing: usize,
    pub compared: usize,
}

/// A baseline that `identify` could not compare with the drive.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedBaseline {
    pub file: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Identification {
    /// Best-matching axis, if one axis matches better than every other.
    pub axis: Option<String>,
    /// Axes tied for the best match (YL and YR can be identical).
    pub tied: Vec<String>,
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<SkippedBaseline>,
    /// Parameters that differ from the factory values, to find a drive that was reset.
    pub factory_differing: Option<usize>,
}

impl Identification {
    /// The smallest number of differences among the baselines of `axis`.
    fn best_of(&self, axis: &str) -> Option<usize> {
        self.candidates
            .iter()
            .filter(|c| c.axis.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(axis)))
            .map(|c| c.differing)
            .min()
    }

    /// Why a write to a drive that the user calls `axis` must stop, if it must.
    pub fn refusal(&self, axis: &str) -> Option<String> {
        let name = axis.to_ascii_uppercase();
        let Some(own) = self.best_of(axis) else {
            return Some(format!("no usable baseline for axis {name}"));
        };
        if !self.consistent_with(axis) {
            let best = if self.tied.is_empty() { "nothing".to_string() } else { self.tied.join("/") };
            return Some(format!("this drive does not look like axis {name}: best match is {best}"));
        }
        // The check only ranks the baselines. It means something only if it rules out another axis.
        let others_worse = self
            .candidates
            .iter()
            .filter_map(|c| c.axis.as_deref())
            .filter(|a| !a.eq_ignore_ascii_case(axis))
            .any(|a| self.best_of(a).is_some_and(|d| d > own));
        if !others_worse {
            return Some(format!(
                "no baseline of another axis is a worse match, so the check cannot tell axis {name} from the other axes: save a baseline of every axis"
            ));
        }
        if let Some(f) = self.factory_differing
            && f < own
        {
            return Some(format!(
                "this drive is closer to the factory values ({f} differ) than to the baseline of axis {name} ({own} differ), so it may have been reset: if that was on purpose, save a new baseline of axis {name}"
            ));
        }
        None
    }
}

impl Identification {
    /// Does the drive look like `axis`? True when `axis` is the best match or tied for it.
    pub fn consistent_with(&self, axis: &str) -> bool {
        self.tied.iter().any(|a| a.eq_ignore_ascii_case(axis))
    }
}

/// Load every snapshot under `dir` (recursively) that has an axis.
pub fn load_baselines(dir: &Path) -> Result<Vec<(PathBuf, Snapshot)>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| Error::Io(std::io::Error::new(e.kind(), format!("{}: {e}", d.display()))))? {
            let path = e?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|x| x.to_str()) == Some("json")
                && let Ok(s) = Snapshot::load(&path)
                && s.axis.is_some()
            {
                out.push((path, s));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Rank baselines by how many parameters differ from the drive's current values.
pub fn identify(schema: &Schema, current: &BTreeMap<String, u16>, baselines: &[(PathBuf, Snapshot)]) -> Identification {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut skipped = Vec::new();
    for (path, s) in baselines {
        let base = match s.raw(schema) {
            Ok(b) => b,
            Err(e) => {
                skipped.push(SkippedBaseline { file: path.clone(), reason: e.to_string() });
                continue;
            }
        };
        let mut compared = 0;
        let mut differing = 0;
        for (code, v) in &base {
            if let Some(c) = current.get(code) {
                compared += 1;
                if c != v {
                    differing += 1;
                }
            }
        }
        if compared == 0 {
            skipped.push(SkippedBaseline { file: path.clone(), reason: "no parameter in common with the drive".into() });
            continue;
        }
        candidates.push(Candidate { axis: s.axis.clone(), file: path.clone(), taken_utc: s.taken_utc.clone(), differing, compared });
    }
    candidates.sort_by_key(|c| c.differing);
    // Best score per axis.
    let mut best: BTreeMap<String, usize> = BTreeMap::new();
    for c in &candidates {
        if let Some(a) = &c.axis {
            let e = best.entry(a.to_ascii_uppercase()).or_insert(usize::MAX);
            *e = (*e).min(c.differing);
        }
    }
    let top = best.values().copied().min();
    let tied: Vec<String> = best.iter().filter(|(_, v)| Some(**v) == top).map(|(k, _)| k.clone()).collect();
    let mut factory_compared = 0;
    let mut factory_differing = 0;
    for p in schema.real() {
        if let (Some(d), Some(c)) = (p.default, current.get(&p.code)) {
            factory_compared += 1;
            if p.value_of(*c) != d {
                factory_differing += 1;
            }
        }
    }
    Identification {
        axis: (tied.len() == 1).then(|| tied[0].clone()),
        tied,
        candidates,
        skipped,
        factory_differing: (factory_compared > 0).then_some(factory_differing),
    }
}

/// Current UTC time as `2026-11-11T08:15:30Z`.
pub fn now_utc() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format_utc(secs)
}

pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// `2026-11-11T08:15:30Z` to `20261111-081530`, for file names.
pub fn stamp_for_file(utc: &str) -> String {
    let digits: String = utc.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 14 {
        format!("{}-{}", &digits[..8], &digits[8..14])
    } else {
        digits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_format() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_794_355_200 + 3661), "2026-11-11T01:01:01Z");
        assert_eq!(stamp_for_file("2026-11-11T01:01:01Z"), "20261111-010101");
    }

    #[test]
    fn diff_and_identify() {
        let schema = Schema::model512();
        let factory = crate::jmcxml::write(schema, &BTreeMap::new()).unwrap();
        let xml = crate::jmcxml::read(&factory).unwrap();
        let mut x = Snapshot::from_xml(schema, &xml, "factory values");
        assert!(x.unread.is_empty(), "{:?}", x.unread);
        x.axis = Some("X".into());
        let mut z = x.clone();
        z.axis = Some("Z".into());
        z.values.insert("P03-09".into(), "2000".into());
        let rows = diff(schema, &x.values, &z.values);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].code, "P03-09");
        let current = z.raw(schema).unwrap();
        let id = identify(schema, &current, &[("x.json".into(), x.clone()), ("z.json".into(), z.clone())]);
        assert_eq!(id.axis.as_deref(), Some("Z"));
        assert!(id.consistent_with("z"));
        assert!(!id.consistent_with("X"));
        let mut y = x.clone();
        y.axis = Some("YL".into());
        let mut yr = x.clone();
        yr.axis = Some("YR".into());
        let id = identify(schema, &x.raw(schema).unwrap(), &[("a".into(), y), ("b".into(), yr), ("c".into(), z)]);
        assert_eq!(id.axis, None);
        assert_eq!(id.tied, vec!["YL".to_string(), "YR".to_string()]);
    }

    #[test]
    fn refusal_needs_another_axis_and_finds_a_reset_drive() {
        let s = Schema::model512();
        let factory: BTreeMap<String, u16> =
            s.real().filter_map(|p| Some((p.code.clone(), p.to_raw(p.default?).ok()?))).collect();
        let tuned = |codes: &[&str]| {
            let mut v = factory.clone();
            for c in codes {
                let r = v.get_mut(*c).unwrap();
                *r = r.wrapping_add(1);
            }
            v
        };
        let snap = |axis: &str, v: &BTreeMap<String, u16>| {
            let mut x = Snapshot::from_raw(s, "test", v);
            x.axis = Some(axis.into());
            (PathBuf::from(format!("{axis}.json")), x)
        };
        let x = tuned(&["P01-03", "P01-04", "P02-03"]);
        let z = tuned(&["P02-00", "P02-01", "P03-09", "P03-15"]);
        // The drive is the X baseline and Z is a worse match: the write can go ahead.
        let id = identify(s, &x, &[snap("X", &x), snap("Z", &z)]);
        assert_eq!(id.refusal("x"), None);
        assert!(id.refusal("z").unwrap().contains("best match is X"));
        assert!(id.refusal("YL").unwrap().contains("no usable baseline"));
        // One axis only: nothing to tell it from.
        let id = identify(s, &x, &[snap("X", &x)]);
        assert!(id.refusal("x").unwrap().contains("cannot tell"));
        // A drive at the factory values is closer to them than to its baseline.
        let id = identify(s, &factory, &[snap("X", &x), snap("Z", &z)]);
        assert_eq!(id.factory_differing, Some(0));
        assert!(id.refusal("x").unwrap().contains("factory"));
    }
}
