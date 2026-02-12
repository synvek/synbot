import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SystemStatus } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import {
  SystemStatusIcon,
  ActiveSessionsIcon,
  ChannelsIcon,
  CronIcon,
  RolesIcon
} from '../components/icons'

const Overview: React.FC = () => {
  const [status, setStatus] = useState<SystemStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  const fetchStatus = async () => {
    try {
      setLoading(true)
      const data = await apiClient.getStatus()
      setStatus(data)
      setError(null)
    } catch (err) {
      setError(t('overview.failedToFetch'))
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchStatus()
    
    // Auto-refresh every 30 seconds
    const interval = setInterval(fetchStatus, 30000)
    
    return () => clearInterval(interval)
  }, [])

  if (loading && !status) {
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

  if (!status) return null

  const cards = [
    {
      title: t('overview.systemStatus'),
      value: status.running ? t('common.running') : t('common.stopped'),
      color: status.running ? 'success' : 'error',
      icon: SystemStatusIcon,
    },
    {
      title: t('overview.activeSessions'),
      value: status.session_count,
      color: 'primary',
      icon: ActiveSessionsIcon,
    },
    {
      title: t('overview.channels'),
      value: status.channel_count,
      color: 'accent',
      icon: ChannelsIcon,
    },
    {
      title: t('overview.cronJobs'),
      value: status.cron_job_count,
      color: 'warning',
      icon: CronIcon,
    },
    {
      title: t('overview.roles'),
      value: status.role_count,
      color: 'secondary',
      icon: RolesIcon,
    },
  ]

  const formatUptime = (seconds: number) => {
    const days = Math.floor(seconds / 86400)
    const hours = Math.floor((seconds % 86400) / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    
    if (days > 0) return `${days}d ${hours}h ${minutes}m`
    if (hours > 0) return `${hours}h ${minutes}m`
    return `${minutes}m`
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('overview.title')}</h2>
        <p className="text-text-secondary mt-1">
          {t('overview.uptime')}: {formatUptime(status.uptime_secs)}
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {cards.map((card) => (
          <div
            key={card.title}
            className="bg-surface border border-border rounded-lg p-6 hover:shadow-lg transition-all hover:scale-[1.02]"
          >
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-text-secondary mb-1">{card.title}</p>
                <p className={`text-3xl font-bold text-${card.color}`}>{card.value}</p>
              </div>
              <card.icon className="w-10 h-10 text-text-secondary" />
            </div>
          </div>
        ))}
      </div>

      <div className="mt-4 text-sm text-text-secondary">
        {t('overview.lastUpdated')}: {new Date().toLocaleTimeString()} • {t('overview.autoRefresh')}: 30s
      </div>
    </div>
  )
}

export default Overview
