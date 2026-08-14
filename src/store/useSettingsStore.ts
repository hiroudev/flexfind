import { create } from 'zustand'
import type { ReindexMode, RootIndexStatus, ScanRoot, SearchScope, Settings } from '../types'
import * as bridge from '../fs/bridge'

export type SettingsTab = 'indexing' | 'scopes' | 'hotkey' | 'startup' | 'elevation' | 'appearance'

interface SettingsState {
  loaded: boolean
  activeTab: SettingsTab
  settings: Settings
  rootStatus: RootIndexStatus[]
  capturingHotkey: boolean
  /** Set when the last capture attempt failed (bad syntax, or an OS-level
   * conflict with another app) — surfaced by HotkeyTab; the previous
   * binding stays active/persisted in this case, nothing is lost. */
  hotkeyError: string | null
  isElevated: boolean

  setActiveTab: (tab: SettingsTab) => void
  loadAll: () => Promise<void>
  refreshRootStatus: () => Promise<void>
  // scan roots
  toggleRoot: (path: string) => Promise<void>
  addFolderRoot: () => Promise<void>
  addUncRoot: (path: string) => Promise<boolean>
  removeRoot: (path: string) => Promise<void>
  // excludes
  addExcludePath: () => Promise<void>
  removeExcludePath: (path: string) => Promise<void>
  setExcludeHidden: (v: boolean) => Promise<void>
  setExcludeSystem: (v: boolean) => Promise<void>
  setExcludeShortcuts: (v: boolean) => Promise<void>
  // scopes
  addScope: (name: string) => Promise<void>
  renameScope: (id: string, name: string) => Promise<void>
  removeScope: (id: string) => Promise<void>
  /** Move scope `srcId` to `destId`'s position; the saved order is what the
   * main window's scope dropdown lists. */
  reorderScopes: (srcId: string, destId: string) => Promise<void>
  addScopeIncludeFolder: (id: string) => Promise<void>
  addScopeIncludeManual: (id: string, path: string) => Promise<boolean>
  removeScopeInclude: (id: string, path: string) => Promise<void>
  // hotkey / startup / elevation / theme
  startHotkeyCapture: () => void
  cancelHotkeyCapture: () => void
  captureHotkeyChord: (chord: string) => Promise<void>
  setLaunchAtLogin: (v: boolean) => Promise<void>
  setStartMinimized: (v: boolean) => Promise<void>
  // index-refresh schedule (no rebuild needed — the Rust tick reads it live)
  setReindexMode: (mode: ReindexMode) => Promise<void>
  setReindexIntervalMinutes: (minutes: number) => Promise<void>
  setReindexDailyTime: (time: string) => Promise<void>
  relaunchElevated: () => Promise<void>
  setTheme: (key: string) => Promise<void>
}

const DEFAULT_SETTINGS: Settings = {
  theme: 'flex-light',
  scanRoots: [],
  scopes: [],
  excludePaths: [],
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
  columns: bridge.DEFAULT_COLUMNS,
}

function isValidUnc(path: string): boolean {
  // Minimal UNC check: starts with \\, and has a server + share component.
  const p = path.trim()
  if (!p.startsWith('\\\\')) return false
  const rest = p.slice(2).replace(/[\\/]+$/, '')
  return rest.split(/[\\/]+/).filter(Boolean).length >= 2
}

function uuid(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : 'scope-' + Math.random().toString(36).slice(2, 10)
}

export const useSettingsStore = create<SettingsState>((set, get) => {
  /** Merge `patch` into settings, persist it, notify the main window. */
  async function persist(patch: Partial<Settings>): Promise<Settings> {
    const next = { ...get().settings, ...patch }
    set({ settings: next })
    await bridge.setSettings(next)
    await bridge.emitSettingsChanged()
    return next
  }

  return {
    loaded: false,
    activeTab: 'indexing',
    settings: DEFAULT_SETTINGS,
    rootStatus: [],
    capturingHotkey: false,
    hotkeyError: null,
    isElevated: false,

    setActiveTab(tab) {
      set({ activeTab: tab })
    },

    async loadAll() {
      const [settings, rootStatus, elevated] = await Promise.all([
        bridge.getSettings(),
        bridge.getIndexStatus(),
        bridge.isElevated(),
      ])
      set({ settings, rootStatus, isElevated: elevated, loaded: true })
    },

    async refreshRootStatus() {
      set({ rootStatus: await bridge.getIndexStatus() })
    },

    async toggleRoot(path) {
      const roots = get().settings.scanRoots.map(r =>
        r.path === path ? { ...r, enabled: !r.enabled } : r,
      )
      await persist({ scanRoots: roots })
      await bridge.rebuildIndex()
      await get().refreshRootStatus()
    },

    async addFolderRoot() {
      const picked = await bridge.pickFolder()
      if (!picked) return
      const cur = get().settings.scanRoots
      if (cur.some(r => r.path.toLowerCase() === picked.toLowerCase())) return
      const root: ScanRoot = { path: picked, enabled: true, isDrive: false }
      await persist({ scanRoots: [...cur, root] })
      await bridge.rebuildIndex()
      await get().refreshRootStatus()
    },

    async addUncRoot(path) {
      const p = path.trim().replace(/[\\/]+$/, '')
      if (!isValidUnc(p)) return false
      const cur = get().settings.scanRoots
      if (cur.some(r => r.path.toLowerCase() === p.toLowerCase())) return true
      const root: ScanRoot = { path: p, enabled: true, isDrive: false }
      await persist({ scanRoots: [...cur, root] })
      await bridge.rebuildIndex()
      await get().refreshRootStatus()
      return true
    },

    async removeRoot(path) {
      // Only custom (non-drive) roots are removable.
      const roots = get().settings.scanRoots.filter(r => r.path !== path || r.isDrive)
      await persist({ scanRoots: roots })
      await bridge.rebuildIndex()
      await get().refreshRootStatus()
    },

    async addExcludePath() {
      const picked = await bridge.pickFolder()
      if (!picked) return
      const cur = get().settings.excludePaths
      if (cur.includes(picked)) return
      await persist({ excludePaths: [...cur, picked] })
      await bridge.rebuildIndex()
    },

    async removeExcludePath(path) {
      const cur = get().settings.excludePaths
      await persist({ excludePaths: cur.filter(p => p !== path) })
      await bridge.rebuildIndex()
    },

    async setExcludeHidden(v) {
      await persist({ excludeHidden: v })
      await bridge.rebuildIndex()
    },

    async setExcludeSystem(v) {
      await persist({ excludeSystem: v })
      await bridge.rebuildIndex()
    },

    async setExcludeShortcuts(v) {
      await persist({ excludeShortcuts: v })
      await bridge.rebuildIndex()
    },

    async addScope(name) {
      const scope: SearchScope = { id: uuid(), name: name.trim() || '新しい検索対象', includePaths: [] }
      await persist({ scopes: [...get().settings.scopes, scope] })
    },

    async renameScope(id, name) {
      const scopes = get().settings.scopes.map(s => (s.id === id ? { ...s, name } : s))
      await persist({ scopes })
    },

    async removeScope(id) {
      await persist({ scopes: get().settings.scopes.filter(s => s.id !== id) })
    },

    async reorderScopes(srcId, destId) {
      if (srcId === destId) return
      const scopes = [...get().settings.scopes]
      const si = scopes.findIndex(s => s.id === srcId)
      const di = scopes.findIndex(s => s.id === destId)
      if (si < 0 || di < 0) return
      const [moved] = scopes.splice(si, 1)
      scopes.splice(di, 0, moved)
      await persist({ scopes })
    },

    async addScopeIncludeFolder(id) {
      const picked = await bridge.pickFolder()
      if (!picked) return
      const scopes = get().settings.scopes.map(s =>
        s.id === id && !s.includePaths.includes(picked)
          ? { ...s, includePaths: [...s.includePaths, picked] }
          : s,
      )
      await persist({ scopes })
    },

    async addScopeIncludeManual(id, path) {
      const p = path.trim().replace(/[\\/]+$/, '')
      // Accept a drive/local path or a valid UNC path.
      const looksLocal = /^[A-Za-z]:/.test(p)
      if (!p || (!looksLocal && !isValidUnc(p))) return false
      const scopes = get().settings.scopes.map(s =>
        s.id === id && !s.includePaths.some(x => x.toLowerCase() === p.toLowerCase())
          ? { ...s, includePaths: [...s.includePaths, p] }
          : s,
      )
      await persist({ scopes })
      return true
    },

    async removeScopeInclude(id, path) {
      const scopes = get().settings.scopes.map(s =>
        s.id === id ? { ...s, includePaths: s.includePaths.filter(p => p !== path) } : s,
      )
      await persist({ scopes })
    },

    startHotkeyCapture() {
      set({ capturingHotkey: true, hotkeyError: null })
    },

    cancelHotkeyCapture() {
      set({ capturingHotkey: false })
    },

    async captureHotkeyChord(chord) {
      set({ capturingHotkey: false })
      const previous = get().settings.hotkey
      try {
        await bridge.registerHotkey(chord)
      } catch (err) {
        await bridge.registerHotkey(previous).catch(() => {})
        set({ hotkeyError: err instanceof Error ? err.message : String(err) })
        return
      }
      set({ hotkeyError: null })
      await persist({ hotkey: chord })
    },

    async setLaunchAtLogin(v) {
      await persist({ launchAtLogin: v })
      await bridge.autostartSetEnabled(v)
    },

    async setStartMinimized(v) {
      await persist({ startMinimizedToTray: v })
    },

    async setReindexMode(mode) {
      await persist({ reindexMode: mode })
    },

    async setReindexIntervalMinutes(minutes) {
      // Clamp to a sane floor; the Rust side also guards with `.max(1)`.
      await persist({ reindexIntervalMinutes: Math.max(1, Math.round(minutes)) })
    },

    async setReindexDailyTime(time) {
      await persist({ reindexDailyTime: time })
    },

    async relaunchElevated() {
      await bridge.elevateRestart()
    },

    async setTheme(key) {
      const { applyTheme, saveThemeChoice } = await import('flex-design/runtime/theme.js')
      applyTheme(key, document.documentElement)
      saveThemeChoice('flexfind', key)
      await persist({ theme: key })
      if (bridge.isTauri) {
        const { emit } = await import('@tauri-apps/api/event')
        await emit('theme-changed', key)
      }
    },
  }
})
