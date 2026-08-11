import { useState } from 'react'

export default function NavItem({
  label,
  active,
  onClick,
}: {
  label: string
  active: boolean
  onClick: () => void
}) {
  const [hover, setHover] = useState(false)
  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: 'flex',
        alignItems: 'center',
        height: 34,
        padding: '0 16px',
        margin: '0 8px 2px',
        borderRadius: 7,
        cursor: 'default',
        fontSize: 12.5,
        fontWeight: active ? 700 : 500,
        color: active ? 'var(--text)' : hover ? 'var(--text)' : 'var(--text-muted)',
        background: active ? 'var(--bg-active)' : hover ? 'var(--bg-hover)' : 'transparent',
      }}
    >
      {label}
    </div>
  )
}
