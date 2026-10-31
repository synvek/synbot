import { useCallback, useEffect, useState } from 'react'
import { ChevronRight, Workflow as WorkflowIcon } from 'lucide-react'
import { apiClient } from '../api/client'
import type { WorkflowDetail, WorkflowStatus, WorkflowSummary } from '../types/api'
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
  StatusDot,
  useToast,
  type BadgeTone,
  type Column,
  type StatusTone,
} from '../ui'

const STATUS_TONE: Record<WorkflowStatus, StatusTone> = {
  running: 'running',
  waiting_user_input: 'warning',
  paused: 'idle',
  completed: 'success',
  failed: 'error',
  cancelled: 'idle',
  waiting_timeout: 'warning',
}

const STATUS_BADGE: Record<WorkflowStatus, BadgeTone> = {
  running: 'info',
  waiting_user_input: 'warning',
  paused: 'neutral',
  completed: 'success',
  failed: 'danger',
  cancelled: 'neutral',
  waiting_timeout: 'warning',
}

const STEP_DOT: Record<string, StatusTone> = {
  completed: 'success',
  current: 'running',
  pending: 'idle',
}

/** Backend errors are `{ error: "Bad request: …" }`; axios' own message is just the status code. */
function errorText(err: unknown): string {
  const withResponse = err as { response?: { data?: { error?: string } } }
  return withResponse.response?.data?.error ?? (err instanceof Error ? err.message : String(err))
}

const formatTime = (value: string) => formatShortDateTime(value, value)

export default function Workflows() {
  const { t } = useI18n()
  const { toast } = useToast()
  const [runs, setRuns] = useState<WorkflowSummary[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [detail, setDetail] = useState<WorkflowDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirm, setConfirm] = useState<{ action: 'restart' | 'delete'; sessionKey: string } | null>(null)
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})

  const load = useCallback(
    async (silent = false) => {
      if (!silent) setLoading(true)
      try {
        const next = await apiClient.listWorkflows()
        setRuns(next)
        setError(null)
        // Keep the open detail panel in sync without a second round trip for the list.
        if (selected && next.some((run) => run.sessionKey === selected)) {
          try {
            setDetail(await apiClient.getWorkflow(selected))
          } catch {
            setDetail(null)
          }
        }
      } catch (err) {
        setError(errorText(err))
      } finally {
        if (!silent) setLoading(false)
      }
    },
    [selected]
  )

  useEffect(() => {
    void load()
    // Only on mount: `load` closes over `selected`, re-running it on selection would refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // A run that is still executing changes on its own; poll only while something is active so an
  // idle page never hammers the daemon.
  const hasActiveRun = runs.some(
    (run) => run.status === 'running' || run.status === 'waiting_user_input'
  )
  useEffect(() => {
    if (!hasActiveRun) return
    const id = window.setInterval(() => {
      void load(true)
    }, 5000)
    return () => window.clearInterval(id)
  }, [hasActiveRun, load])

  const openDetail = async (sessionKey: string) => {
    setSelected(sessionKey)
    setDetail(null)
    try {
      setDetail(await apiClient.getWorkflow(sessionKey))
    } catch (err) {
      setError(errorText(err))
    }
  }

  const act = async (sessionKey: string, action: 'resume' | 'restart' | 'cancel' | 'delete') => {
    setBusy(sessionKey)
    setError(null)
    try {
      if (action === 'resume') {
        await apiClient.runWorkflow(sessionKey, false)
        toast(t('workflows.resumed', 'Workflow resumed'))
      } else if (action === 'restart') {
        await apiClient.runWorkflow(sessionKey, true)
        toast(t('workflows.restarted', 'Workflow restarted from step 1'))
      } else if (action === 'cancel') {
        await apiClient.cancelWorkflow(sessionKey)
        toast(t('workflows.cancelledRun', 'Workflow cancelled'))
      } else {
        await apiClient.deleteWorkflow(sessionKey)
        toast(t('workflows.deleted', 'Workflow record deleted'))
        if (selected === sessionKey) {
          setSelected(null)
          setDetail(null)
        }
      }
      await load(true)
    } catch (err) {
      // The banner next to the list is the durable record; a toast would only duplicate it.
      setError(errorText(err))
    } finally {
      setBusy(null)
    }
  }

  const statusLabel = (status: WorkflowStatus) => t(`workflows.status.${status}`, status)

  const columns: Array<Column<WorkflowSummary>> = [
    {
      key: 'name',
      header: t('workflows.title', 'Workflows'),
      sortValue: (row) => row.name || row.workflowId,
      render: (row) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-ink">{row.name || row.workflowId}</div>
          <div className="truncate font-mono text-micro text-ink-3">{row.sessionKey}</div>
        </div>
      ),
    },
    {
      key: 'status',
      header: t('common.status', 'Status'),
      width: '118px',
      sortValue: (row) => row.status,
      render: (row) => (
        <span className="flex items-center gap-1.5 whitespace-nowrap text-small text-ink-2">
          <StatusDot tone={STATUS_TONE[row.status]} />
          {statusLabel(row.status)}
        </span>
      ),
    },
    {
      key: 'progress',
      header: t('workflows.progress', 'Progress'),
      align: 'right',
      width: '72px',
      sortValue: (row) => row.stepsCompleted,
      render: (row) => (
        <span className="tabular font-mono text-micro text-ink-2">
          {row.stepsCompleted}/{row.stepsTotal}
        </span>
      ),
    },
    {
      key: 'updated',
      header: t('sessions.updated', 'Updated'),
      align: 'right',
      width: '130px',
      sortValue: (row) => row.updatedAt,
      render: (row) => (
        <span className="whitespace-nowrap tabular text-micro text-ink-2">
          {formatTime(row.updatedAt)}
        </span>
      ),
    },
  ]

  const confirmTitle =
    confirm?.action === 'delete'
      ? t('workflows.delete', 'Delete record')
      : t('workflows.restart', 'Restart from step 1')
  const confirmBody =
    confirm?.action === 'delete'
      ? t('workflows.confirmDelete', 'Delete this workflow record? Chat history is untouched.')
      : t('workflows.confirmRestart', 'Restarting clears the recorded step outputs. Continue?')

  return (
    <div>
      <PageHeader
        title={t('workflows.title', 'Workflows')}
        count={runs.length}
        description={t('workflows.description', 'Persisted TurboWorkflow runs, created with /workflow in any chat. One run per session; an interrupted run resumes from its last finished step.')}
        actions={
          <Button variant="ghost" size="sm" onClick={() => void load()}>
            {t('workflows.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : runs.length === 0 ? (
        <EmptyState
          icon={<WorkflowIcon className="h-6 w-6" />}
          title={t('workflows.empty', 'No workflows yet')}
          body={t('workflows.emptyHint', 'Send /workflow <task> in any chat to create one; its progress shows up here.')}
        />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)]">
          <div className="min-w-0">
            <DataTable
              rows={runs}
              columns={columns}
              rowKey={(row) => row.sessionKey}
              density="compact"
              selectedKey={selected}
              onRowClick={(row) => void openDetail(row.sessionKey)}
              className="[&>table]:min-w-0"
            />
          </div>

          <div className="min-w-0">
            {!detail ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {selected ? t('workflows.loading', 'Loading…') : t('workflows.selectHint', 'Pick a run on the left to see its steps and outputs.')}
              </p>
            ) : (
              <div className="rounded-panel border border-line">
                <div className="border-b border-line px-4 py-3">
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div className="min-w-0">
                      <h2 className="truncate text-section font-semibold text-ink">
                        {detail.name || detail.workflowId}
                      </h2>
                      <p className="mt-0.5 truncate font-mono text-micro text-ink-3">{detail.sessionKey}</p>
                    </div>
                    <Badge tone={STATUS_BADGE[detail.status]}>{statusLabel(detail.status)}</Badge>
                  </div>
                  {detail.description && (
                    <p className="mt-2 text-small text-ink-2">{detail.description}</p>
                  )}
                  <p className="mt-2 text-micro text-ink-3">
                    {t('workflows.meta', {
                      updated: formatTime(detail.updatedAt),
                      timeout: Math.round(detail.userInputTimeoutSecs / 60),
                    })}
                  </p>
                  <div className="mt-3 flex flex-wrap items-center gap-2">
                    {detail.canResume ? (
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={busy === detail.sessionKey}
                        onClick={() => void act(detail.sessionKey, 'resume')}
                      >
                        {t('workflows.resume', 'Resume')}
                      </Button>
                    ) : (
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={busy === detail.sessionKey}
                        onClick={() => setConfirm({ action: 'restart', sessionKey: detail.sessionKey })}
                      >
                        {t('workflows.restart', 'Restart from step 1')}
                      </Button>
                    )}
                    {detail.canResume && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy === detail.sessionKey}
                        onClick={() => void act(detail.sessionKey, 'cancel')}
                      >
                        {t('workflows.cancel', 'Cancel')}
                      </Button>
                    )}
                    <Button
                      variant="danger"
                      size="sm"
                      disabled={busy === detail.sessionKey}
                      onClick={() => setConfirm({ action: 'delete', sessionKey: detail.sessionKey })}
                    >
                      {t('workflows.delete', 'Delete record')}
                    </Button>
                  </div>
                </div>

                <div className="space-y-4 px-4 py-3">
                  {Object.keys(detail.inputs).length > 0 && (
                    <div>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('workflows.inputs', 'Inputs')}
                      </h3>
                      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-micro">
                        {Object.entries(detail.inputs).map(([key, value]) => (
                          <div key={key} className="col-span-2 grid grid-cols-subgrid">
                            <dt className="font-mono text-ink-3">{key}</dt>
                            <dd className="whitespace-pre-wrap break-words text-ink">{value}</dd>
                          </div>
                        ))}
                      </dl>
                    </div>
                  )}

                  <div>
                    <h3 className="mb-2 text-micro font-medium uppercase tracking-wide text-ink-3">
                      {t('workflows.steps', 'Steps')}
                    </h3>
                    <ul className="flex flex-col gap-2">
                      {detail.steps.map((step, index) => {
                        const stepKey = `${step.id}-${index}`
                        const open = Boolean(expanded[stepKey])
                        return (
                          <li key={stepKey} className="rounded-panel border border-line">
                            <div className="flex flex-wrap items-center gap-2 px-3 py-2">
                              <StatusDot tone={STEP_DOT[step.status] ?? 'idle'} />
                              <span className="font-mono text-micro text-ink-3">
                                {index + 1}. {step.id}
                              </span>
                              <span className="rounded-control bg-surface-2 px-1.5 py-0.5 font-mono text-micro text-ink-2">
                                {t(`workflows.stepType.${step.type}`, step.type)}
                              </span>
                              <span className="ml-auto text-micro text-ink-3">
                                {t(`workflows.stepStatus.${step.status}`, step.status)}
                              </span>
                            </div>
                            {step.description && (
                              <p className="px-3 pb-2 text-small whitespace-pre-wrap text-ink">
                                {step.description}
                              </p>
                            )}
                            <div className="px-3 pb-2">
                              {step.output ? (
                                <>
                                  <button
                                    type="button"
                                    aria-expanded={open}
                                    onClick={() =>
                                      setExpanded((current) => ({ ...current, [stepKey]: !open }))
                                    }
                                    className="inline-flex items-center gap-1 text-micro text-ink-2 transition-colors duration-fast hover:text-ink"
                                  >
                                    <ChevronRight
                                      className={`h-3 w-3 transition-transform duration-fast ${open ? 'rotate-90' : ''}`}
                                      aria-hidden="true"
                                    />
                                    {t('workflows.output', 'Show output')}
                                  </button>
                                  {open && (
                                    <CodeBlock className="mt-1" maxHeight={280}>
                                      {step.output}
                                    </CodeBlock>
                                  )}
                                </>
                              ) : (
                                <p className="text-micro text-ink-3">
                                  {t('workflows.noOutput', 'This step has not produced output yet.')}
                                </p>
                              )}
                            </div>
                          </li>
                        )
                      })}
                    </ul>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {confirm && (
        <Dialog
          open
          onClose={() => setConfirm(null)}
          title={confirmTitle}
          width="sm"
          closeLabel={t('common.close', 'Close')}
          footer={
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setConfirm(null)}>
                {t('common.cancel', 'Cancel')}
              </Button>
              <Button
                variant={confirm.action === 'delete' ? 'danger' : 'primary'}
                size="sm"
                onClick={() => {
                  const { action, sessionKey } = confirm
                  setConfirm(null)
                  void act(sessionKey, action)
                }}
              >
                {t('workflows.confirm', 'Continue')}
              </Button>
            </div>
          }
        >
          <p className="text-small text-ink-2">{confirmBody}</p>
        </Dialog>
      )}
    </div>
  )
}
