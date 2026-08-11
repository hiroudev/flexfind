//! DTOs shared across the index walker, search engine, and the Tauri
//! commands exposed to the frontend.

use serde::{Deserialize, Serialize};

/// One indexed filesystem entry (file or folder).
#[derive(Clone)]
pub struct IndexedEntry {
    /// File/dir name only (no path).
    pub name: String,
    /// Precomputed lowercase of `name`, for fast case-insensitive matching.
    pub name_lower: String,
    /// Full absolute path.
    pub path: String,
    /// Parent directory path (precomputed so the frontend's path column
    /// doesn't need to re-split `path` on every render).
    pub dir: String,
    pub folder: bool,
    /// Lowercased extension without the dot (empty for folders / no ext).
    pub ext: String,
    pub size: u64,
    /// Modified time, unix seconds.
    pub modified: i64,
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
