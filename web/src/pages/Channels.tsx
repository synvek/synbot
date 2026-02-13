import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { ChannelInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const Channels: React.FC = () => {
  const [channels, setChannels] = useState<ChannelInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    const fetchChannels = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getChannels()
        setChannels(data)
        setError(null)
      } catch (err) {
        setError(t('channels.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchChannels()
  }, [t])

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'connected':
        return 'bg-green-100 text-green-800'
      case 'disconnected':
        return 'bg-gray-100 text-gray-800'
      case 'error':
        return 'bg-red-100 text-red-800'
      case 'disabled':
        return 'bg-yellow-100 text-yellow-800'
      default:
        return 'bg-gray-100 text-gray-800'
    }
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'connected':
        return '✅'
      case 'disconnected':
        return '⭕'
      case 'error':
        return '❌'
      case 'disabled':
        return '🚫'
      default:
        return '❓'
    }
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
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {channels.map((channel) => (
          <div
            key={channel.name}
            className="bg-surface rounded-lg shadow p-6 hover:shadow-lg transition-shadow"
          >
            <div className="flex items-start justify-between mb-4">
              <div>
                <h3 className="text-lg font-semibold text-text capitalize">
                  {channel.name}
                </h3>
                <div className="flex items-center gap-2 mt-2">
                  <span
                    className={`inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-medium ${getStatusColor(
                      channel.status
                    )}`}
                  >
                    {getStatusIcon(channel.status)}
                    {t(`channels.${channel.status}` as keyof typeof t)}
                  </span>
                  {channel.enabled ? (
                    <span className="text-xs text-green-600">{t('channels.enabled')}</span>
                  ) : (
                    <span className="text-xs text-gray-500">{t('channels.disabled')}</span>
                  )}
                </div>
              </div>
            </div>

            {channel.config && (
              <div className="mt-4 pt-4 border-t border-gray-200">
                <p className="text-sm font-medium text-gray-700 mb-2">
                  {t('channels.configuration')}
                </p>
                <div className="bg-gray-50 rounded p-3">
                  <pre className="text-xs text-gray-600 overflow-x-auto">
                    {JSON.stringify(channel.config, null, 2)}
                  </pre>
                </div>
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
