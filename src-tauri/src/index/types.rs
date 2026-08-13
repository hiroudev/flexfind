//! DTOs shared across the index walker, search engine, and the Tauri
//! commands exposed to the frontend, plus `IndexArena` — the packed
//! in-memory index itself.
//!
//! # Why the index is an arena and not `Vec<Entry>`
//!
//! The previous layout was one struct per entry holding five separate
//! `String`s (name / name_lower / path / dir / ext). At a measured 1.27M
//! entries on a single C: drive that came to ~500 bytes and ~5 heap
//! allocations per entry — ~640 MB resident, ~6.3M scattered allocations.
//!
//! That size is what made searching slow, and not because the scan itself
//! was expensive. FlexFind is tray-resident, so Windows trims its working
//! set while it sits idle; a search then had to fault ~640 MB back in from
//! the pagefile, chasing a pointer per entry. Observed on a large index:
//! ~2 MB resident at rest, ~1 GB after one search, ten seconds to return.
//!
//! So the design goal here is not "scan faster", it is **touch fewer
//! pages per search**:
//!
//! * Names live in one contiguous buffer, addressed by an offsets array.
//!   A plain term query reads only `names_lower` + `name_lower_off`
//!   (~48 MB of the ~160 MB total) and reads them sequentially, which
//!   prefetches well and pages in cheaply even when cold.
//! * Everything else is a separate column (structure-of-arrays), so a
//!   filter that isn't used costs nothing — its column is never touched.
//! * Parent directories are interned. Measured 231,850 distinct
//!   directories across 1,269,687 entries (5.5x duplication), so storing a
//!   `dir` string per entry wasted ~90 MB. Interning also turns
//!   scope/path filtering into work-per-directory instead of
//!   work-per-entry.
//! * Full paths are not stored at all. They are rebuilt from
//!   `dir + '\' + name` for the ≤500 hits actually returned.

use serde::{Deserialize, Serialize};

/// Interned string table: every string concatenated into one buffer, with
/// `offsets[i]..offsets[i + 1]` delimiting string `i` (so `offsets` always
/// has one more element than there are strings).
#[derive(Serialize, Deserialize, Clone)]
pub struct StrTable {
    text: String,
    offsets: Vec<u32>,
}

impl Default for StrTable {
    fn default() -> Self {
        StrTable { text: String::new(), offsets: vec![0] }
    }
}

impl StrTable {
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    #[inline]
    pub fn get(&self, i: usize) -> &str {
        &self.text[self.offsets[i] as usize..self.offsets[i + 1] as usize]
    }

    fn push(&mut self, s: &str) -> u32 {
        let id = self.len() as u32;
        self.text.push_str(s);
        self.offsets.push(self.text.len() as u32);
        id
    }

    fn heap_bytes(&self) -> u64 {
        self.text.len() as u64 + (self.offsets.len() * 4) as u64
    }

    /// True when the offsets array is self-consistent and in bounds — used
    /// to reject a corrupt/truncated persisted index rather than letting a
    /// bad offset panic mid-search.
    fn is_valid(&self) -> bool {
        if self.offsets.first() != Some(&0) {
            return false;
        }
        if self.offsets.last() != Some(&(self.text.len() as u32)) {
            return false;
        }
        self.offsets.windows(2).all(|w| w[0] <= w[1])
            && self.offsets.iter().all(|&o| self.text.is_char_boundary(o as usize))
    }
}

/// Heap accounting for one arena, surfaced to the settings UI so the
/// index's real cost is observable.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArenaMemory {
    pub entries: u64,
    pub dirs: u64,
    pub exts: u64,
    pub names_lower_bytes: u64,
    pub names_bytes: u64,
    pub dir_bytes: u64,
    pub ext_bytes: u64,
    pub column_bytes: u64,
    pub total_bytes: u64,
    /// The buffers a plain term query actually reads. This — not
    /// `total_bytes` — is what governs cold-search latency, because it is
    /// what has to be faulted back in after Windows trims the working set.
    pub scanned_bytes: u64,
}

/// The packed index for one scan root.
///
/// Offsets are `u32`, which caps a single root's name/dir text at 4 GiB —
/// roughly a hundred million files, far beyond what this walker-based
/// design targets.
#[derive(Serialize, Deserialize, Clone)]
pub struct IndexArena {
    // ---- hot: what a bare-term search reads, and nothing else ----
    /// All lowercased names concatenated.
    names_lower: String,
    /// `len = count + 1`; entry `i`'s lowercased name is
    /// `names_lower[name_lower_off[i]..name_lower_off[i + 1]]`.
    name_lower_off: Vec<u32>,

    // ---- cold: read only for the ≤500 hits actually returned ----
    /// All original-case names concatenated. Kept separate from
    /// `names_lower` rather than sharing offsets because Unicode
    /// lowercasing is not length-preserving (e.g. `İ`), and correctness
    /// there is worth 5 MB.
    names: String,
    name_off: Vec<u32>,

    /// Interned parent directories, original case.
    dirs: StrTable,
    /// The same directories lowercased, so scope / `path:` / negation
    /// prefix tests run once per directory instead of once per entry, with
    /// no per-entry allocation.
    dirs_lower: StrTable,
    /// Interned lowercase extensions (no dot). Measured 2,318 distinct on
    /// a full C: drive, so an id compare replaces a string compare.
    exts: StrTable,

    // ---- per-entry columns, each `len = count` ----
    dir_id: Vec<u32>,
    ext_id: Vec<u32>,
    size: Vec<u64>,
    modified: Vec<i64>,
    folder: Vec<bool>,
}

impl Default for IndexArena {
    /// An empty arena still has to satisfy the `offsets.len() == count + 1`
    /// invariant, so the offset arrays start at `[0]` rather than empty.
    fn default() -> Self {
        IndexArena {
            names_lower: String::new(),
            name_lower_off: vec![0],
            names: String::new(),
            name_off: vec![0],
            dirs: StrTable::default(),
            dirs_lower: StrTable::default(),
            exts: StrTable::default(),
            dir_id: Vec::new(),
            ext_id: Vec::new(),
            size: Vec::new(),
            modified: Vec::new(),
            folder: Vec::new(),
        }
    }
}

impl IndexArena {
    #[inline]
    pub fn len(&self) -> usize {
        self.dir_id.len()
    }

    #[inline]
    pub fn name_lower(&self, i: usize) -> &str {
        &self.names_lower[self.name_lower_off[i] as usize..self.name_lower_off[i + 1] as usize]
    }

    #[inline]
    pub fn name(&self, i: usize) -> &str {
        &self.names[self.name_off[i] as usize..self.name_off[i + 1] as usize]
    }

    #[inline]
    pub fn dir_id(&self, i: usize) -> u32 {
        self.dir_id[i]
    }

    #[inline]
    pub fn dir(&self, i: usize) -> &str {
        self.dirs.get(self.dir_id[i] as usize)
    }

    #[inline]
    pub fn ext(&self, i: usize) -> &str {
        self.exts.get(self.ext_id[i] as usize)
    }

    #[inline]
    pub fn ext_id(&self, i: usize) -> u32 {
        self.ext_id[i]
    }

    #[inline]
    pub fn size(&self, i: usize) -> u64 {
        self.size[i]
    }

    #[inline]
    pub fn modified(&self, i: usize) -> i64 {
        self.modified[i]
    }

    #[inline]
    pub fn folder(&self, i: usize) -> bool {
        self.folder[i]
    }

    pub fn dir_count(&self) -> usize {
        self.dirs_lower.len()
    }

    #[inline]
    pub fn dir_lower_by_id(&self, id: u32) -> &str {
        self.dirs_lower.get(id as usize)
    }

    /// Resolve a lowercase extension to its interned id, for turning an
    /// `ext:` filter into an integer compare in the scan. `None` when no
    /// indexed entry has that extension (so the query matches nothing).
    pub fn find_ext_id(&self, ext_lower: &str) -> Option<u32> {
        (0..self.exts.len()).find(|&i| self.exts.get(i) == ext_lower).map(|i| i as u32)
    }

    /// Rebuild entry `i`'s full path. Only called for returned hits — the
    /// whole point of interning directories is that the scan never needs
    /// this.
    ///
    /// Joins with `\` because that is what `walkdir` yields on Windows for
    /// everything below a root; the separator inside the root portion is
    /// preserved verbatim in `dir`.
    pub fn full_path(&self, i: usize) -> String {
        let dir = self.dir(i);
        let name = self.name(i);
        if dir.is_empty() {
            return name.to_string();
        }
        let mut s = String::with_capacity(dir.len() + 1 + name.len());
        s.push_str(dir);
        s.push('\\');
        s.push_str(name);
        s
    }

    /// Lowercased full path for entry `i`. Used only by the path-based
    /// predicates (`path:`, negations that reach past the name), which run
    /// on entries that already survived the cheap filters.
    pub fn full_path_lower(&self, i: usize) -> String {
        let dir = self.dir_lower_by_id(self.dir_id[i]);
        let name = self.name_lower(i);
        if dir.is_empty() {
            return name.to_string();
        }
        let mut s = String::with_capacity(dir.len() + 1 + name.len());
        s.push_str(dir);
        s.push('\\');
        s.push_str(name);
        s
    }

    /// Exact heap accounting for this arena, so the memory the index
    /// actually costs can be read out of the running app rather than
    /// inferred from Task Manager (whose working-set figure for a
    /// tray-resident process reflects OS trimming more than it reflects
    /// the index).
    pub fn memory(&self) -> ArenaMemory {
        let n = self.len() as u64;
        let names_lower_bytes =
            self.names_lower.len() as u64 + (self.name_lower_off.len() * 4) as u64;
        let names_bytes = self.names.len() as u64 + (self.name_off.len() * 4) as u64;
        let dir_bytes = self.dirs.heap_bytes() + self.dirs_lower.heap_bytes();
        let ext_bytes = self.exts.heap_bytes();
        // dir_id(4) + ext_id(4) + size(8) + modified(8) + folder(1)
        let column_bytes = n * 25;
        ArenaMemory {
            entries: n,
            dirs: self.dir_count() as u64,
            exts: self.exts.len() as u64,
            names_lower_bytes,
            names_bytes,
            dir_bytes,
            ext_bytes,
            column_bytes,
            total_bytes: names_lower_bytes + names_bytes + dir_bytes + ext_bytes + column_bytes,
            scanned_bytes: names_lower_bytes,
        }
    }

    /// Structural check applied to a freshly deserialized arena. A
    /// truncated or mismatched `.idx` would otherwise surface as an
    /// out-of-bounds panic on the first search.
    pub fn is_valid(&self) -> bool {
        let n = self.len();
        if self.name_lower_off.len() != n + 1 || self.name_off.len() != n + 1 {
            return false;
        }
        if self.ext_id.len() != n
            || self.size.len() != n
            || self.modified.len() != n
            || self.folder.len() != n
        {
            return false;
        }
        if self.dirs.len() != self.dirs_lower.len() {
            return false;
        }
        if self.dir_id.iter().any(|&d| d as usize >= self.dirs.len()) {
            return false;
        }
        if self.ext_id.iter().any(|&e| e as usize >= self.exts.len()) {
            return false;
        }
        let offsets_ok = |text: &str, offs: &[u32]| {
            offs.first() == Some(&0)
                && offs.last() == Some(&(text.len() as u32))
                && offs.windows(2).all(|w| w[0] <= w[1])
                && offs.iter().all(|&o| text.is_char_boundary(o as usize))
        };
        offsets_ok(&self.names_lower, &self.name_lower_off)
            && offsets_ok(&self.names, &self.name_off)
            && self.dirs.is_valid()
            && self.dirs_lower.is_valid()
            && self.exts.is_valid()
    }
}

/// Accumulates an `IndexArena` during a walk. Kept separate from the arena
/// so the interning hash maps — which are large and useless afterwards —
/// are dropped once the walk finishes.
pub struct ArenaBuilder {
    arena: IndexArena,
    dir_ids: std::collections::HashMap<String, u32>,
    ext_ids: std::collections::HashMap<String, u32>,
    /// `walkdir` yields depth-first, so consecutive entries almost always
    /// share a parent. Caching the last one skips the hash lookup for the
    /// large majority of pushes.
    last_dir: Option<(String, u32)>,
}

impl Default for ArenaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ArenaBuilder {
    pub fn new() -> Self {
        ArenaBuilder {
            arena: IndexArena::default(),
            dir_ids: std::collections::HashMap::new(),
            ext_ids: std::collections::HashMap::new(),
            last_dir: None,
        }
    }

    pub fn with_capacity(entries: usize) -> Self {
        let mut b = Self::new();
        b.arena.name_lower_off.reserve(entries + 1);
        b.arena.name_off.reserve(entries + 1);
        b.arena.dir_id.reserve(entries);
        b.arena.ext_id.reserve(entries);
        b.arena.size.reserve(entries);
        b.arena.modified.reserve(entries);
        b.arena.folder.reserve(entries);
        b
    }

    /// Append one entry, deriving name / parent directory / extension from
    /// `path`.
    ///
    /// The derivation is deliberately identical to what the pre-arena
    /// `pathmatch::entry_from_path` did, so query semantics (notably: a
    /// leading dot is not an extension, and folders have no extension) do
    /// not shift underneath the existing query tests.
    pub fn push(&mut self, path: &str, folder: bool, size: u64, modified: i64) {
        let sep = path.rfind(['\\', '/']);
        let (dir, name) = match sep {
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("", path),
        };

        let dir_id = match &self.last_dir {
            Some((d, id)) if d == dir => *id,
            _ => {
                let id = match self.dir_ids.get(dir) {
                    Some(id) => *id,
                    None => {
                        let id = self.arena.dirs.push(dir);
                        self.arena.dirs_lower.push(&dir.to_lowercase());
                        self.dir_ids.insert(dir.to_string(), id);
                        id
                    }
                };
                self.last_dir = Some((dir.to_string(), id));
                id
            }
        };

        let ext_owned = if folder {
            String::new()
        } else {
            match name.rfind('.') {
                Some(i) if i > 0 => name[i + 1..].to_lowercase(),
                _ => String::new(),
            }
        };
        let ext_id = match self.ext_ids.get(&ext_owned) {
            Some(id) => *id,
            None => {
                let id = self.arena.exts.push(&ext_owned);
                self.ext_ids.insert(ext_owned, id);
                id
            }
        };

        self.arena.names.push_str(name);
        self.arena.name_off.push(self.arena.names.len() as u32);
        // `to_lowercase` allocates, but only transiently and only during a
        // walk — never on the search path.
        self.arena.names_lower.push_str(&name.to_lowercase());
        self.arena.name_lower_off.push(self.arena.names_lower.len() as u32);

        self.arena.dir_id.push(dir_id);
        self.arena.ext_id.push(ext_id);
        self.arena.size.push(size);
        self.arena.modified.push(modified);
        self.arena.folder.push(folder);
    }

    pub fn finish(self) -> IndexArena {
        self.arena
    }
}

/// Per-root scan lifecycle state.
#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum RootScanState {
    Pending,
    Scanning,
    Done,
    /// A drive root that couldn't be opened at all (e.g. needs elevation).
    SkippedNoAccess,
    /// A non-drive root (custom folder / UNC file server) that couldn't be
    /// reached this session. Distinct from `SkippedNoAccess` so the UI can
    /// say "offline — searchable from the last index" rather than implying a
    /// permission problem; its previously-persisted entries keep serving.
    Offline,
}

/// Progress/status snapshot for one scan root, sent to the frontend.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootIndexStatus {
    /// Root path in its canonical form: "C:" for drives, or the full folder /
    /// UNC path for custom roots.
    pub root: String,
    /// Volume label for drives, last path component for custom roots.
    pub label: String,
    pub state: RootScanState,
    /// Running count while `state == Scanning`; final count once `Done`.
    pub scanned_count: u64,
    pub is_drive: bool,
    /// True when this session is serving entries loaded from the persisted
    /// on-disk index for this root (so the UI can suppress the big
    /// first-build panel even before/without a live re-walk finishing).
    pub loaded_from_disk: bool,
    /// Subtrees the last walk couldn't read and skipped whole. Non-zero
    /// means results are silently incomplete, which is worth surfacing.
    pub skipped_dirs: u64,
    /// A few example paths from `skipped_dirs`, to identify what's missing.
    pub skipped_samples: Vec<String>,
}

/// Which column a search result set is sorted by.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    Name,
    Path,
    Size,
    Modified,
}

#[derive(Clone, Copy, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub column: SortColumn,
    pub descending: bool,
}

/// One search result row, with the highlight span for the matched filename.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub name: String,
    pub path: String,
    pub dir: String,
    pub folder: bool,
    pub ext: String,
    pub size: u64,
    pub modified: i64,
    /// Byte offset into `name` where the first bare-term match begins.
    /// `0` with `match_len: 0` when the query has no bare terms to highlight
    /// (e.g. an `ext:`/`path:`-only query).
    pub match_start: usize,
    pub match_len: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    /// Capped at the caller's `limit`.
    pub hits: Vec<SearchHit>,
    /// Full count of matches, independent of the cap.
    pub total_matched: u64,
    /// Size of the whole index (for the footer's "N / M" display).
    pub total_indexed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(paths: &[(&str, bool)]) -> IndexArena {
        let mut b = ArenaBuilder::new();
        for (p, folder) in paths {
            b.push(p, *folder, 10, 100);
        }
        b.finish()
    }

    #[test]
    fn derives_name_dir_ext_like_the_old_entry_layout() {
        let a = build(&[("C:\\Projects\\Report.docx", false)]);
        assert_eq!(a.name(0), "Report.docx");
        assert_eq!(a.name_lower(0), "report.docx");
        assert_eq!(a.dir(0), "C:\\Projects");
        assert_eq!(a.ext(0), "docx");
        assert_eq!(a.size(0), 10);
        assert_eq!(a.full_path(0), "C:\\Projects\\Report.docx");
    }

    #[test]
    fn folder_has_no_ext_and_dotfile_has_no_ext() {
        let a = build(&[("C:\\p\\node_modules", true), ("C:\\p\\.gitignore", false)]);
        assert_eq!(a.ext(0), "");
        assert!(a.folder(0));
        assert_eq!(a.ext(1), "");
    }

    #[test]
    fn directories_are_interned_not_repeated() {
        let a = build(&[
            ("C:\\same\\a.txt", false),
            ("C:\\same\\b.txt", false),
            ("C:\\other\\c.txt", false),
        ]);
        assert_eq!(a.dir_count(), 2);
        assert_eq!(a.dir_id(0), a.dir_id(1));
        assert_ne!(a.dir_id(0), a.dir_id(2));
        assert_eq!(a.dir_lower_by_id(a.dir_id(0)), "c:\\same");
    }

    /// The interning cache keys on the previous directory; a walk that
    /// alternates between directories must still reuse ids rather than
    /// appending a duplicate each time.
    #[test]
    fn interning_survives_alternating_directories() {
        let a = build(&[
            ("C:\\x\\1.txt", false),
            ("C:\\y\\2.txt", false),
            ("C:\\x\\3.txt", false),
        ]);
        assert_eq!(a.dir_count(), 2);
        assert_eq!(a.dir_id(0), a.dir_id(2));
    }

    #[test]
    fn extensions_are_interned_and_resolvable() {
        let a = build(&[("C:\\a\\x.png", false), ("C:\\a\\y.PNG", false)]);
        assert_eq!(a.ext_id(0), a.ext_id(1));
        assert_eq!(a.find_ext_id("png"), Some(a.ext_id(0)));
        assert_eq!(a.find_ext_id("jpg"), None);
    }

    #[test]
    fn full_path_round_trips_unc_roots() {
        let a = build(&[("\\\\nas\\share\\docs\\a.txt", false)]);
        assert_eq!(a.dir(0), "\\\\nas\\share\\docs");
        assert_eq!(a.full_path(0), "\\\\nas\\share\\docs\\a.txt");
        assert_eq!(a.full_path_lower(0), "\\\\nas\\share\\docs\\a.txt");
    }

    /// Multi-byte names must be sliced on char boundaries by the offsets
    /// array — a byte-offset bug here would panic rather than misbehave.
    #[test]
    fn handles_multibyte_names() {
        let a = build(&[("C:\\docs\\レポート.docx", false), ("C:\\docs\\next.txt", false)]);
        assert_eq!(a.name(0), "レポート.docx");
        assert_eq!(a.name(1), "next.txt");
        assert_eq!(a.ext(0), "docx");
    }

    #[test]
    fn freshly_built_arena_is_valid() {
        let a = build(&[("C:\\a\\x.png", false), ("C:\\b\\y", true)]);
        assert!(a.is_valid());
        assert!(IndexArena::default().is_valid());
    }

    #[test]
    fn truncated_arena_is_rejected() {
        let mut a = build(&[("C:\\a\\x.png", false), ("C:\\a\\y.png", false)]);
        a.size.pop();
        assert!(!a.is_valid());
    }
}
