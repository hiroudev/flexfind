import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { showSettings, winClose, winMinimize, winToggleMaximize } from '../fs/bridge'

function BarBtn({
  title,
  onClick,
  danger,
  children,
}: {
  title: string
  onClick: () => void
  danger?: boolean
  children: ReactNode
}) {
  const [hover, setHover] = useState(false)
  return (
    <div
      title={title}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 40,
        height: 34,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        cursor: 'default',
        fontSize: danger ? 12 : 13,
        background: hover ? (danger ? 'var(--danger)' : 'var(--bg-hover)') : 'transparent',
        color: hover && danger ? '#fff' : 'var(--text-faint)',
      }}
    >
      {children}
    </div>
  )
}

function GearIcon() {
  // Standard toothed cog with a center hole (a single filled gear outline
  // via even-odd fill), not radial lines — the earlier version read as a
  // sun/light rather than a settings gear.
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="currentColor">
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.03 7.03 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.02-.397-1.11-.94l-.213-1.28c-.062-.375-.312-.687-.644-.87a6.52 6.52 0 0 1-.22-.128c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.93 6.93 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28ZM12 15.75a3.75 3.75 0 1 0 0-7.5 3.75 3.75 0 0 0 0 7.5Z"
      />
    </svg>
  )
}

interface Props {
  title: string
  /** Optional content rendered between the title and window buttons (e.g. a tab bar). */
  children?: ReactNode
  /** When true, show a settings (gear) button that opens the settings window. */
  showSettingsButton?: boolean
  /** When true, pressing Esc closes (hides) this window, and a visible
   * "Esc で閉じる" hint is shown in the titlebar. Intended for secondary /
   * dialog-like windows (e.g. the settings window). Not for the main window,
   * whose Esc is staged (clear query, etc.). */
  escToClose?: boolean
}

/** Small keycap, used for the Esc-to-close hint. */
function Keycap({ children }: { children: ReactNode }) {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        minWidth: 26,
        height: 18,
        padding: '0 5px',
        borderRadius: 4,
        border: '1px solid var(--border-strong)',
        background: 'var(--bg-page)',
        color: 'var(--text-muted)',
        fontSize: 10.5,
        fontWeight: 700,
      }}
    >
      {children}
    </span>
  )
}

/**
 * Custom 34px titlebar shared by the main and settings windows (decorations
 * are disabled). Uses Tauri's native `data-tauri-drag-region`, which gives
 * standard window-manager behavior for free: drag to move, double-click to
 * maximize/restore, and edge-snapping — no manual mousedown/timer handling.
 * Interactive children (tabs, buttons) deliberately omit the attribute so
 * they stay clickable. Close hides to tray rather than quitting (both
 * windows are tray-resident).
 *
 * This is the FlexFind-local shared titlebar; the `escToClose` hint + handler
 * are designed to become a Flex-family standard for dialog-like windows once
 * this component is promoted into the shared `flex-design` package.
 */
export default function AppTitleBar({ title, children, showSettingsButton, escToClose }: Props) {
  useEffect(() => {
    if (!escToClose) return
    function onKeyDown(e: KeyboardEvent) {
      // A deeper handler (e.g. hotkey capture) that wants Esc for itself
      // calls stopPropagation, so this only fires for a "bare" Esc.
      if (e.key === 'Escape') {
        void winClose()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [escToClose])

  return (
    <div
      data-tauri-drag-region
      style={{
        height: 34,
        minHeight: 34,
        display: 'flex',
        alignItems: 'center',
        padding: '0 2px 0 12px',
        background: 'var(--bg-titlebar)',
        color: 'var(--text)',
        gap: 10,
        userSelect: 'none',
      }}
    >
      {title && (
        <div data-tauri-drag-region style={{ fontSize: 12.5, fontWeight: 700, flex: 'none' }}>
          {title}
        </div>
      )}
      {children ? (
        <div style={{ flex: 1, minWidth: 0, display: 'flex', alignItems: 'center' }}>{children}</div>
      ) : (
        <div data-tauri-drag-region style={{ flex: 1 }} />
      )}
      {escToClose && (
        <div
          data-tauri-drag-region
          style={{ display: 'flex', alignItems: 'center', gap: 6, flex: 'none', marginRight: 4, color: 'var(--text-faint)', fontSize: 11 }}
        >
          <Keycap>Esc</Keycap>
          <span>で閉じる</span>
        </div>
      )}
      <div style={{ display: 'flex', flex: 'none' }}>
        {showSettingsButton && (
          <BarBtn title="設定" onClick={() => void showSettings()}>
            <GearIcon />
          </BarBtn>
        )}
        <BarBtn title="最小化" onClick={() => void winMinimize()}>
          ─
        </BarBtn>
        <BarBtn title="最大化" onClick={() => void winToggleMaximize()}>
          ▢
        </BarBtn>
        <BarBtn title={escToClose ? '閉じる (Esc / トレイに常駐)' : '閉じる (トレイに常駐)'} onClick={() => void winClose()} danger>
          ✕
        </BarBtn>
      </div>
    </div>
  )
}
