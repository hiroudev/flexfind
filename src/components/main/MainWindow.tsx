import { useEffect, useRef } from 'react'
import { useSearchStore } from '../../store/useSearchStore'
import { hideMainWindow, isTauri, winSetTitle } from '../../fs/bridge'
import type { RootIndexStatus } from '../../types'
import AppTitleBar from '../AppTitleBar'
import TabBar from './TabBar'
import SearchBar from './SearchBar'
import ColumnHeader from './ColumnHeader'
import ResultList from './ResultList'
import StatusBar from './StatusBar'
import EmptyState from './EmptyState'
import SyntaxHelpPopover from './SyntaxHelpPopover'
import IndexProgressPanel from './IndexProgressPanel'
import ContextMenu from '../ContextMenu'

export default function MainWindow() {
  const inputRef = useRef<HTMLInputElement>(null)

  const activeTabId = useSearchStore(s => s.activeTabId)
  const tab = useSearchStore(s => s.activeTab())
  const columns = useSearchStore(s => s.columns)
  const setSelected = useSearchStore(s => s.setSelected)
  const openSelected = useSearchStore(s => s.openSelected)
  const openContextMenu = useSearchStore(s => s.openContextMenu)
  const setScrollTop = useSearchStore(s => s.setScrollTop)
  const refreshIndexStatus = useSearchStore(s => s.refreshIndexStatus)
  const refreshSiblings = useSearchStore(s => s.refreshSiblings)
  const refreshScopes = useSearchStore(s => s.refreshScopes)

  // Initial load. `main-window-shown` (emitted by the Rust show command on
  // every summon) focuses + selects the input, but does NOT reset — tabs and
  // their queries must survive.
  useEffect(() => {
    void refreshIndexStatus()
    void refreshSiblings()
    void refreshScopes()
    inputRef.current?.focus()
    if (!isTauri) return
    let unlisten: (() => void) | undefined
    void (async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen('main-window-shown', () => {
        inputRef.current?.focus()
        inputRef.current?.select()
      })
    })()
    return () => unlisten?.()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Mirror the active tab's query into the OS window title, so the Windows
  // taskbar shows what's being searched in real time rather than a static
  // app name. Empty query → empty title (no "FlexFind" label, by request).
  useEffect(() => {
    void winSetTitle(tab.query.trim())
  }, [tab.query])

  // Live per-root index status pushed from Rust as the background walk runs.
  useEffect(() => {
    if (!isTauri) return
    let unlisten: (() => void) | undefined
    void (async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen<RootIndexStatus[]>('index://progress', event => {
        useSearchStore.getState().setIndexStatus(event.payload)
      })
    })()
    return () => unlisten?.()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Scopes edited in the settings window → refresh the mirror so the scope
  // dropdown updates live.
  useEffect(() => {
    if (!isTauri) return
    let unlisten: (() => void) | undefined
    void (async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen('settings-changed', () => {
        void refreshScopes()
      })
    })()
    return () => unlisten?.()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Re-theme live when the settings window changes the theme.
  useEffect(() => {
    if (!isTauri) return
    let unlisten: (() => void) | undefined
    void (async () => {
      const { listen } = await import('@tauri-apps/api/event')
      const { applyTheme } = await import('flex-design/runtime/theme.js')
      unlisten = await listen<string>('theme-changed', event => {
        applyTheme(event.payload, document.documentElement)
      })
    })()
    return () => unlisten?.()
  }, [])

  // Keyboard handling lives on a *window* listener, not the root div's
  // onKeyDown — clicking a result row moves focus to <body> (rows aren't
  // focusable), which is an ancestor of the React root, so a div-level
  // handler would stop firing. A window listener keeps arrow-key
  // navigation / Enter / Esc working regardless of where focus is. Store
  // state is read via getState() so this effect can be registered once.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const s = useSearchStore.getState()
      const activeTab = s.activeTab()

      if (e.ctrlKey && (e.key === 't' || e.key === 'T')) {
        e.preventDefault()
        s.addTab()
        inputRef.current?.focus()
        return
      }
      if (e.ctrlKey && (e.key === 'w' || e.key === 'W')) {
        e.preventDefault()
        s.closeTab(s.activeTabId)
        inputRef.current?.focus()
        return
      }
      if (e.ctrlKey && e.key === 'Tab') {
        e.preventDefault()
        s.cycleTab(e.shiftKey ? -1 : 1)
        return
      }
      // Ctrl+PageDown/PageUp alt (WebView2 sometimes swallows Ctrl+Tab).
      if (e.ctrlKey && e.key === 'PageDown') {
        e.preventDefault()
        s.cycleTab(1)
        return
      }
      if (e.ctrlKey && e.key === 'PageUp') {
        e.preventDefault()
        s.cycleTab(-1)
        return
      }

      if (e.key === 'Escape') {
        // Staged: popover → context menu → clear query → hide to tray.
        if (s.helpOpen) {
          s.closeHelp()
          return
        }
        if (s.ctx) {
          s.closeContextMenu()
          return
        }
        if (activeTab.query.trim() !== '') {
          s.clearActiveQuery()
          inputRef.current?.focus()
          return
        }
        void hideMainWindow()
        return
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        s.moveSelection(1)
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        s.moveSelection(-1)
        return
      }
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault()
        void s.revealSelected()
        return
      }
      if (e.key === 'Enter') {
        e.preventDefault()
        void s.openSelected()
        return
      }
      if ((e.key === 'c' || e.key === 'C') && (e.ctrlKey || e.metaKey)) {
        const input = inputRef.current
        const hasTextSelection =
          !!input && document.activeElement === input && input.selectionStart !== input.selectionEnd
        if (!hasTextSelection) {
          e.preventDefault()
          void s.copySelectedPath()
        }
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const showEmptyState = tab.query.trim() !== '' && tab.hits.length === 0

  return (
    <div
      style={{
        width: '100vw',
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--bg-panel)',
        fontFamily: 'var(--font)',
        position: 'relative',
        overflow: 'hidden',
      }}
    >
      <AppTitleBar title="" showSettingsButton>
        <TabBar />
      </AppTitleBar>
      <SearchBar inputRef={inputRef} />
      <SyntaxHelpPopover />
      <IndexProgressPanel />
      <ColumnHeader />
      {showEmptyState ? (
        <div style={{ flex: 1, minHeight: 0 }}>
          <EmptyState />
        </div>
      ) : (
        <ResultList
          key={activeTabId}
          hits={tab.hits}
          selectedIndex={tab.selectedIndex}
          columns={columns}
          initialScrollTop={tab.scrollTop}
          onSelect={setSelected}
          onOpen={i => {
            setSelected(i)
            void openSelected()
          }}
          onContextMenu={(e, i) => openContextMenu(e.clientX, e.clientY, i)}
          onScrollTop={setScrollTop}
        />
      )}
      <StatusBar />
      <ContextMenu />
    </div>
  )
}
