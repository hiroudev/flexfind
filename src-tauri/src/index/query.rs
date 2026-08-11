//! FlexFind's query syntax parser — the *only* parser in the app (see
//! project/design-brief.md's syntax help spec). The frontend never
//! re-implements this grammar; it sends the raw string to `search_index`
//! and renders highlight spans from the `match_start`/`match_len` this
//! module computes.
//!
//! Grammar:
//!   ext:png            — extension filter (case-insensitive, no dot)
//!   path:C:\Projects   — substring filter against the full path
//!   size:>10mb         — comparison (>, <, >=, <=, =), units b/kb/mb/gb
//!   dm:today           — date-modified (today/yesterday/thisweek/YYYY-MM-DD)
//!   "exact phrase"     — quoted phrase kept as one AND-term (not stronger
//!                        "exact match" semantics — see `SyntaxHelpPopover`
//!                        copy, which is honest about this)
//!   !term              — negation: excluded if found in name OR full path
//!   bare term           — space-separated, ANDed, matched against the name

use chrono::{Datelike, Local, NaiveDate, TimeZone};

use super::types::IndexedEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateModifiedFilter {
    Today,
    Yesterday,
    ThisWeek,
    Literal(NaiveDate),
}

#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    pub terms: Vec<String>,
    pub negations: Vec<String>,
    pub ext: Option<String>,
    pub path_contains: Option<String>,
    pub size_filter: Option<(SizeOp, u64)>,
    pub date_modified_filter: Option<DateModifiedFilter>,
}

impl ParsedQuery {
    /// True when the query has no filters/terms at all (blank/whitespace
    /// input). Matching every entry in this case isn't useful, so callers
    /// short-circuit rather than returning an arbitrary slice of the index.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.negations.is_empty()
            && self.ext.is_none()
            && self.path_contains.is_none()
            && self.size_filter.is_none()
            && self.date_modified_filter.is_none()
    }
}

pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut q = ParsedQuery::default();
    for token in tokenize(raw) {
        classify_token(&token, &mut q);
    }
    q
}

/// Splits on whitespace, but keeps double-quoted phrases (which may contain
/// spaces) as a single token. Quote characters themselves are stripped.
fn tokenize(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for c in raw.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if c.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(c);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn classify_token(token: &str, q: &mut ParsedQuery) {
    let lower = token.to_lowercase();
    if let Some(rest) = lower.strip_prefix('!') {
        if !rest.is_empty() {
            q.negations.push(rest.to_string());
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("ext:") {
        let v = v.trim_start_matches('.');
        if !v.is_empty() {
            q.ext = Some(v.to_string());
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("path:") {
        if !v.is_empty() {
            q.path_contains = Some(v.to_string());
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("size:") {
        if let Some(f) = parse_size_filter(v) {
            q.size_filter = Some(f);
        }
        return;
    }
    if let Some(v) = lower.strip_prefix("dm:") {
        if let Some(f) = parse_date_filter(v) {
            q.date_modified_filter = Some(f);
        }
        return;
    }
    if !lower.is_empty() {
        q.terms.push(lower);
    }
}

/// No explicit operator (bare `size:10mb`) defaults to "at least" (`Gte`) —
/// the most useful reading for a search box ("find big files").
fn parse_size_filter(v: &str) -> Option<(SizeOp, u64)> {
    let (op, rest) = if let Some(r) = v.strip_prefix(">=") {
        (SizeOp::Gte, r)
    } else if let Some(r) = v.strip_prefix("<=") {
        (SizeOp::Lte, r)
    } else if let Some(r) = v.strip_prefix('>') {
        (SizeOp::Gt, r)
    } else if let Some(r) = v.strip_prefix('<') {
        (SizeOp::Lt, r)
    } else if let Some(r) = v.strip_prefix('=') {
        (SizeOp::Eq, r)
    } else {
        (SizeOp::Gte, v)
    };
    let bytes = parse_size_value(rest)?;
    Some((op, bytes))
}

fn parse_size_value(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let split_at = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    let (num_part, unit_part) = s.split_at(split_at);
    let num: f64 = num_part.parse().ok()?;
    let mult: f64 = match unit_part.trim() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((num * mult) as u64)
}

fn parse_date_filter(v: &str) -> Option<DateModifiedFilter> {
    match v {
        "today" => Some(DateModifiedFilter::Today),
        "yesterday" => Some(DateModifiedFilter::Yesterday),
        "thisweek" => Some(DateModifiedFilter::ThisWeek),
        _ => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .ok()
            .map(DateModifiedFilter::Literal),
    }
}

fn date_matches(filter: &DateModifiedFilter, modified_unix: i64, now: chrono::DateTime<Local>) -> bool {
    let Some(modified_dt) = Local.timestamp_opt(modified_unix, 0).single() else {
        return false;
    };
    let modified_date = modified_dt.date_naive();
    let today = now.date_naive();
    match filter {
        DateModifiedFilter::Today => modified_date == today,
        DateModifiedFilter::Yesterday => modified_date == today - chrono::Duration::days(1),
        DateModifiedFilter::ThisWeek => {
            let days_since_monday = today.weekday().num_days_from_monday() as i64;
            let week_start = today - chrono::Duration::days(days_since_monday);
            modified_date >= week_start && modified_date <= today
        }
        DateModifiedFilter::Literal(d) => modified_date == *d,
    }
}

/// Returns `Some((match_start, match_len))` — a byte-offset span into
/// `entry.name` for highlighting the first bare-term match — if `entry`
/// satisfies every filter in `q`, or `None` otherwise. `(0, 0)` means "no
/// bare term to highlight" (e.g. an `ext:`/`path:`-only query).
pub fn matches(entry: &IndexedEntry, q: &ParsedQuery, now: chrono::DateTime<Local>) -> Option<(usize, usize)> {
    if let Some(ext) = &q.ext {
        if entry.ext != *ext {
            return None;
        }
    }
    if let Some(p) = &q.path_contains {
        if !entry.path.to_lowercase().contains(p.as_str()) {
            return None;
        }
    }
    if let Some((op, bytes)) = q.size_filter {
        let ok = match op {
            SizeOp::Gt => entry.size > bytes,
            SizeOp::Lt => entry.size < bytes,
            SizeOp::Gte => entry.size >= bytes,
            SizeOp::Lte => entry.size <= bytes,
            SizeOp::Eq => entry.size == bytes,
        };
        if !ok {
            return None;
        }
    }
    if let Some(filter) = &q.date_modified_filter {
        if !date_matches(filter, entry.modified, now) {
            return None;
        }
    }
    for neg in &q.negations {
        if entry.name_lower.contains(neg.as_str()) || entry.path.to_lowercase().contains(neg.as_str()) {
            return None;
        }
    }
    let mut first_match: Option<(usize, usize)> = None;
    for term in &q.terms {
        match entry.name_lower.find(term.as_str()) {
            Some(pos) => {
                if first_match.is_none() {
                    first_match = Some((pos, term.len()));
                }
            }
            None => return None,
        }
    }
    Some(first_match.unwrap_or((0, 0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, path: &str, size: u64, modified: i64) -> IndexedEntry {
        IndexedEntry {
            name: name.to_string(),
            name_lower: name.to_lowercase(),
            path: path.to_string(),
            dir: String::new(),
            folder: false,
            ext: std::path::Path::new(name)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            size,
            modified,
        }
    }

    #[test]
    fn bare_term_matches_case_insensitively() {
        let q = parse_query("flex");
        let e = entry("FlexFind.exe", "C:\\Projects\\FlexFind.exe", 100, 0);
        assert_eq!(matches(&e, &q, Local::now()), Some((0, 4)));
    }

    #[test]
    fn ext_filter() {
        let q = parse_query("ext:png");
        let hit = entry("photo.png", "C:\\pics\\photo.png", 100, 0);
        let miss = entry("photo.jpg", "C:\\pics\\photo.jpg", 100, 0);
        assert!(matches(&hit, &q, Local::now()).is_some());
        assert!(matches(&miss, &q, Local::now()).is_none());
    }

    #[test]
    fn path_filter() {
        let q = parse_query("path:C:\\Projects");
        let hit = entry("a.txt", "C:\\Projects\\a.txt", 1, 0);
        let miss = entry("a.txt", "D:\\Other\\a.txt", 1, 0);
        assert!(matches(&hit, &q, Local::now()).is_some());
        assert!(matches(&miss, &q, Local::now()).is_none());
    }

    #[test]
    fn size_filter_with_operator() {
        let q = parse_query("size:>10mb");
        let big = entry("big.bin", "C:\\big.bin", 20 * 1024 * 1024, 0);
        let small = entry("small.bin", "C:\\small.bin", 1024, 0);
        assert!(matches(&big, &q, Local::now()).is_some());
        assert!(matches(&small, &q, Local::now()).is_none());
    }

    #[test]
    fn size_filter_bare_defaults_to_gte() {
        let q = parse_query("size:10mb");
        let exact = entry("x.bin", "C:\\x.bin", 10 * 1024 * 1024, 0);
        assert!(matches(&exact, &q, Local::now()).is_some());
    }

    #[test]
    fn negation_excludes_by_name_or_path() {
        let q = parse_query("!node_modules");
        let hit = entry("index.ts", "C:\\src\\index.ts", 1, 0);
        let excluded = entry("index.js", "C:\\proj\\node_modules\\pkg\\index.js", 1, 0);
        assert!(matches(&hit, &q, Local::now()).is_some());
        assert!(matches(&excluded, &q, Local::now()).is_none());
    }

    #[test]
    fn quoted_phrase_kept_as_single_term() {
        let q = parse_query("\"Q2 report\"");
        assert_eq!(q.terms, vec!["q2 report".to_string()]);
        let hit = entry("Q2 report.docx", "C:\\docs\\Q2 report.docx", 1, 0);
        let miss = entry("report Q2 old.docx", "C:\\docs\\report Q2 old.docx", 1, 0);
        assert!(matches(&hit, &q, Local::now()).is_some());
        assert!(matches(&miss, &q, Local::now()).is_none());
    }

    #[test]
    fn date_modified_today() {
        let now = Local::now();
        let q = parse_query("dm:today");
        let hit = entry("a.txt", "C:\\a.txt", 1, now.timestamp());
        let miss = entry("b.txt", "C:\\b.txt", 1, now.timestamp() - 86400 * 3);
        assert!(matches(&hit, &q, now).is_some());
        assert!(matches(&miss, &q, now).is_none());
    }

    #[test]
    fn combined_ext_and_bare_term_and_negation() {
        let q = parse_query("ext:png flex !archive");
        let hit = entry("flexbox-cheatsheet.png", "C:\\pics\\flexbox-cheatsheet.png", 1, 0);
        let excluded = entry("flex.png", "D:\\Backup\\archive_flex\\flex.png", 1, 0);
        let wrong_ext = entry("flex.jpg", "C:\\pics\\flex.jpg", 1, 0);
        assert!(matches(&hit, &q, Local::now()).is_some());
        assert!(matches(&excluded, &q, Local::now()).is_none());
        assert!(matches(&wrong_ext, &q, Local::now()).is_none());
    }

    #[test]
    fn blank_query_is_empty() {
        assert!(parse_query("").is_empty());
        assert!(parse_query("   ").is_empty());
        assert!(!parse_query("flex").is_empty());
        assert!(!parse_query("ext:png").is_empty());
    }

    /// Documents the byte-offset contract that `index::engine::search`
    /// relies on to convert into UTF-16 code-unit offsets for the frontend
    /// (see engine.rs's `search`): `matches()` returns a *byte* offset into
    /// `name_lower`, not a char or UTF-16 index — callers on multi-byte
    /// names (e.g. Japanese) must convert before handing the offset to JS.
    #[test]
    fn match_offset_is_a_byte_offset_not_a_char_index() {
        let q = parse_query("report");
        // "レポート" is 4 characters / 4 UTF-16 units but 12 UTF-8 bytes.
        let e = entry("レポートreport.docx", "C:\\docs\\レポートreport.docx", 1, 0);
        let (start, len) = matches(&e, &q, Local::now()).unwrap();
        assert_eq!(start, "レポート".len()); // 12 bytes, not 4
        assert_eq!(len, "report".len());
    }
}
