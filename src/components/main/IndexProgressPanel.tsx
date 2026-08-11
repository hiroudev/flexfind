import { useEffect, useState } from 'react'
import { useSearchStore } from '../../store/useSearchStore'
import { isElevated as fetchIsElevated } from '../../fs/bridge'
import type { RootScanState } from '../../types'

const STATE_LABEL: Record<RootScanState, string> = {
  pending: '待機中',
  scanning: 'スキャン中…',
  done: '完了',
  skippedNoAccess: '権限なしのためスキップ',
  offline: 'オフライン(前回の索引で検索可)',
}

function stateColor(state: RootScanState): string {
  if (state === 'done') return 'var(--good)'
  if (state === 'scanning') return 'var(--accent)'
  return 'var(--text-faint)'
}

export default function IndexProgressPanel() {
  const indexStatus = useSearchStore(s => s.indexStatus)
  const initialBuildDone = useSearchStore(s => s.initialBuildDone)
  const [elevated, setElevated] = useState(true)

  useEffect(() => {
    fetchIsElevated().then(setElevated)
  }, [])

  // Only the *initial* build gets this big inline panel — the periodic
  // freshness re-walk (every 30 min) resets roots to pending/scanning too,
  // but re-showing this for minutes on every refresh would be disruptive;
  // the status bar's pill covers that case instead.
  if (initialBuildDone) return null

  const relevant = indexStatus
  const inProgress = relevant.filter(d => d.state === 'pending' || d.state === 'scanning')
  if (inProgress.length === 0) return null

  return (
    <div style={{ padding: '14px 18px', borderBottom: '1px solid var(--border)' }}>
      <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 2 }}>初回インデックスを作成しています…</div>
      <div style={{ fontSize: 11.5, color: 'var(--text-muted)', marginBottom: 12 }}>
        完了までしばらくお待ちください。作成中も検索は可能です。
      </div>

      {!elevated && (
        <div
          style={{
            display: 'flex',
            gap: 8,
            padding: '8px 10px',
            background: 'var(--warn-soft)',
            borderRadius: 7,
            marginBottom: 12,
          }}
        >
          <div style={{ color: 'var(--warn)', fontWeight: 700 }}>!</div>
          <div style={{ fontSize: 11, color: 'var(--warn)', lineHeight: 1.5 }}>
            管理者権限なしで起動されています。アクセスが拒否されたフォルダは索引から除外されます。
          </div>
        </div>
      )}

      {relevant.map(d => (
        <div key={d.root} style={{ marginBottom: 10 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11.5, marginBottom: 4 }}>
            <div style={{ fontWeight: 700 }}>
              {d.root} <span style={{ fontWeight: 500, color: 'var(--text-muted)' }}>{d.label}</span>
            </div>
            <div style={{ color: stateColor(d.state), fontWeight: 600 }}>
              {d.state === 'scanning' ? `スキャン中… ${d.scannedCount.toLocaleString()} 件` : STATE_LABEL[d.state]}
            </div>
          </div>
          <div style={{ height: 6, borderRadius: 3, background: 'var(--bg-sunken)', overflow: 'hidden' }}>
            {/* No fake percentage: only a "done" drive gets a filled bar.
                Scanning/pending rely on the text counter + color above —
                there's no reliable total-file-count denominator without a
                slow pre-pass, so a proportional fill would just be invented. */}
            <div
              style={{
                height: '100%',
                width: d.state === 'done' ? '100%' : '0%',
                background: stateColor(d.state),
                borderRadius: 3,
              }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}
