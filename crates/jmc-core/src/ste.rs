//! Checks for the project's writing rules (Simplified Technical English), shared by the tests of
//! the parameter table and the help pages.

pub const BANNED: [&str; 9] =
    ["ensure", "utilize", "prior to", "in order to", "subsequently", "commence", "terminate", "facilitate", "numerous"];

/// The longest sentence allowed, in words.
pub const MAX_WORDS: usize = 25;

/// Chinese, Japanese or full-width characters.
pub fn foreign(t: &str) -> bool {
    t.chars().any(|c| matches!(c, '\u{3000}'..='\u{303F}' | '\u{3400}'..='\u{9FFF}' | '\u{FF00}'..='\u{FFEF}'))
}

/// The first banned word or phrase in `t`, as a whole word.
pub fn banned(t: &str) -> Option<&'static str> {
    let low = t.to_lowercase();
    BANNED.into_iter().find(|w| {
        low.match_indices(w).any(|(i, _)| {
            let before = low[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
            let after = low[i + w.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
            before && after
        })
    })
}

/// Sentence ends: `.`, `!` or `?` followed by a space and a capital letter or a digit.
pub fn split_sentences(t: &str) -> Vec<&str> {
    let b = t.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..b.len() {
        if matches!(b[i], b'.' | b'!' | b'?')
            && b.get(i + 1) == Some(&b' ')
            && b.get(i + 2).is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            out.push(t[start..=i].trim());
            start = i + 2;
        }
    }
    if start < t.len() {
        out.push(t[start..].trim());
    }
    out.retain(|s| !s.is_empty());
    out
}

/// Every rule broken by `t`, as messages. Empty when the text is fine.
pub fn problems(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    if foreign(t) {
        out.push(format!("foreign characters: {t}"));
    }
    if let Some(w) = banned(t) {
        out.push(format!("uses {w:?}: {t}"));
    }
    for s in split_sentences(t) {
        let n = s.split_whitespace().count();
        if n > MAX_WORDS {
            out.push(format!("sentence has {n} words: {s}"));
        }
    }
    out
}
