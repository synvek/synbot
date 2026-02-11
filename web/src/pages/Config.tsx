import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SanitizedConfig } from '../types/api'

const Config: React.FC = () => {
  const [config, setConfig] = useState<SanitizedConfig | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set(['channels']))

  useEffect(() => {
    const fetchConfig = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getConfig()
        setConfig(data)
        setError(null)
      } catch (err) {
        setError('Failed to fetch configuration')
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
      return <span className="text-gray-400 italic">null</span>
    }
    if (typeof value === 'boolean') {
      return (
        <span className={value ? 'text-green-600' : 'text-red-600'}>
          {value.toString()}
        </span>
      )
    }
    if (typeof value === 'number') {
      return <span className="text-blue-600">{value}</span>
    }
    if (typeof value === 'string') {
      // Check if it looks like a masked value
      if (value.includes('***') || value === '[REDACTED]') {
        return <span className="text-orange-600 font-mono">{value}</span>
      }
      return <span className="text-gray-800">{value}</span>
    }
    if (Array.isArray(value)) {
      if (value.length === 0) {
        return <span className="text-gray-400 italic">[]</span>
      }
      return (
        <div className="ml-4 space-y-1">
          {value.map((item, idx) => (
            <div key={idx} className="flex items-start gap-2">
              <span className="text-gray-500">-</span>
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
              <span className="font-medium text-gray-700">{key}:</span>{' '}
              {renderValue(val)}
            </div>
          ))}
        </div>
      )
    }
    return <span className="text-gray-800">{String(value)}</span>
  }

  const renderSection = (title: string, data: Record<string, unknown> | undefined | null) => {
    const isExpanded = expandedSections.has(title)
    const isEmpty = !data || Object.keys(data).length === 0

    return (
      <div key={title} className="bg-white rounded-lg shadow">
        <button
          onClick={() => toggleSection(title)}
          className="w-full px-6 py-4 flex items-center justify-between hover:bg-gray-50 transition-colors"
        >
          <div className="flex items-center gap-3">
            <span className="text-xl">
              {title === 'channels' && '📡'}
              {title === 'providers' && '🔌'}
              {title === 'agent' && '🤖'}
              {title === 'tools' && '🔧'}
            </span>
            <h3 className="text-lg font-semibold text-gray-900 capitalize">
              {title}
            </h3>
            {isEmpty && (
              <span className="text-sm text-gray-500 italic">(empty)</span>
            )}
          </div>
          <span className="text-gray-400">
            {isExpanded ? '▼' : '▶'}
          </span>
        </button>

        {isExpanded && !isEmpty && data && (
          <div className="px-6 pb-4 border-t border-gray-100">
            <div className="bg-gray-50 rounded p-4 mt-4">
              <div className="space-y-3 text-sm">
                {Object.entries(data).map(([key, value]) => (
                  <div key={key} className="border-b border-gray-200 pb-2 last:border-0">
                    <div className="font-medium text-gray-700 mb-1">{key}:</div>
                    <div className="ml-2">{renderValue(value)}</div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    )
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

  if (!config) {
    return (
      <div className="text-center py-12 text-gray-500">
        No configuration available
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-gray-900">Configuration</h2>
        <p className="text-gray-600 mt-1">
          System configuration (sensitive values are masked)
        </p>
      </div>

      <div className="space-y-4">
        {renderSection('channels', config.channels)}
        {renderSection('providers', config.providers)}
        {renderSection('agent', config.agent)}
        {renderSection('tools', config.tools)}
        {config.web && renderSection('web', config.web as Record<string, unknown>)}
      </div>

      <div className="mt-6 bg-yellow-50 border border-yellow-200 rounded-lg p-4">
        <p className="text-sm text-yellow-800">
          <span className="font-medium">Note:</span> Sensitive values like API keys,
          tokens, and passwords are masked for security.
        </p>
      </div>
    </div>
  )
}

export default Config
