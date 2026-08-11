export default function Select<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (v: T) => void
  options: { value: T; label: string }[]
}) {
  return (
    <select
      value={value}
      onChange={e => onChange(e.target.value as T)}
      style={{
        height: 28,
        padding: '0 8px',
        borderRadius: 5,
        border: '1px solid var(--border-strong)',
        background: 'var(--bg-page)',
        color: 'var(--text)',
        fontFamily: 'var(--font)',
        fontSize: 12.5,
        outline: 'none',
        cursor: 'default',
      }}
    >
      {options.map(o => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}
