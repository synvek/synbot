import { useEffect, useState } from 'react'
import { BookOpen } from 'lucide-react'
import { apiClient } from '../api/client'
import type { SkillDetail, SkillInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import {
  Badge,
  CodeBlock,
  DataTable,
  EmptyState,
  PageHeader,
  type Column,
} from '../ui'

/** Skills are markdown files on disk; this page only reads them. */
export default function Skills() {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [selectedName, setSelectedName] = useState<string | null>(null)
  const [detail, setDetail] = useState<SkillDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  useEffect(() => {
    let active = true
    apiClient
      .getSkills()
      .then((data) => {
        if (!active) return
        setSkills(data)
        setError(null)
      })
      .catch((err) => {
        console.error(err)
        if (active) setError(t('skills.failedToFetch', 'Failed to fetch skills'))
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [t])

  const openSkill = async (name: string) => {
    setSelectedName(name)
    setDetail(null)
    setDetailLoading(true)
    try {
      setDetail(await apiClient.getSkill(name))
    } catch (err) {
      console.error('Failed to fetch skill detail:', err)
      setError(t('skills.failedToFetch', 'Failed to fetch skills'))
    } finally {
      setDetailLoading(false)
    }
  }

  const columns: Array<Column<SkillInfo>> = [
    {
      key: 'name',
      header: t('skills.title', 'Skills'),
      sortValue: (row) => row.name,
      render: (row) => <span className="truncate font-medium text-ink">{row.name}</span>,
    },
    {
      key: 'agents',
      header: t('skills.assignedRoles', 'Assigned roles'),
      align: 'right',
      width: '130px',
      sortValue: (row) => row.assigned_agents.length,
      render: (row) => (
        <span className="tabular font-mono text-micro text-ink-2">{row.assigned_agents.length}</span>
      ),
    },
  ]

  return (
    <div>
      <PageHeader
        title={t('skills.title', 'Skills')}
        count={skills.length}
        description={t('skills.description', 'Reusable capabilities available to every role')}
      />

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : skills.length === 0 ? (
        <EmptyState
          icon={<BookOpen className="h-6 w-6" />}
          title={t('skills.noSkills', 'No skills found')}
          body={t('skills.description', 'Reusable capabilities available to every role')}
        />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)]">
          <div className="min-w-0">
            <DataTable
              rows={skills}
              columns={columns}
              rowKey={(row) => row.name}
              density="compact"
              selectedKey={selectedName}
              onRowClick={(row) => void openSkill(row.name)}
              className="[&>table]:min-w-0"
            />
          </div>

          <div className="min-w-0">
            {detailLoading ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {t('skills.loadingDetails', 'Loading details…')}
              </p>
            ) : !detail ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {t('skills.selectSkill', 'Select a skill to view its content.')}
              </p>
            ) : (
              <div className="rounded-panel border border-line">
                <div className="border-b border-line px-4 py-3">
                  <h2 className="truncate text-section font-semibold text-ink">{detail.name}</h2>
                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    {detail.assigned_agents.length === 0 ? (
                      <span className="text-micro text-ink-3">{t('skills.notAssigned', 'Not assigned to any role')}</span>
                    ) : (
                      detail.assigned_agents.map((agent) => (
                        <Badge key={agent} tone="accent">
                          {agent}
                        </Badge>
                      ))
                    )}
                  </div>
                </div>
                <div className="px-4 py-3">
                  <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                    {t('skills.skillContent', 'Skill content')}
                  </h3>
                  <CodeBlock maxHeight={480} wrap>
                    {detail.content}
                  </CodeBlock>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
