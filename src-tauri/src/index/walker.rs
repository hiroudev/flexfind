//! Per-drive recursive directory walk used to build the in-memory index.
//!
//! Deliberately NOT NTFS-MFT-based (see project/design-brief.md's scope
//! notes) — this is a plain filesystem walk (`walkdir`), one OS thread per
//! enabled drive (see `index::engine::spawn_scan`). `filter_entry` prunes
//! excluded subtrees *before* descending into them, which matters for perf
//! on things like `node_modules`.

use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

use super::pathmatch::path_has_prefix;
use super::types::RootScanState;

/// How many entries between progress-counter updates / pause checkpoints.
const PROGRESS_INTERVAL: u64 = 2000;

/// Returns true if this entry (by its full lowercased path or bare name)
/// should be pruned from the walk. `excludes_lower` entries are either
/// absolute path prefixes (matched via boundary-aware prefix, e.g.
/// `c:\windows`) or bare folder-name patterns (matched via exact name
/// equality, e.g. `node_modules`) — distinguished by whether the pattern
/// looks path-like.
fn is_excluded(path_lower: &str, name_lower: &str, excludes_lower: &[String]) -> bool {
    excludes_lower.iter().any(|ex| {
        if ex.contains(':') || ex.contains('\\') || ex.contains('/') {
            path_has_prefix(path_lower, ex.as_str())
        } else {
            name_lower == ex.as_str()
        }
    })
}

/// Per-entry attribute/extension filters, sourced from `Settings` — an
/// Everything-style "hide these kinds of results" set, distinct from
/// `exclude_paths` (which prunes whole subtrees by location). Checked per
/// entry rather than as a `filter_entry` subtree prune: a hidden/system
/// *folder* can still contain ordinary files a user does want to find (e.g.
/// some sync-client working folders are marked hidden), so only the
/// individual matching entries are dropped, not the whole branch under them.
#[derive(Clone, Copy, Default)]
pub struct FilterOpts {
    pub exclude_hidden: bool,
    pub exclude_system: bool,
    pub exclude_shortcuts: bool,
}

/// Windows `FILE_ATTRIBUTE_HIDDEN` / `FILE_ATTRIBUTE_SYSTEM` bits, read
/// straight off `Metadata` (no extra stat call — the caller already has the
/// metadata for size/modified). No-op `(false, false)` on non-Windows so
/// this module still compiles there.
#[cfg(windows)]
fn hidden_system_flags(m: &std::fs::Metadata) -> (bool, bool) {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attrs = m.file_attributes();
    (attrs & FILE_ATTRIBUTE_HIDDEN != 0, attrs & FILE_ATTRIBUTE_SYSTEM != 0)
}
#[cfg(not(windows))]
fn hidden_system_flags(_m: &std::fs::Metadata) -> (bool, bool) {
    (false, false)
}

/// Case-insensitive `.lnk` test done on raw bytes, so the shortcut filter
/// doesn't allocate a lowercased copy of every path in the walk.
fn ends_with_lnk(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 4 && b[b.len() - 4..].eq_ignore_ascii_case(b".lnk")
}

/// Subtrees the walk could not read and therefore skipped entirely.
///
/// These were previously swallowed without a trace, which made "is this
/// folder actually being indexed?" unanswerable — a whole cloud-storage or
/// permission-protected tree could be missing from every search with no
/// indication anywhere in the UI. The count feeds the settings screen; the
/// sample paths make it obvious *which* tree is missing.
#[derive(Default, Clone)]
pub struct SkippedDirs {
    pub count: u64,
    pub samples: Vec<String>,
    /// The OS error from opening the root itself, when that is what failed.
    ///
    /// Without this the UI could only say "skipped", which does not
    /// distinguish the cases that need completely different fixes: access
    /// denied (often because the app is running elevated, and per-user
    /// cloud mounts such as Box are invisible to an elevated token), the
    /// path not existing yet (a cloud client that has not mounted), or an
    /// unreachable network share.
    pub root_error: Option<String>,
}

/// How many example paths to keep. Enough to identify the culprit, few
/// enough that a drive full of protected system folders can't grow this
/// without bound.
const MAX_SKIPPED_SAMPLES: usize = 5;

impl SkippedDirs {
    fn record(&mut self, path: Option<&Path>) {
        self.count += 1;
        if self.samples.len() < MAX_SKIPPED_SAMPLES {
            if let Some(p) = path {
                self.samples.push(p.to_string_lossy().to_string());
            }
        }
    }
}

/// Walk one drive/root path, respecting `excludes`, pushing entries into
/// `sink` and invoking `on_progress(scanned_count)` periodically so the
/// frontend counter can update without a per-file event flood. `is_paused`
/// is polled at the same cadence as `on_progress`; while it returns true the
/// walk blocks in place (cooperative pause/resume, no lost progress).
/// `is_cancelled` is polled every iteration (a cheap atomic load, far
/// cheaper than the filesystem work per entry) and inside the pause loop,
/// so a walk superseded by a newer rebuild — including one that got
/// superseded while paused — stops promptly instead of continuing to do
/// wasted work in the background.
///
/// Permission-denied errors on individual subdirectories are swallowed and
/// simply skipped rather than aborting the whole root walk — this is the
/// "some folders skipped" simplified-index behavior for unelevated runs.
/// Only the root itself being inaccessible produces a non-`Done` state:
/// `SkippedNoAccess` for a drive (implies permissions/elevation), `Offline`
/// for a non-drive root (custom folder / UNC file server that's unreachable
/// — its previously-persisted entries keep serving; see `is_drive`).
pub fn walk_root(
    root: &Path,
    is_drive: bool,
    excludes: &[String],
    filters: FilterOpts,
    sink: &mut dyn FnMut(&str, bool, u64, i64),
    on_progress: &mut dyn FnMut(u64),
    is_paused: &dyn Fn() -> bool,
    is_cancelled: &dyn Fn() -> bool,
    skipped: &mut SkippedDirs,
) -> RootScanState {
    if let Err(e) = std::fs::read_dir(root) {
        skipped.root_error = Some(e.to_string());
        return if is_drive {
            RootScanState::SkippedNoAccess
        } else {
            RootScanState::Offline
        };
    }

    let excludes_lower: Vec<String> = excludes
        .iter()
        .map(|e| e.trim().to_lowercase())
        .filter(|e| !e.is_empty())
        .collect();

    let mut count: u64 = 0;

    let walker = WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| {
            let name_lower = entry.file_name().to_string_lossy().to_lowercase();
            let path_lower = entry.path().to_string_lossy().to_lowercase();
            !is_excluded(&path_lower, &name_lower, &excludes_lower)
        });

    for entry_result in walker {
        if is_cancelled() {
            return RootScanState::Done; // caller discards our output on a generation mismatch anyway
        }
        let entry = match entry_result {
            Ok(e) => e,
            // Access-denied or similar on a subdirectory: the walk carries
            // on, but the whole subtree below it is missing from the index,
            // so record it rather than losing the fact.
            Err(e) => {
                skipped.record(e.path());
                continue;
            }
        };
        let path = entry.path();
        let folder = entry.file_type().is_dir();
        // Borrowed for well-formed UTF-8 paths (the overwhelming majority),
        // so this is allocation-free per entry in the common case.
        let path_str = path.to_string_lossy();
        let mut hidden = false;
        let mut system = false;
        let (size, modified) = match entry.metadata() {
            Ok(m) => {
                let size = if folder { 0 } else { m.len() };
                let modified = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                (hidden, system) = hidden_system_flags(&m);
                (size, modified)
            }
            Err(_) => (0, 0),
        };

        if (filters.exclude_hidden && hidden) || (filters.exclude_system && system) {
            continue;
        }
        if filters.exclude_shortcuts && !folder && ends_with_lnk(&path_str) {
            continue;
        }

        sink(&path_str, folder, size, modified);
        count += 1;
        if count % PROGRESS_INTERVAL == 0 {
            on_progress(count);
            while is_paused() && !is_cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            if is_cancelled() {
                return RootScanState::Done;
            }
        }
    }

    on_progress(count);
    RootScanState::Done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_bare_folder_name() {
        let excludes = vec!["node_modules".to_string()];
        assert!(is_excluded(
            "c:\\projects\\app\\node_modules",
            "node_modules",
            &excludes
        ));
        assert!(!is_excluded("c:\\projects\\app\\src", "src", &excludes));
    }

    #[test]
    fn excludes_absolute_path_prefix() {
        let excludes = vec!["c:\\windows".to_string()];
        assert!(is_excluded("c:\\windows\\system32", "system32", &excludes));
        assert!(!is_excluded("c:\\users\\hiro", "hiro", &excludes));
    }

    #[test]
    fn path_prefix_exclusion_respects_separator_boundary() {
        let excludes = vec!["c:\\users\\hiro".to_string()];
        assert!(is_excluded("c:\\users\\hiro", "hiro", &excludes));
        assert!(is_excluded("c:\\users\\hiro\\docs", "docs", &excludes));
        assert!(!is_excluded("c:\\users\\hiroko", "hiroko", &excludes));
        assert!(!is_excluded("c:\\users\\hiroko\\docs", "docs", &excludes));
    }
}
