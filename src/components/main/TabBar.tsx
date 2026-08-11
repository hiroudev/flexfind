import { useSearchStore } from '../../store/useSearchStore'
import type { TabState } from '../../store/useSearchStore'

function tabLabel(t: TabState): string {
  const q = t.query.trim()
  return q || '新しいタブ'
}

export default function TabBar() {
  const tabs = useSearchStore(s => s.tabs)
  const activeTabId = useSearchStore(s => s.activeTabId)
  const setActiveTab = useSearchStore(s => s.setActiveTab)
  const closeTab = useSearchStore(s => s.closeTab)
  const addTab = useSearchStore(s => s.addTab)

  return (
    // The empty area of the tab strip is a window drag region (individual
    // tabs and the + button lack the attribute, so they stay clickable).
    // This is what makes most of the titlebar draggable — without it the
    // tab strip (which fills the bar) would swallow drags.
    <div data-tauri-drag-region style={{ display: 'flex', alignItems: 'center', gap: 2, flex: 1, minWidth: 0, overflowX: 'auto' }}>
      {tabs.map(t => {
        const active = t.id === activeTabId
        return (
          <div
            key={t.id}
            onClick={() => setActiveTab(t.id)}
            onAuxClick={e => {
              if (e.button === 1) closeTab(t.id) // middle-click closes
            }}
            title={tabLabel(t)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              height: 24,
              maxWidth: 160,
              padding: '0 6px 0 10px',
              borderRadius: 6,
              fontSize: 12,
              cursor: 'default',
              flex: 'none',
              background: active ? 'var(--bg-active)' : 'transparent',
              color: active ? 'var(--text)' : 'var(--text-muted)',
              border: `1px solid ${active ? 'var(--border-strong)' : 'transparent'}`,
            }}
          >
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{tabLabel(t)}</span>
            <span
              onClick={e => {
                e.stopPropagation()
                closeTab(t.id)
              }}
              title="タブを閉じる (Ctrl+W)"
              style={{ flex: 'none', width: 15, height: 15, display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: 3, color: 'var(--text-faint)', fontSize: 11 }}
            >
              ×
            </span>
          </div>
        )
      })}
      {/* + sits immediately after the last tab (browser convention), not
          right-aligned. */}
      <div
        onClick={addTab}
        title="新しいタブ (Ctrl+T)"
        style={{ flex: 'none', width: 22, height: 22, display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: 5, color: 'var(--text-muted)', fontSize: 15, cursor: 'default', marginLeft: 2 }}
      >
        +
      </div>
    </div>
  )
}
