import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, Subheading, TextField, Toggle } from '../FormPrimitives'
import { asRecord } from '../immutable'

export const MemorySection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const mem = asRecord(draft.memory)
  const backend = typeof mem.backend === 'string' ? mem.backend : ''
  const embeddingProvider =
    typeof mem.embeddingProvider === 'string' ? mem.embeddingProvider : 'none'
  const embeddingModel = typeof mem.embeddingModel === 'string' ? mem.embeddingModel : ''
  const embeddingDimensions =
    typeof mem.embeddingDimensions === 'number' ? mem.embeddingDimensions : 768
  const vectorWeight = typeof mem.vectorWeight === 'number' ? mem.vectorWeight : 0.7
  const textWeight = typeof mem.textWeight === 'number' ? mem.textWeight : 0.3
  const autoIndex = Boolean(mem.autoIndex ?? true)
  const recentDays = typeof mem.recentDays === 'number' ? mem.recentDays : 1
  const searchLimit = typeof mem.searchLimit === 'number' ? mem.searchLimit : 5
  const longTermMaxChars = typeof mem.longTermMaxChars === 'number' ? mem.longTermMaxChars : 0
  const comp = asRecord(mem.compression)
  const cEnabled = Boolean(comp.enabled ?? false)
  const cMaxTurns =
    typeof comp.maxConversationTurns === 'number' ? comp.maxConversationTurns : 50
  const cSummary = Boolean(comp.summaryWriteToMemory ?? true)
  const cKeep =
    comp.keepRecentMessages === null || comp.keepRecentMessages === undefined
      ? ''
      : String(comp.keepRecentMessages)

  const patchMem = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      memory: { ...asRecord(d.memory), ...patch },
    }))
  }

  const patchCompression = (patch: Record<string, unknown>) => {
    setDraft((d) => {
      const mem = asRecord(d.memory)
      return {
        ...d,
        memory: {
          ...mem,
          compression: { ...asRecord(mem.compression), ...patch },
        },
      }
    })
  }

  return (
    <FieldGroup>
      <TextField
        id="memBackend"
        label={t('config.sections.memory.backend')}
        value={backend}
        onChange={(v) => patchMem({ backend: v })}
      />
      <TextField
        id="embProv"
        label={t('config.sections.memory.embeddingProvider')}
        value={embeddingProvider}
        onChange={(v) => patchMem({ embeddingProvider: v })}
      />
      <TextField
        id="embModel"
        label={t('config.sections.memory.embeddingModel')}
        value={embeddingModel}
        onChange={(v) => patchMem({ embeddingModel: v })}
      />
      <NumberField
        id="embDim"
        label={t('config.sections.memory.embeddingDimensions')}
        value={embeddingDimensions}
        min={1}
        onChange={(v) => patchMem({ embeddingDimensions: v })}
      />
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="vw">
          {t('config.sections.memory.vectorWeight')}
        </label>
        <input
          id="vw"
          type="number"
          step="0.01"
          className="w-full px-3 py-2 rounded-lg bg-surface border border-border text-text text-sm"
          value={vectorWeight}
          onChange={(e) => patchMem({ vectorWeight: Number(e.target.value) })}
        />
      </div>
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="tw">
          {t('config.sections.memory.textWeight')}
        </label>
        <input
          id="tw"
          type="number"
          step="0.01"
          className="w-full px-3 py-2 rounded-lg bg-surface border border-border text-text text-sm"
          value={textWeight}
          onChange={(e) => patchMem({ textWeight: Number(e.target.value) })}
        />
      </div>
      <Toggle
        id="autoIndex"
        label={t('config.sections.memory.autoIndex')}
        checked={autoIndex}
        onChange={(v) => patchMem({ autoIndex: v })}
      />
      <NumberField
        id="recentDays"
        label={t('config.sections.memory.recentDays')}
        value={recentDays}
        min={0}
        onChange={(v) => patchMem({ recentDays: v })}
      />
      <NumberField
        id="searchLimit"
        label={t('config.sections.memory.searchLimit')}
        value={searchLimit}
        min={0}
        onChange={(v) => patchMem({ searchLimit: v })}
      />
      <NumberField
        id="longTermMaxChars"
        label={t('config.sections.memory.longTermMaxChars')}
        value={longTermMaxChars}
        min={0}
        onChange={(v) => patchMem({ longTermMaxChars: v })}
      />

      <Subheading>{t('config.sections.memory.compression')}</Subheading>
      <Toggle
        id="cEn"
        label={t('config.sections.memory.compressionEnabled')}
        checked={cEnabled}
        onChange={(v) => patchCompression({ enabled: v })}
      />
      <NumberField
        id="cMax"
        label={t('config.sections.memory.maxConversationTurns')}
        value={cMaxTurns}
        min={1}
        onChange={(v) => patchCompression({ maxConversationTurns: v })}
      />
      <Toggle
        id="cSum"
        label={t('config.sections.memory.summaryWriteToMemory')}
        checked={cSummary}
        onChange={(v) => patchCompression({ summaryWriteToMemory: v })}
      />
      <TextField
        id="cKeep"
        label={t('config.sections.memory.keepRecentMessages')}
        value={cKeep}
        onChange={(v) =>
          patchCompression({
            keepRecentMessages: v.trim() ? Number(v) : null,
          })
        }
      />
    </FieldGroup>
  )
}
