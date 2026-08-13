// Bridge between the React frontend and FlexFind's Tauri (Rust) commands.
//
// When running inside the Tauri shell, these helpers call the real backend.
// When running as a plain web app (`npm run dev` in a browser), `isTauri` is
// false and callers get small mock data so the UI is still meaningfully
// previewable with zero Tauri runtime present (same philosophy as
// FlexExplorer's src/fs/bridge.ts).

import type {
  ArenaMemory,
  ColumnConfig,
  RootIndexStatus,
  SearchResponse,
  Settings,
  SiblingAvailability,
  SortSpec,
} from '../types'

export const DEFAULT_COLUMNS: ColumnConfig[] = [
  { id: 'name', width: 240 },
  { id: 'path', width: 320 },
  { id: 'size', width: 84 },
  { id: 'modified', width: 132 },
]

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// Lazily imported so the web build never needs the Tauri runtime at module load.
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

// ---- mock data for `npm run dev` (no Tauri runtime) ----

const MOCK_SETTINGS: Settings = {
  theme: 'flex-light',
  scanRoots: [
    { path: 'C:', enabled: true, isDrive: true },
    { path: 'D:', enabled: true, isDrive: true },
    { path: '\\\\nas\\docs', enabled: true, isDrive: false },
  ],
  scopes: [{ id: 'scope-work', name: '仕事', includePaths: ['C:\\Projects', '\\\\nas\\docs'] }],
  excludePaths: ['node_modules', '.git', '$recycle.bin', 'system volume information'],
  excludeHidden: false,
  excludeSystem: false,
  excludeShortcuts: true,
  hotkey: 'Ctrl+Space',
  launchAtLogin: true,
  startMinimizedToTray: true,
  autoCheckUpdates: false,
  reindexMode: 'interval',
  reindexIntervalMinutes: 30,
  reindexDailyTime: '03:00',
  columns: DEFAULT_COLUMNS,
}

const MOCK_STATUS: RootIndexStatus[] = [
  { root: 'C:', label: 'Windows-SSD', state: 'done', scannedCount: 214830, isDrive: true, loadedFromDisk: true, skippedDirs: 0, skippedSamples: [] },
  { root: 'D:', label: 'Data', state: 'done', scannedCount: 98120, isDrive: true, loadedFromDisk: true, skippedDirs: 0, skippedSamples: [] },
  { root: '\\\\nas\\docs', label: 'docs', state: 'done', scannedCount: 51200, isDrive: false, loadedFromDisk: true, skippedDirs: 0, skippedSamples: [] },
]

function mockSearch(
  query: string,
  limit: number,
  includePaths: string[] | null,
  sort: SortSpec | null,
): SearchResponse {
  const all = [
    { name: 'FlexFind.exe', dir: 'C:\\Projects\\FlexFind\\src-tauri\\target\\release', size: 8_400_000, modified: 1751686920 },
    { name: 'FlexExplorer.exe', dir: 'C:\\Projects\\FlexExplorer\\target\\release', size: 11_900_000, modified: 1751600000 },
    { name: 'flex-design', dir: 'C:\\Projects', size: 0, modified: 1751790180, folder: true },
    { name: 'FlexGrep.exe', dir: 'C:\\Projects\\FlexGrep\\target\\release', size: 10_200_000, modified: 1751300000 },
    { name: '議事録.docx', dir: '\\\\nas\\docs\\2026', size: 42_000, modified: 1751790000 },
  ]
  const q = query.trim().toLowerCase()
  if (!q) return { hits: [], totalMatched: 0, totalIndexed: MOCK_STATUS.reduce((n, d) => n + d.scannedCount, 0) }
  let matched = all
    .map(e => ({ ...e, path: e.dir + '\\' + e.name }))
    .filter(e => e.name.toLowerCase().includes(q))
  if (includePaths && includePaths.length) {
    const lows = includePaths.map(p => p.toLowerCase())
    matched = matched.filter(e => lows.some(inc => e.path.toLowerCase().startsWith(inc.toLowerCase())))
  }
  if (sort) {
    const dir = sort.descending ? -1 : 1
    matched.sort((a, b) => {
      let c = 0
      if (sort.column === 'name') c = a.name.toLowerCase().localeCompare(b.name.toLowerCase())
      else if (sort.column === 'path') c = a.path.localeCompare(b.path)
      else if (sort.column === 'size') c = a.size - b.size
      else c = a.modified - b.modified
      return c * dir
    })
  }
  const total = matched.length
  return {
    hits: matched.slice(0, limit).map(e => {
      const matchStart = e.name.toLowerCase().indexOf(q)
      return {
        name: e.name,
        path: e.path,
        dir: e.dir,
        folder: !!e.folder,
        ext: e.folder ? '' : (e.name.split('.').pop() ?? ''),
        size: e.size,
        modified: e.modified,
        matchStart: Math.max(0, matchStart),
        matchLen: q.length,
      }
    }),
    totalMatched: total,
    totalIndexed: MOCK_STATUS.reduce((n, d) => n + d.scannedCount, 0),
  }
}

// ---- index search ----

export interface SearchOpts {
  limit?: number
  includePaths?: string[] | null
  sort?: SortSpec | null
}

export async function searchIndex(query: string, opts: SearchOpts = {}): Promise<SearchResponse> {
  const limit = opts.limit ?? 500
  const includePaths = opts.includePaths ?? null
  const sort = opts.sort ?? null
  if (!isTauri) return mockSearch(query, limit, includePaths, sort)
  return invoke<SearchResponse>('search_index', { query, limit, includePaths, sort })
}

export async function getIndexStatus(): Promise<RootIndexStatus[]> {
  if (!isTauri) return MOCK_STATUS
  return invoke<RootIndexStatus[]>('get_index_status')
}

export async function rebuildIndex(): Promise<void> {
  if (!isTauri) return
  await invoke('rebuild_index')
}

export async function setIndexPaused(paused: boolean): Promise<void> {
  if (!isTauri) return
  await invoke('set_index_paused', { paused })
}

/** Per-root heap accounting for the index: `[root, breakdown][]`. */
export async function getIndexMemory(): Promise<[string, ArenaMemory][]> {
  if (!isTauri) return []
  return invoke<[string, ArenaMemory][]>('get_index_memory')
}

/** Release the process working set so the next search pays the cold cost. */
export async function trimWorkingSet(): Promise<void> {
  if (!isTauri) return
  await invoke('trim_working_set')
}

// ---- settings ----

export async function getSettings(): Promise<Settings> {
  if (!isTauri) return MOCK_SETTINGS
  return invoke<Settings>('get_settings')
}

export async function setSettings(settings: Settings): Promise<void> {
  if (!isTauri) return
  await invoke('set_settings', { settings })
}

export async function pickFolder(): Promise<string | null> {
  if (!isTauri) return null
  return invoke<string | null>('pick_folder')
}

/** Persist just the result-table column layout (read-modify-write on the
 * Rust side, so it won't clobber scopes/roots edited in the settings window). */
export async function setColumns(columns: ColumnConfig[]): Promise<void> {
  if (!isTauri) return
  await invoke('set_columns', { columns })
}

// ---- window management ----

export async function showMainWindow(): Promise<void> {
  if (!isTauri) return
  await invoke('show_main_window')
}

export async function hideMainWindow(): Promise<void> {
  if (!isTauri) return
  await invoke('hide_main_window')
}

export async function showSettings(): Promise<void> {
  if (!isTauri) return
  await invoke('show_settings')
}

// ---- hotkey ----

export async function registerHotkey(accelerator: string): Promise<void> {
  if (!isTauri) return
  await invoke('register_hotkey', { accelerator })
}

// ---- shell ----

export async function openPath(path: string): Promise<void> {
  if (!isTauri) return
  await invoke('open_path', { path })
}

/** Launch an exe directly with argv (no shell involved) — used to hand a
 * target path to the FlexExplorer/FlexGrep sibling apps, which `openPath`
 * can't do since it only opens a single target via a shell verb. */
export async function launchApp(exe: string, args: string[]): Promise<void> {
  if (!isTauri) return
  await invoke('launch_app', { exe, args })
}

export async function revealInExplorer(path: string): Promise<void> {
  if (!isTauri) return
  await invoke('reveal_in_explorer', { path })
}

/** Duplicates a file into the same folder as `<stem>_<YYYYMMDD>_<NN><ext>`.
 * Resolves to the new file's path. */
export async function duplicateAsDatedCopy(path: string): Promise<string> {
  if (!isTauri) return path
  return invoke<string>('duplicate_as_dated_copy', { path })
}

export async function shellVerb(path: string, verb: string): Promise<void> {
  if (!isTauri) return
  await invoke('shell_verb', { path, verb })
}

export async function elevateRestart(): Promise<void> {
  if (!isTauri) return
  await invoke('elevate_restart')
}

export async function isElevated(): Promise<boolean> {
  if (!isTauri) return false
  return invoke<boolean>('is_elevated')
}

/** Copy plain text to the clipboard (works inside the WebView). */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return false
  }
}

// ---- sibling apps (FlexExplorer / FlexGrep) ----

export async function findSiblingApps(): Promise<SiblingAvailability> {
  if (!isTauri) return { flexExplorer: null, flexGrep: null }
  return invoke<SiblingAvailability>('find_sibling_apps')
}

// ---- native Windows shell icons (cached by extension / folder) ----
//
// Icons resolve from the name's extension (SHGFI_USEFILEATTRIBUTES) so they
// are the same for every .png, every .exe, etc. — hence cache by type, not
// by individual file, keeping the icon set tiny even for huge result lists.

const iconCache = new Map<string, string>()
const iconInflight = new Map<string, Promise<string>>()

function iconKey(name: string, folder: boolean): string {
  if (folder) return 'dir'
  const i = name.lastIndexOf('.')
  const ext = i > 0 ? name.slice(i + 1).toLowerCase() : 'noext'
  return 'ext:' + ext
}

/** Synchronously read an already-cached icon data URL, or null. */
export function peekIcon(name: string, folder: boolean): string | null {
  return iconCache.get(iconKey(name, folder)) || null
}

/** Fetch the native shell icon as a PNG data URL (cached by type). */
export async function shellIcon(name: string, folder: boolean): Promise<string> {
  if (!isTauri) return ''
  const key = iconKey(name, folder)
  const cached = iconCache.get(key)
  if (cached) return cached
  const pending = iconInflight.get(key)
  if (pending) return pending
  const p = invoke<string>('shell_icon', { name, folder, large: false })
    .then(url => {
      iconCache.set(key, url)
      iconInflight.delete(key)
      return url
    })
    .catch(() => {
      iconInflight.delete(key)
      return ''
    })
  iconInflight.set(key, p)
  return p
}

// ---- window controls (custom titlebar; decorations are disabled) ----

export async function winMinimize(): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().minimize()
}

export async function winToggleMaximize(): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().toggleMaximize()
}

export async function winClose(): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().hide()
}

/** Set the OS window title (what the Windows taskbar shows). Used to mirror
 * the live search query into the taskbar instead of a static app name. */
export async function winSetTitle(title: string): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().setTitle(title)
}

export async function winStartDragging(): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().startDragging()
}

// ---- autostart (Windows startup entry) ----

export async function autostartIsEnabled(): Promise<boolean> {
  if (!isTauri) return MOCK_SETTINGS.launchAtLogin
  const { isEnabled } = await import('@tauri-apps/plugin-autostart')
  return isEnabled()
}

export async function autostartSetEnabled(enabled: boolean): Promise<void> {
  if (!isTauri) return
  const mod = await import('@tauri-apps/plugin-autostart')
  await (enabled ? mod.enable() : mod.disable())
}

// ---- cross-window settings sync ----

/** Emit a `settings-changed` event so the main window refreshes its scope
 * mirror when the settings window edits scopes. */
export async function emitSettingsChanged(): Promise<void> {
  if (!isTauri) return
  const { emit } = await import('@tauri-apps/api/event')
  await emit('settings-changed')
}
