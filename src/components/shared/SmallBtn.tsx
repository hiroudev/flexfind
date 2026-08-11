import { useState } from 'react'
import type { ReactNode } from 'react'

export default function SmallBtn({
  onClick,
  danger,
  children,
}: {
  onClick: () => void
  danger?: boolean
  children: ReactNode
}) {
  const [hover, setHover] = useState(false)
  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        height: 26,
        padding: '0 10px',
        borderRadius: 5,
        border: `1px solid ${danger ? 'var(--danger)' : 'var(--border-strong)'}`,
        fontSize: 11.5,
        cursor: 'default',
        display: 'flex',
        alignItems: 'center',
        color: danger ? 'var(--danger)' : hover ? 'var(--text)' : 'var(--text-muted)',
        background: hover ? 'var(--bg-hover)' : 'transparent',
      }}
    >
      {children}
    </div>
  )
}
