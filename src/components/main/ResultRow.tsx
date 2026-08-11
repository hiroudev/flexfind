import { useState } from 'react'
import type { MouseEvent } from 'react'
import type { ColumnConfig, SearchHit, SortColumn } from '../../types'
import { splitHighlight } from '../../search/highlight'
import { gridTemplate, ROW_H } from './grid'
import FileIcon from './FileIcon'

interface Props {
  hit: SearchHit
  selected: boolean
  columns: ColumnConfig[]
  onClick: () => void
  onDoubleClick: () => void
  onContextMenu: (e: MouseEvent) => void
}

function formatSize(bytes: number, folder: boolean): string {
  if (folder) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = bytes / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`
}

function formatDate(unixSeconds: number): string {
  if (!unixSeconds) return ''
  const d = new Date(unixSeconds * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function NameCell({ hit, selected }: { hit: SearchHit; selected: boolean }) {
  const { pre, match, post } = splitHighlight(hit.name, hit.matchStart, hit.matchLen)
  return (
    <div
      style={{
        whiteSpace: 'nowrap',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        color: 'var(--text)',
        fontWeight: selected ? 700 : 500,
      }}
    >
      {pre}
      {match && <span style={{ background: 'var(--match)', borderRadius: 2, padding: '0 1px' }}>{match}</span>}
      {post}
    </div>
  )
}

function cell(col: SortColumn, hit: SearchHit, selected: boolean) {
  switch (col) {
    case 'name':
      return <NameCell hit={hit} selected={selected} />
    case 'path':
      return (
        <div style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', direction: 'rtl', textAlign: 'left', color: 'var(--text-muted)' }}>
          {/* direction:rtl keeps the (more useful) tail of a long path visible
              while still truncating with an ellipsis. */}
          &lrm;{hit.dir}
        </div>
      )
    case 'size':
      return (
        <div style={{ textAlign: 'right', color: 'var(--text-faint)', fontVariantNumeric: 'tabular-nums' }}>
          {formatSize(hit.size, hit.folder)}
        </div>
      )
    case 'modified':
      return (
        <div style={{ color: 'var(--text-faint)', fontVariantNumeric: 'tabular-nums' }}>{formatDate(hit.modified)}</div>
      )
  }
}

export default function ResultRow({ hit, selected, columns, onClick, onDoubleClick, onContextMenu }: Props) {
  const [hover, setHover] = useState(false)
  const bg = selected ? 'var(--bg-active)' : hover ? 'var(--bg-hover)' : 'transparent'

  return (
    <div
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      title={hit.path}
      style={{
        height: ROW_H,
        display: 'grid',
        gridTemplateColumns: gridTemplate(columns),
        alignItems: 'center',
        gap: 8,
        padding: '0 14px',
        fontSize: 12.5,
        background: bg,
        cursor: 'default',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <FileIcon name={hit.name} folder={hit.folder} ext={hit.ext} />
      </div>
      {columns.map(col => (
        <div key={col.id} style={{ minWidth: 0 }}>
          {cell(col.id, hit, selected)}
        </div>
      ))}
      {/* trailing 1fr filler cell (grid has an extra track) */}
      <div />
    </div>
  )
}
