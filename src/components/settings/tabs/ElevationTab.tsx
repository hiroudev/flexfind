import { useSettingsStore } from '../../../store/useSettingsStore'
import Section from '../../shared/Section'
import PrimaryBtn from '../../shared/PrimaryBtn'

export default function ElevationTab() {
  const isElevated = useSettingsStore(s => s.isElevated)
  const relaunchElevated = useSettingsStore(s => s.relaunchElevated)

  return (
    <div>
      <Section title="管理者権限" />
      {isElevated ? (
        <div
          style={{
            display: 'flex',
            gap: 8,
            padding: '10px 12px',
            background: 'var(--good-soft)',
            borderRadius: 7,
            marginBottom: 10,
          }}
        >
          <div style={{ color: 'var(--good)', fontWeight: 700, fontSize: 13 }}>✓</div>
          <div style={{ fontSize: 11.5, color: 'var(--good)', lineHeight: 1.5 }}>
            管理者権限で実行中です。すべてのフォルダにアクセスできます。
          </div>
        </div>
      ) : (
        <>
          <div
            style={{
              display: 'flex',
              gap: 8,
              padding: '10px 12px',
              background: 'var(--warn-soft)',
              borderRadius: 7,
              marginBottom: 10,
            }}
          >
            <div style={{ color: 'var(--warn)', fontWeight: 700, fontSize: 13 }}>!</div>
            <div style={{ fontSize: 11.5, color: 'var(--warn)', lineHeight: 1.5 }}>
              標準ユーザー権限で実行中です。アクセスが拒否されたフォルダは索引に含まれません。
            </div>
          </div>
          <PrimaryBtn onClick={() => void relaunchElevated()}>管理者として再起動</PrimaryBtn>
        </>
      )}
    </div>
  )
}
