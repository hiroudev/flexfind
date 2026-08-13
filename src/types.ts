// Mirrors the Rust DTOs in src-tauri/src/index/types.rs, src-tauri/src/settings.rs,
// and src-tauri/src/sibling.rs (all `#[serde(rename_all = "camelCase")]`).

export interface SearchHit {
  name: string
  path: string
  dir: string
  folder: boolean
  ext: string
  size: number
  /** Unix seconds. */
  modified: number
  /** UTF-16 code-unit offset into `name` where the first bare-term match begins. */
  matchStart: number
  matchLen: number
}

export interface SearchResponse {
  hits: SearchHit[]
  totalMatched: number
  totalIndexed: number
}

export type RootScanState =
  | 'pending'
  | 'scanning'
  | 'done'
  | 'skippedNoAccess'
  | 'offline'

export interface RootIndexStatus {
  /** Canonical root: "C:" for drives, or the full folder / UNC path. */
  root: string
  label: string
  state: RootScanState
  scannedCount: number
  isDrive: boolean
  loadedFromDisk: boolean
  /** Subtrees the last walk couldn't read — non-zero means results are incomplete. */
  skippedDirs: number
  skippedSamples: string[]
}

export type SortColumn = 'name' | 'path' | 'size' | 'modified'

export interface SortSpec {
  column: SortColumn
  descending: boolean
}

/** A result-table column's persisted layout; array order = column order. */
export interface ColumnConfig {
  id: SortColumn
  width: number
}

export interface ScanRoot {
  path: string
  enabled: boolean
  isDrive: boolean
}

export interface SearchScope {
  id: string
  name: string
  includePaths: string[]
}

export interface Settings {
  theme: string
  scanRoots: ScanRoot[]
  scopes: SearchScope[]
  excludePaths: string[]
  /** Everything-style result filters, checked per entry (distinct from
   * `excludePaths`, which excludes by location). */
  excludeHidden: boolean
  excludeSystem: boolean
  /** Hides `.lnk` shortcut files from results. */
  excludeShortcuts: boolean
  hotkey: string
  launchAtLogin: boolean
  startMinimizedToTray: boolean
  autoCheckUpdates: boolean
  /** Background index-refresh schedule. */
  reindexMode: ReindexMode
  /** Minutes between refreshes when `reindexMode === 'interval'`. */
  reindexIntervalMinutes: number
  /** "HH:MM" (24h, local) for the daily refresh when `reindexMode === 'daily'`. */
  reindexDailyTime: string
  columns: ColumnConfig[]
}

export type ReindexMode = 'interval' | 'daily' | 'manual'

/** Exact heap accounting for one root's packed index, from Rust. */
export interface ArenaMemory {
  entries: number
  dirs: number
  exts: number
  namesLowerBytes: number
  namesBytes: number
  dirBytes: number
  extBytes: number
  columnBytes: number
  totalBytes: number
  /** Bytes a plain term query actually reads — what governs cold-search latency. */
  scannedBytes: number
}

export interface SiblingAvailability {
  flexExplorer: string | null
  flexGrep: string | null
}

export interface CtxState {
  x: number
  y: number
  index: number
}
