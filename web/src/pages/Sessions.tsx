import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SessionSummary, SessionDetail } from '../types/api'

const Sessions: React.FC = () => {
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [selectedSession, setSelectedSession] = useState<SessionDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [channelFilter, setChannelFilter] = useState<string>('')
  const [scopeFilter, setScopeFilter] = useState<string>('')

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
        setError('Failed to fetch sessions')
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
        <div className="text-gray-500">Loading...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-800">{error}</p>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-gray-900">Sessions</h2>
        <p className="text-gray-600 mt-1">View and manage active sessions</p>
      </div>

      <div className="mb-6 flex gap-4">
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Channel
          </label>
          <input
            type="text"
            value={channelFilter}
            onChange={(e) => setChannelFilter(e.target.value)}
            placeholder="Filter by channel"
            className="px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Scope
          </label>
          <input
            type="text"
            value={scopeFilter}
            onChange={(e) => setScopeFilter(e.target.value)}
            placeholder="Filter by scope"
            className="px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div>
          <h3 className="text-lg font-semibold text-gray-900 mb-4">
            Session List ({sessions.length})
          </h3>
          <div className="space-y-3">
            {sessions.map((session) => (
              <div
                key={session.id}
                onClick={() => handleSessionClick(session.id)}
                className={`bg-white rounded-lg shadow p-4 cursor-pointer hover:shadow-md transition-shadow ${
                  selectedSession?.meta.id === session.id
                    ? 'ring-2 ring-blue-500'
                    : ''
                }`}
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1">
                    <p className="font-medium text-gray-900 truncate">
                      {session.identifier}
                    </p>
                    <div className="flex items-center gap-2 mt-1 text-sm text-gray-600">
                      <span className="px-2 py-0.5 bg-blue-100 text-blue-800 rounded">
                        {session.channel}
                      </span>
                      <span>{session.scope}</span>
                    </div>
                  </div>
                  <div className="text-right text-sm text-gray-500">
                    <div>{session.message_count} msgs</div>
                  </div>
                </div>
                <div className="mt-2 text-xs text-gray-500">
                  Updated: {formatDate(session.updated_at)}
                </div>
              </div>
            ))}
          </div>
          {sessions.length === 0 && (
            <div className="text-center py-12 text-gray-500">
              No sessions found
            </div>
          )}
        </div>

        <div>
          <h3 className="text-lg font-semibold text-gray-900 mb-4">
            Session Details
          </h3>
          {detailLoading ? (
            <div className="bg-white rounded-lg shadow p-6 text-center text-gray-500">
              Loading...
            </div>
          ) : selectedSession ? (
            <div className="bg-white rounded-lg shadow p-6">
              <div className="mb-4 pb-4 border-b border-gray-200">
                <h4 className="font-semibold text-gray-900">
                  {selectedSession.meta.identifier}
                </h4>
                <div className="mt-2 space-y-1 text-sm text-gray-600">
                  <p>
                    <span className="font-medium">ID:</span>{' '}
                    {selectedSession.meta.id}
                  </p>
                  <p>
                    <span className="font-medium">Channel:</span>{' '}
                    {selectedSession.meta.channel}
                  </p>
                  <p>
                    <span className="font-medium">Scope:</span>{' '}
                    {selectedSession.meta.scope}
                  </p>
                  <p>
                    <span className="font-medium">Created:</span>{' '}
                    {formatDate(selectedSession.meta.created_at)}
                  </p>
                </div>
              </div>

              <div>
                <h5 className="font-medium text-gray-900 mb-3">
                  Message History ({selectedSession.messages.length})
                </h5>
                <div className="space-y-3 max-h-96 overflow-y-auto">
                  {selectedSession.messages.map((msg, idx) => (
                    <div
                      key={idx}
                      className="bg-gray-50 rounded p-3 text-sm"
                    >
                      <div className="flex items-center justify-between mb-1">
                        <span className="font-medium text-gray-700">
                          {msg.role}
                        </span>
                        <span className="text-xs text-gray-500">
                          {formatDate(msg.timestamp)}
                        </span>
                      </div>
                      <p className="text-gray-600 whitespace-pre-wrap">
                        {msg.content}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            <div className="bg-white rounded-lg shadow p-6 text-center text-gray-500">
              Select a session to view details
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

export default Sessions
