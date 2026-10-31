import { useCallback, useEffect, useState } from 'react'
import { MessagesSquare } from 'lucide-react'
import { apiClient } from '../api/client'
import type { SessionDetail, SessionSummary } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatDateTime, formatShortDateTime } from '../utils/datetime'
import {
  Badge,
  Button,
  DataTable,
  EmptyState,
  Field,
  Input,
  PageHeader,
  ScrollArea,
  type Column,
} from '../ui'

const formatDate = (value: string) => formatDateTime(value, value)

export default function Sessions() {
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [detail, setDetail] = useState<SessionDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [channelFilter, setChannelFilter] = useState('')
  const [scopeFilter, setScopeFilter] = useState('')
  const { t } = useI18n()

  useEffect(() => {
    let active = true
    setLoading(true)
    apiClient
      .getSessions(undefined, undefined, channelFilter || undefined, scopeFilter || undefined)
      .then((page) => {
        if (!active) return
        setSessions(page.items)
        setError(null)
      })
      .catch((err) => {
        console.error(err)
        if (active) setError(t('sessions.failedToFetch', 'Failed to fetch sessions'))
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [channelFilter, scopeFilter, t])

  const openSession = useCallback(async (id: string) => {
    setSelectedId(id)
    setDetail(null)
    setDetailLoading(true)
    try {
      setDetail(await apiClient.getSession(id))
    } catch (err) {
      console.error('Failed to fetch session detail:', err)
    } finally {
      setDetailLoading(false)
    }
  }, [])

  const columns: Array<Column<SessionSummary>> = [
    {
      key: 'identifier',
      header: t('sessions.sessionList', 'Sessions'),
      sortValue: (row) => row.identifier || '',
      render: (row) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-ink">
            {row.identifier || t('sessions.anonymous', 'Anonymous')}
          </div>
          <div className="truncate font-mono text-micro text-ink-3">{row.id}</div>
        </div>
      ),
    },
    {
      key: 'channel',
      header: t('sessions.channel', 'Channel'),
      width: '130px',
      sortValue: (row) => row.channel,
      render: (row) => <Badge tone="neutral">{row.channel}</Badge>,
    },
    {
      key: 'messages',
      header: t('sessions.messages', 'Messages'),
      align: 'right',
      width: '90px',
      sortValue: (row) => row.message_count,
      render: (row) => <span className="tabular font-mono text-micro text-ink-2">{row.message_count}</span>,
    },
    {
      key: 'updated',
      header: t('sessions.updated', 'Updated'),
      align: 'right',
      width: '130px',
      sortValue: (row) => row.updated_at,
      render: (row) => (
        <span className="whitespace-nowrap tabular text-micro text-ink-2">
          {formatShortDateTime(row.updated_at)}
        </span>
      ),
    },
  ]

  const hasFilters = Boolean(channelFilter || scopeFilter)

  return (
    <div>
      <PageHeader
        title={t('sessions.title', 'Sessions')}
        count={sessions.length}
        description={t('sessions.description', 'Conversation history per channel and scope')}
        actions={
          hasFilters && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setChannelFilter('')
                setScopeFilter('')
              }}
            >
              {t('sessions.clearFilters', 'Clear filters')}
            </Button>
          )
        }
      />

      <div className="mb-4 flex flex-wrap items-end gap-3">
        <Field label={t('sessions.channel', 'Channel')} htmlFor="session-channel" className="w-48">
          <Input
            id="session-channel"
            value={channelFilter}
            onChange={(event) => setChannelFilter(event.target.value)}
            placeholder={t('sessions.filterByChannel', 'Filter by channel')}
          />
        </Field>
        <Field label={t('sessions.scope', 'Scope')} htmlFor="session-scope" className="w-48">
          <Input
            id="session-scope"
            value={scopeFilter}
            onChange={(event) => setScopeFilter(event.target.value)}
            placeholder={t('sessions.filterByScope', 'Filter by scope')}
          />
        </Field>
      </div>

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : sessions.length === 0 ? (
        <EmptyState
          icon={<MessagesSquare className="h-6 w-6" />}
          title={t('sessions.noSessionsFound', 'No sessions found')}
          body={t('sessions.emptyHint', 'Sessions appear here once someone talks to Synbot on any channel.')}
        />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)]">
          <div className="min-w-0">
            <DataTable
              rows={sessions}
              columns={columns}
              rowKey={(row) => row.id}
              density="compact"
              selectedKey={selectedId}
              onRowClick={(row) => void openSession(row.id)}
              className="[&>table]:min-w-0"
            />
          </div>

          <div className="min-w-0">
            {detailLoading ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {t('common.loading', 'Loading…')}
              </p>
            ) : !detail ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {selectedId ? t('common.loading', 'Loading…') : t('sessions.selectSessionToView', 'Select a session to view its messages.')}
              </p>
            ) : (
              <div className="rounded-panel border border-line">
                <div className="border-b border-line px-4 py-3">
                  <h2 className="text-section font-semibold text-ink">
                    {detail.meta.identifier || t('sessions.anonymous', 'Anonymous')}
                  </h2>
                  <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-micro">
                    {[
                      [t('sessions.id', 'ID'), detail.meta.id],
                      [t('sessions.channel', 'Channel'), detail.meta.channel],
                      [t('sessions.scope', 'Scope'), detail.meta.scope || t('sessions.direct', 'Direct')],
                      [t('sessions.created', 'Created'), formatDate(detail.meta.created_at)],
                    ].map(([label, value]) => (
                      <div key={label} className="flex min-w-0 flex-col gap-0.5">
                        <dt className="text-ink-3">{label}</dt>
                        <dd className="truncate font-mono text-ink-2">{value}</dd>
                      </div>
                    ))}
                  </dl>
                </div>

                <div className="px-4 py-3">
                  <h3 className="mb-2 text-micro font-medium uppercase tracking-wide text-ink-3">
                    {t('sessions.messageHistory', 'Message history')} · {detail.messages.length}
                  </h3>
                  {detail.messages.length === 0 ? (
                    <p className="py-6 text-center text-small text-ink-2">
                      {t('sessions.noMessages', 'No messages recorded for this session.')}
                    </p>
                  ) : (
                    <ScrollArea className="max-h-[420px] pr-1">
                      <ul className="flex flex-col gap-3">
                        {detail.messages.map((message, index) => (
                          <li key={`${message.timestamp}-${index}`} className="border-l border-line pl-3">
                            <div className="flex items-baseline gap-2">
                              <span
                                className={
                                  message.role === 'user'
                                    ? 'text-micro font-medium text-accent'
                                    : 'text-micro font-medium text-ink'
                                }
                              >
                                {message.role === 'user'
                                  ? t('chat.user', 'You')
                                  : t('chat.assistant', 'Assistant')}
                              </span>
                              <span className="tabular font-mono text-micro text-ink-3">
                                {formatDate(message.timestamp)}
                              </span>
                            </div>
                            <p className="mt-0.5 whitespace-pre-wrap text-small text-ink">{message.content}</p>
                          </li>
                        ))}
                      </ul>
                    </ScrollArea>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
