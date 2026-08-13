//! In-memory index + search engine, held as Tauri managed state
//! (`Arc<IndexEngine>`, see `lib.rs`).
//!
//! The index is one packed `IndexArena` per scan root (see `types.rs` for
//! why the layout looks like that), searched by a linear scan. A trie or
//! n-gram structure is still deliberately not used: once a search reads
//! only the packed lowercase-name buffer, the scan is bounded by streaming
//! ~50 MB sequentially, which is fast warm and cheap to fault in cold.
//!
//! What made the previous version slow was never the scan's instruction
//! count — it was that one search touched the entire ~640 MB index through
//! a pointer per entry, and Windows had trimmed all of it out of the
//! working set while the app sat in the tray. So the shape of this module
//! is organised around *not touching pages*:
//!
//! * Per-root arenas, so a disabled root is dropped by removing its arena
//!   rather than by scanning every entry's path (the old `retain` lowercased
//!   1.27M paths on every rescan).
//! * Scope/`path:`/negation predicates are resolved per *directory* before
//!   the scan (231,850 directories versus 1,269,687 entries, measured), so
//!   the per-entry test is an array lookup and never an allocation.
//! * Filters are ordered so an entry can be rejected before its name is
//!   read at all.
//! * Full paths are rebuilt only for the hits actually returned.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::iter::once;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};

use chrono::Local;
use tauri::{AppHandle, Emitter, Manager};

use super::pathmatch::path_has_prefix;
use super::persist;
use super::query::{self, CompiledQuery, ParsedQuery};
use super::types::{
    ArenaBuilder, ArenaMemory, IndexArena, RootIndexStatus, RootScanState, SearchHit,
    SearchResponse, SortColumn, SortSpec,
};
use super::walker;
use crate::settings::{self, ScanRoot};

/// Cap on hits returned to the frontend per search — keeps IPC payload size
/// and per-keystroke React diffing bounded even though the backing index
/// can hold millions of entries.
const DEFAULT_LIMIT: usize = 500;

/// A search slower than this gets a one-line breakdown on stderr. Set high
/// enough to stay silent in normal use, low enough to catch the
/// pathological case this module was rewritten to fix.
const SLOW_SEARCH_LOG_MS: u128 = 200;

/// A resolved scan target: its canonical root string (`"C:"` or full path),
/// the filesystem path to actually walk, and whether it's a drive.
#[derive(Clone)]
struct ResolvedRoot {
    root: String, // canonical, matches status-map key
    walk_path: PathBuf,
    label: String,
    is_drive: bool,
}

/// One root's packed index. `Arc` so a search can take a cheap snapshot and
/// then scan without holding the lock — otherwise a multi-hundred-millisecond
/// cold search would block a finishing rescan from swapping in its results.
#[derive(Clone)]
struct RootIndex {
    root: String,
    arena: Arc<IndexArena>,
}

pub struct IndexEngine {
    /// Kept sorted by lowercased root so search result order (and therefore
    /// which entries survive the unsorted fast path's cap) is stable across
    /// rescans.
    roots: RwLock<Vec<RootIndex>>,
    status: RwLock<HashMap<String, RootIndexStatus>>,
    paused: AtomicBool,
    /// Bumped on every (re)scan so a stale scan thread from a superseded
    /// generation stops writing entries/status once a newer one has started.
    generation: AtomicU64,
}

/// Which entries a saved search scope admits, resolved once per search per
/// root.
struct ScopeFilter {
    /// `None` means "no scope set — everything is in scope". Otherwise one
    /// flag per interned directory.
    dir_bits: Option<Vec<bool>>,
    /// The scope roots themselves, as (parent directory id, lowercased leaf
    /// name). A folder entry that *is* the scope root lives in the scope's
    /// parent directory, so it isn't covered by `dir_bits`; the pre-arena
    /// code included it (its path equalled the include path) and this keeps
    /// that behavior.
    exact: Vec<(u32, String)>,
}

impl ScopeFilter {
    fn build(arena: &IndexArena, includes_lower: Option<&[String]>) -> Self {
        let Some(list) = includes_lower else {
            return ScopeFilter { dir_bits: None, exact: Vec::new() };
        };
        if list.is_empty() {
            return ScopeFilter { dir_bits: None, exact: Vec::new() };
        }

        let parents: Vec<(&str, &str)> = list
            .iter()
            .filter_map(|inc| inc.rfind(['\\', '/']).map(|i| (&inc[..i], &inc[i + 1..])))
            .collect();

        let n = arena.dir_count();
        let mut dir_bits = vec![false; n];
        let mut exact = Vec::new();
        for id in 0..n {
            let d = arena.dir_lower_by_id(id as u32);
            dir_bits[id] = list.iter().any(|inc| path_has_prefix(d, inc));
            for (parent, leaf) in &parents {
                if d == *parent {
                    exact.push((id as u32, (*leaf).to_string()));
                }
            }
        }
        ScopeFilter { dir_bits: Some(dir_bits), exact }
    }

    /// True when no entry in this root can be in scope, so the caller can
    /// skip the root's scan entirely (common with a scope pinned to one
    /// drive and several drives indexed).
    fn matches_nothing(&self) -> bool {
        match &self.dir_bits {
            None => false,
            Some(bits) => self.exact.is_empty() && !bits.iter().any(|b| *b),
        }
    }

    #[inline]
    fn allows(&self, arena: &IndexArena, i: usize) -> bool {
        let Some(bits) = &self.dir_bits else {
            return true;
        };
        let did = arena.dir_id(i);
        if bits[did as usize] {
            return true;
        }
        if self.exact.is_empty() {
            return false;
        }
        let name = arena.name_lower(i);
        self.exact.iter().any(|(pid, leaf)| *pid == did && name == leaf.as_str())
    }
}

/// A match, identified by which root's arena it came from. 16 bytes, so
/// even a very broad match set stays cheap to collect before selection.
#[derive(Clone, Copy)]
struct Candidate {
    root: u32,
    idx: u32,
    span_start: u32,
    span_len: u32,
}

impl IndexEngine {
    pub fn new() -> Self {
        IndexEngine {
            roots: RwLock::new(Vec::new()),
            status: RwLock::new(HashMap::new()),
            paused: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(AtomicOrdering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, AtomicOrdering::Relaxed);
    }

    /// Whether any root is currently mid-walk — used to skip a periodic
    /// freshness-rebuild tick that would otherwise supersede (and thus
    /// waste) a walk that's still running, e.g. on a slow HDD / NAS.
    pub fn any_scanning(&self) -> bool {
        self.status
            .read()
            .expect("status lock poisoned")
            .values()
            .any(|d| d.state == RootScanState::Scanning)
    }

    pub fn status_snapshot(&self) -> Vec<RootIndexStatus> {
        let mut v: Vec<RootIndexStatus> = self
            .status
            .read()
            .expect("status lock poisoned")
            .values()
            .cloned()
            .collect();
        v.sort_by(|a, b| a.root.to_lowercase().cmp(&b.root.to_lowercase()));
        v
    }

    /// Cheap snapshot of the per-root arenas: a few `Arc` clones. Searching
    /// off the snapshot instead of under the read guard means a rescan
    /// finishing mid-search doesn't have to wait for it.
    fn snapshot(&self) -> Vec<RootIndex> {
        self.roots.read().expect("roots lock poisoned").clone()
    }

    /// Replace one root's arena, keeping the vector sorted by lowercased
    /// root name.
    fn put_root(roots: &mut Vec<RootIndex>, root: String, arena: Arc<IndexArena>) {
        let key = root.to_lowercase();
        roots.retain(|ri| ri.root.to_lowercase() != key);
        roots.push(RootIndex { root, arena });
        roots.sort_by(|a, b| a.root.to_lowercase().cmp(&b.root.to_lowercase()));
    }

    /// Load persisted per-root indexes at startup, before the first live
    /// rescan, so results are searchable immediately. Roots that load
    /// successfully are marked `loaded_from_disk` + `Done`; the subsequent
    /// `spawn_scan` refreshes them in the background.
    pub fn load_persisted(&self, app: &AppHandle) {
        let Ok(data_dir) = app.path().app_data_dir() else {
            return;
        };
        let s = settings::load_settings(app);
        let resolved = resolve_roots(&s.scan_roots);

        let mut loaded: Vec<RootIndex> = Vec::new();
        for r in &resolved {
            if let Some(arena) = persist::load_root(&data_dir, &r.root) {
                loaded.push(RootIndex { root: r.root.clone(), arena: Arc::new(arena) });
            }
        }
        if loaded.is_empty() {
            return;
        }
        loaded.sort_by(|a, b| a.root.to_lowercase().cmp(&b.root.to_lowercase()));

        let counts: HashMap<String, u64> =
            loaded.iter().map(|ri| (ri.root.clone(), ri.arena.len() as u64)).collect();

        {
            let mut w = self.roots.write().expect("roots lock poisoned");
            *w = loaded;
        }
        {
            let mut st = self.status.write().expect("status lock poisoned");
            for r in &resolved {
                let count = counts.get(&r.root).copied();
                st.insert(
                    r.root.clone(),
                    RootIndexStatus {
                        root: r.root.clone(),
                        label: r.label.clone(),
                        state: if count.is_some() {
                            RootScanState::Done
                        } else {
                            RootScanState::Pending
                        },
                        scanned_count: count.unwrap_or(0),
                        is_drive: r.is_drive,
                        loaded_from_disk: count.is_some(),
                        // Unknown until this session's own walk runs; a
                        // persisted index carries no record of what the
                        // previous run couldn't read.
                        skipped_dirs: 0,
                        skipped_samples: Vec::new(),
                    },
                );
            }
        }
        emit_status(app, self);
    }

    pub fn search(
        &self,
        raw_query: &str,
        limit: usize,
        include_paths: Option<&[String]>,
        sort: Option<SortSpec>,
    ) -> SearchResponse {
        let started = std::time::Instant::now();
        let parsed: ParsedQuery = query::parse_query(raw_query);
        let roots = self.snapshot();
        let total_indexed: u64 = roots.iter().map(|ri| ri.arena.len() as u64).sum();

        if parsed.is_empty() {
            // Blank/whitespace query — an arbitrary slice of the whole index
            // isn't useful. The frontend also short-circuits before calling
            // this; belt-and-braces for any other caller.
            return SearchResponse { hits: Vec::new(), total_matched: 0, total_indexed };
        }

        // Scope include paths: lowered once, here, rather than per entry.
        let includes_lower: Option<Vec<String>> = include_paths.map(|ps| {
            ps.iter()
                .map(|p| p.trim_end_matches(['\\', '/']).to_lowercase())
                .filter(|p| !p.is_empty())
                .collect()
        });

        let now = Local::now();
        let mut total_matched: u64 = 0;
        // Only the sorted path needs every match; the unsorted path keeps
        // the first `limit` in index order, as it always has.
        let mut collected: Vec<Candidate> = Vec::new();
        let mut fast_hits: Vec<Candidate> = Vec::new();

        for (root_i, ri) in roots.iter().enumerate() {
            let arena = &*ri.arena;
            let compiled = CompiledQuery::compile(&parsed, arena, now);
            if compiled.matches_nothing() {
                continue;
            }
            let scope = ScopeFilter::build(arena, includes_lower.as_deref());
            if scope.matches_nothing() {
                continue;
            }

            for i in 0..arena.len() {
                // Scope first: it is a single array lookup against the
                // smallest column, and rejecting here means never reading
                // this entry's name.
                if !scope.allows(arena, i) {
                    continue;
                }
                let Some((s, l)) = compiled.matches(arena, i) else {
                    continue;
                };
                total_matched += 1;
                let c = Candidate {
                    root: root_i as u32,
                    idx: i as u32,
                    span_start: s as u32,
                    span_len: l as u32,
                };
                if sort.is_some() {
                    collected.push(c);
                } else if fast_hits.len() < limit {
                    fast_hits.push(c);
                }
            }
        }

        let chosen = match sort {
            None => fast_hits,
            Some(spec) => {
                select_top(&mut collected, limit, spec, &roots);
                collected
            }
        };

        let hits: Vec<SearchHit> = chosen
            .iter()
            .map(|c| {
                build_hit(
                    &roots[c.root as usize].arena,
                    c.idx as usize,
                    (c.span_start as usize, c.span_len as usize),
                )
            })
            .collect();

        let elapsed = started.elapsed();
        if elapsed.as_millis() >= SLOW_SEARCH_LOG_MS {
            eprintln!(
                "FlexFind: slow search {:?} for {:?} — {} indexed, {} matched, sorted={}",
                elapsed,
                raw_query,
                total_indexed,
                total_matched,
                sort.is_some()
            );
        }

        SearchResponse { hits, total_matched, total_indexed }
    }

    /// (Re)scan every enabled root on its own background OS thread. Disabled
    /// / removed roots have their arenas dropped up front; other roots keep
    /// serving their previously-indexed (or disk-loaded) entries while this
    /// generation's scan is in flight, so searches never see the index go
    /// blank — and an offline root keeps its last-known entries entirely.
    pub fn spawn_scan(engine: Arc<IndexEngine>, app: AppHandle) {
        let my_generation = engine.generation.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        let s = settings::load_settings(&app);
        let excludes = s.exclude_paths.clone();
        let filters = walker::FilterOpts {
            exclude_hidden: s.exclude_hidden,
            exclude_system: s.exclude_system,
            exclude_shortcuts: s.exclude_shortcuts,
        };
        let roots = resolve_roots(&s.scan_roots);
        let enabled_lower: Vec<String> = roots.iter().map(|r| r.root.to_lowercase()).collect();

        // Rebuild the status map. Preserve `loaded_from_disk` from the prior
        // map per root so a warm (disk-loaded) root doesn't flash the big
        // first-build panel just because a refresh walk reset it to Pending.
        {
            let prev = engine.status.read().expect("status lock poisoned").clone();
            let mut status = engine.status.write().expect("status lock poisoned");
            status.clear();
            for r in &roots {
                let prev_root = prev.get(&r.root);
                let loaded_from_disk = prev_root.map(|p| p.loaded_from_disk).unwrap_or(false);
                // Carry the previous walk's skip report until this one
                // replaces it, so the warning doesn't blink out of the UI
                // for the duration of every refresh.
                let (skipped_dirs, skipped_samples) = prev_root
                    .map(|p| (p.skipped_dirs, p.skipped_samples.clone()))
                    .unwrap_or((0, Vec::new()));
                status.insert(
                    r.root.clone(),
                    RootIndexStatus {
                        root: r.root.clone(),
                        label: r.label.clone(),
                        state: RootScanState::Pending,
                        scanned_count: prev.get(&r.root).map(|p| p.scanned_count).unwrap_or(0),
                        is_drive: r.is_drive,
                        loaded_from_disk,
                        skipped_dirs,
                        skipped_samples,
                    },
                );
            }
        }
        // Drop arenas for roots that are no longer enabled. Previously this
        // meant lowercasing and prefix-testing every entry's path; now it's
        // a handful of root-name comparisons.
        {
            let mut w = engine.roots.write().expect("roots lock poisoned");
            w.retain(|ri| enabled_lower.contains(&ri.root.to_lowercase()));
        }
        emit_status(&app, &engine);

        for r in roots {
            let engine = engine.clone();
            let app = app.clone();
            let excludes = excludes.clone();
            std::thread::spawn(move || scan_one_root(engine, app, r, excludes, filters, my_generation));
        }
    }
}

/// Pick the `limit` best candidates under `spec` and leave them sorted.
///
/// `select_nth_unstable_by` partitions in linear time, so only the surviving
/// `limit` are fully ordered. The pre-arena code sorted the entire match set
/// (potentially hundreds of thousands of entries) and only then truncated to
/// 500.
fn select_top(matched: &mut Vec<Candidate>, limit: usize, spec: SortSpec, roots: &[RootIndex]) {
    let cmp = |x: &Candidate, y: &Candidate| -> Ordering {
        let ax = &roots[x.root as usize].arena;
        let ay = &roots[y.root as usize].arena;
        let (xi, yi) = (x.idx as usize, y.idx as usize);
        // Path is the tie-break for every column so ordering is deterministic.
        let by_path = || {
            cmp_full_path((ax.dir(xi), ax.name(xi)), (ay.dir(yi), ay.name(yi)))
        };
        let ord = match spec.column {
            SortColumn::Name => ax.name_lower(xi).cmp(ay.name_lower(yi)).then_with(by_path),
            SortColumn::Path => by_path(),
            SortColumn::Size => ax.size(xi).cmp(&ay.size(yi)).then_with(by_path),
            SortColumn::Modified => ax.modified(xi).cmp(&ay.modified(yi)).then_with(by_path),
        };
        if spec.descending {
            ord.reverse()
        } else {
            ord
        }
    };

    if limit == 0 {
        matched.clear();
        return;
    }
    if matched.len() > limit {
        matched.select_nth_unstable_by(limit - 1, cmp);
        matched.truncate(limit);
    }
    matched.sort_unstable_by(cmp);
}

/// Lexicographic comparison of two entries' full paths without building
/// them, equivalent to comparing `dir + '\' + name` byte-wise — which is
/// what the pre-arena code did when it compared stored `path` strings.
///
/// Comparing `(dir, name)` as a plain tuple would *not* be equivalent: for
/// `C:\a` + `zzz` versus `C:\a\b` + `x`, the tuple says the first is
/// smaller (its directory is a prefix) while the real paths order the other
/// way (`C:\a\b\x` < `C:\a\zzz`).
fn cmp_full_path(a: (&str, &str), b: (&str, &str)) -> Ordering {
    let (ad, an) = a;
    let (bd, bn) = b;
    ad.bytes()
        .chain(once(b'\\'))
        .chain(an.bytes())
        .cmp(bd.bytes().chain(once(b'\\')).chain(bn.bytes()))
}

/// True when `path` is a link-like reparse point — a symlink, a junction, or
/// a volume mount point.
///
/// This is exactly the set of directories that the root walk yields but
/// never descends into, because `walkdir` runs with its default
/// `follow_links(false)` (deliberately: Windows' legacy compatibility
/// junctions such as `C:\Users\All Users` would otherwise duplicate whole
/// subtrees, and `C:\ProgramData\Application Data` points at itself).
///
/// Rust reports precisely the "name surrogate" reparse tags as symlinks,
/// which is the same condition `walkdir` uses, so this agrees with the walk
/// rather than guessing.
fn is_link_like(path: &str) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Resolve settings' `scan_roots` into effective walk targets: enabled only,
/// and with any root nested under another enabled root removed (so
/// overlapping roots don't double-index or race on retain). Drives get a
/// `"C:\"` walk path; custom/UNC roots are walked as-is.
fn resolve_roots(scan_roots: &[ScanRoot]) -> Vec<ResolvedRoot> {
    let enabled: Vec<&ScanRoot> = scan_roots.iter().filter(|r| r.enabled).collect();
    let mut out: Vec<ResolvedRoot> = Vec::new();
    for r in &enabled {
        let canon = r.path.trim_end_matches(['\\', '/']).to_string();
        let canon_lower = canon.to_lowercase();
        // Skip if nested under a *different* enabled root — but only when
        // that outer root's walk can actually reach it. A cloud-storage
        // mount like `C:\Box` sits under `C:` yet is a reparse point the
        // `C:` walk steps over, so treating it as redundant silently made
        // it unindexable: adding it explicitly was dropped here, and the
        // drive walk never descended into it.
        let nested = !is_link_like(&canon)
            && enabled.iter().any(|other| {
                let other_lower = other.path.trim_end_matches(['\\', '/']).to_lowercase();
                other_lower != canon_lower && path_has_prefix(&canon_lower, &other_lower)
            });
        if nested {
            continue;
        }
        let walk_path = if r.is_drive || canon.ends_with(':') {
            PathBuf::from(format!("{canon}\\"))
        } else {
            PathBuf::from(&canon)
        };
        let label = if r.is_drive {
            canon.clone()
        } else {
            canon
                .rsplit(['\\', '/'])
                .find(|s| !s.is_empty())
                .unwrap_or(&canon)
                .to_string()
        };
        out.push(ResolvedRoot { root: canon, walk_path, label, is_drive: r.is_drive });
    }
    out
}

fn build_hit(arena: &IndexArena, i: usize, span: (usize, usize)) -> SearchHit {
    let (match_start, match_len) = span;
    // `CompiledQuery::matches` returns a *byte* offset/length into the
    // lowercased name (Rust string indexing is UTF-8 bytes), but the
    // frontend slices `name` with JS string indices (UTF-16 code units) —
    // convert so Japanese (and any other multi-byte) filenames highlight
    // correctly.
    let (match_start, match_len) = if match_len > 0 {
        let name_lower = arena.name_lower(i);
        let utf16_start = name_lower[..match_start].encode_utf16().count();
        let utf16_len = name_lower[match_start..match_start + match_len].encode_utf16().count();
        (utf16_start, utf16_len)
    } else {
        (0, 0)
    };
    SearchHit {
        name: arena.name(i).to_string(),
        path: arena.full_path(i),
        dir: arena.dir(i).to_string(),
        folder: arena.folder(i),
        ext: arena.ext(i).to_string(),
        size: arena.size(i),
        modified: arena.modified(i),
        match_start,
        match_len,
    }
}

fn scan_one_root(
    engine: Arc<IndexEngine>,
    app: AppHandle,
    r: ResolvedRoot,
    excludes: Vec<String>,
    filters: walker::FilterOpts,
    generation: u64,
) {
    if engine.generation.load(AtomicOrdering::SeqCst) != generation {
        return; // superseded before we even started
    }

    {
        let mut status = engine.status.write().expect("status lock poisoned");
        if let Some(st) = status.get_mut(&r.root) {
            st.state = RootScanState::Scanning;
        }
    }
    emit_status(&app, &engine);

    // Accumulated purely in this thread — the shared index is never touched
    // until the walk finishes (swap-at-end below), so searches keep serving
    // this root's previous/disk-loaded entries for the whole rebuild.
    // Sizing from the previous arena avoids re-growing the buffers on a
    // refresh walk, where the entry count barely changes.
    let previous_len = engine
        .snapshot()
        .iter()
        .find(|ri| ri.root == r.root)
        .map(|ri| ri.arena.len())
        .unwrap_or(0);
    let mut builder = ArenaBuilder::with_capacity(previous_len);
    let mut sink = |path: &str, folder: bool, size: u64, modified: i64| {
        builder.push(path, folder, size, modified);
    };

    let engine_for_progress = engine.clone();
    let root_for_progress = r.root.clone();
    let app_for_progress = app.clone();
    let mut on_progress = move |count: u64| {
        if engine_for_progress.generation.load(AtomicOrdering::SeqCst) != generation {
            return;
        }
        {
            let mut status = engine_for_progress.status.write().expect("status lock poisoned");
            if let Some(st) = status.get_mut(&root_for_progress) {
                st.scanned_count = count;
            }
        }
        emit_status(&app_for_progress, &engine_for_progress);
    };

    let engine_for_pause = engine.clone();
    let is_paused = move || engine_for_pause.is_paused();
    let engine_for_cancel = engine.clone();
    let is_cancelled =
        move || engine_for_cancel.generation.load(AtomicOrdering::SeqCst) != generation;

    let mut skipped = walker::SkippedDirs::default();
    let state = walker::walk_root(
        &r.walk_path,
        r.is_drive,
        &excludes,
        filters,
        &mut sink,
        &mut on_progress,
        &is_paused,
        &is_cancelled,
        &mut skipped,
    );

    // CRITICAL: only swap the index when the walk actually produced a full
    // listing (`Done`). On `Offline`/`SkippedNoAccess` the arena is empty;
    // swapping would wipe this root's previously-loaded (persisted) entries
    // — the exact opposite of the "offline NAS keeps serving" goal. In those
    // cases update status only.
    if state != RootScanState::Done {
        if engine.generation.load(AtomicOrdering::SeqCst) == generation {
            let mut status = engine.status.write().expect("status lock poisoned");
            if let Some(st) = status.get_mut(&r.root) {
                st.state = state;
            }
        }
        emit_status(&app, &engine);
        return;
    }

    let arena = Arc::new(builder.finish());

    // Persist to disk BEFORE publishing into the shared index.
    if let Ok(data_dir) = app.path().app_data_dir() {
        if let Err(e) = persist::save_root(&data_dir, &r.root, &arena) {
            eprintln!("FlexFind: failed to persist index for {}: {e}", r.root);
        }
    }

    // Single atomic swap: re-check generation *under the roots lock* before
    // replacing this root's arena. Checking before acquiring the lock would
    // leave a gap where a concurrent newer-generation writer could interleave.
    let won_race = {
        let mut roots = engine.roots.write().expect("roots lock poisoned");
        if engine.generation.load(AtomicOrdering::SeqCst) != generation {
            false
        } else {
            IndexEngine::put_root(&mut roots, r.root.clone(), arena);
            true
        }
    };
    if !won_race {
        return;
    }

    {
        let mut status = engine.status.write().expect("status lock poisoned");
        if let Some(st) = status.get_mut(&r.root) {
            st.state = state;
            st.loaded_from_disk = false; // now serving a fresh live walk
            st.skipped_dirs = skipped.count;
            st.skipped_samples = skipped.samples.clone();
        }
    }
    if skipped.count > 0 {
        eprintln!(
            "FlexFind: {} skipped {} unreadable subtree(s), e.g. {:?}",
            r.root, skipped.count, skipped.samples
        );
    }
    emit_status(&app, &engine);
}

fn emit_status(app: &AppHandle, engine: &IndexEngine) {
    let _ = app.emit("index://progress", engine.status_snapshot());
}

/// `async` so a linear scan (a cold one especially, where the cost is
/// faulting the name buffer back in) can't block the main thread — this runs
/// once per keystroke, and the main thread also services window ops and the
/// tray.
#[tauri::command]
pub async fn search_index(
    state: tauri::State<'_, Arc<IndexEngine>>,
    query: String,
    limit: Option<usize>,
    include_paths: Option<Vec<String>>,
    sort: Option<SortSpec>,
) -> Result<SearchResponse, String> {
    Ok(state.search(&query, limit.unwrap_or(DEFAULT_LIMIT), include_paths.as_deref(), sort))
}

#[tauri::command]
pub fn get_index_status(state: tauri::State<Arc<IndexEngine>>) -> Vec<RootIndexStatus> {
    state.status_snapshot()
}

#[tauri::command]
pub fn rebuild_index(app: AppHandle, state: tauri::State<Arc<IndexEngine>>) {
    IndexEngine::spawn_scan(state.inner().clone(), app);
}

#[tauri::command]
pub fn set_index_paused(state: tauri::State<Arc<IndexEngine>>, paused: bool) {
    state.set_paused(paused);
}

/// Per-root heap accounting for the index, plus the totals.
#[tauri::command]
pub fn get_index_memory(state: tauri::State<Arc<IndexEngine>>) -> Vec<(String, ArenaMemory)> {
    state
        .snapshot()
        .iter()
        .map(|ri| (ri.root.clone(), ri.arena.memory()))
        .collect()
}

/// Trim this process's working set to zero, so the *next* search pays the
/// full cold-start cost of faulting the index back in.
///
/// This exists because that is the condition users actually hit and the one
/// that is invisible in normal development: FlexFind lives in the tray,
/// Windows reclaims the working set of an idle background process, and the
/// next search has to page the index back from disk. Measuring a search
/// right after using the app measures the warm case and hides the problem
/// entirely. Calling this first makes the cold case reproducible on demand.
///
/// `SetProcessWorkingSetSize` with `(SIZE_T)-1` for both bounds is the
/// documented way to ask for exactly that trim.
#[tauri::command]
pub fn trim_working_set() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        unsafe {
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX)
                .map_err(|e| e.to_string())
        }
    }
    #[cfg(not(windows))]
    {
        Err("unsupported".into())
    }
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<IndexEngine>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seed the engine directly with one arena per root, bypassing the
    /// walker.
    fn engine_with(roots: &[(&str, &[(&str, bool, u64, i64)])]) -> IndexEngine {
        let e = IndexEngine::new();
        {
            let mut w = e.roots.write().unwrap();
            for (root, entries) in roots {
                let mut b = ArenaBuilder::new();
                for (p, folder, size, modified) in *entries {
                    b.push(p, *folder, *size, *modified);
                }
                IndexEngine::put_root(&mut w, (*root).to_string(), Arc::new(b.finish()));
            }
        }
        e
    }

    fn single(entries: &[(&str, bool, u64, i64)]) -> IndexEngine {
        engine_with(&[("C:", entries)])
    }

    /// A folder nested under an enabled drive is normally redundant, but a
    /// reparse point (a cloud-storage mount like `C:\Box`) is stepped over
    /// by the drive's own walk — dropping it here made it unindexable by
    /// any means. This pins the discriminator: plain nested folders are
    /// still deduped.
    #[test]
    fn plain_nested_root_is_deduped() {
        let roots = vec![
            ScanRoot { path: "C:".into(), enabled: true, is_drive: true },
            // A path that does not exist is not link-like, so it stands in
            // for an ordinary nested folder here.
            ScanRoot { path: "C:\\Projects".into(), enabled: true, is_drive: false },
        ];
        let resolved = resolve_roots(&roots);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].root, "C:");
    }

    #[test]
    fn a_disabled_root_is_never_resolved() {
        let roots = vec![ScanRoot { path: "C:".into(), enabled: false, is_drive: true }];
        assert!(resolve_roots(&roots).is_empty());
    }

    #[test]
    fn scope_include_filters_results() {
        let e = engine_with(&[
            ("C:", &[("C:\\Projects\\a.txt", false, 1, 0)][..]),
            ("D:", &[("D:\\Other\\a.txt", false, 1, 0)][..]),
        ]);
        let includes = vec!["c:\\projects".to_string()];
        let res = e.search("a", 500, Some(&includes), None);
        assert_eq!(res.total_matched, 1);
        assert_eq!(res.hits[0].path, "C:\\Projects\\a.txt");
        assert_eq!(res.total_indexed, 2);
    }

    /// The folder that *is* the scope root stays in scope, as it did when
    /// the filter compared whole paths.
    #[test]
    fn scope_includes_the_scope_root_folder_itself() {
        let e = single(&[
            ("C:\\Users\\me\\Downloads", true, 0, 0),
            ("C:\\Users\\me\\Downloads\\a.txt", false, 1, 0),
            ("C:\\Users\\me\\Documents\\b.txt", false, 1, 0),
        ]);
        let includes = vec!["c:\\users\\me\\downloads".to_string()];
        let res = e.search("o", 500, Some(&includes), None);
        let paths: Vec<&str> = res.hits.iter().map(|h| h.path.as_str()).collect();
        assert!(paths.contains(&"C:\\Users\\me\\Downloads"));
        assert!(!paths.iter().any(|p| p.contains("Documents")));
    }

    #[test]
    fn scope_on_another_drive_skips_this_root_entirely() {
        let e = single(&[("C:\\Projects\\a.txt", false, 1, 0)]);
        let includes = vec!["z:\\nowhere".to_string()];
        let res = e.search("a", 500, Some(&includes), None);
        assert_eq!(res.total_matched, 0);
    }

    #[test]
    fn sort_by_size_desc_then_cap_is_global() {
        // 3 matches; cap of 2 must return the 2 largest, not an arbitrary 2.
        let e = single(&[
            ("C:\\a_small.bin", false, 10, 0),
            ("C:\\b_big.bin", false, 9000, 0),
            ("C:\\c_mid.bin", false, 500, 0),
        ]);
        let spec = SortSpec { column: SortColumn::Size, descending: true };
        let res = e.search("bin", 2, None, Some(spec));
        assert_eq!(res.total_matched, 3);
        assert_eq!(res.hits.len(), 2);
        assert_eq!(res.hits[0].size, 9000);
        assert_eq!(res.hits[1].size, 500);
    }

    #[test]
    fn sort_by_name_asc() {
        let e = single(&[("C:\\Zebra.txt", false, 1, 0), ("C:\\alpha.txt", false, 1, 0)]);
        let spec = SortSpec { column: SortColumn::Name, descending: false };
        let res = e.search("txt", 500, None, Some(spec));
        assert_eq!(res.hits[0].name, "alpha.txt"); // case-insensitive: a < z
        assert_eq!(res.hits[1].name, "Zebra.txt");
    }

    #[test]
    fn sort_by_path_orders_subdirectories_like_real_paths() {
        let e = single(&[
            ("C:\\a\\zzz.txt", false, 1, 0),
            ("C:\\a\\b\\x.txt", false, 1, 0),
        ]);
        let spec = SortSpec { column: SortColumn::Path, descending: false };
        let res = e.search("txt", 500, None, Some(spec));
        // "C:\a\b\x.txt" < "C:\a\zzz.txt" byte-wise, even though the first
        // entry's directory is the longer one.
        assert_eq!(res.hits[0].path, "C:\\a\\b\\x.txt");
        assert_eq!(res.hits[1].path, "C:\\a\\zzz.txt");
    }

    /// A zero cap must return nothing on both paths, not everything —
    /// `select_nth_unstable_by(limit - 1, ..)` would underflow.
    #[test]
    fn zero_limit_returns_no_hits_on_either_path() {
        let e = single(&[("C:\\a.txt", false, 1, 0), ("C:\\b.txt", false, 2, 0)]);
        let unsorted = e.search("txt", 0, None, None);
        assert_eq!(unsorted.hits.len(), 0);
        assert_eq!(unsorted.total_matched, 2);
        let spec = SortSpec { column: SortColumn::Size, descending: false };
        let sorted = e.search("txt", 0, None, Some(spec));
        assert_eq!(sorted.hits.len(), 0);
        assert_eq!(sorted.total_matched, 2);
    }

    #[test]
    fn no_sort_preserves_fast_path_count() {
        let e = single(&[("C:\\a.txt", false, 1, 0), ("C:\\b.txt", false, 1, 0)]);
        let res = e.search("txt", 1, None, None);
        assert_eq!(res.total_matched, 2); // full count even with cap 1
        assert_eq!(res.hits.len(), 1);
    }

    #[test]
    fn results_span_multiple_roots() {
        let e = engine_with(&[
            ("C:", &[("C:\\one.txt", false, 1, 0)][..]),
            ("D:", &[("D:\\two.txt", false, 1, 0)][..]),
        ]);
        let res = e.search("txt", 500, None, None);
        assert_eq!(res.total_matched, 2);
        assert_eq!(res.total_indexed, 2);
    }

    /// Highlight spans are handed to JS as UTF-16 offsets, so a multi-byte
    /// prefix must not shift the highlight.
    #[test]
    fn highlight_span_is_converted_to_utf16_units() {
        let e = single(&[("C:\\docs\\レポートreport.docx", false, 1, 0)]);
        let res = e.search("report", 500, None, None);
        assert_eq!(res.hits.len(), 1);
        assert_eq!(res.hits[0].match_start, 4); // 4 UTF-16 units, not 12 bytes
        assert_eq!(res.hits[0].match_len, 6);
    }

    #[test]
    fn blank_query_returns_nothing_but_still_reports_index_size() {
        let e = single(&[("C:\\a.txt", false, 1, 0)]);
        let res = e.search("   ", 500, None, None);
        assert_eq!(res.total_matched, 0);
        assert_eq!(res.total_indexed, 1);
    }
}
