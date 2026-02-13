import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SanitizedConfig } from '../types/api'
import ThemePreview from '../components/ThemePreview'
import { useI18n } from '../i18n/I18nContext'
import {
  ChannelsIcon,
  ProvidersIcon,
  AgentIcon,
  ToolsIcon,
  ThemeIcon,
  WebIcon,
  LogsIcon
} from '../components/icons'

const Config: React.FC = () => {
  const [config, setConfig] = useState<SanitizedConfig | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set(['channels']))
  const { t } = useI18n()

  useEffect(() => {
    const fetchConfig = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getConfig()
        setConfig(data)
        setError(null)
      } catch (err) {
        setError(t('config.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchConfig()
  }, [])

  const toggleSection = (section: string) => {
    setExpandedSections((prev) => {
      const next = new Set(prev)
      if (next.has(section)) {
        next.delete(section)
      } else {
        next.add(section)
      }
      return next
    })
  }

  const renderValue = (value: unknown): React.ReactNode => {
    if (value === null || value === undefined) {
      return <span className="text-text-secondary italic">null</span>
    }
    if (typeof value === 'boolean') {
      return (
        <span className={value ? 'text-success' : 'text-error'}>
          {value.toString()}
        </span>
      )
    }
    if (typeof value === 'number') {
      return <span className="text-primary">{value}</span>
    }
    if (typeof value === 'string') {
      // Check if it looks like a masked value
      if (value.includes('***') || value === '[REDACTED]') {
        return <span className="text-warning font-mono">{value}</span>
      }
      return <span className="text-text">{value}</span>
    }
    if (Array.isArray(value)) {
      if (value.length === 0) {
        return <span className="text-text-secondary italic">[]</span>
      }
      return (
        <div className="ml-4 space-y-1">
          {value.map((item, idx) => (
            <div key={idx} className="flex items-start gap-2">
              <span className="text-text-secondary">-</span>
              {renderValue(item)}
            </div>
          ))}
        </div>
      )
    }
    if (typeof value === 'object') {
      return (
        <div className="ml-4 space-y-2">
          {Object.entries(value as Record<string, unknown>).map(([key, val]) => (
            <div key={key}>
              <span className="font-medium text-text">{key}:</span>{' '}
              {renderValue(val)}
            </div>
          ))}
        </div>
      )
    }
    return <span className="text-text">{String(value)}</span>
  }

  const renderSection = (title: string, data: Record<string, unknown> | undefined | null) => {
    const isExpanded = expandedSections.has(title)
    const isEmpty = !data || Object.keys(data).length === 0

    const getIcon = (sectionTitle: string) => {
      switch (sectionTitle) {
        case 'channels':
          return ChannelsIcon
        case 'providers':
          return ProvidersIcon
        case 'agent':
          return AgentIcon
        case 'tools':
          return ToolsIcon
        case 'theme':
          return ThemeIcon
        case 'web':
          return WebIcon
        case 'log':
          return LogsIcon
        default:
          return ChannelsIcon
      }
    }

    const IconComponent = getIcon(title)

    return (
      <div key={title} className="bg-surface border border-border rounded-lg">
        <button
          onClick={() => toggleSection(title)}
          className="w-full px-6 py-4 flex items-center justify-between hover:bg-surface/80 transition-colors"
        >
          <div className="flex items-center gap-3">
            <IconComponent className="w-5 h-5 text-text-secondary" />
            <h3 className="text-lg font-semibold text-text capitalize">
              {title}
            </h3>
            {isEmpty && title !== 'theme' && (
              <span className="text-sm text-text-secondary italic">{t('config.empty')}</span>
            )}
          </div>
          <span className="text-text-secondary">
            {isExpanded ? '▼' : '▶'}
          </span>
        </button>

        {isExpanded && (
          <div className="px-6 pb-4 border-t border-border">
            {title === 'theme' ? (
              <div className="mt-4">
                <ThemePreview />
              </div>
            ) : !isEmpty && data ? (
              <div className="bg-surface/50 rounded p-4 mt-4">
                <div className="space-y-3 text-sm">
                  {Object.entries(data).map(([key, value]) => (
                    <div key={key} className="border-b border-border pb-2 last:border-0">
                      <div className="font-medium text-text mb-1">{key}:</div>
                      <div className="ml-2">{renderValue(value)}</div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        )}
      </div>
    )
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

  if (!config) {
    return (
      <div className="text-center py-12 text-text-secondary">
        {t('config.noConfiguration')}
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('config.title')}</h2>
        <p className="text-text-secondary mt-1">
          {t('config.description')}
        </p>
      </div>

      <div className="space-y-4">
        {renderSection('channels', config.channels)}
        {renderSection('providers', config.providers)}
        {renderSection('agent', config.agent)}
        {renderSection('tools', config.tools)}
        {config.web && renderSection('web', config.web as Record<string, unknown>)}
        {config.log && renderSection('log', config.log as Record<string, unknown>)}
        {renderSection('theme', { preview: 'theme_preview' })}
      </div>

      <div className="mt-6 bg-warning/10 border border-warning/20 rounded-lg p-4">
        <p className="text-sm text-warning">
          {t('config.note')}
        </p>
      </div>
    </div>
  )
}

export default Config
