//! The parameter table for drive model 512 (JAND/JASD), read from the embedded `model512.toml`.
//!
//! The table holds the facts about each register (code, range, decimals, default, access and when
//! a change applies) and this project's own text for it (name, unit, brief and choice labels).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// This project's parameter table for model 512.
pub const MODEL512_TABLE: &str = include_str!("../data/model512.toml");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub value: i64,
    pub label: String,
}

/// When a change to the parameter takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Applies {
    /// At once.
    Now,
    /// After the drive is powered off and on.
    PowerCycle,
    /// Never: the table marks it read-only.
    ReadOnly,
    /// The table does not say.
    Unspecified,
}

impl Applies {
    /// When the change takes effect, in words. None when the table does not say.
    pub fn text(self) -> Option<&'static str> {
        match self {
            Applies::Now => Some("At once"),
            Applies::PowerCycle => Some("After the drive is powered off and on"),
            Applies::ReadOnly => Some("Read-only"),
            Applies::Unspecified => None,
        }
    }
}

/// Who may change the parameter. Motor and factory parameters are protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    #[default]
    User,
    /// Motor data, set by the factory for the matched motor.
    Motor,
    /// Factory settings.
    Factory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Change {
    #[default]
    Any,
    ServoOff,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamDef {
    /// `P01-04`
    pub code: String,
    pub group: u8,
    pub index: u8,
    /// Modbus register (group * 100 + index).
    pub address: u16,
    pub name: String,
    pub unit: String,
    /// Fixed-point scale: the register holds value * 10^decimals.
    pub decimals: u8,
    pub signed: bool,
    /// Allowed range in raw register units. The full 16-bit range when the table gives none.
    pub min: i64,
    pub max: i64,
    /// The table gives the limit (false: the full 16-bit range applies).
    #[serde(skip)]
    pub min_given: bool,
    #[serde(skip)]
    pub max_given: bool,
    /// Factory value in raw units, when the table has one.
    pub default: Option<i64>,
    pub applies: Applies,
    /// Change the value only with the servo off. A write is refused while the drive reports the servo on.
    pub servo_off: bool,
    pub access: Access,
    /// A spare register: not used and no function, but it reads as a value.
    pub spare: bool,
    /// A reserved register: no function, and no value in exports.
    pub reserved: bool,
    /// A short description in Simplified Technical English.
    pub brief: String,
    pub choices: Vec<Choice>,
}

impl ParamDef {
    fn type_range(&self) -> (i64, i64) {
        type_range(self.signed)
    }

    /// Register value as a signed or unsigned integer, before scaling.
    pub fn value_of(&self, raw: u16) -> i64 {
        if self.signed {
            i64::from(raw as i16)
        } else {
            i64::from(raw)
        }
    }

    pub fn to_raw(&self, v: i64) -> Result<u16> {
        let (lo, hi) = self.type_range();
        if v < lo || v > hi {
            return Err(Error::Value(format!(
                "{} does not fit {} ({} to {})",
                format_scaled(v, self.decimals),
                self.code,
                format_scaled(lo, self.decimals),
                format_scaled(hi, self.decimals)
            )));
        }
        Ok(if self.signed { v as i16 as u16 } else { v as u16 })
    }

    /// Display text with exactly `decimals` decimals, the way JMC's exports write it.
    pub fn format(&self, raw: u16) -> String {
        format_scaled(self.value_of(raw), self.decimals)
    }

    pub fn format_value(&self, v: i64) -> String {
        format_scaled(v, self.decimals)
    }

    /// The allowed range as display text, e.g. `0.0 to 3000.0`.
    pub fn range_text(&self) -> String {
        format!("{} to {}", self.format_value(self.min), self.format_value(self.max))
    }

    /// Parse display text into a register value, checking only the 16-bit type range.
    /// Always uses `.` as the decimal point, whatever the Windows locale is.
    pub fn parse_raw(&self, text: &str) -> Result<u16> {
        let v = parse_scaled(text, self.decimals)
            .ok_or_else(|| Error::Value(format!("{}: cannot read {text:?} as a number with at most {} decimals", self.code, self.decimals)))?;
        self.to_raw(v)
    }

    pub fn check_range(&self, raw: u16) -> Result<()> {
        let v = self.value_of(raw);
        if v < self.min || v > self.max {
            return Err(Error::Value(format!(
                "{} = {} is outside {} to {}",
                self.code,
                self.format(raw),
                self.format_value(self.min),
                self.format_value(self.max)
            )));
        }
        // The choices list every value that has a function (P00-27 has 0 to 7 in a range up to 65535).
        if !self.choices.is_empty() && !self.choices.iter().any(|c| c.value == v) {
            return Err(Error::Value(format!("{} = {} is not one of the listed values", self.code, self.format(raw))));
        }
        Ok(())
    }

    /// Parse and range-check.
    pub fn parse(&self, text: &str) -> Result<u16> {
        let raw = self.parse_raw(text)?;
        self.check_range(raw)?;
        Ok(raw)
    }

    /// Motor data or a factory setting.
    pub fn protected(&self) -> bool {
        self.access != Access::User
    }

    /// A spare or reserved register. It means nothing, so writes are refused by default.
    pub fn placeholder(&self) -> bool {
        self.spare || self.reserved
    }

    /// The table marks it read-only. The table is not always right, so writes are refused
    /// by default rather than forbidden.
    pub fn read_only(&self) -> bool {
        self.applies == Applies::ReadOnly
    }

    /// Change it only with the servo off.
    pub fn needs_servo_off(&self) -> bool {
        self.servo_off
    }

    /// Takes effect only after the drive is powered off and on.
    pub fn needs_power_cycle(&self) -> bool {
        self.applies == Applies::PowerCycle
    }

    pub fn choice_label(&self, raw: u16) -> Option<&str> {
        let v = self.value_of(raw);
        self.choices.iter().find(|c| c.value == v).map(|c| c.label.as_str())
    }
}

fn type_range(signed: bool) -> (i64, i64) {
    if signed {
        (i64::from(i16::MIN), i64::from(i16::MAX))
    } else {
        (0, i64::from(u16::MAX))
    }
}

pub fn format_scaled(v: i64, decimals: u8) -> String {
    if decimals == 0 {
        return v.to_string();
    }
    let p = 10i64.pow(u32::from(decimals));
    let sign = if v < 0 { "-" } else { "" };
    let a = v.unsigned_abs();
    let p = p as u64;
    format!("{sign}{}.{:0w$}", a / p, a % p, w = usize::from(decimals))
}

/// Exact fixed-point parse: `"-12.5"` with 2 decimals gives -1250. Extra decimals are allowed
/// only when they are zeros. Commas, exponents and spaces inside the number are rejected.
pub fn parse_scaled(text: &str, decimals: u8) -> Option<i64> {
    let t = text.trim();
    let (neg, t) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let (int, frac) = match t.split_once('.') {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    if !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d = usize::from(decimals);
    if frac.len() > d && frac[d..].bytes().any(|b| b != b'0') {
        return None;
    }
    let mut v: i64 = 0;
    for b in int.bytes() {
        v = v.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    for k in 0..d {
        let digit = frac.as_bytes().get(k).map_or(0, |b| b - b'0');
        v = v.checked_mul(10)?.checked_add(i64::from(digit))?;
    }
    Some(if neg { -v } else { v })
}

/// `P01-04`, `p1-4`, `P01.04`, `P1_4` all give `P01-04`.
pub fn normalize_code(s: &str) -> Option<String> {
    let t = s.trim();
    let t = t.strip_prefix(['P', 'p'])?;
    let (g, i) = t.split_once(['-', '.', '_'])?;
    let g: u8 = g.parse().ok()?;
    let i: u8 = i.parse().ok()?;
    (g < 100 && i < 100).then(|| format!("P{g:02}-{i:02}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub number: u8,
    pub title: String,
    /// The group's XML tag in the 19-column parameter file, e.g. `P02_Gain_parameter`.
    pub element: String,
}

#[derive(Debug)]
pub struct Schema {
    /// The drive model, e.g. "512".
    pub machine: String,
    /// The drive's name.
    pub drive_name: String,
    pub groups: Vec<Group>,
    /// Every register in address order.
    pub params: Vec<ParamDef>,
    by_code: HashMap<String, usize>,
    by_addr: HashMap<u16, usize>,
}

// ------------------------------------------------------------------ the TOML file

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TableFile {
    drive: DriveRow,
    group: Vec<Group>,
    text: TextRow,
    param: Vec<Row>,
    #[serde(default)]
    spares: Vec<Row>,
    #[serde(default)]
    reserved: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriveRow {
    model: u32,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextRow {
    spare_name: String,
    spare_brief: String,
    reserved_name: String,
    reserved_brief: String,
}

/// One `[[param]]` (with `code`) or one `[[spares]]` / `[[reserved]]` block (with `codes`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    code: Option<String>,
    codes: Option<Vec<String>>,
    name: Option<String>,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    decimals: u8,
    #[serde(default)]
    signed: bool,
    min: Option<i64>,
    max: Option<i64>,
    default: Option<i64>,
    applies: Applies,
    #[serde(default)]
    change: Change,
    #[serde(default)]
    access: Access,
    brief: Option<String>,
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Param,
    Spare,
    Reserved,
}

/// `P00-34..38` gives P00-34 to P00-38; `P00-06` gives itself.
fn expand(range: &str) -> Result<Vec<String>> {
    let bad = || Error::Table(format!("bad code or range {range:?}"));
    let (first, last) = match range.split_once("..") {
        Some((a, b)) => (a, Some(b)),
        None => (range, None),
    };
    let code = normalize_code(first).filter(|c| c == first).ok_or_else(bad)?;
    let group = &code[..4];
    let start: u8 = code[4..].parse().map_err(|_| bad())?;
    let end: u8 = match last {
        None => start,
        Some(b) if b.len() == 2 => b.parse().map_err(|_| bad())?,
        Some(_) => return Err(bad()),
    };
    if end < start || end > 99 {
        return Err(bad());
    }
    Ok((start..=end).map(|i| format!("{group}{i:02}")).collect())
}

impl Schema {
    /// The built-in table for model 512.
    pub fn model512() -> &'static Schema {
        static S: OnceLock<Schema> = OnceLock::new();
        S.get_or_init(|| Schema::parse(MODEL512_TABLE).expect("embedded model 512 table parses"))
    }

    /// Read and check a parameter table in this project's TOML format.
    pub fn parse(text: &str) -> Result<Schema> {
        let file: TableFile = toml::from_str(text).map_err(|e| Error::Table(e.to_string()))?;
        let mut params = Vec::new();
        for row in file.param {
            let code = row.code.clone().ok_or_else(|| Error::Table("a [[param]] has no code".into()))?;
            if row.codes.is_some() {
                return Err(Error::Table(format!("{code}: [[param]] takes code, not codes")));
            }
            let name = row.name.clone().ok_or_else(|| Error::Table(format!("{code} has no name")))?;
            let brief = row.brief.clone().ok_or_else(|| Error::Table(format!("{code} has no brief")))?;
            let unit = row.unit.clone();
            params.push(Self::def(&code, &row, Kind::Param, name, unit, brief)?);
        }
        for (kind, rows) in [(Kind::Spare, &file.spares), (Kind::Reserved, &file.reserved)] {
            let (name, brief) = match kind {
                Kind::Spare => (&file.text.spare_name, &file.text.spare_brief),
                _ => (&file.text.reserved_name, &file.text.reserved_brief),
            };
            for row in rows {
                let codes = row.codes.as_ref().ok_or_else(|| Error::Table("a spare or reserved block has no codes".into()))?;
                if row.code.is_some() || row.name.is_some() || row.brief.is_some() || !row.unit.is_empty() || !row.choices.is_empty() {
                    return Err(Error::Table(format!(
                        "block {codes:?}: spare and reserved blocks take only codes and facts"
                    )));
                }
                for range in codes {
                    for code in expand(range)? {
                        params.push(Self::def(&code, row, kind, name.clone(), String::new(), brief.clone())?);
                    }
                }
            }
        }
        params.sort_by_key(|p| p.address);
        let mut by_code = HashMap::new();
        for (i, p) in params.iter().enumerate() {
            if by_code.insert(p.code.clone(), i).is_some() {
                return Err(Error::Table(format!("{} is listed twice", p.code)));
            }
            if !file.group.iter().any(|g| g.number == p.group) {
                return Err(Error::Table(format!("{} is in group {}, which the table does not define", p.code, p.group)));
            }
        }
        let by_addr = params
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.reserved)
            .map(|(i, p)| (p.address, i))
            .collect();
        Ok(Schema {
            machine: file.drive.model.to_string(),
            drive_name: file.drive.name,
            groups: file.group,
            params,
            by_code,
            by_addr,
        })
    }

    fn def(code: &str, row: &Row, kind: Kind, name: String, unit: String, brief: String) -> Result<ParamDef> {
        let bad = |what: String| Error::Table(format!("{code}: {what}"));
        if normalize_code(code).as_deref() != Some(code) {
            return Err(bad("not a parameter code like P01-04".into()));
        }
        let group: u8 = code[1..3].parse().map_err(|_| bad("bad group".into()))?;
        let index: u8 = code[4..6].parse().map_err(|_| bad("bad index".into()))?;
        if row.decimals > 4 {
            return Err(bad(format!("{} decimals", row.decimals)));
        }
        let (lo, hi) = type_range(row.signed);
        let min = row.min.unwrap_or(lo);
        let max = row.max.unwrap_or(hi);
        if min < lo || max > hi || min > max {
            return Err(bad(format!("range {min} to {max} does not fit the register")));
        }
        if let Some(d) = row.default
            && (d < lo || d > hi)
        {
            return Err(bad(format!("default {d} does not fit the register")));
        }
        let mut seen = HashSet::new();
        for c in &row.choices {
            if c.value < min || c.value > max || !seen.insert(c.value) || c.label.trim().is_empty() {
                return Err(bad(format!("choice {} is outside the range, repeated or has no label", c.value)));
            }
            // The 19-column file joins choices with '#'.
            if c.label.contains(['#', '\r', '\n']) {
                return Err(bad(format!("choice {} label has '#' or a line break", c.value)));
            }
        }
        if name.trim().is_empty() || brief.trim().is_empty() {
            return Err(bad("empty name or brief".into()));
        }
        Ok(ParamDef {
            code: code.to_string(),
            group,
            index,
            address: u16::from(group) * 100 + u16::from(index),
            name,
            unit,
            decimals: row.decimals,
            signed: row.signed,
            min,
            max,
            min_given: row.min.is_some(),
            max_given: row.max.is_some(),
            default: row.default,
            applies: row.applies,
            servo_off: row.change == Change::ServoOff,
            access: row.access,
            spare: kind == Kind::Spare,
            reserved: kind == Kind::Reserved,
            brief,
            choices: row.choices.clone(),
        })
    }

    pub fn get(&self, query: &str) -> Result<&ParamDef> {
        normalize_code(query)
            .and_then(|c| self.by_code.get(&c))
            .map(|&i| &self.params[i])
            .ok_or_else(|| Error::UnknownParam(query.to_string()))
    }

    pub fn by_address(&self, addr: u16) -> Option<&ParamDef> {
        self.by_addr.get(&addr).map(|&i| &self.params[i])
    }

    /// Registers that hold values (no reserved registers). Spares are included: they read as values.
    pub fn real(&self) -> impl Iterator<Item = &ParamDef> {
        self.params.iter().filter(|p| !p.reserved)
    }

    pub fn group(&self, number: u8) -> Option<&Group> {
        self.groups.iter().find(|g| g.number == number)
    }
}

pub(crate) fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

#[cfg(test)]
mod legacy;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point() {
        assert_eq!(parse_scaled("12.5", 2), Some(1250));
        assert_eq!(parse_scaled("-0.05", 2), Some(-5));
        assert_eq!(parse_scaled("3", 1), Some(30));
        assert_eq!(parse_scaled("0.00", 1), Some(0));
        assert_eq!(parse_scaled("0.25", 1), None);
        assert_eq!(parse_scaled("1,5", 1), None);
        assert_eq!(parse_scaled("1e3", 0), None);
        assert_eq!(parse_scaled(".5", 1), Some(5));
        assert_eq!(parse_scaled("", 0), None);
        assert_eq!(parse_scaled("-", 0), None);
        assert_eq!(format_scaled(-5, 2), "-0.05");
        assert_eq!(format_scaled(1250, 2), "12.50");
        assert_eq!(format_scaled(7, 0), "7");
    }

    #[test]
    fn codes() {
        assert_eq!(normalize_code("p1-4").as_deref(), Some("P01-04"));
        assert_eq!(normalize_code("P08.30").as_deref(), Some("P08-30"));
        assert_eq!(normalize_code("104"), None);
        assert_eq!(expand("P00-34..38").unwrap(), ["P00-34", "P00-35", "P00-36", "P00-37", "P00-38"]);
        assert_eq!(expand("P08-17").unwrap(), ["P08-17"]);
        assert!(expand("P08-17..5").is_err());
        assert!(expand("P08-17..16").is_err());
        assert!(expand("p8-17").is_err());
    }

    #[test]
    fn model512_table() {
        let s = Schema::model512();
        assert_eq!(s.machine, "512");
        assert_eq!(s.groups.len(), 9);
        assert_eq!(s.params.len(), 485);
        assert_eq!(s.params.iter().filter(|p| p.reserved).count(), 38);
        assert_eq!(s.params.iter().filter(|p| p.spare).count(), 189);
        assert_eq!(s.real().count(), 447);
        assert_eq!(s.real().filter(|p| !p.placeholder()).count(), 258);
        assert!(s.get("P08-17").unwrap().placeholder());
        assert_eq!(s.real().filter(|p| p.signed).count(), 21);
        let p = s.get("P08-30").unwrap();
        assert_eq!(p.address, 830);
        let gain = s.get("P02-00").unwrap();
        assert_eq!(gain.decimals, 1);
        assert_eq!(gain.parse("500.0").unwrap(), 5000);
        assert_eq!(gain.format(5000), "500.0");
        assert_eq!(gain.range_text(), "0.0 to 3000.0");
        assert!(s.get("P00-00").unwrap().protected());
        assert_eq!(s.get("P00-00").unwrap().access, Access::Motor);
        assert!(!s.get("P01-04").unwrap().protected());
        assert_eq!(s.get("P00-07").unwrap().choices.len(), 4);
        let baud: Vec<&str> = s.get("P00-21").unwrap().choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(baud, ["9600 bit/s", "19200 bit/s", "57600 bit/s", "115200 bit/s"]);
        assert!(s.get("P03-08").unwrap().read_only());
        assert!(s.get("P00-00").unwrap().needs_power_cycle());
        assert_eq!(s.by_address(104).unwrap().code, "P01-04");
        // P04-38 to P04-50: no limits and no default in the table.
        let p = s.get("P04-40").unwrap();
        assert!(!p.min_given && !p.max_given && p.default.is_none());
        assert_eq!((p.min, p.max), (0, 65535));
        for w in s.params.windows(2) {
            assert!(w[0].address < w[1].address, "{} before {}", w[0].code, w[1].code);
        }
    }

    #[test]
    fn bad_tables_are_refused() {
        let head = "[drive]\nmodel = 512\nname = \"x\"\n[[group]]\nnumber = 1\ntitle = \"t\"\nelement = \"P01_x\"\n\
                    [text]\nspare_name = \"s\"\nspare_brief = \"s.\"\nreserved_name = \"r\"\nreserved_brief = \"r.\"\n";
        let ok = format!("{head}[[param]]\ncode = \"P01-04\"\nname = \"n\"\napplies = \"now\"\nbrief = \"b.\"\n");
        assert!(Schema::parse(&ok).is_ok());
        for bad in [
            // listed twice
            format!("{ok}[[spares]]\ncodes = [\"P01-03..04\"]\napplies = \"now\"\n"),
            // no such group
            format!("{ok}[[param]]\ncode = \"P02-04\"\nname = \"m\"\napplies = \"now\"\nbrief = \"b.\"\n"),
            // range past the register
            format!("{head}[[param]]\ncode = \"P01-04\"\nname = \"n\"\nmax = 70000\napplies = \"now\"\nbrief = \"b.\"\n"),
            // choice outside the range
            format!("{head}[[param]]\ncode = \"P01-04\"\nname = \"n\"\nmax = 1\napplies = \"now\"\nbrief = \"b.\"\nchoices = [{{ value = 2, label = \"x\" }}]\n"),
            // unknown key
            format!("{head}[[param]]\ncode = \"P01-04\"\nname = \"n\"\napplies = \"now\"\nbrief = \"b.\"\nauthority = 0\n"),
            // unknown applies
            format!("{head}[[param]]\ncode = \"P01-04\"\nname = \"n\"\napplies = \"later\"\nbrief = \"b.\"\n"),
            // text on a spare block
            format!("{head}[[spares]]\ncodes = [\"P01-05\"]\nname = \"x\"\napplies = \"now\"\n"),
        ] {
            assert!(Schema::parse(&bad).is_err(), "accepted:\n{bad}");
        }
    }

    const UNITS: [&str; 24] = [
        "", "rpm", "N·m", "A", "kg·cm²", "pole pairs", "°", "ms", "s", "Hz", "%", "× 10 W", "Ω", "pulses", "encoder counts",
        "rpm/V", "V", "ms per 1000 rpm", "1/s", "×", "lines", "× 10 pulses", "%/V", "turns",
    ];

    /// The table's text follows the project's writing rules (Simplified Technical English).
    /// The table is edited by hand, so these checks keep it in shape.
    #[test]
    fn table_text_rules() {
        let s = Schema::model512();
        use crate::ste::{banned, foreign, split_sentences};
        let mut names = HashSet::new();
        for g in &s.groups {
            assert!(!foreign(&g.title), "group {}", g.title);
        }
        for p in &s.params {
            let mut texts = vec![("name", p.name.as_str()), ("unit", p.unit.as_str()), ("brief", p.brief.as_str())];
            texts.extend(p.choices.iter().map(|c| ("choice", c.label.as_str())));
            for (what, t) in texts {
                assert!(!foreign(t), "{} {what}: {t}", p.code);
                assert!(banned(t).is_none(), "{} {what} uses {:?}: {t}", p.code, banned(t));
            }
            assert!(UNITS.contains(&p.unit.as_str()), "{} unit {:?} is not in the standard list", p.code, p.unit);
            assert!(!p.name.ends_with('.'), "{} name ends with a period", p.code);
            assert!(p.brief.ends_with('.'), "{} brief does not end with a period", p.code);
            let sentences: Vec<&str> = split_sentences(&p.brief);
            assert!(sentences.len() <= 6, "{} brief has {} sentences", p.code, sentences.len());
            for sn in sentences {
                let n = sn.split_whitespace().count();
                assert!(n <= crate::ste::MAX_WORDS, "{} brief sentence has {n} words: {sn}", p.code);
            }
            if !p.placeholder() {
                assert!(names.insert(p.name.as_str()), "name used twice: {}", p.name);
            }
        }
    }

    #[test]
    fn signed_values() {
        let s = Schema::model512();
        let p = s.real().find(|p| p.signed && p.decimals == 0).unwrap();
        let raw = p.parse_raw("-1").unwrap();
        assert_eq!(raw, 0xFFFF);
        assert_eq!(p.format(raw), "-1");
    }

    #[test]
    fn choices_limit_values_and_known_defaults_outside_range() {
        let s = Schema::model512();
        let p = s.get("P00-27").unwrap();
        assert!(p.check_range(7).is_ok());
        assert!(matches!(p.check_range(8), Err(Error::Value(_))));
        for p in s.real() {
            for c in &p.choices {
                assert!(c.value >= p.min && c.value <= p.max, "{} choice {}", p.code, c.value);
            }
        }
        // Factory values outside their own range: motor data that the encoder fills in, and P05-07.
        // A new one must be checked and described before it joins this list.
        let out: Vec<&str> =
            s.real().filter(|p| p.default.is_some_and(|d| d < p.min || d > p.max)).map(|p| p.code.as_str()).collect();
        assert_eq!(out, ["P00-01", "P00-02", "P00-03", "P00-04", "P00-05", "P05-07"]);
    }
}
