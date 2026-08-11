import { useSearchStore } from '../../store/useSearchStore'

const ALL_SCOPE_NAME = '全体'

/** Always-visible bottom status bar (Everything-style): hit/total counts,
 * active-scope name, and an index-status pill. */
export default function StatusBar() {
  const tab = useSearchStore(s => s.activeTab())
  const totalIndexed = useSearchStore(s => s.totalIndexed)
  const indexStatus = useSearchStore(s => s.indexStatus)
  const initialBuildDone = useSearchStore(s => s.initialBuildDone)
  const scopes = useSearchStore(s => s.scopes)

  const scanning = indexStatus.filter(d => d.state === 'scanning')
  const anySkipped = indexStatus.some(d => d.state === 'skippedNoAccess')
  const anyOffline = indexStatus.some(d => d.state === 'offline')

  let label = '索引済み'
  let color = 'var(--good)'
  if (scanning.length > 0) {
    const count = scanning.reduce((n, d) => n + d.scannedCount, 0)
    const verb = initialBuildDone ? '索引を更新中…' : '初回スキャン中…'
    label = `${verb} ${count.toLocaleString()} 件`
    color = 'var(--warn)'
  } else if (anyOffline) {
    label = '一部オフライン(前回の索引で検索可)'
    color = 'var(--warn)'
  } else if (anySkipped) {
    label = '簡易索引(一部フォルダ未索引)'
    color = 'var(--warn)'
  }

  const scopeName = tab.scopeId ? scopes.find(s => s.id === tab.scopeId)?.name ?? ALL_SCOPE_NAME : ALL_SCOPE_NAME

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 12,
        padding: '5px 14px',
        minHeight: 26,
        background: 'var(--bg-sunken)',
        borderTop: '1px solid var(--border)',
        fontSize: 11,
        color: 'var(--text-faint)',
      }}
    >
      <div style={{ display: 'flex', gap: 14 }}>
        <span>
          {tab.totalMatched.toLocaleString()} 件 / {totalIndexed.toLocaleString()} 件中
        </span>
        <span>
          検索対象: <span style={{ color: 'var(--text-muted)', fontWeight: 600 }}>{scopeName}</span>
        </span>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 5, fontWeight: 600, color }}>
        <span style={{ width: 6, height: 6, borderRadius: '50%', background: color, display: 'inline-block' }} />
        {label}
      </div>
    </div>
  )
}
