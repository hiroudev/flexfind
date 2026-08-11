import { create } from 'zustand'
import type {
  ColumnConfig,
  CtxState,
  RootIndexStatus,
  SearchHit,
  SearchScope,
  SiblingAvailability,
  SortColumn,
  SortSpec,
} from '../types'
import {
  copyText,
  DEFAULT_COLUMNS,
  findSiblingApps,
  getIndexStatus,
  getSettings,
  openPath,
  revealInExplorer,
  searchIndex,
  setColumns as persistColumns,
} from '../fs/bridge'

/** One search tab: an independent query + scope + sort + result/scroll state. */
export interface TabState {
  id: string
  query: string
  scopeId: string | null // null = 全体 (no include filter)
  sort: SortSpec | null
  hits: SearchHit[]
  selectedIndex: number
  totalMatched: number
  scrollTop: number
  /** Per-tab stale-response guard (a slow search for an old keystroke in
   * this tab must not overwrite a newer one). */
  requestSeq: number
}

interface SearchState {
  tabs: TabState[]
  activeTabId: string
  // window-global:
  totalIndexed: number
  indexStatus: RootIndexStatus[]
  initialBuildDone: boolean
  scopes: SearchScope[]
  columns: ColumnConfig[]
  helpOpen: boolean
  ctx: CtxState | null
  siblings: SiblingAvailability

  activeTab: () => TabState
  setQuery: (q: string) => void
  runSearch: (tabId: string) => Promise<void>
  addTab: () => void
  closeTab: (id: string) => void
  cycleTab: (dir: 1 | -1) => void
  setActiveTab: (id: string) => void
  setScope: (scopeId: string | null) => void
  setSort: (column: SortColumn) => void
  setScrollTop: (v: number) => void
  moveSelection: (delta: number) => void
  setSelected: (index: number) => void
  openSelected: () => Promise<void>
  revealSelected: () => Promise<void>
  copySelectedPath: () => Promise<void>
  clearActiveQuery: () => void
  openContextMenu: (x: number, y: number, index: number) => void
  closeContextMenu: () => void
  toggleHelp: () => void
  closeHelp: () => void
  setIndexStatus: (status: RootIndexStatus[]) => void
  refreshIndexStatus: () => Promise<void>
  refreshSiblings: () => Promise<void>
  refreshScopes: () => Promise<void>
  loadColumns: () => Promise<void>
  setColumnWidth: (id: SortColumn, width: number) => void
  reorderColumns: (srcId: SortColumn, destId: SortColumn) => void
  persistColumns: () => void
}

const SEARCH_DEBOUNCE_MS = 30

/** Per-tab debounce timers, keyed by tab id (cleared on tab close). */
const debounceTimers = new Map<string, ReturnType<typeof setTimeout>>()

let tabCounter = 0
function newTab(): TabState {
  tabCounter += 1
  return {
    id: `tab-${tabCounter}-${Math.random().toString(36).slice(2, 8)}`,
    query: '',
    scopeId: null,
    sort: null,
    hits: [],
    selectedIndex: 0,
    totalMatched: 0,
    scrollTop: 0,
    requestSeq: 0,
  }
}

const firstTab = newTab()

export const useSearchStore = create<SearchState>((set, get) => {
  /** Merge a patch into a tab by id, returning a new tabs array. */
  function patchTab(id: string, patch: Partial<TabState>): TabState[] {
    return get().tabs.map(t => (t.id === id ? { ...t, ...patch } : t))
  }

  return {
    tabs: [firstTab],
    activeTabId: firstTab.id,
    totalIndexed: 0,
    indexStatus: [],
    initialBuildDone: false,
    scopes: [],
    columns: DEFAULT_COLUMNS,
    helpOpen: false,
    ctx: null,
    siblings: { flexExplorer: null, flexGrep: null },

    activeTab() {
      const { tabs, activeTabId } = get()
      return tabs.find(t => t.id === activeTabId) ?? tabs[0]
    },

    setQuery(q) {
      const id = get().activeTabId
      set({ tabs: patchTab(id, { query: q }) })
      const existing = debounceTimers.get(id)
      if (existing) clearTimeout(existing)
      debounceTimers.set(
        id,
        setTimeout(() => {
          void get().runSearch(id)
        }, SEARCH_DEBOUNCE_MS),
      )
    },

    async runSearch(tabId) {
      const tab = get().tabs.find(t => t.id === tabId)
      if (!tab) return
      const mySeq = tab.requestSeq + 1
      set({ tabs: patchTab(tabId, { requestSeq: mySeq }) })
      if (!tab.query.trim()) {
        set({ tabs: patchTab(tabId, { hits: [], totalMatched: 0, selectedIndex: 0 }) })
        return
      }
      const scope = tab.scopeId ? get().scopes.find(s => s.id === tab.scopeId) : null
      const res = await searchIndex(tab.query, {
        includePaths: scope ? scope.includePaths : null,
        sort: tab.sort,
      })
      // Guard on both the per-tab seq AND the tab still existing.
      const cur = get().tabs.find(t => t.id === tabId)
      if (!cur || cur.requestSeq !== mySeq) return
      set({
        totalIndexed: res.totalIndexed,
        tabs: patchTab(tabId, {
          hits: res.hits,
          totalMatched: res.totalMatched,
          selectedIndex: 0,
        }),
      })
    },

    addTab() {
      const t = newTab()
      set(s => ({ tabs: [...s.tabs, t], activeTabId: t.id }))
    },

    closeTab(id) {
      const timer = debounceTimers.get(id)
      if (timer) {
        clearTimeout(timer)
        debounceTimers.delete(id)
      }
      const { tabs, activeTabId } = get()
      if (tabs.length <= 1) {
        // Last tab: reset it in place rather than leaving zero tabs.
        const t = newTab()
        set({ tabs: [t], activeTabId: t.id })
        return
      }
      const idx = tabs.findIndex(t => t.id === id)
      const remaining = tabs.filter(t => t.id !== id)
      let nextActive = activeTabId
      if (activeTabId === id) {
        nextActive = remaining[Math.min(idx, remaining.length - 1)].id
      }
      set({ tabs: remaining, activeTabId: nextActive })
    },

    cycleTab(dir) {
      const { tabs, activeTabId } = get()
      if (tabs.length <= 1) return
      const idx = tabs.findIndex(t => t.id === activeTabId)
      const next = (idx + dir + tabs.length) % tabs.length
      set({ activeTabId: tabs[next].id })
    },

    setActiveTab(id) {
      set({ activeTabId: id })
    },

    setScope(scopeId) {
      const id = get().activeTabId
      set({ tabs: patchTab(id, { scopeId, selectedIndex: 0, scrollTop: 0 }) })
      void get().runSearch(id)
    },

    setSort(column) {
      const id = get().activeTabId
      const tab = get().activeTab()
      // Same column toggles asc→desc→(back to asc); a new column starts asc.
      let sort: SortSpec | null
      if (tab.sort && tab.sort.column === column) {
        sort = { column, descending: !tab.sort.descending }
      } else {
        sort = { column, descending: false }
      }
      set({ tabs: patchTab(id, { sort, selectedIndex: 0, scrollTop: 0 }) })
      void get().runSearch(id)
    },

    setScrollTop(v) {
      const id = get().activeTabId
      set({ tabs: patchTab(id, { scrollTop: v }) })
    },

    moveSelection(delta) {
      const tab = get().activeTab()
      if (tab.hits.length === 0) return
      const next = Math.min(Math.max(tab.selectedIndex + delta, 0), tab.hits.length - 1)
      set({ tabs: patchTab(tab.id, { selectedIndex: next }) })
    },

    setSelected(index) {
      const id = get().activeTabId
      set({ tabs: patchTab(id, { selectedIndex: index }) })
    },

    async openSelected() {
      const tab = get().activeTab()
      const hit = tab.hits[tab.selectedIndex]
      if (hit) await openPath(hit.path)
    },

    async revealSelected() {
      const tab = get().activeTab()
      const hit = tab.hits[tab.selectedIndex]
      if (hit) await revealInExplorer(hit.path)
    },

    async copySelectedPath() {
      const tab = get().activeTab()
      const hit = tab.hits[tab.selectedIndex]
      if (hit) await copyText(hit.path)
    },

    clearActiveQuery() {
      const id = get().activeTabId
      set({ tabs: patchTab(id, { query: '', hits: [], totalMatched: 0, selectedIndex: 0 }) })
    },

    openContextMenu(x, y, index) {
      const id = get().activeTabId
      set({ ctx: { x, y, index }, tabs: patchTab(id, { selectedIndex: index }) })
    },

    closeContextMenu() {
      set({ ctx: null })
    },

    toggleHelp() {
      set(s => ({ helpOpen: !s.helpOpen }))
    },

    closeHelp() {
      set({ helpOpen: false })
    },

    setIndexStatus(status) {
      // Monotonic: once every root has finished its first walk OR is serving
      // a disk-loaded index, stays true for the rest of the process's life —
      // a later periodic freshness re-walk resets roots to pending/scanning,
      // but that shouldn't re-show the big first-build panel (see
      // IndexProgressPanel).
      const allReady =
        status.length > 0 &&
        status.every(d => d.loadedFromDisk || (d.state !== 'pending' && d.state !== 'scanning'))
      set(s => ({ indexStatus: status, initialBuildDone: s.initialBuildDone || allReady }))
    },

    async refreshIndexStatus() {
      get().setIndexStatus(await getIndexStatus())
    },

    async refreshSiblings() {
      set({ siblings: await findSiblingApps() })
    },

    async refreshScopes() {
      // Scopes AND columns both live in settings.json; one fetch covers both.
      const settings = await getSettings()
      set({ scopes: settings.scopes, columns: settings.columns })
    },

    async loadColumns() {
      const settings = await getSettings()
      set({ columns: settings.columns })
    },

    setColumnWidth(id, width) {
      const MIN = 60
      set(s => ({
        columns: s.columns.map(c => (c.id === id ? { ...c, width: Math.max(MIN, Math.round(width)) } : c)),
      }))
    },

    reorderColumns(srcId, destId) {
      if (srcId === destId) return
      set(s => {
        const cols = [...s.columns]
        const si = cols.findIndex(c => c.id === srcId)
        const di = cols.findIndex(c => c.id === destId)
        if (si < 0 || di < 0) return {}
        const [moved] = cols.splice(si, 1)
        cols.splice(di, 0, moved)
        return { columns: cols }
      })
      get().persistColumns()
    },

    persistColumns() {
      void persistColumns(get().columns)
    },
  }
})
