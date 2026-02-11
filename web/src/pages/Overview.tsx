import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SystemStatus } from '../types/api'

const Overview: React.FC = () => {
  const [status, setStatus] = useState<SystemStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const fetchStatus = async () => {
    try {
      setLoading(true)
      const data = await apiClient.getStatus()
      setStatus(data)
      setError(null)
    } catch (err) {
      setError('Failed to fetch system status')
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
        <div className="text-text-secondary">Loading...</div>
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
      title: 'System Status',
      value: status.running ? 'Running' : 'Stopped',
      color: status.running ? 'success' : 'error',
      icon: '🟢',
    },
    {
      title: 'Active Sessions',
      value: status.session_count,
      color: 'primary',
      icon: '💬',
    },
    {
      title: 'Channels',
      value: status.channel_count,
      color: 'accent',
      icon: '📡',
    },
    {
      title: 'Cron Jobs',
      value: status.cron_job_count,
      color: 'warning',
      icon: '⏰',
    },
    {
      title: 'Roles',
      value: status.role_count,
      color: 'secondary',
      icon: '👤',
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
        <h2 className="text-2xl font-bold text-text">System Overview</h2>
        <p className="text-text-secondary mt-1">
          Uptime: {formatUptime(status.uptime_secs)}
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
              <div className="text-4xl">{card.icon}</div>
            </div>
          </div>
        ))}
      </div>

      <div className="mt-4 text-sm text-text-secondary">
        Last updated: {new Date().toLocaleTimeString()} • Auto-refresh: 30s
      </div>
    </div>
  )
}

export default Overview
