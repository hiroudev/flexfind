import type { ColumnConfig, SortColumn } from '../../types'

export const ROW_H = 28
/** Fixed leading icon column. */
export const ICON_COL = 22

export const COLUMN_LABEL: Record<SortColumn, string> = {
  name: '名前',
  path: 'パス',
  size: 'サイズ',
  modified: '更新日時',
}

export const RIGHT_ALIGNED: Record<SortColumn, boolean> = {
  name: false,
  path: false,
  size: true,
  modified: false,
}

/**
 * Build the shared `grid-template-columns` for header + rows from the user's
 * column config: a fixed icon column, each configured column at its px
 * width in order, then a `1fr` filler so the table fills the window (extra
 * space sits after the last column, Explorer-style). Widths beyond the
 * window scroll horizontally.
 */
export function gridTemplate(columns: ColumnConfig[]): string {
  const cols = columns.map(c => `${c.width}px`).join(' ')
  return `${ICON_COL}px ${cols} 1fr`
}
