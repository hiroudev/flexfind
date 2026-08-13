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
//!
//! # Parse once, then compile once per search
//!
//! `ParsedQuery` is the grammar. `CompiledQuery` is that query bound to one
//! `IndexArena`, with everything resolvable up front already resolved:
//! `ext:` becomes an interned id compare, `dm:` becomes a unix-second range
//! (instead of a chrono conversion per entry), and the path-based
//! predicates become per-*directory* bit tables. The per-entry loop is then
//! only the tests the query actually uses — an unused filter costs nothing
//! and, just as importantly, never touches its column.

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use memchr::memmem;

use super::types::IndexArena;

/// A bare term with its needle-searcher prebuilt.
///
/// Both the needle and the haystacks are valid UTF-8 and UTF-8 is
/// self-synchronising, so a byte-level match can only ever land on a char
/// boundary — which is what lets the returned byte offset be sliced safely
/// downstream when it is converted to UTF-16 for the frontend.
struct Term {
    finder: memmem::Finder<'static>,
    len: usize,
}

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

/// Local midnight of `d` as a unix timestamp. `earliest()` rather than
/// `single()` so a DST spring-forward day (where local midnight may not
/// exist) still yields a usable boundary instead of disabling the filter.
fn local_day_start(d: NaiveDate) -> Option<i64> {
    let naive = d.and_hms_opt(0, 0, 0)?;
    Local.from_local_datetime(&naive).earliest().map(|dt| dt.timestamp())
}

/// Resolve a `dm:` filter to an inclusive unix-second range, once per
/// search. The previous implementation converted every entry's timestamp
/// into a `DateTime<Local>` to compare calendar dates, which is far more
/// expensive than two integer compares.
fn date_range(filter: &DateModifiedFilter, now: chrono::DateTime<Local>) -> Option<(i64, i64)> {
    let today = now.date_naive();
    let (from, through) = match filter {
        DateModifiedFilter::Today => (today, today),
        DateModifiedFilter::Yesterday => {
            let y = today - chrono::Duration::days(1);
            (y, y)
        }
        DateModifiedFilter::ThisWeek => {
            let days_since_monday = today.weekday().num_days_from_monday() as i64;
            (today - chrono::Duration::days(days_since_monday), today)
        }
        DateModifiedFilter::Literal(d) => (*d, *d),
    };
    let start = local_day_start(from)?;
    let end = local_day_start(through.succ_opt()?)? - 1;
    Some((start, end))
}

/// Maximum separator positions a `path:`/negation pattern can have before
/// the per-directory bit table stops being able to hold them all. A pattern
/// with more than this many backslashes falls back to building the full
/// path for the entries that reach it.
const MAX_SPLITS: usize = 31;

/// A substring pattern precompiled against one arena's directory table, so
/// "does the full path contain this?" is answerable per entry without
/// building the path.
///
/// A full path is `dir + '\' + name`, so a match lies entirely in `dir`,
/// entirely in `name`, or straddles the separator. Straddling requires the
/// pattern to contain the separator itself, and then the match can only
/// line up at one of the pattern's own separator positions — so those are
/// the only splits worth precomputing. Everything about `dir` is hoisted
/// into a bit per directory (231,850 of them on a measured C: drive, versus
/// 1,269,687 entries).
struct PathPattern {
    pat: String,
    finder: memmem::Finder<'static>,
    /// Bit 0: this directory contains `pat`.
    /// Bit `1 + i`: this directory ends with `pat[..splits[i]]`.
    dir_bits: Vec<u32>,
    splits: Vec<usize>,
    /// Set for absurd patterns with more separators than `dir_bits` can
    /// encode; forces the exact (allocating) path check.
    needs_full_path: bool,
}

impl PathPattern {
    fn compile(pat: &str, arena: &IndexArena) -> Self {
        let splits: Vec<usize> = pat.match_indices('\\').map(|(i, _)| i).collect();
        let needs_full_path = splits.len() > MAX_SPLITS;
        let splits = if needs_full_path { Vec::new() } else { splits };
        let finder = memmem::Finder::new(pat).into_owned();

        let mut dir_bits = Vec::with_capacity(arena.dir_count());
        for id in 0..arena.dir_count() {
            let dir = arena.dir_lower_by_id(id as u32);
            let mut bits = 0u32;
            if finder.find(dir.as_bytes()).is_some() {
                bits |= 1;
            }
            for (i, &k) in splits.iter().enumerate() {
                if dir.ends_with(&pat[..k]) {
                    bits |= 1 << (i + 1);
                }
            }
            dir_bits.push(bits);
        }
        PathPattern { pat: pat.to_string(), finder, dir_bits, splits, needs_full_path }
    }

    #[inline]
    fn matches(&self, arena: &IndexArena, i: usize, name_lower: &str) -> bool {
        if self.needs_full_path {
            return arena.full_path_lower(i).contains(&self.pat);
        }
        let bits = self.dir_bits[arena.dir_id(i) as usize];
        if bits & 1 != 0 {
            return true;
        }
        if self.finder.find(name_lower.as_bytes()).is_some() {
            return true;
        }
        for (idx, &k) in self.splits.iter().enumerate() {
            if bits & (1 << (idx + 1)) != 0 && name_lower.starts_with(&self.pat[k + 1..]) {
                return true;
            }
        }
        false
    }
}

/// A `ParsedQuery` bound to one arena, ready to run.
pub struct CompiledQuery {
    terms: Vec<Term>,
    negations: Vec<PathPattern>,
    path_contains: Option<PathPattern>,
    ext_id: Option<u32>,
    size_filter: Option<(SizeOp, u64)>,
    date_range: Option<(i64, i64)>,
    /// True when some filter cannot match anything in this arena — e.g. an
    /// `ext:` for an extension no indexed file has, or a `dm:` whose range
    /// couldn't be resolved. The caller skips the scan entirely.
    matches_nothing: bool,
}

impl CompiledQuery {
    pub fn compile(q: &ParsedQuery, arena: &IndexArena, now: chrono::DateTime<Local>) -> Self {
        let mut matches_nothing = false;

        let ext_id = match &q.ext {
            None => None,
            Some(e) => match arena.find_ext_id(e) {
                Some(id) => Some(id),
                None => {
                    matches_nothing = true;
                    None
                }
            },
        };

        let date_range = match &q.date_modified_filter {
            None => None,
            Some(f) => match date_range(f, now) {
                Some(r) => Some(r),
                None => {
                    matches_nothing = true;
                    None
                }
            },
        };

        // Building the per-directory tables costs a pass over the directory
        // list, so only do it for patterns the query actually has.
        let (negations, path_contains) = if matches_nothing {
            (Vec::new(), None)
        } else {
            (
                q.negations.iter().map(|n| PathPattern::compile(n, arena)).collect(),
                q.path_contains.as_ref().map(|p| PathPattern::compile(p, arena)),
            )
        };

        let terms = q
            .terms
            .iter()
            .map(|t| Term { finder: memmem::Finder::new(t).into_owned(), len: t.len() })
            .collect();

        CompiledQuery {
            terms,
            negations,
            path_contains,
            ext_id,
            size_filter: q.size_filter,
            date_range,
            matches_nothing,
        }
    }

    pub fn matches_nothing(&self) -> bool {
        self.matches_nothing
    }

    /// Returns `Some((match_start, match_len))` — a byte-offset span into
    /// entry `i`'s lowercased name, for highlighting the first bare-term
    /// match — if the entry satisfies every filter, or `None` otherwise.
    /// `(0, 0)` means "no bare term to highlight" (e.g. an
    /// `ext:`/`path:`-only query).
    ///
    /// Filters are ordered cheapest-and-narrowest first. That is not only
    /// about instruction count: each filter reads a different column, and
    /// rejecting an entry early means never touching the (much larger) name
    /// data for it.
    #[inline]
    pub fn matches(&self, arena: &IndexArena, i: usize) -> Option<(usize, usize)> {
        if let Some(want) = self.ext_id {
            if arena.ext_id(i) != want {
                return None;
            }
        }
        if let Some((op, bytes)) = self.size_filter {
            let size = arena.size(i);
            let ok = match op {
                SizeOp::Gt => size > bytes,
                SizeOp::Lt => size < bytes,
                SizeOp::Gte => size >= bytes,
                SizeOp::Lte => size <= bytes,
                SizeOp::Eq => size == bytes,
            };
            if !ok {
                return None;
            }
        }
        if let Some((start, end)) = self.date_range {
            let m = arena.modified(i);
            if m < start || m > end {
                return None;
            }
        }

        let name_lower = arena.name_lower(i);

        let mut first_match: Option<(usize, usize)> = None;
        for term in &self.terms {
            match term.finder.find(name_lower.as_bytes()) {
                Some(pos) => {
                    if first_match.is_none() {
                        first_match = Some((pos, term.len));
                    }
                }
                None => return None,
            }
        }

        for neg in &self.negations {
            if neg.matches(arena, i, name_lower) {
                return None;
            }
        }
        if let Some(p) = &self.path_contains {
            if !p.matches(arena, i, name_lower) {
                return None;
            }
        }

        Some(first_match.unwrap_or((0, 0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::types::ArenaBuilder;

    /// Build a one-entry arena and run a query against it, returning the
    /// highlight span exactly as the engine would see it.
    fn run(query: &str, path: &str, folder: bool, size: u64, modified: i64) -> Option<(usize, usize)> {
        run_at(query, path, folder, size, modified, Local::now())
    }

    fn run_at(
        query: &str,
        path: &str,
        folder: bool,
        size: u64,
        modified: i64,
        now: chrono::DateTime<Local>,
    ) -> Option<(usize, usize)> {
        let mut b = ArenaBuilder::new();
        b.push(path, folder, size, modified);
        let arena = b.finish();
        let parsed = parse_query(query);
        let compiled = CompiledQuery::compile(&parsed, &arena, now);
        if compiled.matches_nothing() {
            return None;
        }
        compiled.matches(&arena, 0)
    }

    #[test]
    fn bare_term_matches_case_insensitively() {
        assert_eq!(run("flex", "C:\\Projects\\FlexFind.exe", false, 100, 0), Some((0, 4)));
    }

    #[test]
    fn ext_filter() {
        assert!(run("ext:png", "C:\\pics\\photo.png", false, 100, 0).is_some());
        assert!(run("ext:png", "C:\\pics\\photo.jpg", false, 100, 0).is_none());
    }

    #[test]
    fn path_filter() {
        assert!(run("path:C:\\Projects", "C:\\Projects\\a.txt", false, 1, 0).is_some());
        assert!(run("path:C:\\Projects", "D:\\Other\\a.txt", false, 1, 0).is_none());
    }

    /// The per-directory precompute must still catch a `path:` pattern
    /// whose match straddles the directory/name separator.
    #[test]
    fn path_filter_matches_across_the_separator() {
        assert!(run("path:projects\\rep", "C:\\Projects\\Report.docx", false, 1, 0).is_some());
        assert!(run("path:projects\\zzz", "C:\\Projects\\Report.docx", false, 1, 0).is_none());
    }

    #[test]
    fn path_filter_matches_within_the_name_alone() {
        assert!(run("path:report", "C:\\Projects\\Report.docx", false, 1, 0).is_some());
    }

    #[test]
    fn size_filter_with_operator() {
        assert!(run("size:>10mb", "C:\\big.bin", false, 20 * 1024 * 1024, 0).is_some());
        assert!(run("size:>10mb", "C:\\small.bin", false, 1024, 0).is_none());
    }

    #[test]
    fn size_filter_bare_defaults_to_gte() {
        assert!(run("size:10mb", "C:\\x.bin", false, 10 * 1024 * 1024, 0).is_some());
    }

    #[test]
    fn negation_excludes_by_name_or_path() {
        assert!(run("!node_modules", "C:\\src\\index.ts", false, 1, 0).is_some());
        assert!(run("!node_modules", "C:\\proj\\node_modules\\pkg\\index.js", false, 1, 0).is_none());
    }

    #[test]
    fn negation_excludes_by_name_when_path_is_clean() {
        assert!(run("!draft", "C:\\docs\\draft-report.docx", false, 1, 0).is_none());
    }

    #[test]
    fn quoted_phrase_kept_as_single_term() {
        let q = parse_query("\"Q2 report\"");
        assert_eq!(q.terms, vec!["q2 report".to_string()]);
        assert!(run("\"Q2 report\"", "C:\\docs\\Q2 report.docx", false, 1, 0).is_some());
        assert!(run("\"Q2 report\"", "C:\\docs\\report Q2 old.docx", false, 1, 0).is_none());
    }

    #[test]
    fn date_modified_today() {
        let now = Local::now();
        assert!(run_at("dm:today", "C:\\a.txt", false, 1, now.timestamp(), now).is_some());
        assert!(run_at("dm:today", "C:\\b.txt", false, 1, now.timestamp() - 86400 * 3, now).is_none());
    }

    #[test]
    fn date_modified_yesterday_and_thisweek() {
        let now = Local::now();
        let yesterday = now.timestamp() - 86400;
        assert!(run_at("dm:yesterday", "C:\\a.txt", false, 1, yesterday, now).is_some());
        assert!(run_at("dm:yesterday", "C:\\b.txt", false, 1, now.timestamp(), now).is_none());
        // Today is always inside "this week" regardless of which weekday it is.
        assert!(run_at("dm:thisweek", "C:\\c.txt", false, 1, now.timestamp(), now).is_some());
    }

    #[test]
    fn combined_ext_and_bare_term_and_negation() {
        assert!(run("ext:png flex !archive", "C:\\pics\\flexbox-cheatsheet.png", false, 1, 0).is_some());
        assert!(run("ext:png flex !archive", "D:\\Backup\\archive_flex\\flex.png", false, 1, 0).is_none());
        assert!(run("ext:png flex !archive", "C:\\pics\\flex.jpg", false, 1, 0).is_none());
    }

    #[test]
    fn blank_query_is_empty() {
        assert!(parse_query("").is_empty());
        assert!(parse_query("   ").is_empty());
        assert!(!parse_query("flex").is_empty());
        assert!(!parse_query("ext:png").is_empty());
    }

    /// An `ext:` nothing in the arena has lets the engine skip the scan
    /// outright rather than testing every entry.
    #[test]
    fn unknown_extension_short_circuits_the_whole_scan() {
        let mut b = ArenaBuilder::new();
        b.push("C:\\a\\x.png", false, 1, 0);
        let arena = b.finish();
        let parsed = parse_query("ext:xyz");
        let compiled = CompiledQuery::compile(&parsed, &arena, Local::now());
        assert!(compiled.matches_nothing());
    }

    /// Documents the byte-offset contract that `index::engine` relies on to
    /// convert into UTF-16 code-unit offsets for the frontend: the span is
    /// a *byte* offset into the lowercased name, not a char or UTF-16
    /// index — callers on multi-byte names (e.g. Japanese) must convert
    /// before handing the offset to JS.
    #[test]
    fn match_offset_is_a_byte_offset_not_a_char_index() {
        // "レポート" is 4 characters / 4 UTF-16 units but 12 UTF-8 bytes.
        let (start, len) =
            run("report", "C:\\docs\\レポートreport.docx", false, 1, 0).unwrap();
        assert_eq!(start, "レポート".len()); // 12 bytes, not 4
        assert_eq!(len, "report".len());
    }
}
