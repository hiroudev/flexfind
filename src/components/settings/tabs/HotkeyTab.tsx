import type { KeyboardEvent } from 'react'
import { useSettingsStore } from '../../../store/useSettingsStore'
import Section from '../../shared/Section'

function Keycap({ children }: { children: string }) {
  return (
    <div
      style={{
        padding: '5px 9px',
        background: 'var(--bg-page)',
        border: '1px solid var(--border-strong)',
        borderRadius: 6,
        fontSize: 11.5,
        fontWeight: 700,
      }}
    >
      {children}
    </div>
  )
}

const MODIFIER_KEYS = new Set(['Control', 'Alt', 'Shift', 'Meta'])

/** Named keys `tauri-plugin-global-shortcut`'s accelerator parser accepts
 * (verified against the `global-hotkey` crate's parser). Deliberately an
 * allow-list, not a "reject known-bad" list: `e.key` reports the *shifted*
 * character for symbol keys (e.g. Shift+1 is `"!"`, not `"1"`), and IME
 * composition on Japanese keyboards can report `"Process"`/`"Dead"` — none
 * of those parse, so silently ignoring anything not on this list (rather
 * than trying to enumerate every rejected variant) is what keeps capture
 * from ever committing an unparseable chord in the first place.
 */
const NAMED_MAIN_KEYS = new Set([
  'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight',
  'Enter', 'Tab', 'Backspace', 'Delete',
  'Home', 'End', 'PageUp', 'PageDown',
])
const PUNCTUATION_MAIN_KEYS = new Set([',', '.', '/', ';', "'", '=', '-', '`', '[', ']', '\\'])
const F_KEY_RE = /^F(?:[1-9]|1\d|2[0-4])$/

function mainKeyName(key: string): string | null {
  if (key === ' ') return 'Space'
  if (/^[a-zA-Z0-9]$/.test(key)) return key.toUpperCase()
  if (F_KEY_RE.test(key)) return key
  if (NAMED_MAIN_KEYS.has(key)) return key
  if (PUNCTUATION_MAIN_KEYS.has(key)) return key
  return null
}

export default function HotkeyTab() {
  const hotkey = useSettingsStore(s => s.settings.hotkey)
  const capturing = useSettingsStore(s => s.capturingHotkey)
  const hotkeyError = useSettingsStore(s => s.hotkeyError)
  const startHotkeyCapture = useSettingsStore(s => s.startHotkeyCapture)
  const cancelHotkeyCapture = useSettingsStore(s => s.cancelHotkeyCapture)
  const captureHotkeyChord = useSettingsStore(s => s.captureHotkeyChord)

  function onCaptureKeyDown(e: KeyboardEvent) {
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      cancelHotkeyCapture()
      return
    }
    if (MODIFIER_KEYS.has(e.key)) return // still building the chord

    const main = mainKeyName(e.key)
    if (!main) return // unsupported key (shifted symbol, IME composition, …) — ignore, keep capturing

    const parts: string[] = []
    if (e.ctrlKey) parts.push('Ctrl')
    if (e.altKey) parts.push('Alt')
    if (e.shiftKey) parts.push('Shift')
    if (e.metaKey) parts.push('Super')
    parts.push(main)
    if (parts.length < 2) return // require at least one modifier + the main key

    void captureHotkeyChord(parts.join('+'))
  }

  return (
    <div>
      <Section title="ホットキー" />
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 14 }}>
        検索オーバーレイを呼び出すキーの組み合わせです。
      </div>
      {capturing ? (
        <div
          onKeyDown={onCaptureKeyDown}
          tabIndex={0}
          autoFocus
          style={{
            padding: '8px 12px',
            borderRadius: 6,
            background: 'var(--accent-soft)',
            border: '1px solid var(--accent)',
            fontSize: 12,
            color: 'var(--accent)',
            outline: 'none',
          }}
        >
          キーを押してください… (Esc でキャンセル)
        </div>
      ) : (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div style={{ display: 'flex', gap: 4 }}>
            {hotkey.split('+').map((k, i) => (
              <Keycap key={i}>{k}</Keycap>
            ))}
          </div>
          <div style={{ flex: 1 }} />
          <div onClick={startHotkeyCapture} style={{ fontSize: 11.5, fontWeight: 700, color: 'var(--accent)', cursor: 'default' }}>
            変更
          </div>
        </div>
      )}
      {hotkeyError && (
        <div
          style={{
            marginTop: 10,
            padding: '8px 10px',
            borderRadius: 6,
            background: 'var(--danger-soft)',
            color: 'var(--danger)',
            fontSize: 11,
          }}
        >
          変更を適用できませんでした({hotkeyError})。以前のキー割り当てのままです。
        </div>
      )}
    </div>
  )
}
