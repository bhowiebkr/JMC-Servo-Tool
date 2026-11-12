//! Safety check: our table against JMC 2.4.3's 19-column export for model 512, read the way the
//! tool read it before it had its own table.
//!
//! The rule: our table may make the tool more careful, never less. Every write the old table
//! refused, the new one refuses too; every servo-off warning stays; ranges, decimals, defaults,
//! sign and addresses are the same.
//!
//! JMC's file is not part of this project. Set `JMC512_XML` to a model 512 parameter file saved
//! by JMC 2.4.3 to run these tests; they skip without it.

use super::*;
use crate::drive::{Drive, Persist, WriteOptions};

/// Codes where we added choices that JMC's table does not have (values from the drive manual).
const ADDED_CHOICES: [&str; 5] = ["P00-33", "P02-30", "P03-50", "P03-51", "P08-11"];

fn legacy_xml() -> Option<String> {
    let Some(path) = std::env::var_os("JMC512_XML") else {
        eprintln!("skipped: set JMC512_XML to a model 512 file saved by JMC 2.4.3");
        return None;
    };
    Some(std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("JMC512_XML {}: {e}", path.to_string_lossy())))
}

fn field<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> &'a str {
    node.children().find(|c| c.is_element() && c.tag_name().name() == name).and_then(|c| c.text()).unwrap_or("").trim()
}

/// One entry, read with the old rules.
struct Old {
    def: ParamDef,
    /// JMC's `Setting_method` says "Shutdown Setting".
    servo_off_method: bool,
    /// JMC's `Shutdown_setting` column is TRUE.
    servo_off_column: bool,
    /// JMC's choice values (labels ignored).
    choices: Vec<i64>,
}

fn read_old(xml: &str) -> Vec<Old> {
    let doc = roxmltree::Document::parse(strip_bom(xml)).expect("JMC export parses");
    let mut out = Vec::new();
    for g in doc.root_element().children().filter(|n| n.is_element()) {
        for p in g.children().filter(|n| n.is_element()) {
            let code = normalize_code(field(p, "Code")).expect("entry has a code");
            let group: u8 = code[1..3].parse().unwrap();
            let index: u8 = code[4..6].parse().unwrap();
            let decimals_text = field(p, "Decimal_place");
            let keep = field(p, "Keep").eq_ignore_ascii_case("TRUE");
            let reserved = decimals_text.is_empty() || (keep && field(p, "Current_Value").is_empty());
            let decimals: u8 = decimals_text.parse().unwrap_or(0);
            let signed = field(p, "Symbol").eq_ignore_ascii_case("TRUE");
            let (lo, hi) = type_range(signed);
            let min_text = parse_scaled(field(p, "Minimum"), decimals);
            let max_text = parse_scaled(field(p, "Maximum"), decimals);
            let mut min = min_text.map_or(lo, |v| v.clamp(lo, hi));
            let mut max = max_text.map_or(hi, |v| v.clamp(lo, hi));
            let (mut min_given, mut max_given) = (min_text.is_some(), max_text.is_some());
            if min > max {
                (min, max) = (lo, hi);
                (min_given, max_given) = (false, false);
            }
            let effective = field(p, "Effective_time");
            let e = effective.to_ascii_lowercase().replace(['-', ' '], "");
            let applies = if effective.eq_ignore_ascii_case("read only") || effective == "\u{53EA}\u{8BFB}" {
                Applies::ReadOnly
            } else if e.contains("repower") || e.contains("poweronagain") {
                Applies::PowerCycle
            } else {
                Applies::Now
            };
            let access = match field(p, "Authority").parse::<u8>().unwrap_or(0) {
                0 => Access::User,
                1 => Access::Motor,
                _ => Access::Factory,
            };
            let values = field(p, "SelectValueItem");
            let choices = if values.is_empty() || values.eq_ignore_ascii_case("FALSE") {
                Vec::new()
            } else {
                values.split('#').filter_map(|v| v.trim().parse().ok()).collect()
            };
            let servo_off_method = field(p, "Setting_method").to_ascii_lowercase().contains("shutdown");
            let def = ParamDef {
                code: code.clone(),
                group,
                index,
                address: u16::from(group) * 100 + u16::from(index),
                name: String::new(),
                unit: String::new(),
                decimals,
                signed,
                min,
                max,
                min_given,
                max_given,
                default: parse_scaled(field(p, "Initialize"), decimals),
                applies,
                servo_off: servo_off_method,
                access,
                spare: !reserved && field(p, "Name").eq_ignore_ascii_case("keep"),
                reserved,
                brief: String::new(),
                choices: Vec::new(),
            };
            out.push(Old {
                def,
                servo_off_method,
                servo_off_column: field(p, "Shutdown_setting").eq_ignore_ascii_case("TRUE"),
                choices,
            });
        }
    }
    out.sort_by_key(|o| o.def.address);
    out
}

#[test]
fn new_table_is_never_less_careful_than_jmcs() {
    let Some(xml) = legacy_xml() else { return };
    let old = read_old(&xml);
    let new = Schema::model512();
    let codes = |v: Vec<&str>| v.join(" ");
    assert_eq!(
        codes(old.iter().map(|o| o.def.code.as_str()).collect()),
        codes(new.params.iter().map(|p| p.code.as_str()).collect()),
        "the same codes in the same order"
    );
    let mut faults = Vec::new();
    for (o, n) in old.iter().zip(&new.params) {
        let od = &o.def;
        let mut fault = |what: String| faults.push(format!("{}: {what}", n.code));
        // Facts that must be identical.
        if (od.address, od.decimals, od.signed) != (n.address, n.decimals, n.signed) {
            fault(format!("address/decimals/signed {:?} vs {:?}", (od.address, od.decimals, od.signed), (n.address, n.decimals, n.signed)));
        }
        if (od.min, od.max, od.min_given, od.max_given) != (n.min, n.max, n.min_given, n.max_given) {
            fault(format!("range {:?} vs {:?}", (od.min, od.max, od.min_given, od.max_given), (n.min, n.max, n.min_given, n.max_given)));
        }
        if od.default != n.default {
            fault(format!("default {:?} vs {:?}", od.default, n.default));
        }
        if od.reserved != n.reserved {
            fault(format!("reserved {} vs {}", od.reserved, n.reserved));
        }
        if od.access != n.access {
            fault(format!("access {:?} vs {:?}", od.access, n.access));
        }
        if od.read_only() != n.read_only() || od.needs_power_cycle() != n.needs_power_cycle() {
            fault(format!("applies {:?} vs {:?}", od.applies, n.applies));
        }
        // Servo off is the union of JMC's two columns.
        if n.servo_off != (o.servo_off_method || o.servo_off_column) {
            fault(format!("servo_off {} vs method {} / column {}", n.servo_off, o.servo_off_method, o.servo_off_column));
        }
        if od.needs_servo_off() && !n.needs_servo_off() {
            fault("lost a servo-off warning".into());
        }
        if od.placeholder() && !n.placeholder() {
            fault("was a placeholder, now is not".into());
        }
        // Choice values: JMC's values inside the range, or added from the manual where JMC has none.
        let ours: Vec<i64> = n.choices.iter().map(|c| c.value).collect();
        let jmc: Vec<i64> = o.choices.iter().copied().filter(|v| (n.min..=n.max).contains(v)).collect();
        let added = ADDED_CHOICES.contains(&n.code.as_str()) && o.choices.is_empty() && !ours.is_empty();
        if ours != jmc && !added {
            fault(format!("choices {ours:?} vs JMC {jmc:?}"));
        }
        // Writes: whatever the old table refused, the new one refuses.
        for allow_protected in [false, true] {
            for persist in [Persist::Ram, Persist::Eeprom] {
                let opts = WriteOptions { persist, allow_protected };
                if Drive::check_write(n, opts).is_ok() && Drive::check_write(od, opts).is_err() {
                    fault(format!("write allowed now, refused before (allow_protected {allow_protected}, {persist:?})"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} differences:\n{}", faults.len(), faults.join("\n"));
}

/// Each entry of a 19-column file: the tag and its columns as (tag, trimmed text).
fn columns(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let doc = roxmltree::Document::parse(strip_bom(xml)).expect("file parses");
    let mut out = Vec::new();
    for g in doc.root_element().children().filter(|n| n.is_element()) {
        for p in g.children().filter(|n| n.is_element()) {
            let cols = p
                .children()
                .filter(|c| c.is_element())
                .map(|c| (c.tag_name().name().to_string(), c.text().unwrap_or("").trim().to_string()))
                .collect();
            out.push((p.tag_name().name().to_string(), cols));
        }
    }
    out
}

fn list(text: &str) -> Vec<&str> {
    if text.eq_ignore_ascii_case("FALSE") { Vec::new() } else { text.split('#').collect() }
}

/// Compatibility: the file we write carries the same facts as JMC's in every column that JMC
/// 2.4.3 reads, so JMC loads it and downloads the same values with the same limits.
#[test]
fn our_file_has_the_columns_jmc_reads() {
    use crate::jmcxml::{COLUMNS, write};
    let Some(xml) = legacy_xml() else { return };
    let schema = Schema::model512();
    let theirs = columns(&xml);
    let ours = columns(&write(schema, &std::collections::BTreeMap::new()).unwrap());
    assert_eq!(theirs.len(), ours.len());
    let mut faults = Vec::new();
    for (((code, t), (our_code, o)), d) in theirs.iter().zip(&ours).zip(&schema.params) {
        let mut fault = |what: String| faults.push(format!("{code}: {what}"));
        if code != our_code {
            fault(format!("entry order: ours is {our_code}"));
            continue;
        }
        let tags = |c: &[(String, String)]| c.iter().map(|x| x.0.clone()).collect::<Vec<_>>();
        if tags(t) != COLUMNS || tags(o) != COLUMNS {
            fault(format!("columns {:?} vs ours {:?}", tags(t), tags(o)));
            continue;
        }
        let (t, o): (Vec<&str>, Vec<&str>) = (t.iter().map(|x| x.1.as_str()).collect(), o.iter().map(|x| x.1.as_str()).collect());
        let col = |name: &str| COLUMNS.iter().position(|c| *c == name).unwrap();
        // The same text.
        for name in ["Code", "Address", "Authority", "Keep"] {
            let i = col(name);
            if !t[i].eq_ignore_ascii_case(o[i]) {
                fault(format!("{name} {:?} vs ours {:?}", t[i], o[i]));
            }
        }
        // JMC leaves Decimal_place and Symbol blank on some reserved registers; ours says 0 and FALSE.
        for name in ["Decimal_place", "Symbol"] {
            let i = col(name);
            if !t[i].eq_ignore_ascii_case(o[i]) && !(d.reserved && t[i].is_empty()) {
                fault(format!("{name} {:?} vs ours {:?}", t[i], o[i]));
            }
        }
        // The same number, or both blank. Limits past the 16-bit register are clamped to it
        // (JMC's table sets some limits past it, for example the maximums of P01-35, P05-16 and
        // P08-17), so JMC refuses what the drive cannot hold.
        let (lo, hi) = type_range(d.signed);
        for name in ["Minimum", "Maximum", "Initialize"] {
            let i = col(name);
            let mut a = parse_scaled(t[i], d.decimals);
            if name != "Initialize" {
                a = a.map(|v| v.clamp(lo, hi));
            }
            let b = parse_scaled(o[i], d.decimals);
            if a != b || t[i].is_empty() != o[i].is_empty() {
                fault(format!("{name} {:?} vs ours {:?}", t[i], o[i]));
            }
        }
        // Servo off: ours keeps every TRUE of JMC's.
        let i = col("Shutdown_setting");
        if t[i].eq_ignore_ascii_case("TRUE") && o[i] != "TRUE" {
            fault(format!("Shutdown_setting {:?} vs ours {:?}", t[i], o[i]));
        }
        // Choices: as many labels as values (JMC turns choices off for the whole file otherwise),
        // and the same values as JMC's inside the range, or ones added from the manual.
        let (labels, values) = (list(o[col("SelectDescriptionItem")]), list(o[col("SelectValueItem")]));
        if labels.len() != values.len() {
            fault(format!("{} choice labels for {} values", labels.len(), values.len()));
        }
        let jmc: Vec<i64> = list(t[col("SelectValueItem")])
            .iter()
            .filter_map(|v| v.trim().parse().ok())
            .filter(|v| (d.min..=d.max).contains(v))
            .collect();
        let ours: Vec<i64> = values.iter().map(|v| v.parse().expect("choice value is a number")).collect();
        let added = ADDED_CHOICES.contains(&code.as_str()) && jmc.is_empty();
        if ours != jmc && !added {
            fault(format!("choice values {ours:?} vs JMC {jmc:?}"));
        }
    }
    assert!(faults.is_empty(), "{} differences:\n{}", faults.len(), faults.join("\n"));
}
