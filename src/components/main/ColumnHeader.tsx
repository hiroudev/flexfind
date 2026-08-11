import { useEffect, useRef, useState } from 'react'
import { useSearchStore } from '../../store/useSearchStore'
import type { SortColumn } from '../../types'
import { COLUMN_LABEL, gridTemplate, RIGHT_ALIGNED } from './grid'

/**
 * Sortable / reorderable / resizable column header, driven by the persisted
 * column config. Click a label to sort; drag a label onto another to reorder;
 * drag the right-edge handle to resize. Reorder persists immediately; width
 * persists on drag release.
 */
export default function ColumnHeader() {
  const columns = useSearchStore(s => s.columns)
  const sort = useSearchStore(s => s.activeTab().sort)
  const setSort = useSearchStore(s => s.setSort)
  const setColumnWidth = useSearchStore(s => s.setColumnWidth)
  const reorderColumns = useSearchStore(s => s.reorderColumns)
  const persist = useSearchStore(s => s.persistColumns)

  const [overId, setOverId] = useState<SortColumn | null>(null)
  const dragColRef = useRef<SortColumn | null>(null)
  // Width-resize drag state (global listeners so the pointer can leave the
  // handle while dragging).
  const resizeRef = useRef<{ id: SortColumn; startX: number; startW: number } | null>(null)

  useEffect(() => {
    function onMove(e: MouseEvent) {
      const r = resizeRef.current
      if (!r) return
      // Suppress text selection across the whole window while dragging.
      document.body.style.userSelect = 'none'
      setColumnWidth(r.id, r.startW + (e.clientX - r.startX))
    }
    function onUp() {
      if (resizeRef.current) {
        resizeRef.current = null
        document.body.style.userSelect = ''
        persist()
      }
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
    return () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
  }, [setColumnWidth, persist])

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: gridTemplate(columns),
        alignItems: 'center',
        height: 26,
        flex: '0 0 26px',
        padding: '0 14px',
        gap: 8,
        fontSize: 11,
        fontWeight: 600,
        color: 'var(--text-muted)',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-panel)',
        userSelect: 'none',
      }}
    >
      {/* Empty leading cell aligns with the row's icon column. */}
      <div />
      {columns.map(col => {
        const active = sort?.column === col.id
        const right = RIGHT_ALIGNED[col.id]
        return (
          <div
            key={col.id}
            draggable
            onDragStart={e => {
              dragColRef.current = col.id
              e.dataTransfer.effectAllowed = 'move'
              e.dataTransfer.setData('text/plain', col.id)
            }}
            onDragOver={e => {
              e.preventDefault()
              e.dataTransfer.dropEffect = 'move'
              if (overId !== col.id) setOverId(col.id)
            }}
            onDragLeave={() => setOverId(prev => (prev === col.id ? null : prev))}
            onDrop={e => {
              e.preventDefault()
              if (dragColRef.current) reorderColumns(dragColRef.current, col.id)
              dragColRef.current = null
              setOverId(null)
            }}
            onDragEnd={() => {
              dragColRef.current = null
              setOverId(null)
            }}
            onClick={() => setSort(col.id)}
            title="クリックで並べ替え・ドラッグで列を移動"
            style={{
              position: 'relative',
              display: 'flex',
              alignItems: 'center',
              gap: 3,
              justifyContent: right ? 'flex-end' : 'flex-start',
              minWidth: 0,
              cursor: 'default',
              boxShadow: overId === col.id ? 'inset 2px 0 0 var(--accent)' : 'none',
            }}
          >
            <span
              style={{
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                color: active ? 'var(--accent)' : undefined,
                fontWeight: active ? 700 : 600,
              }}
            >
              {COLUMN_LABEL[col.id]}
            </span>
            {active && <span style={{ fontSize: 9, color: 'var(--accent)', flex: '0 0 auto' }}>{sort?.descending ? '▼' : '▲'}</span>}
            {/* Right-edge resize handle. */}
            <span
              onMouseDown={e => {
                e.preventDefault()
                e.stopPropagation()
                resizeRef.current = { id: col.id, startX: e.clientX, startW: col.width }
              }}
              onClick={e => e.stopPropagation()}
              onDragStart={e => e.preventDefault()}
              draggable={false}
              title="ドラッグで列幅を調整"
              style={{ position: 'absolute', right: -12, top: -6, width: 14, height: 38, cursor: 'col-resize', zIndex: 2 }}
            />
          </div>
        )
      })}
      {/* trailing 1fr filler track */}
      <div />
    </div>
  )
}
