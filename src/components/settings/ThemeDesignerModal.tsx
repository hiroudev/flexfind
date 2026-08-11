import { useEffect, useRef } from 'react'
import { mountThemeDesigner } from 'flex-design/theme-forge/designer.js'
import { applyTheme } from 'flex-design/runtime/theme.js'
import type { FlexTheme } from 'flex-design/themes/presets.js'
import type { ThemeDesignerInitial, ThemeDesignerSaveResult } from 'flex-design/theme-forge/designer.js'

export type { ThemeDesignerSaveResult }

export default function ThemeDesignerModal({
  initial,
  restoreThemeKey,
  onSave,
  onClose,
}: {
  initial?: ThemeDesignerInitial
  restoreThemeKey: string
  onSave: (result: ThemeDesignerSaveResult) => void
  onClose: () => void
}) {
  const hostRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!hostRef.current) return
    const instance = mountThemeDesigner(hostRef.current, {
      initial,
      onPreview: (theme: FlexTheme) => {
        applyTheme(theme, document.documentElement)
      },
      onSave: (result: ThemeDesignerSaveResult) => onSave(result),
      onCancel: () => {
        applyTheme(restoreThemeKey, document.documentElement)
        onClose()
      },
    })
    return () => instance.destroy()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <>
      <div
        onMouseDown={() => {
          applyTheme(restoreThemeKey, document.documentElement)
          onClose()
        }}
        style={{ position: 'fixed', inset: 0, zIndex: 120, background: 'rgba(0,0,0,.4)' }}
      />
      <div
        style={{
          position: 'fixed',
          left: '50%',
          top: '50%',
          transform: 'translate(-50%,-50%)',
          zIndex: 121,
          width: 600,
          maxHeight: '86vh',
          overflowY: 'auto',
          background: 'var(--bg-panel)',
          border: '1px solid var(--border)',
          borderRadius: 12,
          boxShadow: '0 24px 70px var(--shadow)',
          padding: 18,
          fontFamily: 'var(--font)',
        }}
      >
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 14 }}>テーマを作成</div>
        <div ref={hostRef} />
      </div>
    </>
  )
}
