import React, { useCallback, useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { WorkflowDetail, WorkflowStatus, WorkflowSummary } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

// Fixed colours (not theme tokens) so status stays readable in both colour schemes; the rest of
// the page uses the theme tokens the other panels use.
const STATUS_TONE: Record<WorkflowStatus, string> = {
  running: 'bg-blue-100 text-blue-800',
  waiting_user_input: 'bg-amber-100 text-amber-800',
  paused: 'bg-slate-200 text-slate-700',
  completed: 'bg-green-100 text-green-800',
  failed: 'bg-red-100 text-red-800',
  cancelled: 'bg-gray-100 text-gray-600',
  waiting_timeout: 'bg-amber-100 text-amber-800',
}

const STEP_DOT: Record<string, string> = {
  completed: 'bg-green-500',
  current: 'bg-blue-500',
  pending: 'bg-gray-300',
}

/** Backend errors are `{ error: "Bad request: …" }`; axios' own message is just the status code. */
function errorText(err: unknown): string {
  const withResponse = err as { response?: { data?: { error?: string } } }
  return withResponse.response?.data?.error ?? (err instanceof Error ? err.message : String(err))
}

function formatTime(value: string): string {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}

const Workflows: React.FC = () => {
  const { t } = useI18n()
  const [runs, setRuns] = useState<WorkflowSummary[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [detail, setDetail] = useState<WorkflowDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

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
    if (action === 'delete' && !window.confirm(t('workflows.confirmDelete'))) return
    if (action === 'restart' && !window.confirm(t('workflows.confirmRestart'))) return
    setBusy(sessionKey)
    setError(null)
    try {
      if (action === 'resume') {
        await apiClient.runWorkflow(sessionKey, false)
      } else if (action === 'restart') {
        await apiClient.runWorkflow(sessionKey, true)
      } else if (action === 'cancel') {
        await apiClient.cancelWorkflow(sessionKey)
      } else {
        await apiClient.deleteWorkflow(sessionKey)
        if (selected === sessionKey) {
          setSelected(null)
          setDetail(null)
        }
      }
      await load(true)
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(null)
    }
  }

  const statusLabel = (status: WorkflowStatus) =>
    t(`workflows.status.${status}`, status)

  return (
    <div>
      <div className="flex flex-wrap items-start justify-between gap-3 mb-6">
        <div>
          <h2 className="text-2xl font-bold text-text">{t('workflows.title')}</h2>
          <p className="text-text-secondary mt-1">{t('workflows.description')}</p>
        </div>
        <button
          type="button"
          onClick={() => void load()}
          className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
        >
          {t('workflows.refresh')}
        </button>
      </div>

      {error && (
        <div className="mb-4 bg-red-50 border border-red-200 rounded-lg p-3">
          <p className="text-red-800 text-sm">{error}</p>
        </div>
      )}

      {loading ? (
        <div className="flex items-center justify-center h-40 text-text-secondary">
          {t('common.loading')}
        </div>
      ) : runs.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-text-secondary">{t('workflows.empty')}</p>
          <p className="text-text-secondary text-sm mt-2">{t('workflows.emptyHint')}</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-5 gap-4">
          <ul className="lg:col-span-2 space-y-3">
            {runs.map((run) => {
              const active = run.sessionKey === selected
              return (
                <li key={run.sessionKey}>
                  <button
                    type="button"
                    onClick={() => void openDetail(run.sessionKey)}
                    className={`w-full text-left bg-surface rounded-lg shadow p-4 border transition-colors ${
                      active ? 'border-primary' : 'border-transparent hover:border-border'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <span className="font-medium text-text truncate">
                        {run.name || run.workflowId}
                      </span>
                      <span
                        className={`shrink-0 px-2 py-0.5 rounded text-xs font-medium ${STATUS_TONE[run.status]}`}
                      >
                        {statusLabel(run.status)}
                      </span>
                    </div>
                    <p className="font-mono text-xs text-text-secondary mt-1 truncate">
                      {run.sessionKey}
                    </p>
                    <div className="flex items-center justify-between mt-3 text-xs text-text-secondary">
                      <span>
                        {t('workflows.progress')}: {run.stepsCompleted}/{run.stepsTotal}
                      </span>
                      <span>{formatTime(run.updatedAt)}</span>
                    </div>
                    <div className="mt-2 h-1.5 bg-background rounded overflow-hidden">
                      <div
                        className="h-full bg-primary"
                        style={{
                          width: `${
                            run.stepsTotal > 0
                              ? Math.round((run.stepsCompleted / run.stepsTotal) * 100)
                              : 0
                          }%`,
                        }}
                      />
                    </div>
                  </button>
                </li>
              )
            })}
          </ul>

          <div className="lg:col-span-3">
            {!detail ? (
              <div className="bg-surface rounded-lg shadow p-6 text-sm text-text-secondary">
                {selected ? t('workflows.loading') : t('workflows.selectHint')}
              </div>
            ) : (
              <div className="bg-surface rounded-lg shadow p-6 space-y-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="text-lg font-semibold text-text">
                      {detail.name || detail.workflowId}
                    </h3>
                    <p className="font-mono text-xs text-text-secondary mt-1">
                      {detail.sessionKey}
                    </p>
                    {detail.description && (
                      <p className="text-sm text-text-secondary mt-2">{detail.description}</p>
                    )}
                  </div>
                  <span
                    className={`px-2 py-0.5 rounded text-xs font-medium ${STATUS_TONE[detail.status]}`}
                  >
                    {statusLabel(detail.status)}
                  </span>
                </div>

                <div className="flex flex-wrap gap-2">
                  {detail.canResume ? (
                    <button
                      type="button"
                      disabled={busy === detail.sessionKey}
                      onClick={() => void act(detail.sessionKey, 'resume')}
                      className="px-3 py-1.5 rounded-lg bg-primary text-white text-sm disabled:opacity-50"
                    >
                      {t('workflows.resume')}
                    </button>
                  ) : (
                    <button
                      type="button"
                      disabled={busy === detail.sessionKey}
                      onClick={() => void act(detail.sessionKey, 'restart')}
                      className="px-3 py-1.5 rounded-lg bg-primary text-white text-sm disabled:opacity-50"
                    >
                      {t('workflows.restart')}
                    </button>
                  )}
                  {detail.canResume && (
                    <button
                      type="button"
                      disabled={busy === detail.sessionKey}
                      onClick={() => void act(detail.sessionKey, 'cancel')}
                      className="px-3 py-1.5 rounded-lg bg-surface border border-border text-text text-sm disabled:opacity-50"
                    >
                      {t('workflows.cancel')}
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={busy === detail.sessionKey}
                    onClick={() => void act(detail.sessionKey, 'delete')}
                    className="px-3 py-1.5 rounded-lg text-error border border-border text-sm disabled:opacity-50"
                  >
                    {t('workflows.delete')}
                  </button>
                </div>

                <p className="text-xs text-text-secondary">
                  {t('workflows.meta', {
                    updated: formatTime(detail.updatedAt),
                    timeout: Math.round(detail.userInputTimeoutSecs / 60),
                  })}
                </p>

                {Object.keys(detail.inputs).length > 0 && (
                  <div>
                    <h4 className="text-sm font-medium text-text mb-1">{t('workflows.inputs')}</h4>
                    <dl className="text-xs grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
                      {Object.entries(detail.inputs).map(([key, value]) => (
                        <React.Fragment key={key}>
                          <dt className="font-mono text-text-secondary">{key}</dt>
                          <dd className="text-text break-words whitespace-pre-wrap">{value}</dd>
                        </React.Fragment>
                      ))}
                    </dl>
                  </div>
                )}

                <div>
                  <h4 className="text-sm font-medium text-text mb-2">{t('workflows.steps')}</h4>
                  <ul className="space-y-2">
                    {detail.steps.map((step, index) => (
                      <li
                        key={`${step.id}-${index}`}
                        className="border border-border rounded-lg p-3"
                      >
                        <div className="flex items-center gap-2">
                          <span
                            className={`w-2 h-2 rounded-full ${STEP_DOT[step.status] ?? STEP_DOT.pending}`}
                          />
                          <span className="font-mono text-xs text-text-secondary">
                            {index + 1}. {step.id}
                          </span>
                          <span className="text-xs px-2 py-0.5 rounded bg-background border border-border text-text-secondary">
                            {t(`workflows.stepType.${step.type}`, step.type)}
                          </span>
                          <span className="ml-auto text-xs text-text-secondary">
                            {t(`workflows.stepStatus.${step.status}`, step.status)}
                          </span>
                        </div>
                        {step.description && (
                          <p className="text-sm text-text mt-1 whitespace-pre-wrap">
                            {step.description}
                          </p>
                        )}
                        {step.output ? (
                          <details className="mt-2">
                            <summary className="text-xs text-text-secondary cursor-pointer">
                              {t('workflows.output')}
                            </summary>
                            <pre className="mt-1 text-xs whitespace-pre-wrap break-words bg-background rounded p-2 max-h-64 overflow-auto">
                              {step.output}
                            </pre>
                          </details>
                        ) : (
                          <p className="text-xs text-text-secondary mt-2">{t('workflows.noOutput')}</p>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

export default Workflows
