import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, TextField } from '../FormPrimitives'
import { asArray, asRecord } from '../immutable'

export const PairingsSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const pairings = asArray<unknown>(draft.pairings)

  const setPairings = (next: unknown[]) => {
    setDraft((d) => ({ ...d, pairings: next }))
  }

  return (
    <FieldGroup>
      {pairings.map((p, idx) => {
        const row = asRecord(p)
        return (
          <div key={idx} className="border border-border rounded-lg p-4 space-y-2">
            <div className="flex justify-between">
              <span className="text-sm font-medium">
                {t('config.sections.pairings.entry')} {idx + 1}
              </span>
              <button
                type="button"
                className="text-sm text-error"
                onClick={() => setPairings(pairings.filter((_, i) => i !== idx))}
              >
                {t('common.delete')}
              </button>
            </div>
            <TextField
              id={`pch-${idx}`}
              label={t('config.sections.pairings.channel')}
              value={typeof row.channel === 'string' ? row.channel : ''}
              onChange={(v) => {
                const next = [...pairings]
                next[idx] = { ...asRecord(next[idx]), channel: v }
                setPairings(next)
              }}
            />
            <TextField
              id={`pp-${idx}`}
              label={t('config.sections.pairings.pairingCode')}
              value={typeof row.pairingCode === 'string' ? row.pairingCode : ''}
              onChange={(v) => {
                const next = [...pairings]
                next[idx] = { ...asRecord(next[idx]), pairingCode: v }
                setPairings(next)
              }}
            />
          </div>
        )
      })}
      <button
        type="button"
        className="px-4 py-2 rounded-lg bg-surface border border-border text-sm"
        onClick={() => setPairings([...pairings, { channel: '', pairingCode: '' }])}
      >
        {t('config.sections.pairings.add')}
      </button>
    </FieldGroup>
  )
}
