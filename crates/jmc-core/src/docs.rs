//! Help pages, built into the library from `docs/parameters/<code>.md` and `docs/topics/<name>.md`.
//!
//! A page is Markdown with optional front matter between two `---` lines:
//!
//! ```text
//! ---
//! title: Control loops          (topic pages only)
//! drawing: notch                (a drawing kind and its arguments)
//! see: P02-10 topic:filters     (related pages, separated by spaces)
//! ---
//! ```
//!
//! In the body, every parameter code (`P02-10`) is a link. `[text](P02-10)` and
//! `[text](topic:filters)` are links too. A parameter with no page still has a page in the help
//! window, made from the table: its facts, its brief, its choices and a drawing.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::params::ParamDef;

include!(concat!(env!("OUT_DIR"), "/pages.rs"));

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// A parameter, by code (`P02-10`).
    Param(String),
    /// A topic page, by name (`control-loops`).
    Topic(String),
    /// A web page (`https://...`).
    Web(String),
}

impl Target {
    pub fn parse(s: &str) -> Option<Target> {
        let s = s.trim();
        if let Some(name) = s.strip_prefix("topic:") {
            let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            return ok.then(|| Target::Topic(name.to_string()));
        }
        if s.starts_with("https://") {
            return Some(Target::Web(s.to_string()));
        }
        is_code(s).then(|| Target::Param(s.to_string()))
    }

    /// The page key: the code, `topic:<name>`, or the address.
    pub fn key(&self) -> String {
        match self {
            Target::Param(c) => c.clone(),
            Target::Topic(n) => format!("topic:{n}"),
            Target::Web(u) => u.clone(),
        }
    }
}

/// `P` + two digits + `-` + two digits.
pub fn is_code(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 6
        && b[0] == b'P'
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3] == b'-'
        && b[4].is_ascii_digit()
        && b[5].is_ascii_digit()
}

/// A piece of text, split at the parameter codes in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    Text(&'a str),
    Code(&'a str),
}

/// Split `text` at every parameter code that stands as a word.
pub fn segments(text: &str) -> Vec<Segment<'_>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i + 6 <= b.len() {
        let before_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        let after_ok = b.get(i + 6).is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok && text.is_char_boundary(i) && text.is_char_boundary(i + 6) && is_code(&text[i..i + 6]) {
            if start < i {
                out.push(Segment::Text(&text[start..i]));
            }
            out.push(Segment::Code(&text[i..i + 6]));
            i += 6;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < text.len() {
        out.push(Segment::Text(&text[start..]));
    }
    out
}

/// The kinds of drawing in the help window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The value on its range, with the default.
    Gauge,
    /// The list of choices, with the one set.
    Choices,
    /// A move: the following error with the position loop and the speed feedforward.
    PositionLoop,
    /// A step of the speed command: the speed loop's response.
    SpeedLoop,
    /// A first-order filter: step response and gain against frequency.
    Lowpass,
    /// A notch filter: gain against frequency.
    Notch,
    /// A speed ramp. Argument `up` or `down`.
    Ramp,
    /// A command clipped at a limit.
    Limit,
    /// A signal that grows past an alarm level.
    AlarmLevel,
    /// An output that switches when a signal is in a band. Argument `inside` or `outside`.
    Band,
    /// Electronic gear: input pulses to motor counts.
    Gear,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Gauge,
        Kind::Choices,
        Kind::PositionLoop,
        Kind::SpeedLoop,
        Kind::Lowpass,
        Kind::Notch,
        Kind::Ramp,
        Kind::Limit,
        Kind::AlarmLevel,
        Kind::Band,
        Kind::Gear,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Gauge => "gauge",
            Kind::Choices => "choices",
            Kind::PositionLoop => "position-loop",
            Kind::SpeedLoop => "speed-loop",
            Kind::Lowpass => "lowpass",
            Kind::Notch => "notch",
            Kind::Ramp => "ramp",
            Kind::Limit => "limit",
            Kind::AlarmLevel => "alarm-level",
            Kind::Band => "band",
            Kind::Gear => "gear",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// A drawing kind and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawing {
    pub kind: Kind,
    pub args: Vec<String>,
}

impl Drawing {
    fn new(kind: Kind, args: &[&str]) -> Drawing {
        Drawing { kind, args: args.iter().map(|a| a.to_string()).collect() }
    }

    pub fn parse(s: &str) -> Result<Drawing, String> {
        let mut words = s.split_whitespace();
        let name = words.next().ok_or("drawing: no kind")?;
        let kind = Kind::parse(name).ok_or_else(|| format!("drawing: unknown kind {name:?}"))?;
        let args: Vec<String> = words.map(str::to_string).collect();
        let allowed: &[&str] = match kind {
            Kind::Ramp => &["up", "down"],
            Kind::Band => &["inside", "outside"],
            _ => &[],
        };
        if let Some(a) = args.iter().find(|a| !allowed.contains(&a.as_str())) {
            return Err(format!("drawing: {name} does not take {a:?}"));
        }
        Ok(Drawing { kind, args })
    }

    pub fn has(&self, arg: &str) -> bool {
        self.args.iter().any(|a| a == arg)
    }
}

/// One help page.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// The parameter code, or `topic:<name>`.
    pub key: String,
    /// Topic pages only. A parameter page takes its title from the table.
    pub title: Option<String>,
    /// Topic pages only: the parameter group (0 to 10) that the topic explains.
    pub group: Option<u8>,
    /// Topic pages only: the place in the Topics menu. Lower numbers come first.
    pub order: Option<u16>,
    pub drawing: Option<Drawing>,
    pub see: Vec<Target>,
    /// Markdown.
    pub body: String,
}

impl Page {
    /// Every link on the page: the `see` list first, then the body in order. Each once.
    pub fn links(&self) -> Vec<Target> {
        let mut out = self.see.clone();
        for t in body_links(&self.body) {
            if !out.contains(&t) && t.key() != self.key {
                out.push(t);
            }
        }
        out
    }
}

/// Parse a page. `key` is the code or `topic:<name>`.
pub fn parse(key: &str, text: &str) -> Result<Page, String> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text).replace("\r\n", "\n");
    let mut page =
        Page { key: key.to_string(), title: None, group: None, order: None, drawing: None, see: Vec::new(), body: String::new() };
    let body = if let Some(rest) = text.strip_prefix("---\n") {
        let end = rest.find("\n---\n").map(|i| (i, i + 5)).or_else(|| rest.strip_suffix("\n---").map(|r| (r.len(), rest.len())));
        let (end, after) = end.ok_or("front matter has no closing ---")?;
        for line in rest[..end].lines().filter(|l| !l.trim().is_empty()) {
            let (k, v) = line.split_once(':').ok_or_else(|| format!("front matter line without a key: {line:?}"))?;
            let v = v.trim();
            match k.trim() {
                "title" => page.title = Some(v.to_string()),
                "group" => page.group = Some(v.parse().ok().filter(|g| *g <= 10).ok_or_else(|| format!("group: not 0 to 10: {v:?}"))?),
                "order" => page.order = Some(v.parse().map_err(|_| format!("order: not a number: {v:?}"))?),
                "drawing" => page.drawing = Some(Drawing::parse(v)?),
                "see" => {
                    for w in v.split_whitespace() {
                        page.see.push(Target::parse(w).ok_or_else(|| format!("see: not a link {w:?}"))?);
                    }
                }
                other => return Err(format!("front matter: unknown key {other:?}")),
            }
        }
        &rest[after..]
    } else {
        &text[..]
    };
    page.body = body.trim().to_string();
    if key.starts_with("topic:") {
        if page.title.as_deref().is_none_or(str::is_empty) {
            return Err("a topic page needs a title".into());
        }
    } else if page.title.is_some() {
        return Err("a parameter page takes its title from the table, not from title:".into());
    } else if page.group.is_some() || page.order.is_some() {
        return Err("group: and order: are for topic pages".into());
    }
    if Target::parse(key).is_none_or(|t| matches!(t, Target::Web(_))) {
        return Err(format!("{key:?} is not a parameter code or topic:<name>"));
    }
    Ok(page)
}

/// The links in a page body: `[text](target)` and bare parameter codes, in order, each once.
pub fn body_links(body: &str) -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let mut push = |t: Target| {
        if !out.contains(&t) {
            out.push(t);
        }
    };
    let mut rest = body;
    while let Some(i) = rest.find("](") {
        let before = &rest[..i];
        for s in segments(before) {
            if let Segment::Code(c) = s {
                push(Target::Param(c.to_string()));
            }
        }
        let after = &rest[i + 2..];
        let end = after.find(')').unwrap_or(after.len());
        if let Some(t) = Target::parse(&after[..end]) {
            push(t);
        }
        rest = &after[(end + 1).min(after.len())..];
    }
    for s in segments(rest) {
        if let Segment::Code(c) = s {
            push(Target::Param(c.to_string()));
        }
    }
    out
}

struct Index {
    pages: Vec<Page>,
    errors: Vec<(String, String)>,
    backlinks: BTreeMap<String, Vec<String>>,
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        let (mut pages, mut errors) = (Vec::new(), Vec::new());
        for (key, text) in RAW_PAGES {
            match parse(key, text) {
                Ok(p) => pages.push(p),
                Err(e) => errors.push((key.to_string(), e)),
            }
        }
        let mut backlinks: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for p in &pages {
            for t in p.links() {
                let list = backlinks.entry(t.key()).or_default();
                if !list.contains(&p.key) {
                    list.push(p.key.clone());
                }
            }
        }
        Index { pages, errors, backlinks }
    })
}

/// Every page that parsed, parameter pages first, in key order.
pub fn pages() -> &'static [Page] {
    &index().pages
}

/// Pages that did not parse, with the reason. The tests keep this empty.
pub fn errors() -> &'static [(String, String)] {
    &index().errors
}

/// The page for a code or `topic:<name>`.
pub fn page(key: &str) -> Option<&'static Page> {
    index().pages.iter().find(|p| p.key == key)
}

/// The keys of the pages that link to `key`.
pub fn backlinks(key: &str) -> &'static [String] {
    index().backlinks.get(key).map(Vec::as_slice).unwrap_or(&[])
}

/// The topic pages, in reading order: by `order:`, then by key.
pub fn topics() -> impl Iterator<Item = &'static Page> {
    static ORDER: OnceLock<Vec<&'static Page>> = OnceLock::new();
    ORDER
        .get_or_init(|| {
            let mut v: Vec<&'static Page> = index().pages.iter().filter(|p| p.key.starts_with("topic:")).collect();
            v.sort_by_key(|p| (p.order.unwrap_or(u16::MAX), p.key.as_str()));
            v
        })
        .iter()
        .copied()
}

/// The topic page that explains parameter group `n`.
pub fn group_topic(n: u8) -> Option<&'static Page> {
    topics().find(|p| p.group == Some(n))
}

/// The drawing for a parameter: the page's choice, else the one chosen here, else a gauge or the
/// list of choices.
pub fn drawing_for(def: &ParamDef) -> Drawing {
    if let Some(d) = page(&def.code).and_then(|p| p.drawing.clone()) {
        return d;
    }
    if let Some(d) = builtin_drawing(&def.code) {
        return d;
    }
    let switch = def.choices.is_empty() && def.min == 0 && def.max == 1 && def.decimals == 0;
    if !def.choices.is_empty() || switch {
        Drawing::new(Kind::Choices, &[])
    } else {
        Drawing::new(Kind::Gauge, &[])
    }
}

/// The notch set (1 to 4) and the role of a notch register.
pub fn notch_role(code: &str) -> Option<(u8, NotchRole)> {
    let n: u8 = code.strip_prefix("P08-")?.parse().ok()?;
    if !(30..=41).contains(&n) {
        return None;
    }
    let role = match (n - 30) % 3 {
        0 => NotchRole::Frequency,
        1 => NotchRole::Width,
        _ => NotchRole::Depth,
    };
    Some(((n - 30) / 3 + 1, role))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotchRole {
    Frequency,
    Width,
    Depth,
}

/// The codes of notch set `n` (1 to 4): frequency, width, depth.
pub fn notch_codes(n: u8) -> [String; 3] {
    let base = 30 + 3 * (n.clamp(1, 4) as u32 - 1);
    [format!("P08-{base}"), format!("P08-{}", base + 1), format!("P08-{}", base + 2)]
}

fn builtin_drawing(code: &str) -> Option<Drawing> {
    use Kind::*;
    if notch_role(code).is_some() {
        return Some(Drawing::new(Notch, &[]));
    }
    let (kind, args): (Kind, &[&str]) = match code {
        "P02-00" | "P02-01" | "P02-03" => (PositionLoop, &[]),
        "P02-10" | "P02-11" | "P02-12" | "P02-13" | "P02-14" | "P02-15" | "P01-04" => (SpeedLoop, &[]),
        "P02-04" | "P02-20" | "P03-16" | "P06-41" | "P06-44" | "P08-19" | "P08-20" | "P08-21" | "P08-26" => (Lowpass, &[]),
        "P04-14" => (Ramp, &["up"]),
        "P04-15" => (Ramp, &["down"]),
        "P04-06" | "P04-07" | "P05-02" | "P05-10" | "P05-11" | "P05-12" | "P05-13" | "P03-46" => (Limit, &[]),
        "P03-15" | "P04-05" | "P03-53" => (AlarmLevel, &[]),
        "P03-06" | "P04-04" | "P04-10" | "P04-12" => (Band, &["inside"]),
        "P04-11" => (Band, &["outside"]),
        "P03-09" | "P03-10" | "P03-11" | "P03-12" | "P03-13" | "P03-14" | "P03-22" | "P03-23" => (Gear, &[]),
        _ => return None,
    };
    Some(Drawing::new(kind, args))
}

/// The codes that have a drawing chosen here (not a gauge or a list of choices by default).
pub fn builtin_codes() -> impl Iterator<Item = String> {
    (0..=10u32)
        .flat_map(|g| (0..=99u32).map(move |i| format!("P{g:02}-{i:02}")))
        .filter(|c| builtin_drawing(c).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Schema;

    #[test]
    fn every_page_parses() {
        assert!(errors().is_empty(), "{:?}", errors());
        assert!(!pages().is_empty());
    }

    #[test]
    fn every_link_goes_somewhere() {
        let s = Schema::model512();
        for p in pages() {
            if !p.key.starts_with("topic:") {
                let def = s.get(&p.key).unwrap_or_else(|_| panic!("page {} is not a parameter", p.key));
                assert!(!def.placeholder(), "page {} is for a spare or reserved register", p.key);
            }
            for t in p.links() {
                match &t {
                    Target::Param(c) => {
                        let def = s.get(c).unwrap_or_else(|_| panic!("{} links to {c}, which is not in the table", p.key));
                        assert!(!def.placeholder(), "{} links to {c}, a spare or reserved register", p.key);
                    }
                    Target::Topic(_) => assert!(page(&t.key()).is_some(), "{} links to {}, which has no page", p.key, t.key()),
                    Target::Web(_) => {}
                }
            }
        }
    }

    /// Every parameter that the main table shows has its own help page. Factory settings and
    /// spare or reserved registers are hidden by default, so they do not need one.
    #[test]
    fn every_visible_parameter_has_a_page() {
        let missing: Vec<&str> = Schema::model512()
            .params
            .iter()
            .filter(|d| d.access != crate::params::Access::Factory && !d.placeholder())
            .filter(|d| page(&d.code).is_none())
            .map(|d| d.code.as_str())
            .collect();
        assert!(missing.is_empty(), "parameters without a page: {missing:?}");
    }

    #[test]
    fn every_topic_is_linked_from_somewhere() {
        for t in topics() {
            assert!(!backlinks(&t.key).is_empty(), "nothing links to {}", t.key);
        }
    }

    /// The page text follows the writing rules. Markdown marks are removed first; each list item,
    /// table cell and paragraph is checked on its own.
    #[test]
    fn page_text_rules() {
        for p in pages() {
            if let Some(t) = &p.title {
                assert!(crate::ste::problems(t).is_empty(), "{} title: {:?}", p.key, crate::ste::problems(t));
            }
            for para in plain_paragraphs(&p.body) {
                let problems = crate::ste::problems(&para);
                assert!(problems.is_empty(), "{}: {problems:?}", p.key);
            }
        }
    }

    fn plain_paragraphs(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let flush = |cur: &mut String, out: &mut Vec<String>| {
            if !cur.trim().is_empty() {
                out.push(std::mem::take(cur));
            }
            cur.clear();
        };
        for line in body.lines() {
            let l = line.trim();
            if l.is_empty() {
                flush(&mut cur, &mut out);
                continue;
            }
            let item = l.starts_with("- ") || l.starts_with("* ") || l.starts_with('#') || l.starts_with('|')
                || l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if item {
                flush(&mut cur, &mut out);
                if l.starts_with('|') {
                    out.extend(l.split('|').map(plain).filter(|c| !c.trim().is_empty() && !c.contains("---")));
                } else {
                    out.push(plain(l));
                }
            } else {
                cur.push(' ');
                cur.push_str(&plain(l));
            }
        }
        flush(&mut cur, &mut out);
        out
    }

    /// Remove Markdown marks: list marks, `#`, `**`, `` ` `` and the targets of links.
    fn plain(l: &str) -> String {
        let l = l.trim_start_matches(['#', '-', '*', ' ']);
        let l = match l.split_once(". ") {
            Some((n, rest)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest,
            _ => l,
        };
        let mut s = String::new();
        let mut rest = l;
        while let Some(i) = rest.find("](") {
            s.push_str(&rest[..i]);
            let after = &rest[i + 2..];
            rest = &after[after.find(')').map_or(after.len(), |e| e + 1)..];
        }
        s.push_str(rest);
        s.replace("**", "").replace(['`', '['], "").trim().to_string()
    }

    #[test]
    fn builtin_drawings_name_real_parameters() {
        let s = Schema::model512();
        for c in builtin_codes() {
            let def = s.get(&c).unwrap_or_else(|_| panic!("builtin drawing for {c}, which is not in the table"));
            assert!(!def.placeholder(), "builtin drawing for {c}, a spare or reserved register");
        }
        // Every real parameter gets a drawing.
        for d in s.real().filter(|p| !p.placeholder()) {
            let _ = drawing_for(d);
        }
        assert_eq!(drawing_for(s.get("P08-34").unwrap()).kind, Kind::Notch);
        assert_eq!(drawing_for(s.get("P01-01").unwrap()).kind, Kind::Choices);
        assert_eq!(drawing_for(s.get("P01-50").unwrap()).kind, Kind::Choices);
        assert_eq!(drawing_for(s.get("P01-20").unwrap()).kind, Kind::Gauge);
        assert_eq!(notch_role("P08-34"), Some((2, NotchRole::Width)));
        assert_eq!(notch_codes(4), ["P08-39", "P08-40", "P08-41"]);
    }

    #[test]
    fn parsing() {
        let p = parse("P02-10", "---\ndrawing: ramp up\nsee: P02-11 topic:control-loops\n---\n\nSee [the gain](P02-13) and P02-14.\n")
            .unwrap();
        assert_eq!(p.drawing, Some(Drawing::new(Kind::Ramp, &["up"])));
        assert_eq!(
            p.links(),
            [
                Target::Param("P02-11".into()),
                Target::Topic("control-loops".into()),
                Target::Param("P02-13".into()),
                Target::Param("P02-14".into())
            ]
        );
        assert_eq!(p.body, "See [the gain](P02-13) and P02-14.");
        assert!(parse("P02-10", "---\ntitle: x\n---\n").is_err());
        assert!(parse("topic:x", "no title").is_err());
        assert!(parse("topic:x", "---\ntitle: X\ncolour: red\n---\n").is_err());
        assert!(parse("P02-10", "---\ndrawing: spiral\n---\n").is_err());
        assert!(parse("P02-10", "---\ndrawing: notch wide\n---\n").is_err());
        assert!(parse("P02-10", "---\nsee: nowhere\n---\n").is_err());
        assert!(parse("P02-10", "---\ndrawing: notch\n").is_err());
        assert!(parse("topic:x", "---\r\ntitle: X\r\n---\r\nBody.\r\n").is_ok());
        let t = parse("topic:x", "---\ntitle: X\ngroup: 3\norder: 20\n---\n").unwrap();
        assert_eq!((t.group, t.order), (Some(3), Some(20)));
        assert!(parse("topic:x", "---\ntitle: X\ngroup: 11\n---\n").is_err());
        assert!(parse("topic:x", "---\ntitle: X\norder: first\n---\n").is_err());
        assert!(parse("P02-10", "---\ngroup: 2\n---\n").is_err());
    }

    #[test]
    fn codes_in_text() {
        assert_eq!(
            segments("Set P02-10, not XP02-10 or P02-100."),
            [Segment::Text("Set "), Segment::Code("P02-10"), Segment::Text(", not XP02-10 or P02-100.")]
        );
        assert_eq!(segments("P01-04"), [Segment::Code("P01-04")]);
        assert_eq!(segments("é P01-04 é"), [Segment::Text("é "), Segment::Code("P01-04"), Segment::Text(" é")]);
        assert!(Target::parse("topic:Bad").is_none());
        assert!(Target::parse("http://x").is_none());
        assert_eq!(Target::parse("https://x"), Some(Target::Web("https://x".into())));
    }
}
