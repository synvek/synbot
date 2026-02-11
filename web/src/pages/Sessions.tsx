import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SessionSummary, SessionDetail } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const Sessions: React.FC = () => {
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [selectedSession, setSelectedSession] = useState<SessionDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [channelFilter, setChannelFilter] = useState<string>('')
  const [scopeFilter, setScopeFilter] = useState<string>('')
  const { t } = useI18n()

  useEffect(() => {
    const fetchSessions = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getSessions(
          undefined,
          undefined,
          channelFilter || undefined,
          scopeFilter || undefined
        )
        setSessions(data.items)
        setError(null)
      } catch (err) {
        setError(t('sessions.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchSessions()
  }, [channelFilter, scopeFilter])

  const handleSessionClick = async (id: string) => {
    try {
      setDetailLoading(true)
      const detail = await apiClient.getSession(id)
      setSelectedSession(detail)
    } catch (err) {
      console.error('Failed to fetch session detail:', err)
    } finally {
      setDetailLoading(false)
    }
  }

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString()
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-text-secondary">{t('common.loading')}</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="bg-error/10 border border-error/20 rounded-lg p-4">
        <p className="text-error">{error}</p>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('sessions.title')}</h2>
        <p className="text-text-secondary mt-1">{t('sessions.description')}</p>
      </div>

      <div className="mb-6 flex gap-4">
        <div>
          <label className="block text-sm font-medium text-text-secondary mb-1">
            {t('sessions.channel')}
          </label>
          <input
            type="text"
            value={channelFilter}
            onChange={(e) => setChannelFilter(e.target.value)}
            placeholder={t('sessions.filterByChannel')}
            className="px-3 py-2 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-surface text-text"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-text-secondary mb-1">
            {t('sessions.scope')}
          </label>
          <input
            type="text"
            value={scopeFilter}
            onChange={(e) => setScopeFilter(e.target.value)}
            placeholder={t('sessions.filterByScope')}
            className="px-3 py-2 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-surface text-text"
          />
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div>
          <h3 className="text-lg font-semibold text-text mb-4">
            {t('sessions.sessionList')} ({sessions.length})
          </h3>
          <div className="space-y-3">
            {sessions.map((session) => (
              <div
                key={session.id}
                onClick={() => handleSessionClick(session.id)}
                className={`bg-surface border border-border rounded-lg p-4 cursor-pointer hover:shadow-md transition-all hover:scale-[1.01] ${
                  selectedSession?.meta.id === session.id
                    ? 'ring-2 ring-primary'
                    : ''
                }`}
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <p className="font-medium text-text truncate">
                      {session.identifier || t('sessions.anonymous')}
                    </p>
                    <div className="flex items-center gap-2 mt-1 text-sm text-text-secondary">
                      <span className="px-2 py-0.5 bg-primary/10 text-primary rounded">
                        {session.channel}
                      </span>
                      <span>{session.scope || t('sessions.direct')}</span>
                    </div>
                  </div>
                  <div className="text-right text-sm text-text-secondary">
                    <div>{session.message_count} {t('sessions.messages')}</div>
                  </div>
                </div>
                <div className="mt-2 text-xs text-text-secondary">
                  {t('sessions.updated')}: {formatDate(session.updated_at)}
                </div>
              </div>
            ))}
          </div>
          {sessions.length === 0 && (
            <div className="text-center py-12 text-text-secondary">
              {t('sessions.noSessionsFound')}
            </div>
          )}
        </div>

        <div>
          <h3 className="text-lg font-semibold text-text mb-4">
            {t('sessions.sessionDetails')}
          </h3>
          {detailLoading ? (
            <div className="bg-surface border border-border rounded-lg p-6 text-center text-text-secondary">
              {t('common.loading')}
            </div>
          ) : selectedSession ? (
            <div className="bg-surface border border-border rounded-lg p-6">
              <div className="mb-4 pb-4 border-b border-border">
                <h4 className="font-semibold text-text">
                  {selectedSession.meta.identifier || t('sessions.anonymous')}
                </h4>
                <div className="mt-2 space-y-1 text-sm text-text-secondary">
                  <p>
                    <span className="font-medium">{t('sessions.id')}:</span>{' '}
                    {selectedSession.meta.id}
                  </p>
                  <p>
                    <span className="font-medium">{t('sessions.channel')}:</span>{' '}
                    {selectedSession.meta.channel}
                  </p>
                  <p>
                    <span className="font-medium">{t('sessions.scope')}:</span>{' '}
                    {selectedSession.meta.scope || t('sessions.direct')}
                  </p>
                  <p>
                    <span className="font-medium">{t('sessions.created')}:</span>{' '}
                    {formatDate(selectedSession.meta.created_at)}
                  </p>
                </div>
              </div>

              <div>
                <h5 className="font-medium text-text mb-3">
                  {t('sessions.messageHistory')} ({selectedSession.messages.length})
                </h5>
                <div className="space-y-3 max-h-96 overflow-y-auto">
                  {selectedSession.messages.map((msg, idx) => (
                    <div
                      key={idx}
                      className="bg-background rounded p-3 text-sm border border-border"
                    >
                      <div className="flex items-center justify-between mb-1">
                        <span className={`font-medium ${
                          msg.role === 'user' ? 'text-primary' : 'text-text'
                        }`}>
                          {msg.role === 'user' ? t('chat.user') : t('chat.assistant')}
                        </span>
                        <span className="text-xs text-text-secondary">
                          {formatDate(msg.timestamp)}
                        </span>
                      </div>
                      <p className="text-text whitespace-pre-wrap">
                        {msg.content}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            <div className="bg-surface border border-border rounded-lg p-6 text-center text-text-secondary">
              {t('sessions.selectSessionToView')}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default Sessions
