import { useEffect, useState } from 'react'
import { peekIcon, shellIcon } from '../../fs/bridge'

interface Props {
  name: string
  folder: boolean
  ext: string
}

/** Fallback CSS-shape icon used until the real shell icon loads (or on the
 * non-Tauri web preview where no shell icon is available). */
function FallbackIcon({ folder, ext }: { folder: boolean; ext: string }) {
  if (folder) {
    return (
      <div
        style={{
          width: 15,
          height: 11,
          clipPath: 'polygon(0% 15%,42% 15%,52% 32%,100% 32%,100% 92%,0% 92%)',
          background: 'var(--accent-soft)',
          border: '1px solid var(--accent)',
        }}
      />
    )
  }
  const isExe = ext === 'exe'
  return (
    <div
      style={{
        width: 14,
        height: 16,
        clipPath: 'polygon(0 0,68% 0,100% 32%,100% 100%,0 100%)',
        background: isExe ? 'var(--accent-soft)' : 'var(--bg-sunken)',
        border: `1px solid ${isExe ? 'var(--accent)' : 'var(--border-strong)'}`,
      }}
    />
  )
}

/**
 * Shows the real Windows shell icon for the file type (resolved by
 * extension, cached per type in the bridge). Renders the cached icon
 * synchronously when available (no flash on scroll, since icons are cached
 * by extension not by file), otherwise fetches it and shows the CSS
 * fallback in the meantime.
 */
export default function FileIcon({ name, folder, ext }: Props) {
  const [url, setUrl] = useState<string | null>(() => peekIcon(name, folder))

  useEffect(() => {
    const cached = peekIcon(name, folder)
    if (cached) {
      setUrl(cached)
      return
    }
    let alive = true
    void shellIcon(name, folder).then(u => {
      if (alive && u) setUrl(u)
    })
    return () => {
      alive = false
    }
  }, [name, folder])

  if (url) {
    return <img src={url} alt="" width={16} height={16} draggable={false} style={{ objectFit: 'contain' }} />
  }
  return <FallbackIcon folder={folder} ext={ext} />
}
