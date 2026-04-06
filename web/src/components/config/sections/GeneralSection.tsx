import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, TextField, Toggle } from '../FormPrimitives'

export const GeneralSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const showToolCalls = Boolean(draft.showToolCalls ?? true)
  const toolResultPreviewChars = typeof draft.toolResultPreviewChars === 'number' ? draft.toolResultPreviewChars : 2048
  const mainChannel = typeof draft.mainChannel === 'string' ? draft.mainChannel : ''
  const configVersion = typeof draft.configVersion === 'number' ? draft.configVersion : 1

  return (
    <FieldGroup>
      <Toggle
        id="showToolCalls"
        label={t('config.sections.general.showToolCalls')}
        checked={showToolCalls}
        onChange={(v) => setDraft((d) => ({ ...d, showToolCalls: v }))}
      />
      <NumberField
        id="toolResultPreviewChars"
        label={t('config.sections.general.toolResultPreviewChars')}
        value={toolResultPreviewChars}
        min={0}
        onChange={(v) => setDraft((d) => ({ ...d, toolResultPreviewChars: v }))}
      />
      <TextField
        id="mainChannel"
        label={t('config.sections.general.mainChannel')}
        value={mainChannel}
        onChange={(v) => setDraft((d) => ({ ...d, mainChannel: v }))}
      />
      <div>
        <p className="text-sm font-medium text-text mb-1">{t('config.sections.general.configVersion')}</p>
        <p className="text-sm text-text-secondary font-mono">{configVersion}</p>
        <p className="text-xs text-text-secondary mt-1">{t('config.sections.general.configVersionHint')}</p>
      </div>
    </FieldGroup>
  )
}
