import { useRef, useState } from 'react'
import { useSettingsStore } from '../../../store/useSettingsStore'
import SmallBtn from '../../shared/SmallBtn'

/** Six-dot grip, the conventional "drag me" affordance. */
function GripIcon() {
  return (
    <svg width={10} height={16} viewBox="0 0 10 16" fill="currentColor" aria-hidden>
      {[3, 8, 13].map(y =>
        [2, 8].map(x => <circle key={`${x}-${y}`} cx={x} cy={y} r={1.3} />),
      )}
    </svg>
  )
}

function ScopeCard({
  id,
  dragging,
  isOver,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
}: {
  id: string
  dragging: boolean
  isOver: boolean
  onDragStart: () => void
  onDragOver: () => void
  onDragLeave: () => void
  onDrop: () => void
  onDragEnd: () => void
}) {
  const scope = useSettingsStore(s => s.settings.scopes.find(sc => sc.id === id)!)
  const renameScope = useSettingsStore(s => s.renameScope)
  const removeScope = useSettingsStore(s => s.removeScope)
  const addScopeIncludeFolder = useSettingsStore(s => s.addScopeIncludeFolder)
  const addScopeIncludeManual = useSettingsStore(s => s.addScopeIncludeManual)
  const removeScopeInclude = useSettingsStore(s => s.removeScopeInclude)

  const [manual, setManual] = useState('')
  const [manualError, setManualError] = useState(false)

  async function commitManual() {
    if (!manual.trim()) return
    const ok = await addScopeIncludeManual(id, manual)
    if (ok) {
      setManual('')
      setManualError(false)
    } else {
      setManualError(true)
    }
  }

  return (
    <div
      onDragOver={e => {
        e.preventDefault()
        e.dataTransfer.dropEffect = 'move'
        onDragOver()
      }}
      onDragLeave={onDragLeave}
      onDrop={e => {
        e.preventDefault()
        onDrop()
      }}
      style={{
        border: '1px solid ' + (isOver ? 'var(--accent)' : 'var(--border)'),
        borderRadius: 8,
        padding: 14,
        marginBottom: 12,
        opacity: dragging ? 0.5 : 1,
        background: isOver ? 'var(--accent-soft)' : 'transparent',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
        {/* Only the grip is draggable: making the whole card draggable would
            steal text selection and caret placement from the name field. */}
        <div
          draggable
          onDragStart={e => {
            e.dataTransfer.effectAllowed = 'move'
            e.dataTransfer.setData('text/plain', id)
            onDragStart()
          }}
          onDragEnd={onDragEnd}
          title="ドラッグして並べ替え"
          style={{
            flex: 'none',
            display: 'flex',
            alignItems: 'center',
            padding: '0 2px',
            color: 'var(--text-faint)',
            cursor: 'grab',
          }}
        >
          <GripIcon />
        </div>
        <input
          value={scope.name}
          onChange={e => void renameScope(id, e.target.value)}
          className="fx-input"
          style={{ height: 28, flex: 1, fontWeight: 600 }}
        />
        <SmallBtn onClick={() => void removeScope(id)} danger>
          削除
        </SmallBtn>
      </div>

      <div style={{ fontSize: 11, color: 'var(--text-faint)', marginBottom: 6 }}>この検索対象に含めるパス</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {scope.includePaths.length === 0 && (
          <div style={{ fontSize: 11.5, color: 'var(--text-faint)' }}>
            パス未設定。1つ以上追加すると、この検索対象での検索がそのパス配下に絞り込まれます。
          </div>
        )}
        {scope.includePaths.map(p => (
          <div
            key={p}
            style={{ display: 'flex', alignItems: 'center', gap: 10, height: 28, padding: '0 10px', background: 'var(--bg-page)', border: '1px solid var(--border)', borderRadius: 6, fontSize: 12, color: 'var(--text-muted)', fontFamily: 'var(--mono)' }}
          >
            <div style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{p}</div>
            <div onClick={() => void removeScopeInclude(id, p)} style={{ color: 'var(--text-faint)', fontWeight: 700, cursor: 'default' }}>
              ×
            </div>
          </div>
        ))}
      </div>

      <div style={{ display: 'flex', gap: 8, marginTop: 10, alignItems: 'center', flexWrap: 'wrap' }}>
        <SmallBtn onClick={() => void addScopeIncludeFolder(id)}>+ フォルダーを追加</SmallBtn>
        <input
          value={manual}
          onChange={e => {
            setManual(e.target.value)
            setManualError(false)
          }}
          onKeyDown={e => {
            if (e.key === 'Enter') void commitManual()
          }}
          placeholder="C:\Projects または \\server\share"
          className="fx-input"
          style={{ height: 26, width: 240, fontFamily: 'var(--mono)', fontSize: 12 }}
        />
        <SmallBtn onClick={() => void commitManual()}>+ パスを追加</SmallBtn>
      </div>
      {manualError && (
        <div style={{ marginTop: 6, fontSize: 11, color: 'var(--danger)' }}>
          有効なパスを入力してください(例: C:\Projects または \\server\share)。
        </div>
      )}
    </div>
  )
}

export default function ScopesTab() {
  const scopes = useSettingsStore(s => s.settings.scopes)
  const addScope = useSettingsStore(s => s.addScope)
  const reorderScopes = useSettingsStore(s => s.reorderScopes)

  // Held in a ref rather than state: the drag source never needs to trigger
  // a render on its own, and a ref survives the drag without re-running the
  // dragstart handler.
  const dragIdRef = useRef<string | null>(null)
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [overId, setOverId] = useState<string | null>(null)

  return (
    <div>
      <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 4 }}>検索対象</div>
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 18 }}>
        よく使う検索範囲を「検索対象」として保存できます。メイン画面のドロップダウンでワンタッチ切替。
        「全体」(すべての索引を検索)は常に利用でき、設定不要です。
        左端の <span style={{ fontFamily: 'var(--mono)' }}>⠿</span> をドラッグすると並び順を入れ替えられ、その順序がドロップダウンにも反映されます。
      </div>

      {scopes.map(s => (
        <ScopeCard
          key={s.id}
          id={s.id}
          dragging={draggingId === s.id}
          isOver={overId === s.id && draggingId !== null && draggingId !== s.id}
          onDragStart={() => {
            dragIdRef.current = s.id
            setDraggingId(s.id)
          }}
          onDragOver={() => setOverId(prev => (prev === s.id ? prev : s.id))}
          onDragLeave={() => setOverId(prev => (prev === s.id ? null : prev))}
          onDrop={() => {
            const src = dragIdRef.current
            if (src) void reorderScopes(src, s.id)
            dragIdRef.current = null
            setDraggingId(null)
            setOverId(null)
          }}
          onDragEnd={() => {
            dragIdRef.current = null
            setDraggingId(null)
            setOverId(null)
          }}
        />
      ))}

      <div
        onClick={() => void addScope('新しい検索対象')}
        style={{ height: 30, display: 'inline-flex', alignItems: 'center', padding: '0 14px', border: '1px solid var(--border)', borderRadius: 6, fontSize: 12.5, fontWeight: 600, color: 'var(--text-muted)', cursor: 'default' }}
      >
        + 検索対象を追加
      </div>
    </div>
  )
}
