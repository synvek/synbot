import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, Subheading, TextField } from '../FormPrimitives'
import { asArray, asRecord } from '../immutable'

type AgentRow = {
  name: string
  role: string
  provider: string
  model: string
  maxTokens: string
  temperature: string
  maxIterations: string
  skills: string
  tools: string
}

function rowFromAgent(a: unknown): AgentRow {
  const o = asRecord(a)
  return {
    name: typeof o.name === 'string' ? o.name : '',
    role: typeof o.role === 'string' ? o.role : '',
    provider: o.provider != null ? String(o.provider) : '',
    model: o.model != null ? String(o.model) : '',
    maxTokens: o.maxTokens != null ? String(o.maxTokens) : '',
    temperature: o.temperature != null ? String(o.temperature) : '',
    maxIterations: o.maxIterations != null ? String(o.maxIterations) : '',
    skills: Array.isArray(o.skills) ? (o.skills as string[]).join(', ') : '',
    tools: Array.isArray(o.tools) ? (o.tools as string[]).join(', ') : '',
  }
}

function agentFromRow(r: AgentRow): Record<string, unknown> {
  const out: Record<string, unknown> = {
    name: r.name,
    role: r.role,
    skills: r.skills
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    tools: r.tools
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
  }
  if (r.provider.trim()) out.provider = r.provider.trim()
  if (r.model.trim()) out.model = r.model.trim()
  if (r.maxTokens.trim()) out.maxTokens = Number(r.maxTokens)
  if (r.temperature.trim()) out.temperature = Number(r.temperature)
  if (r.maxIterations.trim()) out.maxIterations = Number(r.maxIterations)
  return out
}

export const MainAgentSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const m = asRecord(draft.mainAgent)
  const workspace = typeof m.workspace === 'string' ? m.workspace : ''
  const provider = typeof m.provider === 'string' ? m.provider : ''
  const model = typeof m.model === 'string' ? m.model : ''
  const maxTokens = typeof m.maxTokens === 'number' ? m.maxTokens : 8192
  const temperature = typeof m.temperature === 'number' ? m.temperature : 0.7
  const maxToolIterations = typeof m.maxToolIterations === 'number' ? m.maxToolIterations : 99
  const maxConsecutiveToolErrors =
    typeof m.maxConsecutiveToolErrors === 'number' ? m.maxConsecutiveToolErrors : 8
  const maxChatHistoryMessages =
    typeof m.maxChatHistoryMessages === 'number' ? m.maxChatHistoryMessages : 20
  const maxConcurrentSubagents =
    typeof m.maxConcurrentSubagents === 'number' ? m.maxConcurrentSubagents : 5
  const subagentTaskTimeoutSecs =
    typeof m.subagentTaskTimeoutSecs === 'number' ? m.subagentTaskTimeoutSecs : 600
  const agents = asArray<unknown>(m.agents).map(rowFromAgent)

  const patchMain = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      mainAgent: { ...asRecord(d.mainAgent), ...patch },
    }))
  }

  const setAgents = (next: AgentRow[]) => {
    patchMain({ agents: next.map(agentFromRow) })
  }

  return (
    <FieldGroup>
      <TextField
        id="maWorkspace"
        label={t('config.sections.mainAgent.workspace')}
        value={workspace}
        onChange={(v) => patchMain({ workspace: v })}
      />
      <TextField
        id="maProvider"
        label={t('config.sections.mainAgent.provider')}
        value={provider}
        onChange={(v) => patchMain({ provider: v })}
      />
      <TextField
        id="maModel"
        label={t('config.sections.mainAgent.model')}
        value={model}
        onChange={(v) => patchMain({ model: v })}
      />
      <NumberField
        id="maMaxTokens"
        label={t('config.sections.mainAgent.maxTokens')}
        value={maxTokens}
        min={1}
        onChange={(v) => patchMain({ maxTokens: v })}
      />
      <div>
        <label className={labelClass} htmlFor="maTemp">
          {t('config.sections.mainAgent.temperature')}
        </label>
        <input
          id="maTemp"
          type="number"
          step="0.01"
          className={inputClass}
          value={temperature}
          onChange={(e) => patchMain({ temperature: Number(e.target.value) })}
        />
      </div>
      <NumberField
        id="maMaxToolIter"
        label={t('config.sections.mainAgent.maxToolIterations')}
        value={maxToolIterations}
        min={1}
        onChange={(v) => patchMain({ maxToolIterations: v })}
      />
      <NumberField
        id="maMaxConsecErr"
        label={t('config.sections.mainAgent.maxConsecutiveToolErrors')}
        value={maxConsecutiveToolErrors}
        min={0}
        onChange={(v) => patchMain({ maxConsecutiveToolErrors: v })}
      />
      <NumberField
        id="maMaxHist"
        label={t('config.sections.mainAgent.maxChatHistoryMessages')}
        value={maxChatHistoryMessages}
        min={0}
        onChange={(v) => patchMain({ maxChatHistoryMessages: v })}
      />
      <NumberField
        id="maMaxSub"
        label={t('config.sections.mainAgent.maxConcurrentSubagents')}
        value={maxConcurrentSubagents}
        min={0}
        onChange={(v) => patchMain({ maxConcurrentSubagents: v })}
      />
      <NumberField
        id="maSubTimeout"
        label={t('config.sections.mainAgent.subagentTaskTimeoutSecs')}
        value={subagentTaskTimeoutSecs}
        min={1}
        onChange={(v) => patchMain({ subagentTaskTimeoutSecs: v })}
      />

      <Subheading>{t('config.sections.mainAgent.agents')}</Subheading>
      <p className="text-sm text-text-secondary -mt-2 mb-2">{t('config.sections.mainAgent.agentsHint')}</p>
      {agents.map((row, idx) => (
        <div
          key={idx}
          className="border border-border rounded-lg p-4 space-y-3 bg-background/50"
        >
          <div className="flex justify-between items-center">
            <span className="text-sm font-medium text-text">
              {t('config.sections.mainAgent.agentCard')} #{idx + 1}
            </span>
            <button
              type="button"
              className="text-sm text-error hover:underline"
              onClick={() => setAgents(agents.filter((_, i) => i !== idx))}
            >
              {t('common.delete')}
            </button>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <Field
              label={t('config.sections.mainAgent.agentName')}
              value={row.name}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], name: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentRole')}
              value={row.role}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], role: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentProvider')}
              value={row.provider}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], provider: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentModel')}
              value={row.model}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], model: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentMaxTokens')}
              value={row.maxTokens}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], maxTokens: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentTemperature')}
              value={row.temperature}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], temperature: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentMaxIterations')}
              value={row.maxIterations}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], maxIterations: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentSkills')}
              value={row.skills}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], skills: v }
                setAgents(n)
              }}
            />
            <Field
              label={t('config.sections.mainAgent.agentTools')}
              value={row.tools}
              onChange={(v) => {
                const n = [...agents]
                n[idx] = { ...n[idx], tools: v }
                setAgents(n)
              }}
            />
          </div>
        </div>
      ))}
      <button
        type="button"
        className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
        onClick={() => setAgents([...agents, rowFromAgent({})])}
      >
        {t('config.sections.mainAgent.addAgent')}
      </button>
    </FieldGroup>
  )
}

const labelClass = 'block text-sm font-medium text-text mb-1'
const inputClass =
  'w-full px-3 py-2 rounded-lg bg-surface border border-border text-text text-sm focus:outline-none focus:ring-2 focus:ring-primary/30'

const Field: React.FC<{ label: string; value: string; onChange: (v: string) => void }> = ({
  label,
  value,
  onChange,
}) => (
  <div>
    <label className={labelClass}>{label}</label>
    <input className={inputClass} value={value} onChange={(e) => onChange(e.target.value)} />
  </div>
)
