import type { RefObject } from 'react'
import { useSearchStore } from '../../store/useSearchStore'
import Select from '../shared/Select'

interface Props {
  inputRef: RefObject<HTMLInputElement>
}

const ALL_SCOPE = '__all__'

export default function SearchBar({ inputRef }: Props) {
  const query = useSearchStore(s => s.activeTab().query)
  const scopeId = useSearchStore(s => s.activeTab().scopeId)
  const scopes = useSearchStore(s => s.scopes)
  const setQuery = useSearchStore(s => s.setQuery)
  const setScope = useSearchStore(s => s.setScope)
  const helpOpen = useSearchStore(s => s.helpOpen)
  const toggleHelp = useSearchStore(s => s.toggleHelp)

  const scopeOptions = [
    { value: ALL_SCOPE, label: '全体' },
    ...scopes.map(s => ({ value: s.id, label: s.name })),
  ]

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px', borderBottom: '1px solid var(--border)' }}>
      <svg width={16} height={16} viewBox="0 0 18 18" fill="none" style={{ flex: 'none' }}>
        <circle cx={8} cy={8} r={6} stroke="var(--text-faint)" strokeWidth={1.6} />
        <line x1={12.4} y1={12.4} x2={16} y2={16} stroke="var(--text-faint)" strokeWidth={1.6} strokeLinecap="round" />
      </svg>
      <input
        ref={inputRef}
        value={query}
        onChange={e => setQuery(e.target.value)}
        placeholder="ファイル名を入力..."
        spellCheck={false}
        style={{
          flex: 1,
          minWidth: 0,
          border: 'none',
          outline: 'none',
          background: 'transparent',
          fontSize: 14,
          fontWeight: 500,
          color: 'var(--text)',
          fontFamily: 'var(--font)',
        }}
      />
      <Select
        value={scopeId ?? ALL_SCOPE}
        onChange={v => setScope(v === ALL_SCOPE ? null : v)}
        options={scopeOptions}
      />
      <div
        onClick={toggleHelp}
        title="クエリ構文ヘルプ"
        style={{
          width: 26,
          height: 26,
          borderRadius: 6,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: 13,
          fontWeight: 700,
          color: helpOpen ? 'var(--accent)' : 'var(--text-faint)',
          border: `1px solid ${helpOpen ? 'var(--accent)' : 'var(--border)'}`,
          cursor: 'default',
          flex: 'none',
        }}
      >
        ?
      </div>
    </div>
  )
}
