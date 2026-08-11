import { useEffect } from 'react'
import type { ReactNode } from 'react'
import { useSettingsStore } from '../../store/useSettingsStore'
import type { SettingsTab } from '../../store/useSettingsStore'
import { isTauri } from '../../fs/bridge'
import type { RootIndexStatus } from '../../types'
import AppTitleBar from '../AppTitleBar'
import NavItem from '../shared/NavItem'
import IndexingTab from './tabs/IndexingTab'
import ScopesTab from './tabs/ScopesTab'
import HotkeyTab from './tabs/HotkeyTab'
import StartupTab from './tabs/StartupTab'
import ElevationTab from './tabs/ElevationTab'
import AppearanceTab from './tabs/AppearanceTab'

const NAV: { id: SettingsTab; label: string }[] = [
  { id: 'indexing', label: '索引対象' },
  { id: 'scopes', label: '検索対象' },
  { id: 'hotkey', label: 'ホットキー' },
  { id: 'startup', label: '起動時の動作' },
  { id: 'elevation', label: '管理者権限' },
  { id: 'appearance', label: 'テーマ' },
]

export default function SettingsShell() {
  const activeTab = useSettingsStore(s => s.activeTab)
  const setActiveTab = useSettingsStore(s => s.setActiveTab)
  const loadAll = useSettingsStore(s => s.loadAll)
  const loaded = useSettingsStore(s => s.loaded)

  useEffect(() => {
    void loadAll()
  }, [loadAll])

  // Keep IndexingTab's per-root status badges live as a walk runs (the main
  // window isn't the only place that needs `index://progress`).
  useEffect(() => {
    if (!isTauri) return
    let unlisten: (() => void) | undefined
    void (async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen<RootIndexStatus[]>('index://progress', event => {
        useSettingsStore.setState({ rootStatus: event.payload })
      })
    })()
    return () => unlisten?.()
  }, [])

  if (!loaded) return null

  const tabContent: Record<SettingsTab, ReactNode> = {
    indexing: <IndexingTab />,
    scopes: <ScopesTab />,
    hotkey: <HotkeyTab />,
    startup: <StartupTab />,
    elevation: <ElevationTab />,
    appearance: <AppearanceTab />,
  }

  return (
    <div
      style={{
        width: '100vw',
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--bg-panel)',
        fontFamily: 'var(--font)',
      }}
    >
      <AppTitleBar title="設定" escToClose />
      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        <div style={{ width: 190, flex: '0 0 190px', background: 'var(--bg-sunken)', borderRight: '1px solid var(--border)', padding: '14px 0' }}>
          {NAV.map(n => (
            <NavItem key={n.id} label={n.label} active={activeTab === n.id} onClick={() => setActiveTab(n.id)} />
          ))}
        </div>
        <div style={{ flex: 1, minWidth: 0, overflowY: 'auto', padding: '24px 28px' }}>{tabContent[activeTab]}</div>
      </div>
    </div>
  )
}
