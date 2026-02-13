import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { RoleInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const Roles: React.FC = () => {
  const [roles, setRoles] = useState<RoleInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedRole, setSelectedRole] = useState<RoleInfo | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    const fetchRoles = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getRoles()
        setRoles(data)
        setError(null)
      } catch (err) {
        setError(t('roles.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchRoles()
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
          {roles.map((role) => (
            <div
              key={role.name}
              onClick={() => setSelectedRole(role)}
              className={`bg-surface rounded-lg shadow p-4 cursor-pointer transition-all hover:shadow-lg ${
                selectedRole?.name === role.name ? 'ring-2 ring-blue-500' : ''
              }`}
            >
              <h3 className="text-lg font-semibold text-text">{role.name}</h3>
              <div className="mt-2 space-y-1 text-sm text-text-secondary">
                <p>{t('roles.model')}: {role.provider}/{role.model}</p>
                <p>{t('roles.skills')}: {role.skills.length}</p>
                <p>{t('roles.tools')}: {role.tools.length}</p>
              </div>
            </div>
          ))}
        </div>

        {selectedRole && (
          <div className="bg-surface rounded-lg shadow p-6">
            <h3 className="text-xl font-bold text-text mb-4">
              {selectedRole.name}
            </h3>

            <div className="space-y-4">
              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('roles.systemPrompt')}</h4>
                <div className="bg-gray-50 rounded p-3 text-sm text-gray-800 whitespace-pre-wrap max-h-48 overflow-y-auto">
                  {selectedRole.system_prompt}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('roles.modelConfiguration')}</h4>
                <div className="bg-gray-50 rounded p-3 space-y-1 text-sm">
                  <p><span className="font-medium">{t('roles.provider')}:</span> {selectedRole.provider}</p>
                  <p><span className="font-medium">{t('roles.model')}:</span> {selectedRole.model}</p>
                  <p><span className="font-medium">{t('roles.maxTokens')}:</span> {selectedRole.max_tokens}</p>
                  <p><span className="font-medium">{t('roles.temperature')}:</span> {selectedRole.temperature}</p>
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('roles.skills')}</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedRole.skills.map((skill) => (
                    <span
                      key={skill}
                      className="px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-sm"
                    >
                      {skill}
                    </span>
                  ))}
                  {selectedRole.skills.length === 0 && (
                    <span className="text-gray-500 text-sm">{t('roles.noSkills')}</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('roles.tools')}</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedRole.tools.map((tool) => (
                    <span
                      key={tool}
                      className="px-3 py-1 bg-green-100 text-green-800 rounded-full text-sm"
                    >
                      {tool}
                    </span>
                  ))}
                  {selectedRole.tools.length === 0 && (
                    <span className="text-gray-500 text-sm">{t('roles.noTools')}</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('roles.workspace')}</h4>
                <code className="block bg-gray-50 rounded p-3 text-sm text-gray-800">
                  {selectedRole.workspace_dir}
                </code>
              </div>
            </div>
          </div>
        )}
      </div>

      {roles.length === 0 && (
        <div className="text-center py-12 text-text-secondary">
          {t('roles.noRoles')}
        </div>
      )}
    </div>
  )
}

export default Roles
