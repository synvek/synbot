import React, { useEffect, useState, useRef } from 'react'
import { apiClient } from '../api/client'
import type { LogEntry, LogQueryParams } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const LOG_LEVELS = ['error', 'warn', 'info', 'debug']

const Logs: React.FC = () => {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [filters, setFilters] = useState<LogQueryParams>({
    level: undefined,
    keyword: '',
    page: 0,
    page_size: 100,
  })
  const [isStreaming, setIsStreaming] = useState(false)
  const wsRef = useRef<WebSocket | null>(null)
  const logsEndRef = useRef<HTMLDivElement>(null)
  const { t } = useI18n()

  const fetchLogs = async () => {
    try {
      setLoading(true)
      const response = await apiClient.getLogs(filters)
      setLogs(response.items)
      setError(null)
    } catch (err) {
      setError(t('logs.failedToFetch'))
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchLogs()
  }, [filters.level, filters.keyword])

  const scrollToBottom = () => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }

  const startStreaming = () => {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsUrl = `${protocol}//${window.location.host}/ws/logs`
    
    const ws = new WebSocket(wsUrl)
    
    ws.onopen = () => {
      setIsStreaming(true)
      console.log('WebSocket connected for log streaming')
    }
    
    ws.onmessage = (event) => {
      try {
        const logEntry: LogEntry = JSON.parse(event.data)
        setLogs((prev) => [logEntry, ...prev].slice(0, filters.page_size || 100))
      } catch (err) {
        console.error('Failed to parse log message:', err)
      }
    }
    
    ws.onerror = (err) => {
      console.error('WebSocket error:', err)
      setError(t('logs.wsError'))
    }
    
    ws.onclose = () => {
      setIsStreaming(false)
      console.log('WebSocket disconnected')
    }
    
    wsRef.current = ws
  }

  const stopStreaming = () => {
    if (wsRef.current) {
      wsRef.current.close()
      wsRef.current = null
    }
    setIsStreaming(false)
  }

  useEffect(() => {
    return () => {
      if (wsRef.current) {
        wsRef.current.close()
      }
    }
  }, [])

  const getLevelColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'error':
        return 'text-red-600 bg-red-50'
      case 'warn':
        return 'text-yellow-600 bg-yellow-50'
      case 'info':
        return 'text-blue-600 bg-blue-50'
      case 'debug':
        return 'text-gray-600 bg-gray-50'
      default:
        return 'text-gray-600 bg-gray-50'
    }
  }

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString()
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('logs.title')}</h2>
        <p className="text-text-secondary mt-1">{t('logs.description')}</p>
      </div>

      <div className="bg-surface rounded-lg shadow p-4 mb-4">
        <div className="flex flex-wrap gap-4 items-end">
          <div className="flex-1 min-w-[200px]">
            <label className="block text-sm font-medium text-text mb-1">
              {t('logs.search')}
            </label>
            <input
              type="text"
              value={filters.keyword || ''}
              onChange={(e) => setFilters({ ...filters, keyword: e.target.value })}
              placeholder={t('logs.searchPlaceholder')}
              className="w-full px-3 py-2 bg-background border border-border rounded-lg text-text placeholder:text-text-secondary focus:ring-2 focus:ring-primary focus:border-transparent"
            />
          </div>

          <div className="min-w-[150px]">
            <label className="block text-sm font-medium text-text mb-1">
              {t('logs.level')}
            </label>
            <select
              value={filters.level || ''}
              onChange={(e) => setFilters({ ...filters, level: e.target.value || undefined })}
              className="w-full px-3 py-2 bg-background border border-border rounded-lg text-text focus:ring-2 focus:ring-primary focus:border-transparent"
            >
              <option value="">{t('logs.allLevels')}</option>
              {LOG_LEVELS.map((level) => (
                <option key={level} value={level}>
                  {level.toUpperCase()}
                </option>
              ))}
            </select>
          </div>

          <div className="flex gap-2">
            <button
              onClick={fetchLogs}
              disabled={loading}
              className="px-4 py-2 border border-border bg-surface text-text rounded-lg hover:bg-background disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {loading ? t('common.loading') : t('logs.refresh')}
            </button>

            <button
              onClick={isStreaming ? stopStreaming : startStreaming}
              className={`px-4 py-2 rounded-lg text-white transition-colors ${
                isStreaming
                  ? 'bg-error hover:opacity-90'
                  : 'bg-primary hover:opacity-90'
              }`}
            >
              {isStreaming ? t('logs.stopStream') : t('logs.startStream')}
            </button>
          </div>
        </div>
      </div>

      {error && (
        <div className="bg-error/10 border border-error/30 rounded-lg p-4 mb-4">
          <p className="text-error">{error}</p>
        </div>
      )}

      <div className="bg-surface rounded-lg shadow">
        <div className="p-4 border-b border-border flex items-center justify-between">
          <div className="text-sm text-text-secondary">
            {t('logs.showing')} {logs.length} {t('logs.logEntries')}
            {isStreaming && (
              <span className="ml-2 inline-flex items-center gap-1 text-success">
                <span className="w-2 h-2 bg-success rounded-full animate-pulse"></span>
                {t('logs.live')}
              </span>
            )}
          </div>
          <button
            onClick={scrollToBottom}
            className="text-sm text-primary hover:opacity-90"
          >
            {t('logs.scrollToBottom')}
          </button>
        </div>

        <div className="max-h-[600px] overflow-y-auto">
          {loading && logs.length === 0 ? (
            <div className="flex items-center justify-center h-64">
              <div className="text-text-secondary">{t('common.loading')}</div>
            </div>
          ) : logs.length === 0 ? (
            <div className="text-center py-12 text-text-secondary">
              {t('logs.noLogs')}
            </div>
          ) : (
            <div className="divide-y divide-border">
              {logs.map((log, idx) => (
                <div
                  key={`${log.timestamp}-${idx}`}
                  className="p-3 hover:bg-surface transition-colors font-mono text-sm"
                >
                  <div className="flex items-start gap-3">
                    <span className="text-text-secondary text-xs whitespace-nowrap">
                      {formatTimestamp(log.timestamp)}
                    </span>
                    <span
                      className={`px-2 py-0.5 rounded text-xs font-medium uppercase whitespace-nowrap ${getLevelColor(
                        log.level
                      )}`}
                    >
                      {log.level}
                    </span>
                    <span className="text-text-secondary text-xs whitespace-nowrap">
                      {log.target}
                    </span>
                    <span className="text-text flex-1 break-words">
                      {log.message}
                    </span>
                  </div>
                </div>
              ))}
              <div ref={logsEndRef} />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default Logs
