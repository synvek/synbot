import { useEffect, useState } from 'react'
import { Bot, RefreshCw } from 'lucide-react'
import { apiClient } from '../api/client'
import type { AgentInfo } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import {
  Badge,
  Button,
  CodeBlock,
  DataTable,
  EmptyState,
  PageHeader,
  ScrollArea,
  type Column,
} from '../ui'

/** A read-only inspector: every field here is authored in the config file, never in this page. */
export default function Roles() {
  const [agents, setAgents] = useState<AgentInfo[]>([])
  const [selectedName, setSelectedName] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const { t } = useI18n()

  const load = () => {
    setLoading(true)
    apiClient
      .getAgents()
      .then((data) => {
        setAgents(data)
        setError(null)
        setSelectedName((current) => current ?? data[0]?.name ?? null)
      })
      .catch((err) => {
        console.error(err)
        setError(t('roles.failedToFetch', 'Failed to fetch roles'))
      })
      .finally(() => setLoading(false))
  }

  useEffect(load, [t])

  const selected = agents.find((agent) => agent.name === selectedName) ?? null

  const columns: Array<Column<AgentInfo>> = [
    {
      key: 'name',
      header: t('roles.title', 'Roles'),
      sortValue: (row) => row.name,
      render: (row) => (
        <div className="min-w-0">
          <div className="truncate font-medium text-ink">{row.name}</div>
          <div className="truncate font-mono text-micro text-ink-3">{row.role}</div>
        </div>
      ),
    },
    {
      key: 'model',
      header: t('roles.model', 'Model'),
      sortValue: (row) => `${row.provider}/${row.model}`,
      render: (row) => (
        <span className="truncate font-mono text-micro text-ink-3">
          {row.provider}/{row.model}
        </span>
      ),
    },
    {
      key: 'skills',
      header: t('roles.skills', 'Skills'),
      align: 'right',
      width: '80px',
      sortValue: (row) => row.skills.length,
      render: (row) => <span className="tabular font-mono text-micro text-ink-2">{row.skills.length}</span>,
    },
    {
      key: 'tools',
      header: t('roles.tools', 'Tools'),
      align: 'right',
      width: '80px',
      sortValue: (row) => row.tools.length,
      render: (row) => <span className="tabular font-mono text-micro text-ink-2">{row.tools.length}</span>,
    },
  ]

  return (
    <div>
      <PageHeader
        title={t('roles.title', 'Roles')}
        count={agents.length}
        description={t('roles.description', 'Agent definitions loaded from the configuration file')}
        actions={
          <Button variant="ghost" size="sm" icon={<RefreshCw className="h-3.5 w-3.5" />} onClick={load}>
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {loading ? (
        <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
      ) : agents.length === 0 ? (
        <EmptyState
          icon={<Bot className="h-6 w-6" />}
          title={t('roles.noRoles', 'No roles defined')}
          body={t('roles.description', 'Agent definitions loaded from the configuration file')}
        />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)]">
          <div className="min-w-0">
            <DataTable
              rows={agents}
              columns={columns}
              rowKey={(row) => row.name}
              density="compact"
              selectedKey={selectedName}
              onRowClick={(row) => setSelectedName(row.name)}
              className="[&>table]:min-w-0"
            />
          </div>

          <div className="min-w-0">
            {selected && (
              <div className="rounded-panel border border-line">
                <div className="border-b border-line px-4 py-3">
                  <h2 className="truncate text-section font-semibold text-ink">{selected.name}</h2>
                  <p className="mt-0.5 text-small text-ink-2">{selected.role}</p>
                </div>

                <ScrollArea className="max-h-[520px] px-4 py-3">
                  <div className="space-y-4">
                    <section>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('roles.modelConfiguration', 'Model configuration')}
                      </h3>
                      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-micro">
                        {[
                          [t('roles.provider', 'Provider'), selected.provider],
                          [t('roles.model', 'Model'), selected.model],
                          [t('roles.maxTokens', 'Max tokens'), String(selected.max_tokens)],
                          [t('roles.temperature', 'Temperature'), String(selected.temperature)],
                        ].map(([label, value]) => (
                          <div key={label} className="flex min-w-0 flex-col gap-0.5">
                            <dt className="text-ink-3">{label}</dt>
                            <dd className="truncate font-mono text-ink">{value}</dd>
                          </div>
                        ))}
                      </dl>
                    </section>

                    <section>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('roles.systemPrompt', 'System prompt')}
                      </h3>
                      <p className="max-h-48 overflow-y-auto whitespace-pre-wrap rounded-control bg-surface-2 px-2.5 py-2 text-small text-ink">
                        {selected.system_prompt}
                      </p>
                    </section>

                    <section>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('roles.skills', 'Skills')}
                      </h3>
                      {selected.skills.length === 0 ? (
                        <p className="text-small text-ink-3">{t('roles.noSkills', 'No skills assigned')}</p>
                      ) : (
                        <div className="flex flex-wrap gap-1">
                          {selected.skills.map((skill) => (
                            <Badge key={skill} tone="neutral">
                              {skill}
                            </Badge>
                          ))}
                        </div>
                      )}
                    </section>

                    <section>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('roles.tools', 'Tools')}
                      </h3>
                      {selected.tools.length === 0 ? (
                        <p className="text-small text-ink-3">{t('roles.noTools', 'No tools enabled')}</p>
                      ) : (
                        <div className="flex flex-wrap gap-1">
                          {selected.tools.map((tool) => (
                            <Badge key={tool} tone="neutral">
                              {tool}
                            </Badge>
                          ))}
                        </div>
                      )}
                    </section>

                    <section>
                      <h3 className="mb-1 text-micro font-medium uppercase tracking-wide text-ink-3">
                        {t('roles.workspace', 'Workspace')}
                      </h3>
                      <CodeBlock maxHeight={100}>{selected.workspace_dir}</CodeBlock>
                    </section>
                  </div>
                </ScrollArea>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
