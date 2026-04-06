import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, TextField, Toggle } from '../FormPrimitives'
import { asArray, asRecord } from '../immutable'

export const CronSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const cron = asRecord(draft.cron)
  const tasks = asArray<unknown>(cron.tasks)

  const patch = (p: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      cron: { ...asRecord(d.cron), ...p },
    }))
  }

  return (
    <FieldGroup>
      {tasks.map((task, idx) => {
        const row = asRecord(task)
        return (
          <div key={idx} className="border border-border rounded-lg p-4 space-y-2">
            <div className="flex justify-between">
              <span className="text-sm font-medium">
                {t('config.sections.cron.task')} {idx + 1}
              </span>
              <button
                type="button"
                className="text-sm text-error"
                onClick={() => patch({ tasks: tasks.filter((_, i) => i !== idx) })}
              >
                {t('common.delete')}
              </button>
            </div>
            <TextField
              id={`cs-${idx}`}
              label={t('config.sections.cron.schedule')}
              value={typeof row.schedule === 'string' ? row.schedule : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), schedule: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`cd-${idx}`}
              label={t('config.sections.cron.description')}
              value={typeof row.description === 'string' ? row.description : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), description: v }
                patch({ tasks: next })
              }}
            />
            <Toggle
              id={`ce-${idx}`}
              label={t('common.enabled')}
              checked={Boolean(row.enabled ?? true)}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), enabled: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`cc-${idx}`}
              label={t('config.sections.cron.command')}
              value={typeof row.command === 'string' ? row.command : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), command: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`cch-${idx}`}
              label={t('config.sections.cron.channel')}
              value={typeof row.channel === 'string' ? row.channel : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), channel: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`cu-${idx}`}
              label={t('config.sections.cron.userId')}
              value={typeof row.userId === 'string' ? row.userId : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), userId: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`cchat-${idx}`}
              label={t('config.sections.cron.chatId')}
              value={row.chatId != null ? String(row.chatId) : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = {
                  ...asRecord(next[idx]),
                  chatId: v.trim() ? v : null,
                }
                patch({ tasks: next })
              }}
            />
          </div>
        )
      })}
      <button
        type="button"
        className="px-4 py-2 rounded-lg bg-surface border border-border text-sm"
        onClick={() =>
          patch({
            tasks: [
              ...tasks,
              {
                schedule: '',
                description: '',
                enabled: true,
                command: '',
                channel: '',
                userId: '',
                chatId: null,
              },
            ],
          })
        }
      >
        {t('config.sections.cron.addTask')}
      </button>
    </FieldGroup>
  )
}
