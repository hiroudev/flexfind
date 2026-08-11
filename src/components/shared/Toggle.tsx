export default function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <div
      onClick={() => onChange(!value)}
      style={{
        width: 34,
        height: 18,
        borderRadius: 9,
        background: value ? 'var(--accent)' : 'var(--border-strong)',
        position: 'relative',
        cursor: 'default',
        transition: 'background .15s',
        flex: '0 0 34px',
      }}
    >
      <span
        style={{
          position: 'absolute',
          top: 2,
          left: value ? 18 : 2,
          width: 14,
          height: 14,
          borderRadius: '50%',
          background: '#fff',
          transition: 'left .15s',
          boxShadow: '0 1px 3px rgba(0,0,0,.3)',
        }}
      />
    </div>
  )
}
