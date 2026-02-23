import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { AgentInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const Roles: React.FC = () => {
  const [agents, setAgents] = useState<AgentInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedAgent, setSelectedAgent] = useState<AgentInfo | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    const fetchAgents = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getAgents()
        setAgents(data)
        setError(null)
      } catch (err) {
        setError(t('roles.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchAgents()
  }, [t])

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
        <h2 className="text-2xl font-bold text-text">{t('roles.title')}</h2>
        <p className="text-text-secondary mt-1">{t('roles.description')}</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="space-y-4">
          {agents.map((agent) => (
            <div
              key={agent.name}
              onClick={() => setSelectedAgent(agent)}
              className={`bg-surface rounded-lg shadow p-4 cursor-pointer transition-all hover:shadow-lg ${
                selectedAgent?.name === agent.name ? 'ring-2 ring-primary' : ''
              }`}
            >
              <h3 className="text-lg font-semibold text-text">{agent.name}</h3>
              <div className="mt-2 space-y-1 text-sm text-text-secondary">
                <p>{t('roles.role')}: {agent.role}</p>
                <p>{t('roles.model')}: {agent.provider}/{agent.model}</p>
                <p>{t('roles.skills')}: {agent.skills.length}</p>
                <p>{t('roles.tools')}: {agent.tools.length}</p>
              </div>
            </div>
          ))}
        </div>

        {selectedAgent && (
          <div className="bg-surface rounded-lg shadow p-6">
            <h3 className="text-xl font-bold text-text mb-4">
              {selectedAgent.name}
            </h3>

            <div className="space-y-4">
              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.role')}</h4>
                <p className="text-text">{selectedAgent.role}</p>
              </div>

              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.systemPrompt')}</h4>
                <div className="bg-background border border-border rounded p-3 text-sm text-text whitespace-pre-wrap max-h-48 overflow-y-auto">
                  {selectedAgent.system_prompt}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.modelConfiguration')}</h4>
                <div className="bg-background border border-border rounded p-3 space-y-1 text-sm text-text">
                  <p><span className="font-medium">{t('roles.provider')}:</span> {selectedAgent.provider}</p>
                  <p><span className="font-medium">{t('roles.model')}:</span> {selectedAgent.model}</p>
                  <p><span className="font-medium">{t('roles.maxTokens')}:</span> {selectedAgent.max_tokens}</p>
                  <p><span className="font-medium">{t('roles.temperature')}:</span> {selectedAgent.temperature}</p>
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.skills')}</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedAgent.skills.map((skill) => (
                    <span
                      key={skill}
                      className="px-3 py-1 bg-primary-muted text-primary rounded-full text-sm"
                    >
                      {skill}
                    </span>
                  ))}
                  {selectedAgent.skills.length === 0 && (
                    <span className="text-text-secondary text-sm">{t('roles.noSkills')}</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.tools')}</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedAgent.tools.map((tool) => (
                    <span
                      key={tool}
                      className="px-3 py-1 bg-background border border-border text-text rounded-full text-sm"
                    >
                      {tool}
                    </span>
                  ))}
                  {selectedAgent.tools.length === 0 && (
                    <span className="text-text-secondary text-sm">{t('roles.noTools')}</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-text-secondary mb-2">{t('roles.workspace')}</h4>
                <code className="block bg-background border border-border rounded p-3 text-sm text-text font-mono">
                  {selectedAgent.workspace_dir}
                </code>
              </div>
            </div>
          </div>
        )}
      </div>

      {agents.length === 0 && (
        <div className="text-center py-12 text-text-secondary">
          {t('roles.noRoles')}
        </div>
      )}
    </div>
  )
}

export default Roles
