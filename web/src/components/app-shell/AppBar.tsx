import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { LogOut, Search, User } from 'lucide-react'
import { apiClient } from '../../api'
import { useI18n } from '../../i18n/I18nContext'
import { IconButton, Kbd, MenuItem, MenuLabel, Popover, StatusDot, Tooltip } from '../../ui'
import { LanguageSwitcher } from '../../i18n'
import { useShell } from './ShellContext'
import { ThemeMenu } from './ThemeMenu'

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
 * whether the thing the console administers is actually up.
 */
function DaemonStatus() {
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
  const label =
    online === null
      ? host
      : `${online ? t('shell.daemonOnline') : t('shell.daemonOffline')} · ${host}${
          online && uptime !== null ? ` · ${t('shell.daemonUptime', { uptime: formatUptime(uptime) })}` : ''
        }`

  return (
    <Tooltip label={label}>
      <span className="inline-flex h-8 items-center gap-1.5 px-1.5">
        <StatusDot
          tone={online === null ? 'idle' : online ? 'success' : 'error'}
          label={online === null ? host : online ? t('shell.daemonOnline') : t('shell.daemonOffline')}
        />
      </span>
    </Tooltip>
  )
}

function AccountMenu() {
  const { t } = useI18n()
  const navigate = useNavigate()

  const logout = () => {
    apiClient.clearAuth()
    navigate('/login')
  }

  return (
    <Popover label={t('shell.account')} icon={<User className="h-4 w-4" />}>
      <MenuLabel>{t('shell.signedInAs', { name: 'web_admin' })}</MenuLabel>
      <MenuItem icon={<LogOut className="h-3.5 w-3.5" />} onSelect={logout}>
        {t('common.logout')}
      </MenuItem>
    </Popover>
  )
}

/** 52px application bar: identity, search, and the four global controls. Nothing else fits. */
export function AppBar() {
  const { t } = useI18n()
  const { setPaletteOpen } = useShell()

  return (
    <header className="flex h-[52px] shrink-0 items-center gap-2 border-b border-line bg-surface px-3">
      <Link to="/" className="flex items-center gap-2 rounded-control px-1 py-1" aria-label={t('app.title')}>
        <span
          aria-hidden="true"
          className="grid h-6 w-6 place-items-center rounded-[7px] bg-ink font-mono text-[12px] font-semibold leading-none text-background"
        >
          S
        </span>
        <span className="text-body font-semibold tracking-tight text-ink">Synbot</span>
      </Link>

      <span aria-hidden="true" className="mx-1 h-4 w-px bg-line" />

      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="group flex h-8 w-[240px] items-center gap-2 rounded-control border border-line bg-surface-2 px-2 text-left text-small text-ink-3 transition-colors duration-fast hover:border-line-strong hover:text-ink-2"
      >
        <Search className="h-3.5 w-3.5" aria-hidden="true" />
        <span className="flex-1 truncate">{t('shell.search')}</span>
        <Kbd>⌘K</Kbd>
      </button>

      <div className="ml-auto flex items-center gap-0.5">
        <DaemonStatus />
        <ThemeMenu />
        <LanguageSwitcher />
        <AccountMenu />
        <IconButton
          label={t('shell.search')}
          className="md:hidden"
          onClick={() => setPaletteOpen(true)}
        >
          <Search className="h-4 w-4" />
        </IconButton>
      </div>
    </header>
  )
}
