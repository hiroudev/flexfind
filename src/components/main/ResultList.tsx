import { useEffect, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import type { ColumnConfig, SearchHit } from '../../types'
import ResultRow from './ResultRow'
import { ROW_H } from './grid'

const OVERSCAN = 6

interface Props {
  hits: SearchHit[]
  selectedIndex: number
  columns: ColumnConfig[]
  initialScrollTop: number
  onSelect: (index: number) => void
  onOpen: (index: number) => void
  onContextMenu: (e: MouseEvent, index: number) => void
  onScrollTop: (v: number) => void
}

/**
 * Hand-rolled scroll-position virtualization (no library, matching the rest
 * of this repo). Fills its flex container (window is resizable now), so only
 * the visible slice (+ overscan) is ever mounted even for large result sets.
 * Mounted per-tab (keyed on tab id by the parent), so `initialScrollTop`
 * restores that tab's saved scroll position.
 */
export default function ResultList({
  hits,
  selectedIndex,
  columns,
  initialScrollTop,
  onSelect,
  onOpen,
  onContextMenu,
  onScrollTop,
}: Props) {
  const scrollerRef = useRef<HTMLDivElement>(null)
  const [scrollTop, setScrollTop] = useState(initialScrollTop)
  const [viewportH, setViewportH] = useState(400)

  // Restore this tab's saved scroll position on mount.
  useEffect(() => {
    if (scrollerRef.current) scrollerRef.current.scrollTop = initialScrollTop
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const el = scrollerRef.current
    if (!el) return
    setViewportH(el.clientHeight)
    const ro = new ResizeObserver(entries => setViewportH(entries[0].contentRect.height))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Keyboard selection (Up/Down) scrolls the selected row into view; mouse
  // hover never triggers this path.
  useEffect(() => {
    const el = scrollerRef.current
    if (!el) return
    const rowTop = selectedIndex * ROW_H
    const rowBottom = rowTop + ROW_H
    if (rowTop < el.scrollTop) el.scrollTop = rowTop
    else if (rowBottom > el.scrollTop + el.clientHeight) el.scrollTop = rowBottom - el.clientHeight
  }, [selectedIndex])

  const totalH = hits.length * ROW_H
  const firstVisible = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN)
  const visibleCount = Math.ceil(viewportH / ROW_H) + OVERSCAN * 2
  const lastVisible = Math.min(hits.length, firstVisible + visibleCount)
  const slice = hits.slice(firstVisible, lastVisible)

  return (
    <div
      ref={scrollerRef}
      onScroll={e => {
        const v = e.currentTarget.scrollTop
        setScrollTop(v)
        onScrollTop(v)
      }}
      style={{ flex: 1, minHeight: 0, overflowY: 'auto', position: 'relative' }}
    >
      <div style={{ height: totalH, position: 'relative' }}>
        {slice.map((hit, i) => {
          const index = firstVisible + i
          return (
            <div key={hit.path} style={{ position: 'absolute', top: index * ROW_H, left: 0, right: 0, height: ROW_H }}>
              <ResultRow
                hit={hit}
                selected={index === selectedIndex}
                columns={columns}
                onClick={() => onSelect(index)}
                onDoubleClick={() => onOpen(index)}
                onContextMenu={e => {
                  e.preventDefault()
                  onContextMenu(e, index)
                }}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}
