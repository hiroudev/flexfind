import { useEffect, useState } from 'react'
import { Checkbox } from 'flex-design/components'
import { useSettingsStore } from '../../../store/useSettingsStore'
import { getIndexMemory, trimWorkingSet } from '../../../fs/bridge'
import Section from '../../shared/Section'
import SmallBtn from '../../shared/SmallBtn'
import type { ArenaMemory, ReindexMode, RootIndexStatus, RootScanState } from '../../../types'

function mb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function badge(state: RootScanState, count: number): { label: string; bg: string; fg: string } {
  switch (state) {
    case 'done':
      return { label: '索引済み', bg: 'var(--good-soft)', fg: 'var(--good)' }
    case 'scanning':
      return { label: `スキャン中… ${count.toLocaleString()}件`, bg: 'var(--warn-soft)', fg: 'var(--warn)' }
    case 'skippedNoAccess':
      return { label: '権限なし(一部スキップ)', bg: 'var(--warn-soft)', fg: 'var(--warn)' }
    case 'offline':
      return { label: 'オフライン(前回の索引で検索可)', bg: 'var(--warn-soft)', fg: 'var(--warn)' }
    default:
      return { label: '待機中', bg: 'transparent', fg: 'var(--text-faint)' }
  }
}

export default function IndexingTab() {
  const scanRoots = useSettingsStore(s => s.settings.scanRoots)
  const rootStatus = useSettingsStore(s => s.rootStatus)
  const excludePaths = useSettingsStore(s => s.settings.excludePaths)
  const toggleRoot = useSettingsStore(s => s.toggleRoot)
  const addFolderRoot = useSettingsStore(s => s.addFolderRoot)
  const addUncRoot = useSettingsStore(s => s.addUncRoot)
  const removeRoot = useSettingsStore(s => s.removeRoot)
  const addExcludePath = useSettingsStore(s => s.addExcludePath)
  const removeExcludePath = useSettingsStore(s => s.removeExcludePath)
  const excludeHidden = useSettingsStore(s => s.settings.excludeHidden)
  const excludeSystem = useSettingsStore(s => s.settings.excludeSystem)
  const excludeShortcuts = useSettingsStore(s => s.settings.excludeShortcuts)
  const setExcludeHidden = useSettingsStore(s => s.setExcludeHidden)
  const setExcludeSystem = useSettingsStore(s => s.setExcludeSystem)
  const setExcludeShortcuts = useSettingsStore(s => s.setExcludeShortcuts)
  const reindexMode = useSettingsStore(s => s.settings.reindexMode)
  const reindexIntervalMinutes = useSettingsStore(s => s.settings.reindexIntervalMinutes)
  const reindexDailyTime = useSettingsStore(s => s.settings.reindexDailyTime)
  const setReindexMode = useSettingsStore(s => s.setReindexMode)
  const setReindexIntervalMinutes = useSettingsStore(s => s.setReindexIntervalMinutes)
  const setReindexDailyTime = useSettingsStore(s => s.setReindexDailyTime)

  const [unc, setUnc] = useState('')
  const [uncError, setUncError] = useState(false)
  const [memory, setMemory] = useState<[string, ArenaMemory][]>([])
  const [trimmed, setTrimmed] = useState(false)

  useEffect(() => {
    void getIndexMemory().then(setMemory)
  }, [rootStatus])
  // Local mirror so the field can be transiently empty while typing without
  // snapping back; commit to settings only when it parses to a valid minute.
  const [intervalText, setIntervalText] = useState(String(reindexIntervalMinutes))
  useEffect(() => {
    setIntervalText(String(reindexIntervalMinutes))
  }, [reindexIntervalMinutes])

  const statusByRoot = new Map<string, RootIndexStatus>(
    rootStatus.map(r => [r.root.toLowerCase(), r]),
  )

  async function commitUnc() {
    if (!unc.trim()) return
    const ok = await addUncRoot(unc)
    if (ok) {
      setUnc('')
      setUncError(false)
    } else {
      setUncError(true)
    }
  }

  return (
    <div>
      <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 4 }}>索引対象</div>
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 18 }}>
        索引を作成するドライブ・フォルダー・ネットワーク上の場所と、除外するパスを設定します。
      </div>

      {scanRoots.map(r => {
        const st = statusByRoot.get(r.path.toLowerCase())
        const b = st ? badge(st.state, st.scannedCount) : null
        const label = st?.label ?? (r.isDrive ? r.path : r.path.split(/[\\/]/).filter(Boolean).pop() ?? r.path)
        return (
          <div
            key={r.path}
            style={{ display: 'flex', alignItems: 'center', gap: 12, minHeight: 38, padding: '4px 0', borderBottom: '1px solid var(--border)' }}
          >
            <Checkbox checked={r.enabled} onChange={() => void toggleRoot(r.path)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 12.5, color: 'var(--text)', fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                {r.path}
              </div>
              {!r.isDrive && <div style={{ fontSize: 10.5, color: 'var(--text-faint)' }}>{label}</div>}
              {r.isDrive && <div style={{ fontSize: 10.5, color: 'var(--text-faint)' }}>{label}</div>}
            </div>
            {r.enabled && b && (
              <div style={{ fontSize: 11, fontWeight: 700, padding: '3px 9px', borderRadius: 9, background: b.bg, color: b.fg, flex: 'none' }}>
                {b.label}
              </div>
            )}
            {!r.enabled && (
              <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-faint)', flex: 'none' }}>無効</div>
            )}
            {r.enabled && st?.lastError && (
              <div
                title={st.lastError}
                style={{ fontSize: 10.5, color: 'var(--danger)', flex: 'none', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              >
                {st.lastError}
              </div>
            )}
            {!r.isDrive && (
              <div
                onClick={() => void removeRoot(r.path)}
                title="この索引対象を削除"
                style={{ color: 'var(--text-faint)', fontWeight: 700, cursor: 'default', flex: 'none', width: 18, textAlign: 'center' }}
              >
                ×
              </div>
            )}
          </div>
        )
      })}

      {scanRoots.some(r => (statusByRoot.get(r.path.toLowerCase())?.skippedDirs ?? 0) > 0) && (
        <div
          style={{
            marginTop: 10,
            padding: '10px 12px',
            borderRadius: 6,
            background: 'var(--warn-soft)',
            color: 'var(--warn)',
            fontSize: 11.5,
            lineHeight: 1.6,
          }}
        >
          <div style={{ fontWeight: 700, marginBottom: 4 }}>読み取れずスキップしたフォルダーがあります</div>
          <div style={{ color: 'var(--text-muted)' }}>
            これらの配下は検索結果に出ません。クラウドストレージ(Box/OneDrive等)のマウント先や権限のないフォルダーが該当します。索引対象に個別に追加すると走査できる場合があります。
          </div>
          {rootStatus
            .filter(s => s.skippedDirs > 0)
            .map(s => (
              <div key={s.root} style={{ marginTop: 6 }}>
                <span style={{ fontWeight: 600 }}>
                  {s.root}: {s.skippedDirs.toLocaleString()} 件
                </span>
                {s.skippedSamples.map(p => (
                  <div key={p} style={{ fontFamily: 'var(--mono)', color: 'var(--text-faint)', fontSize: 10.5 }}>
                    {p}
                  </div>
                ))}
              </div>
            ))}
        </div>
      )}

      <div style={{ display: 'flex', gap: 8, marginTop: 12, alignItems: 'center', flexWrap: 'wrap' }}>
        <div
          onClick={() => void addFolderRoot()}
          style={{ height: 28, display: 'inline-flex', alignItems: 'center', padding: '0 12px', border: '1px solid var(--border)', borderRadius: 6, fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', cursor: 'default' }}
        >
          + フォルダーを追加
        </div>
        <input
          value={unc}
          onChange={e => {
            setUnc(e.target.value)
            setUncError(false)
          }}
          onKeyDown={e => {
            if (e.key === 'Enter') void commitUnc()
          }}
          placeholder="\\server\share"
          className="fx-input"
          style={{ height: 28, width: 220, fontFamily: 'var(--mono)', fontSize: 12 }}
        />
        <div
          onClick={() => void commitUnc()}
          style={{ height: 28, display: 'inline-flex', alignItems: 'center', padding: '0 12px', border: '1px solid var(--border)', borderRadius: 6, fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', cursor: 'default' }}
        >
          + ネットワークパスを追加
        </div>
      </div>
      {uncError && (
        <div style={{ marginTop: 6, fontSize: 11, color: 'var(--danger)' }}>
          有効なネットワークパスを入力してください(例: \\server\share)。
        </div>
      )}

      <Section title="索引の自動更新" />
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
        索引を最新に保つための再スキャンのタイミングです。フォルダー監視ではなく全体の再スキャンのため、負荷を避けたい場合は夜間の時刻指定がおすすめです。
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <input
            type="radio"
            name="reindex-mode"
            checked={reindexMode === 'interval'}
            onChange={() => void setReindexMode('interval' as ReindexMode)}
          />
          <span>一定間隔ごと</span>
          <input
            type="number"
            min={1}
            className="fx-input"
            value={intervalText}
            disabled={reindexMode !== 'interval'}
            onChange={e => {
              setIntervalText(e.target.value)
              const n = parseInt(e.target.value, 10)
              if (Number.isFinite(n) && n >= 1) void setReindexIntervalMinutes(n)
            }}
            onBlur={() => setIntervalText(String(reindexIntervalMinutes))}
            style={{ width: 72, height: 26, fontSize: 12, textAlign: 'right' }}
          />
          <span style={{ color: 'var(--text-muted)' }}>分ごと</span>
        </label>

        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <input
            type="radio"
            name="reindex-mode"
            checked={reindexMode === 'daily'}
            onChange={() => void setReindexMode('daily' as ReindexMode)}
          />
          <span>毎日</span>
          <input
            type="time"
            className="fx-input"
            value={reindexDailyTime}
            disabled={reindexMode !== 'daily'}
            onChange={e => void setReindexDailyTime(e.target.value)}
            style={{ height: 26, fontSize: 12 }}
          />
          <span style={{ color: 'var(--text-muted)' }}>に実行(深夜など)</span>
        </label>

        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <input
            type="radio"
            name="reindex-mode"
            checked={reindexMode === 'manual'}
            onChange={() => void setReindexMode('manual' as ReindexMode)}
          />
          <span>自動更新しない(手動・起動時のみ)</span>
        </label>
      </div>

      {memory.length > 0 && (
        <>
          <Section title="索引のメモリ" />
          <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
            索引は常時メモリに保持されます。検索の速さを決めるのは総量ではなく「1回の検索が読む量」です。
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, marginBottom: 10 }}>
            {memory.map(([root, m]) => (
              <div
                key={root}
                style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12, color: 'var(--text-muted)' }}
              >
                <div style={{ width: 150, flex: 'none', color: 'var(--text)', fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {root}
                </div>
                <div style={{ flex: 1 }}>
                  {m.entries.toLocaleString()} 件 / フォルダー {m.dirs.toLocaleString()} 種
                </div>
                <div style={{ flex: 'none', fontFamily: 'var(--mono)' }}>{mb(m.totalBytes)}</div>
              </div>
            ))}
          </div>
          <div style={{ display: 'flex', gap: 18, fontSize: 12, marginBottom: 10 }}>
            <div>
              <span style={{ color: 'var(--text-muted)' }}>合計 </span>
              <span style={{ fontFamily: 'var(--mono)', fontWeight: 700, color: 'var(--text)' }}>
                {mb(memory.reduce((a, [, m]) => a + m.totalBytes, 0))}
              </span>
            </div>
            <div>
              <span style={{ color: 'var(--text-muted)' }}>1回の検索が読む量 </span>
              <span style={{ fontFamily: 'var(--mono)', fontWeight: 700, color: 'var(--accent)' }}>
                {mb(memory.reduce((a, [, m]) => a + m.scannedBytes, 0))}
              </span>
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <SmallBtn
              onClick={() => {
                void trimWorkingSet().then(() => setTrimmed(true))
              }}
            >
              メモリを解放して次の検索を計測
            </SmallBtn>
            <div style={{ fontSize: 11, color: trimmed ? 'var(--good)' : 'var(--text-faint)' }}>
              {trimmed
                ? '解放しました。次の検索がコールド状態の実測値になります。'
                : 'トレイ常駐で放置した直後と同じ「冷えた」状態を再現します。'}
            </div>
          </div>
        </>
      )}

      <Section title="結果フィルター" />
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12 }}>
        場所によらず、種類で検索結果から除外します。変更すると索引を再構築します。
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginBottom: 4 }}>
        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <Checkbox checked={excludeHidden} onChange={() => void setExcludeHidden(!excludeHidden)} />
          <span>隠しファイル/フォルダーを除外</span>
        </label>
        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <Checkbox checked={excludeSystem} onChange={() => void setExcludeSystem(!excludeSystem)} />
          <span>システムファイル/フォルダーを除外</span>
        </label>
        <label style={{ display: 'flex', alignItems: 'center', gap: 10, fontSize: 12.5, color: 'var(--text)', cursor: 'default' }}>
          <Checkbox checked={excludeShortcuts} onChange={() => void setExcludeShortcuts(!excludeShortcuts)} />
          <span>ショートカット(.lnk)を除外</span>
        </label>
      </div>

      <Section title="除外パス" />
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {excludePaths.map(p => (
          <div
            key={p}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              height: 28,
              padding: '0 10px',
              background: 'var(--bg-page)',
              border: '1px solid var(--border)',
              borderRadius: 6,
              fontSize: 12,
              color: 'var(--text-muted)',
              fontFamily: 'var(--mono)',
            }}
          >
            <div style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{p}</div>
            <div onClick={() => void removeExcludePath(p)} style={{ color: 'var(--text-faint)', fontWeight: 700, cursor: 'default' }}>
              ×
            </div>
          </div>
        ))}
      </div>
      <div
        onClick={() => void addExcludePath()}
        style={{
          marginTop: 10,
          height: 28,
          display: 'inline-flex',
          alignItems: 'center',
          padding: '0 12px',
          border: '1px solid var(--border)',
          borderRadius: 6,
          fontSize: 12,
          fontWeight: 600,
          color: 'var(--text-muted)',
          cursor: 'default',
        }}
      >
        + パスを追加
      </div>
    </div>
  )
}
