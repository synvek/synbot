import { useCallback, useEffect, useState } from 'react'
import { Cable, RefreshCw } from 'lucide-react'
import { apiClient } from '../api/client'
import type { ChannelInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatDateTime, formatShortDateTime } from '../utils/datetime'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  EmptyState,
  PageHeader,
  StatusDot,
  type Column,
  type StatusTone,
} from '../ui'

const formatTime = (value: string | null) => formatDateTime(value)
const formatCellTime = (value: string | null) => formatShortDateTime(value)

/** Channel states mapped once, so no template string ever builds a class name. */
const STATUS_TONE: Record<string, StatusTone> = {
  connected: 'success',
  starting: 'running',
  reconnecting: 'running',
  failed: 'error',
  disabled: 'warning',
  stopped: 'idle',
}

/** Labels stay in the locale files: a Chinese console must not print `reconnecting` raw. */
const STATUS_LABEL: Record<string, { key: string; fallback: string }> = {
  connected: { key: 'channels.stConnected', fallback: 'Connected' },
  starting: { key: 'channels.stStarting', fallback: 'Starting' },
  reconnecting: { key: 'channels.stReconnecting', fallback: 'Reconnecting' },
  failed: { key: 'channels.stFailed', fallback: 'Failed' },
  disabled: { key: 'channels.stDisabled', fallback: 'Disabled' },
  stopped: { key: 'channels.stStopped', fallback: 'Stopped' },
}

function statusLabel(status: string, t: (key: string, fallback: string) => string): string {
  const entry = STATUS_LABEL[status]
  return entry ? t(entry.key, entry.fallback) : status
}

export default function Channels() {
  const [channels, setChannels] = useState<ChannelInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [detail, setDetail] = useState<ChannelInfo | null>(null)
  const { t } = useI18n()

  const fetchChannels = useCallback(
    async (quiet = false) => {
      try {
        const data = await apiClient.getChannels()
        setChannels(data)
        setError(null)
      } catch {
        if (!quiet) setError(t('channels.failedToFetch', 'Failed to fetch channels'))
      } finally {
        setLoading(false)
      }
    },
    [t]
  )

  useEffect(() => {
    let active = true
    const tick = () => {
      if (active) void fetchChannels(true)
    }
    void fetchChannels()
    const interval = window.setInterval(tick, 5000)
    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [fetchChannels])

  const columns: Array<Column<ChannelInfo>> = [
    {
      key: 'name',
      header: t('channels.title', 'Channels'),
      sortValue: (row) => row.name,
      render: (row) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-ink">{row.name}</div>
          <div className="truncate font-mono text-micro text-ink-3">{row.channel_type}</div>
        </div>
      ),
    },
    {
      key: 'status',
      header: t('channels.status', 'Status'),
      width: '160px',
      sortValue: (row) => row.status,
      render: (row) => (
        <span className="flex items-center gap-1.5 text-small text-ink-2">
          <StatusDot tone={STATUS_TONE[row.status] ?? 'idle'} />
          {statusLabel(row.status, t)}
        </span>
      ),
    },
    {
      key: 'enabled',
      header: t('channels.enabled', 'Enabled'),
      width: '120px',
      sortValue: (row) => (row.enabled ? 1 : 0),
      render: (row) => (
        <Badge tone={row.enabled ? 'neutral' : 'warning'}>
          {row.enabled ? t('channels.enabled', 'Enabled') : t('channels.disabled', 'Disabled')}
        </Badge>
      ),
    },
    {
      key: 'latency',
      header: t('channels.latency', 'Latency'),
      align: 'right',
      width: '110px',
      sortValue: (row) => row.last_latency_ms ?? Number.MAX_SAFE_INTEGER,
      render: (row) => (
        <span className="tabular font-mono text-micro text-ink-2">
          {row.last_latency_ms == null ? '—' : `${row.last_latency_ms} ms`}
        </span>
      ),
    },
    {
      key: 'reconnects',
      header: t('channels.reconnects', 'Reconnects'),
      align: 'right',
      width: '120px',
      sortValue: (row) => row.reconnect_count,
      render: (row) => <span className="tabular font-mono text-micro text-ink-2">{row.reconnect_count}</span>,
    },
    {
      key: 'last',
      header: t('channels.lastActivity', 'Last activity'),
      align: 'right',
      width: '180px',
      sortValue: (row) => row.last_received_at ?? '',
      render: (row) => (
        <span className="whitespace-nowrap tabular text-micro text-ink-2">
          {formatCellTime(row.last_received_at)}
        </span>
      ),
    },
  ]

  const failed = channels.filter((channel) => channel.status === 'failed').length

  return (
    <div>
      <PageHeader
        title={t('channels.title', 'Channels')}
        count={channels.length}
        description={t('channels.description', 'Manage messaging channels')}
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon={<RefreshCw className="h-3.5 w-3.5" />}
            onClick={() => void fetchChannels()}
          >
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}
      {failed > 0 && (
        <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-warning">
          {t('channels.failedChannels', { count: failed })}
        </div>
      )}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : (
        <DataTable
          rows={channels}
          columns={columns}
          rowKey={(row) => row.instance_id}
          density="compact"
          onRowClick={setDetail}
          empty={
            <EmptyState
              size="inline"
              icon={<Cable className="h-5 w-5" aria-hidden="true" />}
              title={t('channels.noChannels', 'No channels configured')}
              body={t('channels.emptyHint', 'Add a channel in Configuration to let Synbot answer where you already talk.')}
              action={
                <Button size="sm" onClick={() => (window.location.hash = '#/config')}>
                  {t('overview.configure', 'Configure')}
                </Button>
              }
            />
          }
        />
      )}

      <Dialog
        open={!!detail}
        onClose={() => setDetail(null)}
        title={detail?.name ?? ''}
        description={detail?.channel_type}
        closeLabel={t('common.close', 'Close')}
      >
        {detail && (
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-small">
            {[
              [t('channels.status', 'Status'), detail.status],
              [t('channels.enabled', 'Enabled'), detail.enabled ? t('common.yes', 'Yes') : t('common.no', 'No')],
              [t('channels.started', 'Started'), formatTime(detail.started_at)],
              [t('channels.lastConnected', 'Last connected'), formatTime(detail.last_connected_at)],
              [t('channels.lastReceived', 'Last received'), formatTime(detail.last_received_at)],
              [t('channels.lastSent', 'Last sent'), formatTime(detail.last_sent_at)],
              [t('channels.reconnects', 'Reconnects'), String(detail.reconnect_count)],
              [
                t('channels.latency', 'Latency'),
                detail.last_latency_ms == null ? '—' : `${detail.last_latency_ms} ms`,
              ],
              [t('channels.supportsSend', 'Send'), detail.supports_send ? t('common.yes', 'Yes') : t('common.no', 'No')],
              [
                t('channels.supportsReceive', 'Receive'),
                detail.supports_receive ? t('common.yes', 'Yes') : t('common.no', 'No'),
              ],
            ].map(([label, value]) => (
              <div key={label} className="flex flex-col gap-0.5">
                <dt className="text-micro text-ink-3">{label}</dt>
                <dd className="break-all font-mono text-micro text-ink">{value}</dd>
              </div>
            ))}
          </dl>
        )}
        {detail?.last_error && (
          <div className="mt-3 rounded-control border px-2.5 py-2 text-small tone-danger">
            {detail.last_error}
          </div>
        )}
      </Dialog>
    </div>
  )
}
