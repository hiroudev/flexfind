//! In-memory index + search engine, held as Tauri managed state
//! (`Arc<IndexEngine>`, see `lib.rs`).
//!
//! Deliberately a flat `Vec<IndexedEntry>` scanned linearly per search,
//! rather than a trie/n-gram structure: substring search over ~500k
//! pre-lowercased short strings is sub-10ms in a release build, which is
//! "feels instant". The real Everything-class speed advantage comes from
//! NTFS-MFT access, which is explicitly out of scope (see
//! project/design-brief.md) — a cleverer in-memory structure over the same
//! walked file list wouldn't close that gap, so it isn't worth the
//! complexity here. If a future profiling pass shows this is too slow at
//! real-world sizes (esp. sort over a broad match set), the documented
//! fallback is a parallel (rayon) scan with a top-N heap instead of a full
//! collect+sort.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::Local;
use tauri::{AppHandle, Emitter, Manager};

use super::pathmatch::path_has_prefix;
use super::persist;
use super::query::{self, ParsedQuery};
use super::types::{
    IndexedEntry, RootIndexStatus, RootScanState, SearchHit, SearchResponse, SortColumn, SortSpec,
};
use super::walker;
use crate::settings::{self, ScanRoot};

/// Cap on hits returned to the frontend per search — keeps IPC payload size
/// and per-keystroke React diffing bounded even though the backing index
/// can hold hundreds of thousands of entries.
const DEFAULT_LIMIT: usize = 500;

/// A resolved scan target: its canonical root string (`"C:"` or full path),
/// the filesystem path to actually walk, and whether it's a drive.
#[derive(Clone)]
struct ResolvedRoot {
    root: String, // canonical, matches status-map key
    walk_path: PathBuf,
    label: String,
    is_drive: bool,
}

pub struct IndexEngine {
    entries: RwLock<Vec<IndexedEntry>>,
    status: RwLock<HashMap<String, RootIndexStatus>>,
    paused: AtomicBool,
    /// Bumped on every (re)scan so a stale scan thread from a superseded
    /// generation stops writing entries/status once a newer one has started.
    generation: AtomicU64,
}

impl IndexEngine {
    pub fn new() -> Self {
        IndexEngine {
            entries: RwLock::new(Vec::new()),
            status: RwLock::new(HashMap::new()),
            paused: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
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

    /// Load persisted per-root indexes at startup, before the first live
    /// rescan, so results are searchable immediately. Roots that load
    /// successfully are marked `loaded_from_disk` + `Done`; the subsequent
    /// `spawn_scan` refreshes them in the background.
    pub fn load_persisted(&self, app: &AppHandle) {
        let Ok(data_dir) = app.path().app_data_dir() else {
            return;
        };
        let s = settings::load_settings(app);
        let roots = resolve_roots(&s.scan_roots);
        let mut loaded_all: Vec<IndexedEntry> = Vec::new();
        let mut statuses: Vec<(String, usize)> = Vec::new();
        for r in &roots {
            if let Some(mut entries) = persist::load_root(&data_dir, &r.root) {
                let n = entries.len();
                loaded_all.append(&mut entries);
                statuses.push((r.root.clone(), n));
            }
        }
        if loaded_all.is_empty() {
            return;
        }
        {
            let mut e = self.entries.write().expect("entries lock poisoned");
            *e = loaded_all;
        }
        {
            let mut st = self.status.write().expect("status lock poisoned");
            for r in &roots {
                let count = statuses
                    .iter()
                    .find(|(root, _)| root == &r.root)
                    .map(|(_, n)| *n as u64);
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
        let parsed: ParsedQuery = query::parse_query(raw_query);
        let entries = self.entries.read().expect("entries lock poisoned");
        let total_indexed = entries.len() as u64;
        if parsed.is_empty() {
            // Blank/whitespace query — an arbitrary slice of the whole index
            // isn't useful. The frontend also short-circuits before calling
            // this; belt-and-braces for any other caller.
            return SearchResponse { hits: Vec::new(), total_matched: 0, total_indexed };
        }

        // Scope pre-filter: lowered once, boundary-aware prefix.
        let includes_lower: Option<Vec<String>> = include_paths.map(|ps| {
            ps.iter()
                .map(|p| p.trim_end_matches(['\\', '/']).to_lowercase())
                .filter(|p| !p.is_empty())
                .collect()
        });
        let in_scope = |e: &IndexedEntry| -> bool {
            match &includes_lower {
                None => true,
                Some(list) if list.is_empty() => true,
                Some(list) => {
                    let pl = e.path.to_lowercase();
                    list.iter().any(|inc| path_has_prefix(&pl, inc))
                }
            }
        };

        let now = Local::now();

        match sort {
            None => {
                // Fast path (v1 behavior): early cap, full count, no collect.
                let mut hits: Vec<SearchHit> = Vec::new();
                let mut total_matched: u64 = 0;
                for e in entries.iter() {
                    if !in_scope(e) {
                        continue;
                    }
                    if let Some(span) = query::matches(e, &parsed, now) {
                        total_matched += 1;
                        if hits.len() < limit {
                            hits.push(build_hit(e, span));
                        }
                    }
                }
                SearchResponse { hits, total_matched, total_indexed }
            }
            Some(spec) => {
                // Sorted path: collect all matches (ref + span), sort the
                // full set, THEN cap — sorting only the first `limit` would
                // sort an arbitrary subset. ~24B/match, ~12MB at 500k: fine.
                let mut matched: Vec<(&IndexedEntry, (usize, usize))> = Vec::new();
                for e in entries.iter() {
                    if !in_scope(e) {
                        continue;
                    }
                    if let Some(span) = query::matches(e, &parsed, now) {
                        matched.push((e, span));
                    }
                }
                let total_matched = matched.len() as u64;
                sort_matches(&mut matched, spec);
                matched.truncate(limit);
                let hits = matched.into_iter().map(|(e, span)| build_hit(e, span)).collect();
                SearchResponse { hits, total_matched, total_indexed }
            }
        }
    }

    /// (Re)scan every enabled root on its own background OS thread. Disabled
    /// / removed roots have their entries dropped up front; other roots keep
    /// serving their previously-indexed (or disk-loaded) entries while this
    /// generation's scan is in flight, so searches never see the index go
    /// blank — and an offline root keeps its last-known entries entirely.
    pub fn spawn_scan(engine: Arc<IndexEngine>, app: AppHandle) {
        let my_generation = engine.generation.fetch_add(1, Ordering::SeqCst) + 1;
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
                let loaded_from_disk = prev.get(&r.root).map(|p| p.loaded_from_disk).unwrap_or(false);
                status.insert(
                    r.root.clone(),
                    RootIndexStatus {
                        root: r.root.clone(),
                        label: r.label.clone(),
                        state: RootScanState::Pending,
                        scanned_count: prev.get(&r.root).map(|p| p.scanned_count).unwrap_or(0),
                        is_drive: r.is_drive,
                        loaded_from_disk,
                    },
                );
            }
        }
        // Drop entries whose path is under no currently-enabled root.
        {
            let mut entries = engine.entries.write().expect("entries lock poisoned");
            entries.retain(|e| {
                let pl = e.path.to_lowercase();
                enabled_lower.iter().any(|root| path_has_prefix(&pl, root))
            });
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
        // Skip if nested under a *different* enabled root.
        let nested = enabled.iter().any(|other| {
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

fn build_hit(e: &IndexedEntry, span: (usize, usize)) -> SearchHit {
    let (match_start, match_len) = span;
    // `query::matches` returns a *byte* offset/length into `name_lower`
    // (Rust string indexing is UTF-8 bytes), but the frontend slices `name`
    // with JS string indices (UTF-16 code units) — convert so Japanese (and
    // any other multi-byte) filenames highlight correctly.
    let (match_start, match_len) = if match_len > 0 {
        let utf16_start = e.name_lower[..match_start].encode_utf16().count();
        let utf16_len = e.name_lower[match_start..match_start + match_len]
            .encode_utf16()
            .count();
        (utf16_start, utf16_len)
    } else {
        (0, 0)
    };
    SearchHit {
        name: e.name.clone(),
        path: e.path.clone(),
        dir: e.dir.clone(),
        folder: e.folder,
        ext: e.ext.clone(),
        size: e.size,
        modified: e.modified,
        match_start,
        match_len,
    }
}

fn sort_matches(matched: &mut [(&IndexedEntry, (usize, usize))], spec: SortSpec) {
    // Name compares pre-lowered `name_lower`; Path compares raw `path` bytes
    // (case-sensitively) to avoid a per-comparison `to_lowercase` allocation.
    // Path is the tie-break for every column so ordering is deterministic.
    matched.sort_unstable_by(|(a, _), (b, _)| {
        let ord = match spec.column {
            SortColumn::Name => a.name_lower.cmp(&b.name_lower).then_with(|| a.path.cmp(&b.path)),
            SortColumn::Path => a.path.cmp(&b.path),
            SortColumn::Size => a.size.cmp(&b.size).then_with(|| a.path.cmp(&b.path)),
            SortColumn::Modified => a.modified.cmp(&b.modified).then_with(|| a.path.cmp(&b.path)),
        };
        if spec.descending {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn scan_one_root(
    engine: Arc<IndexEngine>,
    app: AppHandle,
    r: ResolvedRoot,
    excludes: Vec<String>,
    filters: walker::FilterOpts,
    generation: u64,
) {
    if engine.generation.load(Ordering::SeqCst) != generation {
        return; // superseded before we even started
    }

    {
        let mut status = engine.status.write().expect("status lock poisoned");
        if let Some(st) = status.get_mut(&r.root) {
            st.state = RootScanState::Scanning;
        }
    }
    emit_status(&app, &engine);

    let root_lower = r.root.to_lowercase();

    // Accumulated purely in this thread — the shared index is never touched
    // until the walk finishes (swap-at-end below), so searches keep serving
    // this root's previous/disk-loaded entries for the whole rebuild.
    let mut local_batch: Vec<IndexedEntry> = Vec::with_capacity(4096);
    let mut sink = |entry: IndexedEntry| local_batch.push(entry);

    let engine_for_progress = engine.clone();
    let root_for_progress = r.root.clone();
    let app_for_progress = app.clone();
    let mut on_progress = move |count: u64| {
        if engine_for_progress.generation.load(Ordering::SeqCst) != generation {
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
    let is_cancelled = move || engine_for_cancel.generation.load(Ordering::SeqCst) != generation;

    let state = walker::walk_root(
        &r.walk_path,
        r.is_drive,
        &excludes,
        filters,
        &mut sink,
        &mut on_progress,
        &is_paused,
        &is_cancelled,
    );

    // CRITICAL: only swap the index when the walk actually produced a full
    // listing (`Done`). On `Offline`/`SkippedNoAccess` the batch is empty;
    // swapping would wipe this root's previously-loaded (persisted) entries
    // — the exact opposite of the "offline NAS keeps serving" goal. In those
    // cases update status only.
    if state != RootScanState::Done {
        if engine.generation.load(Ordering::SeqCst) == generation {
            let mut status = engine.status.write().expect("status lock poisoned");
            if let Some(st) = status.get_mut(&r.root) {
                st.state = state;
            }
        }
        emit_status(&app, &engine);
        return;
    }

    // Persist to disk BEFORE moving `local_batch` into the shared index.
    if let Ok(data_dir) = app.path().app_data_dir() {
        if let Err(e) = persist::save_root(&data_dir, &r.root, &local_batch) {
            eprintln!("FlexFind: failed to persist index for {}: {e}", r.root);
        }
    }

    // Single atomic swap: re-check generation *under the entries lock* before
    // replacing this root's slice. Checking before acquiring the lock would
    // leave a gap where a concurrent newer-generation writer could interleave.
    let won_race = {
        let mut entries = engine.entries.write().expect("entries lock poisoned");
        if engine.generation.load(Ordering::SeqCst) != generation {
            false
        } else {
            entries.retain(|e| !path_has_prefix(&e.path.to_lowercase(), &root_lower));
            entries.append(&mut local_batch);
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
        }
    }
    emit_status(&app, &engine);
}

fn emit_status(app: &AppHandle, engine: &IndexEngine) {
    let _ = app.emit("index://progress", engine.status_snapshot());
}

/// `async` so a linear scan (sorted broad match sets especially) can't block
/// the main thread — this runs once per keystroke, and the main thread also
/// services window ops and the tray.
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

#[allow(dead_code)]
fn _assert_send_sync() {
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<IndexEngine>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::pathmatch::entry_from_path;

    fn engine_with(paths: &[(&str, bool, u64, i64)]) -> IndexEngine {
        let e = IndexEngine::new();
        {
            let mut ent = e.entries.write().unwrap();
            for (p, folder, size, modified) in paths {
                ent.push(entry_from_path(p.to_string(), *folder, *size, *modified));
            }
        }
        e
    }

    #[test]
    fn scope_include_filters_results() {
        let e = engine_with(&[
            ("C:\\Projects\\a.txt", false, 1, 0),
            ("D:\\Other\\a.txt", false, 1, 0),
        ]);
        let includes = vec!["C:\\Projects".to_string()];
        let res = e.search("a", 500, Some(&includes), None);
        assert_eq!(res.total_matched, 1);
        assert_eq!(res.hits[0].path, "C:\\Projects\\a.txt");
    }

    #[test]
    fn sort_by_size_desc_then_cap_is_global() {
        // 3 matches; cap of 2 must return the 2 largest, not an arbitrary 2.
        let e = engine_with(&[
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
        let e = engine_with(&[
            ("C:\\Zebra.txt", false, 1, 0),
            ("C:\\alpha.txt", false, 1, 0),
        ]);
        let spec = SortSpec { column: SortColumn::Name, descending: false };
        let res = e.search("txt", 500, None, Some(spec));
        assert_eq!(res.hits[0].name, "alpha.txt"); // case-insensitive: a < z
        assert_eq!(res.hits[1].name, "Zebra.txt");
    }

    #[test]
    fn no_sort_preserves_fast_path_count() {
        let e = engine_with(&[
            ("C:\\a.txt", false, 1, 0),
            ("C:\\b.txt", false, 1, 0),
        ]);
        let res = e.search("txt", 1, None, None);
        assert_eq!(res.total_matched, 2); // full count even with cap 1
        assert_eq!(res.hits.len(), 1);
    }
}
