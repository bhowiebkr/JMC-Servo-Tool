//! Read and write the drive's 19-column parameter file: the format of the files that "Upload" and
//! "Download" in JMC 2.4.3 use.
//!
//! Reading takes the values. Writing makes the whole file from this project's table: the facts go
//! in the columns that JMC 2.4.3 reads (address, range, decimals, sign, access, servo-off, spare
//! flag and choice values), and this project's own text goes in the display columns.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::params::{Access, ParamDef, Schema, parse_scaled, strip_bom};

/// The child elements of each entry, in file order. JMC 2.4.3 reads them by position, so the
/// order is fixed. "Descritpion" is spelled the way the format spells it.
pub const COLUMNS: [&str; 19] = [
    "Code",
    "Name",
    "Current_Value",
    "Setting_range",
    "Initialize",
    "Unit",
    "Setting_method",
    "Effective_time",
    "Address",
    "Minimum",
    "Maximum",
    "Descritpion",
    "Symbol",
    "Decimal_place",
    "Authority",
    "Shutdown_setting",
    "Keep",
    "SelectDescriptionItem",
    "SelectValueItem",
];

#[derive(Debug, Clone, Default)]
pub struct XmlParams {
    /// `Machine` attribute of the root, e.g. "512".
    pub machine: Option<String>,
    /// Parameter code to the Current_Value text, for entries that have one.
    pub values: BTreeMap<String, String>,
}

fn child_text<'a>(node: roxmltree::Node<'a, 'a>, names: &[&str]) -> Option<&'a str> {
    node.children()
        .find(|c| c.is_element() && names.contains(&c.tag_name().name()))
        .map(|c| c.text().unwrap_or(""))
}

/// Read a parameter file. Accepts the 19-column files (`SystemConfig`), JMC 2.4.3's config files
/// (`ParameterConfig`) and the Chinese tag names that some older files use.
pub fn read(text: &str) -> Result<XmlParams> {
    let doc = roxmltree::Document::parse(strip_bom(text)).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    let mut out = XmlParams { machine: root.attribute("Machine").map(str::to_string), values: BTreeMap::new() };
    for node in root.descendants().filter(|n| n.is_element()) {
        let Some(code) = child_text(node, &["Code", "\u{53C2}\u{6570}\u{4EE3}\u{7801}"]) else { continue };
        let Some(value) = child_text(node, &["Current_Value", "\u{5F53}\u{524D}\u{503C}"]) else { continue };
        let Some(code) = crate::params::normalize_code(code) else { continue };
        let value = value.trim();
        if !value.is_empty() {
            out.values.insert(code, value.to_string());
        }
    }
    if out.values.is_empty() {
        return Err(Error::Xml("no parameter values found; is this a drive parameter file?".into()));
    }
    Ok(out)
}

/// Write a 19-column parameter file for every register in the table.
///
/// `values` maps codes to display text (as in a snapshot). Each value is written with the table's
/// number of decimals. A parameter with no value gets its factory value. Reserved registers never
/// get a value. Codes that the table does not know are ignored.
pub fn write(schema: &Schema, values: &BTreeMap<String, String>) -> Result<String> {
    let mut out = String::with_capacity(1200 * schema.params.len());
    out.push('\u{FEFF}');
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n");
    out.push_str(&format!("<SystemConfig Machine=\"{}\">\r\n", escape(&schema.machine)));
    for g in &schema.groups {
        check_tag(&g.element)?;
        out.push_str(&format!("  <{}>\r\n", g.element));
        for d in schema.params.iter().filter(|d| d.group == g.number) {
            entry(&mut out, d, values.get(&d.code))?;
        }
        out.push_str(&format!("  </{}>\r\n", g.element));
    }
    out.push_str("</SystemConfig>");
    Ok(out)
}

/// One `<Pxx-yy>` entry with its 19 columns.
fn entry(out: &mut String, d: &ParamDef, value: Option<&String>) -> Result<()> {
    let num = |v: i64| d.format_value(v);
    let current = match value {
        _ if d.reserved => String::new(),
        Some(text) => {
            let v = parse_scaled(text, d.decimals)
                .ok_or_else(|| Error::Value(format!("{}: {text:?} is not a number with {} decimals", d.code, d.decimals)))?;
            num(v)
        }
        None => d.default.map(num).unwrap_or_default(),
    };
    let min = if d.min_given { num(d.min) } else { String::new() };
    let max = if d.max_given { num(d.max) } else { String::new() };
    let range = if d.reserved { "-".to_string() } else { format!("{min}-{max}") };
    let initialize = if d.reserved { String::new() } else { d.default.map(num).unwrap_or_default() };
    let (method, effective) = if d.reserved {
        ("", "")
    } else {
        (if d.servo_off { "Servo off" } else { "Any time" }, d.applies.text().unwrap_or(""))
    };
    let authority = match d.access {
        Access::User => "0",
        Access::Motor => "1",
        Access::Factory => "10",
    };
    let (labels, choice_values) = if d.choices.is_empty() {
        ("FALSE".to_string(), "FALSE".to_string())
    } else {
        let labels: Vec<String> = d.choices.iter().map(|c| format!("{}:{}", num(c.value), c.label)).collect();
        let values: Vec<String> = d.choices.iter().map(|c| num(c.value)).collect();
        (labels.join("#"), values.join("#"))
    };
    let flag = |b: bool| if b { "TRUE" } else { "FALSE" }.to_string();
    let cells: [String; 19] = [
        d.code.clone(),
        d.name.clone(),
        current,
        range,
        initialize,
        d.unit.trim().to_string(),
        method.to_string(),
        effective.to_string(),
        format!("0x{:04X}", d.address),
        min,
        max,
        d.brief.clone(),
        flag(d.signed),
        d.decimals.to_string(),
        authority.to_string(),
        flag(d.servo_off),
        flag(d.placeholder()),
        labels,
        choice_values,
    ];
    out.push_str(&format!("    <{}>\r\n", d.code));
    for (tag, text) in COLUMNS.iter().zip(&cells) {
        if text.is_empty() {
            // The way .NET writes an empty element in this file.
            out.push_str(&format!("      <{tag}>\r\n      </{tag}>\r\n"));
        } else {
            out.push_str(&format!("      <{tag}>{}</{tag}>\r\n", escape(text)));
        }
    }
    out.push_str(&format!("    </{}>\r\n", d.code));
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A group tag from the table must be a plain XML name.
fn check_tag(tag: &str) -> Result<()> {
    let ok = tag.starts_with(|c: char| c.is_ascii_alphabetic())
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(()) } else { Err(Error::Table(format!("group tag {tag:?} is not a plain XML name"))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
        let doc = roxmltree::Document::parse(strip_bom(xml)).unwrap();
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

    #[test]
    fn writes_every_register_in_the_19_column_layout() {
        let s = Schema::model512();
        let xml = write(s, &BTreeMap::new()).unwrap();
        assert!(xml.starts_with("\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<SystemConfig Machine=\"512\">\r\n"));
        assert!(!xml.replace("\r\n", "").contains(['\r', '\n']), "every line ends with CRLF");
        let doc = roxmltree::Document::parse(strip_bom(&xml)).unwrap();
        let tags: Vec<&str> = doc.root_element().children().filter(|n| n.is_element()).map(|n| n.tag_name().name()).collect();
        assert_eq!(tags, s.groups.iter().map(|g| g.element.as_str()).collect::<Vec<_>>());
        let e = entries(&xml);
        assert_eq!(e.len(), 485);
        for ((code, cols), d) in e.iter().zip(&s.params) {
            assert_eq!(code, &d.code);
            assert_eq!(cols.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), COLUMNS, "{code}");
            assert_eq!(cols[0].1, d.code);
            assert_eq!(cols[16].1 == "TRUE", d.placeholder(), "{code} Keep");
        }
        assert_eq!(e.iter().filter(|(_, c)| c[16].1 == "TRUE").count(), 227);
        let p00_21 = &e.iter().find(|(c, _)| c == "P00-21").unwrap().1;
        assert_eq!(p00_21[18].1, "0#1#2#3");
        assert_eq!(p00_21[17].1.split('#').count(), 4);
        assert!(p00_21[17].1.starts_with("0:9600 bit/s#"));
        let p00_42 = &e.iter().find(|(c, _)| c == "P00-42").unwrap().1;
        assert_eq!((p00_42[2].1.as_str(), p00_42[8].1.as_str(), p00_42[14].1.as_str()), ("", "0x002A", "10"));
        assert!(xml.contains("      <Current_Value>\r\n      </Current_Value>\r\n"));
    }

    #[test]
    fn values_round_trip() {
        let s = Schema::model512();
        let mut values = BTreeMap::new();
        for d in s.params.iter().filter(|d| !d.reserved) {
            values.insert(d.code.clone(), d.format_value(d.default.unwrap_or(d.min)));
        }
        values.insert("P02-00".into(), "812.5".into());
        values.insert("P05-15".into(), s.get("P05-15").unwrap().format_value(s.get("P05-15").unwrap().max));
        let xml = write(s, &values).unwrap();
        let back = read(&xml).unwrap();
        assert_eq!(back.machine.as_deref(), Some("512"));
        assert_eq!(back.values.len(), 447);
        for (code, text) in &values {
            let d = s.get(code).unwrap();
            assert_eq!(parse_scaled(&back.values[code], d.decimals), parse_scaled(text, d.decimals), "{code}");
        }
        assert_eq!(back.values["P02-00"], "812.5");
        assert_eq!(write(s, &back.values).unwrap(), xml, "a second round trip changes nothing");
    }

    #[test]
    fn missing_values_get_the_factory_value() {
        let s = Schema::model512();
        let back = read(&write(s, &BTreeMap::new()).unwrap()).unwrap();
        for d in s.params.iter().filter(|d| !d.reserved) {
            assert_eq!(back.values.get(&d.code), d.default.map(|v| d.format_value(v)).as_ref(), "{}", d.code);
        }
    }

    #[test]
    fn values_are_checked() {
        let s = Schema::model512();
        for bad in ["1<2", "abc", "1.25"] {
            let v = BTreeMap::from([("P02-00".to_string(), bad.to_string())]);
            assert!(write(s, &v).is_err(), "{bad}");
        }
        // Reserved registers never get a value; unknown codes are ignored.
        let v = BTreeMap::from([("P00-42".to_string(), "5".to_string()), ("P99-99".to_string(), "1".to_string())]);
        let back = read(&write(s, &v).unwrap()).unwrap();
        assert!(!back.values.contains_key("P00-42"));
    }

    /// Real exports: set JMC_EXPORTS_DIR to a folder of JMC 2.4.3 exports. Each one goes
    /// through a snapshot (as `jmc import-xml` then `jmc export-xml` would) and back to a file with
    /// the same values.
    #[test]
    fn user_exports_round_trip() {
        let Ok(dir) = std::env::var("JMC_EXPORTS_DIR") else { return };
        let s = Schema::model512();
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.extension().and_then(|x| x.to_str()) != Some("xml") {
                continue;
            }
            let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
            let x = read(&text).unwrap();
            let snap = crate::snapshot::Snapshot::from_xml(s, &x, "test");
            assert!(snap.unread.is_empty(), "{}: {:?}", path.display(), snap.unread);
            let back = read(&write(s, &snap.values).unwrap()).unwrap();
            for (code, text) in &x.values {
                let d = s.get(code).unwrap();
                if d.reserved {
                    continue;
                }
                let ours = back.values.get(code).and_then(|t| parse_scaled(t, d.decimals));
                assert_eq!(ours, parse_scaled(text, d.decimals), "{}: {code}", path.display());
            }
            n += 1;
        }
        assert!(n > 0);
    }
}
