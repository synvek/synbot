import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { ChannelInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const formatTime = (value: string | null) => {
  if (!value) return '—'
  return new Date(value).toLocaleString()
}

const Channels: React.FC = () => {
  const [channels, setChannels] = useState<ChannelInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    let active = true
    const fetchChannels = async () => {
      try {
        const data = await apiClient.getChannels()
        if (active) {
          setChannels(data)
          setError(null)
        }
      } catch (err) {
        if (active) setError(t('channels.failedToFetch'))
        console.error(err)
      } finally {
        if (active) setLoading(false)
      }
    }

    fetchChannels()
    const interval = window.setInterval(fetchChannels, 5000)
    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [t])

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'connected':
        return 'bg-green-100 text-green-800'
      case 'starting':
      case 'reconnecting':
        return 'bg-blue-100 text-blue-800'
      case 'failed':
        return 'bg-red-100 text-red-800'
      case 'disabled':
        return 'bg-yellow-100 text-yellow-800'
      case 'stopped':
        return 'bg-gray-100 text-gray-800'
      default:
        return 'bg-gray-100 text-gray-800'
    }
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'connected': return '✅'
      case 'starting': return '⏳'
      case 'reconnecting': return '🔄'
      case 'failed': return '❌'
      case 'disabled': return '🚫'
      case 'stopped': return '⏹️'
      default: return '❓'
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-text-secondary">{t('common.loading')}</div>
      </div>
    )
  }

  if (error && channels.length === 0) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-800">{error}</p>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('channels.title')}</h2>
        <p className="text-text-secondary mt-1">{t('channels.description')}</p>
        {error && <p className="text-sm text-red-700 mt-2">{error}</p>}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {channels.map((channel) => (
          <div
            key={channel.instance_id}
            className="bg-surface rounded-lg shadow p-6 hover:shadow-lg transition-shadow"
          >
            <div className="flex items-start justify-between mb-4">
              <div>
                <h3 className="text-lg font-semibold text-text capitalize">
                  {channel.name}
                </h3>
                <p className="text-xs text-text-secondary mt-1">{channel.channel_type}</p>
                <div className="flex items-center gap-2 mt-2">
                  <span className={`inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-medium ${getStatusColor(channel.status)}`}>
                    {getStatusIcon(channel.status)}
                    {channel.status}
                  </span>
                  <span className="text-xs text-text-secondary">
                    {channel.enabled ? t('channels.enabled') : t('channels.disabled')}
                  </span>
                </div>
              </div>
            </div>

            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs text-text-secondary">
              <div><dt>Started</dt><dd className="text-text">{formatTime(channel.started_at)}</dd></div>
              <div><dt>Last connected</dt><dd className="text-text">{formatTime(channel.last_connected_at)}</dd></div>
              <div><dt>Last received</dt><dd className="text-text">{formatTime(channel.last_received_at)}</dd></div>
              <div><dt>Last sent</dt><dd className="text-text">{formatTime(channel.last_sent_at)}</dd></div>
              <div><dt>Reconnects</dt><dd className="text-text">{channel.reconnect_count}</dd></div>
              <div><dt>Latency</dt><dd className="text-text">{channel.last_latency_ms == null ? '—' : `${channel.last_latency_ms} ms`}</dd></div>
              <div><dt>Send</dt><dd className="text-text">{channel.supports_send ? 'Yes' : 'No'}</dd></div>
              <div><dt>Receive</dt><dd className="text-text">{channel.supports_receive ? 'Yes' : 'No'}</dd></div>
            </dl>

            {channel.last_error && (
              <div className="mt-4 rounded bg-red-50 p-3 text-xs text-red-800 break-words">
                {channel.last_error}
              </div>
            )}
          </div>
        ))}
      </div>

      {channels.length === 0 && (
        <div className="text-center py-12 text-text-secondary">
          {t('channels.noChannels')}
        </div>
      )}
    </div>
  )
}

export default Channels
