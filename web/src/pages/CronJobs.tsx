import { useCallback, useEffect, useState } from 'react'
import { Clock, RefreshCw } from 'lucide-react'
import { apiClient } from '../api/client'
import type { CronJobInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatShortDateTime } from '../utils/datetime'
import {
  Badge,
  Button,
  CodeBlock,
  DataTable,
  Dialog,
  EmptyState,
  PageHeader,
  useToast,
  type Column,
} from '../ui'

const formatTimestamp = (ms: number | null | undefined, fallback: string) =>
  ms ? formatShortDateTime(ms) : fallback

/** Last-run outcomes come back as enum strings; translate them instead of printing them. */
const RUN_STATUS: Record<string, { key: string; fallback: string }> = {
  success: { key: 'cron.runSuccess', fallback: 'Succeeded' },
  running: { key: 'cron.runRunning', fallback: 'Running' },
  failed: { key: 'cron.runFailed', fallback: 'Failed' },
}

function runStatusLabel(status: string, t: (key: string, fallback: string) => string): string {
  const entry = RUN_STATUS[status]
  return entry ? t(entry.key, entry.fallback) : status
}

export default function CronJobs() {
  const [jobs, setJobs] = useState<CronJobInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [updating, setUpdating] = useState<string | null>(null)
  const [detail, setDetail] = useState<CronJobInfo | null>(null)
  const { t } = useI18n()
  const { toast } = useToast()

  const fetchJobs = useCallback(
    async (quiet = false) => {
      if (!quiet) setLoading(true)
      try {
        setJobs(await apiClient.getCronJobs())
        setError(null)
      } catch (err) {
        console.error(err)
        if (!quiet) setError(t('cron.failedToFetch', 'Failed to fetch cron jobs'))
      } finally {
        if (!quiet) setLoading(false)
      }
    },
    [t]
  )

  useEffect(() => {
    void fetchJobs()
  }, [fetchJobs])

  const toggle = async (job: CronJobInfo) => {
    setUpdating(job.id)
    try {
      await apiClient.updateCronJob(job.id, !job.enabled)
      await fetchJobs(true)
      toast(t('cron.updated', { name: job.name }))
    } catch (err) {
      console.error('Failed to update cron job:', err)
      setError(t('cron.updateFailed', 'Could not update the job'))
      toast(t('cron.updateFailed', 'Could not update the job'), 'error')
    } finally {
      setUpdating(null)
    }
  }

  const columns: Array<Column<CronJobInfo>> = [
    {
      key: 'name',
      header: t('cron.title', 'Cron'),
      sortValue: (row) => row.name,
      render: (row) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-ink">{row.name}</div>
          <div className="truncate font-mono text-micro text-ink-3">{row.id}</div>
        </div>
      ),
    },
    {
      key: 'schedule',
      header: t('cron.schedule', 'Schedule'),
      width: '150px',
      sortValue: (row) => row.schedule,
      render: (row) => (
        <span className="rounded-control bg-surface-2 px-1.5 py-0.5 font-mono text-micro text-ink-2">
          {row.schedule}
        </span>
      ),
    },
    {
      key: 'enabled',
      header: t('cron.status', 'Status'),
      width: '120px',
      sortValue: (row) => (row.enabled ? 1 : 0),
      render: (row) => (
        <Badge tone={row.enabled ? 'success' : 'neutral'}>
          {row.enabled ? t('cron.enabled', 'Enabled') : t('cron.disabled', 'Disabled')}
        </Badge>
      ),
    },
    {
      key: 'lastRun',
      header: t('cron.lastRun', 'Last run'),
      width: '170px',
      sortValue: (row) => row.state.last_run_at_ms ?? 0,
      render: (row) => (
        <div className="flex flex-col">
          <span className="whitespace-nowrap tabular text-micro text-ink-2">
            {formatTimestamp(row.state.last_run_at_ms, t('cron.na', '—'))}
          </span>
          {row.state.last_status && (
            <span className="truncate text-micro text-ink-3">
              {runStatusLabel(row.state.last_status, t)}
            </span>
          )}
        </div>
      ),
    },
    {
      key: 'nextRun',
      header: t('cron.nextRun', 'Next run'),
      width: '170px',
      sortValue: (row) => row.state.next_run_at_ms ?? Number.MAX_SAFE_INTEGER,
      render: (row) => (
        <span className="whitespace-nowrap tabular text-micro text-ink-2">
          {formatTimestamp(row.state.next_run_at_ms, t('cron.na', '—'))}
        </span>
      ),
    },
    {
      key: 'actions',
      header: <span className="sr-only">{t('common.status', 'Status')}</span>,
      align: 'right',
      width: '110px',
      render: (row) => (
        <Button
          variant="ghost"
          size="sm"
          disabled={updating === row.id}
          onClick={(event) => {
            event.stopPropagation()
            void toggle(row)
          }}
        >
          {updating === row.id
            ? t('cron.updating', 'Updating…')
            : row.enabled
              ? t('cron.disable', 'Disable')
              : t('cron.enable', 'Enable')}
        </Button>
      ),
    },
  ]

  return (
    <div>
      <PageHeader
        title={t('cron.title', 'Cron')}
        count={jobs.length}
        description={t('cron.description', 'Scheduled jobs run by the daemon')}
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon={<RefreshCw className="h-3.5 w-3.5" />}
            onClick={() => void fetchJobs()}
          >
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : (
        <DataTable
          rows={jobs}
          columns={columns}
          rowKey={(row) => row.id}
          density="compact"
          onRowClick={setDetail}
          empty={
            <EmptyState
              size="inline"
              icon={<Clock className="h-5 w-5" aria-hidden="true" />}
              title={t('cron.noJobs', 'No cron jobs')}
              body={t('cron.emptyHint', 'Jobs are declared in the configuration file; nothing is scheduled yet.')}
            />
          }
        />
      )}

      <Dialog
        open={!!detail}
        onClose={() => setDetail(null)}
        title={detail?.name ?? ''}
        description={detail?.schedule}
        closeLabel={t('common.close', 'Close')}
        width="md"
      >
        {detail && (
          <div className="space-y-3">
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-small">
              {[
                [t('cron.id', 'ID'), detail.id],
                [t('cron.schedule', 'Schedule'), detail.schedule],
                [t('cron.status', 'Status'), detail.enabled ? t('cron.enabled', 'Enabled') : t('cron.disabled', 'Disabled')],
                [
                  t('cron.lastRun', 'Last run'),
                  formatTimestamp(detail.state.last_run_at_ms, t('cron.na', '—')),
                ],
                [
                  t('cron.nextRun', 'Next run'),
                  formatTimestamp(detail.state.next_run_at_ms, t('cron.na', '—')),
                ],
                [t('cron.status', 'Status'), detail.state.last_status ?? t('cron.na', '—')],
              ].map(([label, value]) => (
                <div key={label} className="flex flex-col gap-0.5">
                  <dt className="text-micro text-ink-3">{label}</dt>
                  <dd className="break-all font-mono text-micro text-ink">{value}</dd>
                </div>
              ))}
            </dl>
            <div>
              <p className="mb-1 text-micro text-ink-3">{t('cron.payload', 'Payload')}</p>
              <CodeBlock title={detail.id} maxHeight={240}>
                {JSON.stringify(detail.payload, null, 2)}
              </CodeBlock>
            </div>
            <div className="flex justify-end">
              <Button
                variant={detail.enabled ? 'ghost' : 'primary'}
                size="sm"
                disabled={updating === detail.id}
                onClick={() => void toggle(detail)}
              >
                {detail.enabled ? t('cron.disable', 'Disable') : t('cron.enable', 'Enable')}
              </Button>
            </div>
          </div>
        )}
      </Dialog>
    </div>
  )
}
