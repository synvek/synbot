import { useCallback, useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Activity, ArrowRight, Cable, Cpu, RefreshCw, ShieldCheck } from 'lucide-react'
import { apiClient } from '../api/client'
import type { Project, SecurityOverview, SessionSummary, SystemStatus } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatClock, formatShortDateTime, formatUptime } from '../utils/datetime'
import {
  Button,
  DataTable,
  PageHeader,
  SectionHeader,
  Skeleton,
  StatTile,
  StatusDot,
  type Column,
} from '../ui'

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {}
}

/**
 * The overview answers "is this thing working, and what do I fix first" before it shows counts.
 * Four state tiles, then the two lists that lead somewhere: sessions and setup.
 */
export default function Overview() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const [status, setStatus] = useState<SystemStatus | null>(null)
  const [config, setConfig] = useState<Record<string, unknown> | null>(null)
  const [security, setSecurity] = useState<SecurityOverview | null>(null)
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const fetchStatus = useCallback(async () => {
    try {
      const data = await apiClient.getStatus()
      setStatus(data)
      setError(null)
    } catch {
      setError(t('overview.failedToFetch', 'Failed to fetch system status'))
    } finally {
      setLoading(false)
    }
  }, [t])

  useEffect(() => {
    void fetchStatus()
    const interval = window.setInterval(() => void fetchStatus(), 5000)
    return () => window.clearInterval(interval)
  }, [fetchStatus])

  // Everything below the tiles is slow-moving state: read it once, not every five seconds.
  useEffect(() => {
    let active = true
    void Promise.allSettled([
      apiClient.getConfig(),
      apiClient.securityOverview(),
      apiClient.getSessions(1, 5),
      apiClient.getProjects(),
    ]).then(([configResult, securityResult, sessionsResult, projectsResult]) => {
      if (!active) return
      if (configResult.status === 'fulfilled') setConfig(configResult.value.config)
      if (securityResult.status === 'fulfilled') setSecurity(securityResult.value)
      if (sessionsResult.status === 'fulfilled') setSessions(sessionsResult.value.items)
      if (projectsResult.status === 'fulfilled') setProjects(projectsResult.value)
    })
    return () => {
      active = false
    }
  }, [])

  const enabledChannels = status?.channels.filter((channel) => channel.enabled) ?? []
  const connectedChannels = enabledChannels.filter((channel) => channel.status === 'connected')
  const failedChannels = enabledChannels.filter((channel) => channel.status === 'failed')

  const mainAgent = asRecord(config?.mainAgent)
  const model = typeof mainAgent.model === 'string' ? mainAgent.model : ''
  const provider = typeof mainAgent.provider === 'string' ? mainAgent.provider : ''
  const sandboxOn = Boolean(security?.app_sandbox || security?.tool_sandbox)
  const sandboxHint = security?.app_sandbox?.platform ?? security?.tool_sandbox?.backend ?? null

  const sessionColumns: Array<Column<SessionSummary>> = useMemo(
    () => [
      {
        key: 'channel',
        header: t('sessions.channel', 'Channel'),
        width: '20%',
        render: (row) => <span className="font-mono text-micro text-ink-2">{row.channel}</span>,
      },
      {
        key: 'identifier',
        header: t('sessions.sessionList', 'Session'),
        render: (row) => (
          <span className="truncate font-mono text-micro text-ink">
            {row.identifier || t('sessions.anonymous', 'Anonymous')}
          </span>
        ),
      },
      {
        key: 'messages',
        header: t('sessions.messages', 'messages'),
        align: 'right' as const,
        width: '96px',
        render: (row) => <span className="tabular text-micro">{row.message_count}</span>,
      },
      {
        key: 'updated',
        header: t('sessions.updated', 'Updated'),
        align: 'right' as const,
        width: '130px',
        render: (row) => (
          <span className="tabular whitespace-nowrap text-micro text-ink-2">
            {formatShortDateTime(row.updated_at)}
          </span>
        ),
      },
    ],
    [t]
  )

  const checklist = [
    {
      id: 'model',
      label: t('overview.setupModel', 'Set a provider and model'),
      done: Boolean(provider && model),
      to: '/config',
    },
    {
      id: 'channels',
      label: t('overview.setupChannels', 'Enable a channel'),
      done: enabledChannels.length > 0,
      to: '/channels',
    },
    {
      id: 'workspace',
      label: t('overview.setupWorkspace', 'Open a workspace folder'),
      done: projects.length > 0,
      to: '/chat',
    },
    {
      id: 'sandbox',
      label: t('overview.setupSandbox', 'Turn on a sandbox'),
      done: sandboxOn,
      to: '/security',
    },
  ]
  const remaining = checklist.filter((item) => !item.done).length

  if (loading && !status) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={t('overview.title', 'Overview')}
        description={
          status
            ? `${t('overview.uptime', 'Uptime')} ${formatUptime(status.uptime_secs)} · ${t('overview.lastUpdated', 'Last updated')} ${formatClock(Date.now())}`
            : undefined
        }
        actions={
          <Button variant="ghost" size="sm" icon={<RefreshCw className="h-3.5 w-3.5" />} onClick={() => void fetchStatus()}>
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && (
        <div className="rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>
      )}

      <div className="grid gap-px overflow-hidden rounded-panel border border-line bg-line sm:grid-cols-2 lg:grid-cols-4">
        <StatTile
          className="bg-surface"
          label={t('overview.daemon', 'Daemon')}
          icon={<Activity className="h-3 w-3" aria-hidden="true" />}
          value={status?.running ? t('common.running', 'Running') : t('common.stopped', 'Stopped')}
          tone={status?.running ? 'success' : 'danger'}
          hint={status ? `${window.location.host} · ${formatUptime(status.uptime_secs)}` : undefined}
          aside={<StatusDot tone={status?.running ? 'success' : 'error'} />}
        />
        <StatTile
          className="bg-surface"
          label={t('overview.model', 'Model')}
          icon={<Cpu className="h-3 w-3" aria-hidden="true" />}
          value={model || t('overview.modelUnset', 'Not set')}
          tone={model ? 'neutral' : 'warning'}
          hint={model ? provider || undefined : t('overview.modelHintUnset', 'Pick a provider and model')}
        />
        <StatTile
          className="bg-surface"
          label={t('overview.channels', 'Channels')}
          icon={<Cable className="h-3 w-3" aria-hidden="true" />}
          value={`${connectedChannels.length}/${enabledChannels.length}`}
          tone={failedChannels.length > 0 ? 'danger' : 'neutral'}
          hint={
            failedChannels.length > 0
              ? t('overview.failedChannels', { count: failedChannels.length })
              : status
                ? `${status.channel_count}`
                : undefined
          }
        />
        <StatTile
          className="bg-surface"
          label={t('overview.sandbox', 'Sandbox')}
          icon={<ShieldCheck className="h-3 w-3" aria-hidden="true" />}
          value={sandboxOn ? security?.profile ?? 'on' : t('overview.sandboxOff', 'Off')}
          tone={sandboxOn ? 'success' : 'warning'}
          hint={sandboxOn ? sandboxHint ?? undefined : t('overview.sandboxHintOff', 'Runs on the host')}
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)]">
        <section className="min-w-0">
          <SectionHeader
            title={t('overview.recentSessions', 'Recent sessions')}
            actions={
              <Button
                variant="ghost"
                size="sm"
                icon={<ArrowRight className="h-3.5 w-3.5" />}
                onClick={() => navigate('/sessions')}
              >
                {t('overview.viewAll', 'View all')}
              </Button>
            }
            className="pb-2"
          />
          <DataTable
            rows={sessions}
            columns={sessionColumns}
            rowKey={(row) => row.id}
            density="compact"
            onRowClick={() => navigate('/sessions')}
            empty={t('overview.recentSessionsEmpty', 'No sessions yet — start one in Chat.')}
            className="[&>table]:min-w-0"
          />
        </section>

        <section className="min-w-0">
          <SectionHeader
            title={t('overview.setup', 'First-run checklist')}
            description={
              remaining === 0
                ? t('overview.setupAllDone', 'Everything is configured.')
                : t('overview.setupHint', 'Four settings make this console useful.')
            }
            className="pb-2"
          />
          <ul className="divide-y divide-line rounded-panel border border-line">
            {checklist.map((item) => (
              <li key={item.id} className="flex items-center gap-2 px-3 py-2">
                <StatusDot tone={item.done ? 'success' : 'idle'} />
                <span className={`min-w-0 flex-1 truncate text-small ${item.done ? 'text-ink-2' : 'text-ink'}`}>
                  {item.label}
                </span>
                {item.done ? (
                  <span className="text-micro text-ink-3">{t('overview.setupDone', 'Done')}</span>
                ) : (
                  <Button variant="ghost" size="sm" onClick={() => navigate(item.to)}>
                    {item.id === 'workspace' ? t('overview.openChat', 'Open chat') : t('overview.configure', 'Configure')}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  )
}
