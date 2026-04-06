import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, TextField } from '../FormPrimitives'
import { asRecord } from '../immutable'

export const WorkflowSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const wf = asRecord(draft.workflow)
  const userInputTimeoutSecs =
    typeof wf.userInputTimeoutSecs === 'number' ? wf.userInputTimeoutSecs : 1800
  const workflowsRoot =
    wf.workflowsRoot === null || wf.workflowsRoot === undefined
      ? ''
      : String(wf.workflowsRoot)

  const patch = (p: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      workflow: { ...asRecord(d.workflow), ...p },
    }))
  }

  return (
    <FieldGroup>
      <NumberField
        id="wfTimeout"
        label={t('config.sections.workflow.userInputTimeoutSecs')}
        value={userInputTimeoutSecs}
        min={1}
        onChange={(v) => patch({ userInputTimeoutSecs: v })}
      />
      <TextField
        id="wfRoot"
        label={t('config.sections.workflow.workflowsRoot')}
        value={workflowsRoot}
        onChange={(v) => patch({ workflowsRoot: v.trim() ? v : null })}
      />
    </FieldGroup>
  )
}
