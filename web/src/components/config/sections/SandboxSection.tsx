import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, Subheading } from '../FormPrimitives'
import { JsonObjectEditor } from '../JsonObjectEditor'

export const SandboxSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => (
  <FieldGroup>
    <Subheading>{t('config.sections.sandbox.app')}</Subheading>
    <JsonObjectEditor
      label={t('config.sections.sandbox.appJson')}
      hint={t('config.sections.sandbox.appHint')}
      nullable
      value={draft.appSandbox ?? null}
      onChange={(v) => setDraft((d) => ({ ...d, appSandbox: v }))}
    />
    <Subheading>{t('config.sections.sandbox.tool')}</Subheading>
    <JsonObjectEditor
      label={t('config.sections.sandbox.toolJson')}
      hint={t('config.sections.sandbox.toolHint')}
      nullable
      value={draft.toolSandbox ?? null}
      onChange={(v) => setDraft((d) => ({ ...d, toolSandbox: v }))}
    />
    <Subheading>{t('config.sections.sandbox.monitoring')}</Subheading>
    <JsonObjectEditor
      label={t('config.sections.sandbox.monitoringJson')}
      hint={t('config.sections.sandbox.monitoringHint')}
      nullable
      value={draft.sandboxMonitoring ?? null}
      onChange={(v) => setDraft((d) => ({ ...d, sandboxMonitoring: v }))}
    />
  </FieldGroup>
)
