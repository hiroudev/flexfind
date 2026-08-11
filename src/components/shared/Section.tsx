export default function Section({ title }: { title: string }) {
  return (
    <div
      style={{
        fontSize: 10.5,
        fontWeight: 700,
        letterSpacing: '.06em',
        color: 'var(--text-faint)',
        textTransform: 'uppercase',
        marginTop: 18,
        marginBottom: 6,
      }}
    >
      {title}
    </div>
  )
}
