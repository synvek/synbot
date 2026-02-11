import React, { useEffect, useState } from 'react'
import { apiClient } from '../api/client'
import type { CronJobInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'

const CronJobs: React.FC = () => {
  const [jobs, setJobs] = useState<CronJobInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [updating, setUpdating] = useState<string | null>(null)
  const { t } = useI18n()

  const fetchJobs = async () => {
    try {
      setLoading(true)
      const data = await apiClient.getCronJobs()
      setJobs(data)
      setError(null)
    } catch (err) {
      setError(t('cron.failedToFetch'))
      console.error(err)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    fetchJobs()
  }, [])

  const handleToggle = async (id: string, currentEnabled: boolean) => {
    try {
      setUpdating(id)
      await apiClient.updateCronJob(id, !currentEnabled)
      await fetchJobs()
    } catch (err) {
      console.error('Failed to update cron job:', err)
      alert(t('cron.failedToFetch'))
    } finally {
      setUpdating(null)
    }
  }

  const formatTimestamp = (ms: number | null | undefined) => {
    if (!ms) return t('cron.na')
    return new Date(ms).toLocaleString()
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
        <h2 className="text-2xl font-bold text-gray-900">{t('cron.title')}</h2>
        <p className="text-gray-600 mt-1">{t('cron.description')}</p>
      </div>

      <div className="space-y-4">
        {jobs.map((job) => (
          <div
            key={job.id}
            className="bg-surface rounded-lg shadow p-6 hover:shadow-lg transition-shadow"
          >
            <div className="flex items-start justify-between">
              <div className="flex-1">
                <div className="flex items-center gap-3">
                  <h3 className="text-lg font-semibold text-gray-900">
                    {job.name}
                  </h3>
                  <span
                    className={`px-2 py-1 rounded text-xs font-medium ${
                      job.enabled
                        ? 'bg-green-100 text-green-800'
                        : 'bg-gray-100 text-gray-600'
                    }`}
                  >
                    {job.enabled ? t('cron.enabled') : t('cron.disabled')}
                  </span>
                </div>
                <p className="text-sm text-gray-600 mt-1">{t('cron.id')}: {job.id}</p>
                <p className="text-sm text-gray-600 mt-1">
                  {t('cron.schedule')}: <code className="bg-gray-100 px-2 py-0.5 rounded">{job.schedule}</code>
                </p>
              </div>

              <button
                onClick={() => handleToggle(job.id, job.enabled)}
                disabled={updating === job.id}
                className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                  job.enabled
                    ? 'bg-red-100 text-red-700 hover:bg-red-200'
                    : 'bg-green-100 text-green-700 hover:bg-green-200'
                } disabled:opacity-50 disabled:cursor-not-allowed`}
              >
                {updating === job.id
                  ? t('cron.updating')
                  : job.enabled
                  ? t('cron.disable')
                  : t('cron.enable')}
              </button>
            </div>

            <div className="mt-4 pt-4 border-t border-gray-200 grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <p className="text-sm font-medium text-gray-700">{t('cron.lastRun')}</p>
                <p className="text-sm text-gray-600 mt-1">
                  {formatTimestamp(job.state.last_run_at_ms)}
                </p>
                {job.state.last_status && (
                  <p className="text-xs text-gray-500 mt-1">
                    {t('cron.status')}: {job.state.last_status}
                  </p>
                )}
              </div>

              <div>
                <p className="text-sm font-medium text-gray-700">{t('cron.nextRun')}</p>
                <p className="text-sm text-gray-600 mt-1">
                  {formatTimestamp(job.state.next_run_at_ms)}
                </p>
              </div>

              <div>
                <p className="text-sm font-medium text-gray-700">{t('cron.payload')}</p>
                <div className="mt-1 bg-gray-50 rounded p-2">
                  <pre className="text-xs text-gray-600 overflow-x-auto">
                    {JSON.stringify(job.payload, null, 2)}
                  </pre>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      {jobs.length === 0 && (
        <div className="text-center py-12 text-gray-500">
          {t('cron.noJobs')}
        </div>
      )}
    </div>
  )
}

export default CronJobs
