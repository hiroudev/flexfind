import { useEffect } from 'react'
import { useSearchStore } from '../store/useSearchStore'
import { duplicateAsDatedCopy, launchApp, openPath, revealInExplorer, shellVerb } from '../fs/bridge'

interface ItemProps {
  label: string
  shortcut?: string
  onClick: () => void
}

function Item({ label, shortcut, onClick }: ItemProps) {
  return (
    <div
      onClick={onClick}
      style={{
        minHeight: 28,
        display: 'flex',
        alignItems: 'center',
        padding: '0 10px',
        borderRadius: 6,
        fontSize: 12,
        color: 'var(--text)',
        gap: 14,
        cursor: 'default',
      }}
      onMouseEnter={e => {
        e.currentTarget.style.background = 'var(--bg-hover)'
      }}
      onMouseLeave={e => {
        e.currentTarget.style.background = 'transparent'
      }}
    >
      <div style={{ flex: 1, lineHeight: 1.3 }}>{label}</div>
      {shortcut && <div style={{ fontSize: 10.5, color: 'var(--text-faint)', whiteSpace: 'nowrap' }}>{shortcut}</div>}
    </div>
  )
}

function Sep() {
  return <div style={{ height: 1, background: 'var(--border)', margin: '4px 6px' }} />
}

/** Result-row right-click menu (design screen 5). */
export default function ContextMenu() {
  const ctx = useSearchStore(s => s.ctx)
  const hits = useSearchStore(s => s.activeTab().hits)
  const siblings = useSearchStore(s => s.siblings)
  const closeContextMenu = useSearchStore(s => s.closeContextMenu)
  const copySelectedPath = useSearchStore(s => s.copySelectedPath)

  useEffect(() => {
    window.addEventListener('mousedown', closeContextMenu)
    return () => window.removeEventListener('mousedown', closeContextMenu)
  }, [closeContextMenu])

  if (!ctx) return null
  const hit = hits[ctx.index]
  if (!hit) return null

  return (
    <div
      onMouseDown={e => e.stopPropagation()}
      style={{
        position: 'fixed',
        left: ctx.x,
        top: ctx.y,
        width: 270,
        background: 'var(--bg-panel)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius)',
        boxShadow: '0 16px 40px -16px var(--shadow)',
        padding: 6,
        zIndex: 100,
      }}
    >
      <Item
        label="開く"
        shortcut="Enter"
        onClick={() => {
          closeContextMenu()
          void openPath(hit.path)
        }}
      />
      {!hit.folder && (
        <Item
          label="新規"
          onClick={() => {
            closeContextMenu()
            // ShellExecuteExW "new" verb — what Explorer's own "新規"
            // context-menu entry invokes for file types that register it
            // (Office documents notably: Excel/Word/PowerPoint open an
            // unsaved copy seeded from this file, editing the original never
            // touched). Silently a no-op via shell_verb's error return if
            // the file's type doesn't register the verb.
            void shellVerb(hit.path, 'new')
          }}
        />
      )}
      <Item
        label="エクスプローラーで表示"
        shortcut="Ctrl+Enter"
        onClick={() => {
          closeContextMenu()
          void revealInExplorer(hit.path)
        }}
      />
      {siblings.flexExplorer && (
        <Item
          label="FlexExplorerで表示"
          onClick={() => {
            closeContextMenu()
            // Hands off the containing folder — FlexExplorer opens at a
            // directory, not a single file selection.
            void launchApp(siblings.flexExplorer as string, [hit.dir])
          }}
        />
      )}
      {siblings.flexGrep && !hit.folder && (
        <Item
          label="FlexGrepでこのファイルの中身を検索"
          onClick={() => {
            closeContextMenu()
            void launchApp(siblings.flexGrep as string, [hit.path])
          }}
        />
      )}
      <Sep />
      <Item
        label="パスをコピー"
        shortcut="Ctrl+C"
        onClick={() => {
          closeContextMenu()
          void copySelectedPath()
        }}
      />
      {!hit.folder && (
        <Item
          label="コピーを日付付きで保存"
          onClick={() => {
            closeContextMenu()
            void duplicateAsDatedCopy(hit.path)
          }}
        />
      )}
      <Sep />
      <Item
        label="プロパティ"
        onClick={() => {
          closeContextMenu()
          void shellVerb(hit.path, 'properties')
        }}
      />
    </div>
  )
}
