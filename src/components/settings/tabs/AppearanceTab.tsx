import { useMemo, useRef, useState } from 'react'
import type { ChangeEvent } from 'react'
import { THEME_LIST, THEMES } from 'flex-design/themes/presets.js'
import type { FlexTheme } from 'flex-design/themes/presets.js'
import { deleteCustomTheme, loadCustomThemes, saveCustomTheme } from 'flex-design/runtime/theme.js'
import { themePreviewHTML } from 'flex-design/runtime/theme-preview.js'
import { parseFlexThemeFile, serializeFlexThemeFile } from 'flex-design/theme-forge/schema.js'
import { useSettingsStore } from '../../../store/useSettingsStore'
import Section from '../../shared/Section'
import SmallBtn from '../../shared/SmallBtn'
import ThemeDesignerModal from '../ThemeDesignerModal'

function downloadJson(filename: string, json: string) {
  const blob = new Blob([json], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

// FlexFind's result list has a fixed 28px row height with no user-facing
// density controls, so unlike FlexExplorer's AppearanceTab this only keeps
// the theme grid + custom-theme create/import/export section, dropping the
// accent-dot/font-size/row-height/icon-size/radius/zebra options that don't
// apply here.
export default function AppearanceTab() {
  const theme = useSettingsStore(s => s.settings.theme)
  const setTheme = useSettingsStore(s => s.setTheme)

  const [refresh, setRefresh] = useState(0)
  const bump = () => setRefresh(r => r + 1)
  const customThemes = useMemo(() => loadCustomThemes(), [refresh])
  const [designerOpen, setDesignerOpen] = useState(false)
  const [importErrors, setImportErrors] = useState<string[] | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  function importCustomTheme(t: FlexTheme) {
    if (customThemes.some(existing => existing.key === t.key)) {
      if (!window.confirm(`同名のテーマ「${t.label}」が既にあります。上書きしますか？`)) return
    }
    saveCustomTheme(t)
    void setTheme(t.key)
    bump()
    setImportErrors(null)
  }

  function handleImportFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      const result = parseFlexThemeFile(String(reader.result))
      if (!result.ok) {
        setImportErrors(result.errors)
        return
      }
      importCustomTheme(result.theme)
    }
    reader.readAsText(file)
  }

  function exportCustomTheme(t: FlexTheme) {
    const json = serializeFlexThemeFile({ theme: t, meta: { name: t.label, sub: t.sub } })
    downloadJson(`${t.key.replace(/^custom:/, '')}.flextheme.json`, json)
  }

  function removeCustomTheme(t: FlexTheme) {
    if (!window.confirm(`テーマ「${t.label}」を削除しますか？`)) return
    deleteCustomTheme(t.key)
    if (theme === t.key) void setTheme('flex-light')
    bump()
  }

  return (
    <div>
      <Section title="テーマ" />
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10, marginBottom: 4 }}>
        {THEME_LIST.map(t => {
          const tt = THEMES[t.key]
          const active = theme === t.key
          return (
            <div
              key={t.key}
              onClick={() => void setTheme(t.key)}
              style={{
                padding: 8,
                borderRadius: 6,
                border: '1px solid ' + (active ? 'var(--accent)' : 'var(--border)'),
                background: active ? 'var(--accent-soft)' : 'var(--bg-page)',
                cursor: 'default',
              }}
            >
              <div
                style={{ borderRadius: 6, overflow: 'hidden', border: '1px solid ' + tt.border }}
                dangerouslySetInnerHTML={{ __html: themePreviewHTML(tt) }}
              />
              <div style={{ fontSize: 11.5, fontWeight: 600, color: 'var(--text)', marginTop: 6 }}>{t.label}</div>
              <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>{t.sub}</div>
            </div>
          )
        })}
      </div>

      {customThemes.length > 0 && (
        <>
          <div style={{ fontSize: 10, color: 'var(--text-faint)', marginTop: 10, marginBottom: 6 }}>カスタムテーマ</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10, marginBottom: 4 }}>
            {customThemes.map(tt => {
              const active = theme === tt.key
              return (
                <div
                  key={tt.key}
                  onClick={() => void setTheme(tt.key)}
                  style={{
                    position: 'relative',
                    padding: 8,
                    borderRadius: 6,
                    border: '1px solid ' + (active ? 'var(--accent)' : 'var(--border)'),
                    background: active ? 'var(--accent-soft)' : 'var(--bg-page)',
                    cursor: 'default',
                  }}
                >
                  <div style={{ position: 'absolute', top: 4, right: 4, display: 'flex', gap: 2 }}>
                    <span
                      title="書き出し"
                      onClick={e => {
                        e.stopPropagation()
                        exportCustomTheme(tt)
                      }}
                      style={{ width: 16, height: 16, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, color: 'var(--text-faint)', borderRadius: 3 }}
                    >
                      ⭳
                    </span>
                    <span
                      title="削除"
                      onClick={e => {
                        e.stopPropagation()
                        removeCustomTheme(tt)
                      }}
                      style={{ width: 16, height: 16, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9, color: 'var(--text-faint)', borderRadius: 3 }}
                    >
                      ✕
                    </span>
                  </div>
                  <div
                    style={{ borderRadius: 6, overflow: 'hidden', border: '1px solid ' + tt.border }}
                    dangerouslySetInnerHTML={{ __html: themePreviewHTML(tt) }}
                  />
                  <div
                    style={{
                      fontSize: 11.5,
                      fontWeight: 600,
                      color: 'var(--text)',
                      marginTop: 6,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {tt.label}
                  </div>
                  <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>{tt.sub || 'カスタムテーマ'}</div>
                </div>
              )
            })}
          </div>
        </>
      )}

      <div style={{ display: 'flex', gap: 8, marginTop: 8, marginBottom: 4 }}>
        <SmallBtn onClick={() => setDesignerOpen(true)}>＋ テーマを作成…</SmallBtn>
        <SmallBtn onClick={() => fileInputRef.current?.click()}>インポート…</SmallBtn>
        <input ref={fileInputRef} type="file" accept="application/json,.json" onChange={handleImportFile} style={{ display: 'none' }} />
      </div>
      {importErrors && (
        <div style={{ marginTop: 4, marginBottom: 4, padding: '8px 10px', borderRadius: 6, background: 'var(--danger-soft)', color: 'var(--danger)', fontSize: 11 }}>
          {importErrors.map((e, i) => (
            <div key={i}>・{e}</div>
          ))}
        </div>
      )}
      {designerOpen && (
        <ThemeDesignerModal
          restoreThemeKey={theme}
          onClose={() => setDesignerOpen(false)}
          onSave={({ theme: savedTheme }) => {
            importCustomTheme(savedTheme)
            setDesignerOpen(false)
          }}
        />
      )}
    </div>
  )
}
