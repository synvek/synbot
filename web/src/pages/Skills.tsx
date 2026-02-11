import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { SkillInfo, SkillDetail } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const Skills: React.FC = () => {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [selectedSkill, setSelectedSkill] = useState<SkillDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadingDetail, setLoadingDetail] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    const fetchSkills = async () => {
      try {
        setLoading(true)
        const data = await apiClient.getSkills()
        setSkills(data)
        setError(null)
      } catch (err) {
        setError(t('skills.failedToFetch'))
        console.error(err)
      } finally {
        setLoading(false)
      }
    }

    fetchSkills()
  }, [t])

  const handleSkillClick = async (skillName: string) => {
    try {
      setLoadingDetail(true)
      const detail = await apiClient.getSkill(skillName)
      setSelectedSkill(detail)
    } catch (err) {
      console.error('Failed to fetch skill detail:', err)
      alert(t('skills.failedToFetch'))
    } finally {
      setLoadingDetail(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-gray-500">{t('common.loading')}</div>
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
        <h2 className="text-2xl font-bold text-gray-900">{t('skills.title')}</h2>
        <p className="text-gray-600 mt-1">{t('skills.description')}</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="space-y-4">
          {skills.map((skill) => (
            <div
              key={skill.name}
              onClick={() => handleSkillClick(skill.name)}
              className={`bg-white rounded-lg shadow p-4 cursor-pointer transition-all hover:shadow-lg ${
                selectedSkill?.name === skill.name ? 'ring-2 ring-blue-500' : ''
              }`}
            >
              <h3 className="text-lg font-semibold text-gray-900">{skill.name}</h3>
              <div className="mt-2">
                <p className="text-sm text-gray-600">
                  {t('roles.assignedTo')} {skill.assigned_roles.length} {t('roles.role')}
                </p>
                {skill.assigned_roles.length > 0 && (
                  <div className="flex flex-wrap gap-2 mt-2">
                    {skill.assigned_roles.map((role) => (
                      <span
                        key={role}
                        className="px-2 py-1 bg-blue-100 text-blue-800 rounded text-xs"
                      >
                        {role}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="bg-white rounded-lg shadow p-6">
          {loadingDetail ? (
            <div className="flex items-center justify-center h-64">
              <div className="text-gray-500">{t('skills.loadingDetails')}</div>
            </div>
          ) : selectedSkill ? (
            <>
              <h3 className="text-xl font-bold text-gray-900 mb-4">
                {selectedSkill.name}
              </h3>

              <div className="mb-4">
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('skills.assignedRoles')}</h4>
                <div className="flex flex-wrap gap-2">
                  {selectedSkill.assigned_roles.map((role) => (
                    <span
                      key={role}
                      className="px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-sm"
                    >
                      {role}
                    </span>
                  ))}
                  {selectedSkill.assigned_roles.length === 0 && (
                    <span className="text-gray-500 text-sm">{t('skills.notAssigned')}</span>
                  )}
                </div>
              </div>

              <div>
                <h4 className="text-sm font-medium text-gray-700 mb-2">{t('skills.skillContent')}</h4>
                <div className="bg-gray-50 rounded p-4 max-h-96 overflow-y-auto">
                  <pre className="text-sm text-gray-800 whitespace-pre-wrap font-mono">
                    {selectedSkill.content}
                  </pre>
                </div>
              </div>
            </>
          ) : (
            <div className="flex items-center justify-center h-64 text-gray-500">
              {t('skills.selectSkill')}
            </div>
          )}
        </div>
      </div>

      {skills.length === 0 && (
        <div className="text-center py-12 text-gray-500">
          {t('skills.noSkills')}
        </div>
      )}
    </div>
  )
}

export default Skills
