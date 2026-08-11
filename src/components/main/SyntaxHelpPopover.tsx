import { useSearchStore } from '../../store/useSearchStore'

// Kept in sync with the grammar implemented in src-tauri/src/index/query.rs.
const SYNTAX_ROWS = [
  { syntax: 'ext:png', desc: '拡張子で絞り込み' },
  { syntax: 'path:C:\\Projects', desc: 'パスに含まれる文字列で絞り込み' },
  { syntax: 'size:>10mb', desc: 'サイズで絞り込み(> < >= <= = に対応)' },
  { syntax: 'dm:today', desc: '更新日で絞り込み(today/yesterday/thisweek/YYYY-MM-DD)' },
  { syntax: '"完全一致"', desc: 'ダブルクォートでフレーズをまとめて1語として扱う(厳密な完全一致検索ではありません)' },
  { syntax: '!node_modules', desc: '!で除外(ファイル名・パスのどちらかに一致すれば除外)' },
]

export default function SyntaxHelpPopover() {
  const helpOpen = useSearchStore(s => s.helpOpen)
  if (!helpOpen) return null

  return (
    <div
      style={{
        position: 'absolute',
        top: 82,
        right: 12,
        width: 300,
        background: 'var(--bg-panel)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius)',
        boxShadow: '0 16px 40px -16px var(--shadow)',
        padding: '14px 16px',
        zIndex: 20,
      }}
    >
      <div
        style={{
          fontSize: 11.5,
          fontWeight: 700,
          color: 'var(--text-faint)',
          textTransform: 'uppercase',
          letterSpacing: '.04em',
          marginBottom: 10,
        }}
      >
        クエリ構文
      </div>
      {SYNTAX_ROWS.map(row => (
        <div key={row.syntax} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '5px 0' }}>
          <div
            style={{
              fontFamily: 'var(--mono)',
              fontSize: 11.5,
              fontWeight: 700,
              color: 'var(--accent)',
              background: 'var(--bg-sunken)',
              padding: '2px 7px',
              borderRadius: 5,
              whiteSpace: 'nowrap',
              flex: 'none',
            }}
          >
            {row.syntax}
          </div>
          <div style={{ fontSize: 11.5, color: 'var(--text-muted)' }}>{row.desc}</div>
        </div>
      ))}
    </div>
  )
}
