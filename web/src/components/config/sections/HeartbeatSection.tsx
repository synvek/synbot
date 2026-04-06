import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, TextField, Toggle } from '../FormPrimitives'
import { asArray, asRecord } from '../immutable'

export const HeartbeatSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const hb = asRecord(draft.heartbeat)
  const enabled = Boolean(hb.enabled ?? true)
  const interval = typeof hb.interval === 'number' ? hb.interval : 300
  const tasks = asArray<unknown>(hb.tasks)

  const patch = (p: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      heartbeat: { ...asRecord(d.heartbeat), ...p },
    }))
  }

  return (
    <FieldGroup>
      <Toggle
        id="hbEn"
        label={t('config.sections.heartbeat.enabled')}
        checked={enabled}
        onChange={(v) => patch({ enabled: v })}
      />
      <NumberField
        id="hbInt"
        label={t('config.sections.heartbeat.interval')}
        value={interval}
        min={1}
        onChange={(v) => patch({ interval: v })}
      />
      {tasks.map((task, idx) => {
        const row = asRecord(task)
        return (
          <div key={idx} className="border border-border rounded-lg p-4 space-y-2">
            <div className="flex justify-between">
              <span className="text-sm font-medium">
                {t('config.sections.heartbeat.task')} {idx + 1}
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
              id={`hbc-${idx}`}
              label={t('config.sections.heartbeat.channel')}
              value={typeof row.channel === 'string' ? row.channel : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), channel: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`hbch-${idx}`}
              label={t('config.sections.heartbeat.chatId')}
              value={typeof row.chatId === 'string' ? row.chatId : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), chatId: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`hbu-${idx}`}
              label={t('config.sections.heartbeat.userId')}
              value={typeof row.userId === 'string' ? row.userId : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), userId: v }
                patch({ tasks: next })
              }}
            />
            <TextField
              id={`hbt-${idx}`}
              label={t('config.sections.heartbeat.target')}
              value={typeof row.target === 'string' ? row.target : ''}
              onChange={(v) => {
                const next = [...tasks]
                next[idx] = { ...asRecord(next[idx]), target: v }
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
              { channel: '', chatId: '', userId: '', target: '' },
            ],
          })
        }
      >
        {t('config.sections.heartbeat.addTask')}
      </button>
    </FieldGroup>
  )
}
