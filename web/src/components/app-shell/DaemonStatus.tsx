import { useEffect, useState } from 'react'
import { apiClient } from '../../api'
import { useI18n } from '../../i18n/I18nContext'
import { StatusDot, Tooltip, cn } from '../../ui'

/** Daemon uptime in "3d 4h" form; the sidebar footer never prints raw seconds. */
function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${minutes}m`
  return `${minutes}m`
}

/**
 * Daemon health, polled quietly. It is the one global state an operator needs from every page:
 * whether the thing the console administers is actually up. It lives in the sidebar footer now
 * that the app bar is gone — always visible, never in the way.
 */
export function DaemonStatus({ compact = false }: { compact?: boolean }) {
  const { t } = useI18n()
  const [online, setOnline] = useState<boolean | null>(null)
  const [uptime, setUptime] = useState<number | null>(null)

  useEffect(() => {
    let active = true
    const poll = async () => {
      try {
        const status = await apiClient.getStatus()
        if (!active) return
        setOnline(status.running)
        setUptime(status.uptime_secs)
      } catch {
        if (active) setOnline(false)
      }
    }
    void poll()
    const timer = window.setInterval(poll, 30_000)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [])

  const host = apiClient.isDesktop() ? 'desktop sidecar' : window.location.host
  const state = online === null ? host : online ? t('shell.daemonOnline') : t('shell.daemonOffline')
  const detail =
    online && uptime !== null ? `${host} · ${t('shell.daemonUptime', { uptime: formatUptime(uptime) })}` : host
  const label = `${state} · ${detail}`

  return (
    <Tooltip label={label}>
      <span
        className={cn(
          'flex h-7 items-center gap-1.5 rounded-control px-2 text-micro text-ink-3',
          compact && 'justify-center px-0'
        )}
      >
        <StatusDot
          tone={online === null ? 'idle' : online ? 'success' : 'error'}
          label={state}
        />
        {!compact && <span className="truncate">{state}</span>}
      </span>
    </Tooltip>
  )
}
