import { useCallback, useEffect, useRef, useState } from 'react'
import { ArrowDownToLine, Pause, Play, RefreshCw, ScrollText } from 'lucide-react'
import { apiClient } from '../api/client'
import type { LogEntry, LogQueryParams } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import {
  Button,
  EmptyState,
  Field,
  Input,
  PageHeader,
  Select,
  StatusDot,
  cn,
} from '../ui'

const LEVELS = ['error', 'warn', 'info', 'debug'] as const
const PAGE_SIZE = 100

/** Level → text tone, mapped once so no class name is ever built from the payload. */
const LEVEL_CLASS: Record<string, string> = {
  error: 'text-error',
  warn: 'text-warning',
  info: 'text-info',
  debug: 'text-ink-3',
}

const formatTimestamp = (value: string) => {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const pad = (n: number, width = 2) => String(n).padStart(width, '0')
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`
}

export default function Logs() {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [level, setLevel] = useState('')
  const [keyword, setKeyword] = useState('')
  const [streaming, setStreaming] = useState(false)
  const [follow, setFollow] = useState(true)
  const socketRef = useRef<WebSocket | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const { t } = useI18n()

  const fetchLogs = useCallback(async () => {
    setLoading(true)
    const params: LogQueryParams = {
      level: level || undefined,
      keyword,
      page: 1,
      page_size: PAGE_SIZE,
    }
    try {
      const response = await apiClient.getLogs(params)
      setLogs(response.items)
      setError(null)
    } catch (err) {
      console.error(err)
      setError(t('logs.failedToFetch', 'Failed to fetch logs'))
    } finally {
      setLoading(false)
    }
  }, [keyword, level, t])

  useEffect(() => {
    void fetchLogs()
  }, [fetchLogs])

  // Streaming appends to the top of the list, so "following" means staying pinned there.
  useEffect(() => {
    if (!follow) return
    listRef.current?.scrollTo({ top: 0 })
  }, [logs, follow])

  const stopStreaming = useCallback(() => {
    socketRef.current?.close()
    socketRef.current = null
    setStreaming(false)
  }, [])

  const startStreaming = () => {
    const socket = new WebSocket(apiClient.wsUrl('/ws/logs'))
    socket.onopen = () => setStreaming(true)
    socket.onmessage = (event) => {
      try {
        const entry: LogEntry = JSON.parse(event.data)
        setLogs((current) => [entry, ...current].slice(0, PAGE_SIZE))
      } catch (err) {
        console.error('Failed to parse log message:', err)
      }
    }
    socket.onerror = (err) => {
      console.error('WebSocket error:', err)
      setError(t('logs.wsError', 'Log stream failed'))
      setStreaming(false)
    }
    socket.onclose = () => setStreaming(false)
    socketRef.current = socket
  }

  useEffect(() => () => socketRef.current?.close(), [])

  return (
    <div className="flex min-h-0 flex-col">
      <PageHeader
        title={t('logs.title', 'Logs')}
        count={logs.length}
        description={t('logs.description', 'Daemon log tail, newest first')}
        actions={
          <>
            <Button
              variant="ghost"
              size="sm"
              icon={streaming ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
              onClick={streaming ? stopStreaming : startStreaming}
            >
              {streaming ? t('logs.stopStream', 'Stop stream') : t('logs.startStream', 'Start stream')}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={<RefreshCw className="h-3.5 w-3.5" />}
              onClick={() => void fetchLogs()}
            >
              {t('logs.refresh', 'Refresh')}
            </Button>
          </>
        }
      />

      <div className="mb-3 flex flex-wrap items-end gap-3">
        <Field label={t('logs.search', 'Search')} htmlFor="log-search" className="w-64">
          <Input
            id="log-search"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            placeholder={t('logs.searchPlaceholder', 'Substring of the message')}
          />
        </Field>
        <Field label={t('logs.level', 'Level')} htmlFor="log-level" className="w-40">
          <Select id="log-level" value={level} onChange={(event) => setLevel(event.target.value)}>
            <option value="">{t('logs.allLevels', 'All levels')}</option>
            {LEVELS.map((item) => (
              <option key={item} value={item}>
                {item.toUpperCase()}
              </option>
            ))}
          </Select>
        </Field>
        <div className="ml-auto flex items-center gap-2">
          {streaming && (
            <span className="flex items-center gap-1.5 text-micro text-ink-2">
              <StatusDot tone="running" pulse label={t('logs.live', 'Live')} />
              {t('logs.live', 'Live')}
            </span>
          )}
          <Button
            variant="ghost"
            size="sm"
            aria-pressed={follow}
            icon={<ArrowDownToLine className="h-3.5 w-3.5" />}
            onClick={() => setFollow((current) => !current)}
          >
            {follow ? t('logs.following', 'Following') : t('logs.paused', 'Paused')}
          </Button>
        </div>
      </div>

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      <div className="min-h-0 flex-1 overflow-hidden rounded-panel border border-line">
        {loading && logs.length === 0 ? (
          <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
        ) : logs.length === 0 ? (
          <EmptyState
            size="inline"
            icon={<ScrollText className="h-5 w-5" />}
            title={t('logs.noLogs', 'No log entries')}
            body={t('logs.description', 'Daemon log tail, newest first')}
          />
        ) : (
          <div
            ref={listRef}
            // Scrollable regions must be reachable by keyboard so the tail can be read without a mouse.
            tabIndex={0}
            role="log"
            aria-label={t('logs.title', 'Logs')}
            className="max-h-[calc(100vh-260px)] overflow-auto focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          >
            <ul className="min-w-[720px]">
              {logs.map((log, index) => (
                <li
                  key={`${log.timestamp}-${index}`}
                  className="flex items-baseline gap-3 border-b border-line px-3 py-1.5 text-micro last:border-b-0 hover-tint"
                >
                  <span className="tabular shrink-0 font-mono text-ink-3">
                    {formatTimestamp(log.timestamp)}
                  </span>
                  <span
                    className={cn(
                      'w-12 shrink-0 font-mono uppercase',
                      LEVEL_CLASS[log.level.toLowerCase()] ?? 'text-ink-3'
                    )}
                  >
                    {log.level}
                  </span>
                  <span className="w-40 shrink-0 truncate font-mono text-ink-3">{log.target}</span>
                  <span className="min-w-0 flex-1 break-words font-mono text-ink">{log.message}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  )
}
