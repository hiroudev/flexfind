import { useState } from 'react'
import type { ReactNode } from 'react'

export default function PrimaryBtn({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  const [hover, setHover] = useState(false)
  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        height: 30,
        padding: '0 16px',
        borderRadius: 6,
        cursor: 'default',
        fontSize: 12.5,
        fontWeight: 550,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: 'var(--accent-contrast)',
        background: hover ? 'var(--accent-hover)' : 'var(--accent)',
      }}
    >
      {children}
    </div>
  )
}
