import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, SelectField, TextField, Toggle } from '../FormPrimitives'
import { asRecord } from '../immutable'

export const LogSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const log = asRecord(draft.log)
  const level = typeof log.level === 'string' ? log.level : 'info'
  const format = typeof log.format === 'string' ? log.format : 'text'
  const dir = typeof log.dir === 'string' ? log.dir : 'logs'
  const maxFiles = typeof log.maxFiles === 'number' ? log.maxFiles : 20
  const maxFileSizeMb = typeof log.maxFileSizeMb === 'number' ? log.maxFileSizeMb : 50
  const showTimestamp = Boolean(log.showTimestamp ?? true)
  const showLevel = Boolean(log.showLevel ?? true)
  const showTarget = Boolean(log.showTarget ?? true)
  const showThreadNames = Boolean(log.showThreadNames ?? false)
  const showThreadIds = Boolean(log.showThreadIds ?? false)
  const showFile = Boolean(log.showFile ?? false)
  const timestampFormat = typeof log.timestampFormat === 'string' ? log.timestampFormat : 'local'
  const customTf =
    log.customTimestampFormat === null || log.customTimestampFormat === undefined
      ? ''
      : String(log.customTimestampFormat)
  const moduleLevels = asRecord(log.moduleLevels)
  const moduleLines = Object.entries(moduleLevels)
    .map(([k, v]) => `${k}=${v}`)
    .join('\n')

  const patchLog = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      log: { ...asRecord(d.log), ...patch },
    }))
  }

  const parseModuleLevels = (text: string): Record<string, string> => {
    const out: Record<string, string> = {}
    for (const line of text.split(/\r?\n/)) {
      const t = line.trim()
      if (!t || t.startsWith('#')) continue
      const eq = t.indexOf('=')
      if (eq <= 0) continue
      const k = t.slice(0, eq).trim()
      const v = t.slice(eq + 1).trim()
      if (k) out[k] = v
    }
    return out
  }

  return (
    <FieldGroup>
      <SelectField
        id="logLevel"
        label={t('config.sections.log.level')}
        value={level}
        onChange={(v) => patchLog({ level: v })}
        options={['trace', 'debug', 'info', 'warn', 'error'].map((x) => ({ value: x, label: x }))}
      />
      <SelectField
        id="logFormat"
        label={t('config.sections.log.format')}
        value={format}
        onChange={(v) => patchLog({ format: v })}
        options={['json', 'text', 'compact', 'pretty'].map((x) => ({ value: x, label: x }))}
      />
      <TextField id="logDir" label={t('config.sections.log.dir')} value={dir} onChange={(v) => patchLog({ dir: v })} />
      <NumberField
        id="maxFiles"
        label={t('config.sections.log.maxFiles')}
        value={maxFiles}
        min={1}
        onChange={(v) => patchLog({ maxFiles: v })}
      />
      <NumberField
        id="maxFileSizeMb"
        label={t('config.sections.log.maxFileSizeMb')}
        value={maxFileSizeMb}
        min={1}
        onChange={(v) => patchLog({ maxFileSizeMb: v })}
      />
      <Toggle
        id="showTimestamp"
        label={t('config.sections.log.showTimestamp')}
        checked={showTimestamp}
        onChange={(v) => patchLog({ showTimestamp: v })}
      />
      <Toggle
        id="showLevel"
        label={t('config.sections.log.showLevel')}
        checked={showLevel}
        onChange={(v) => patchLog({ showLevel: v })}
      />
      <Toggle
        id="showTarget"
        label={t('config.sections.log.showTarget')}
        checked={showTarget}
        onChange={(v) => patchLog({ showTarget: v })}
      />
      <Toggle
        id="showThreadNames"
        label={t('config.sections.log.showThreadNames')}
        checked={showThreadNames}
        onChange={(v) => patchLog({ showThreadNames: v })}
      />
      <Toggle
        id="showThreadIds"
        label={t('config.sections.log.showThreadIds')}
        checked={showThreadIds}
        onChange={(v) => patchLog({ showThreadIds: v })}
      />
      <Toggle
        id="showFile"
        label={t('config.sections.log.showFile')}
        checked={showFile}
        onChange={(v) => patchLog({ showFile: v })}
      />
      <SelectField
        id="timestampFormat"
        label={t('config.sections.log.timestampFormat')}
        value={timestampFormat}
        onChange={(v) => patchLog({ timestampFormat: v })}
        options={['rfc3339', 'local', 'utc', 'custom'].map((x) => ({ value: x, label: x }))}
      />
      <TextField
        id="customTimestampFormat"
        label={t('config.sections.log.customTimestampFormat')}
        value={customTf}
        onChange={(v) => patchLog({ customTimestampFormat: v.trim() ? v : null })}
        placeholder={t('config.sections.log.customTimestampFormatPlaceholder')}
      />
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="moduleLevels">
          {t('config.sections.log.moduleLevels')}
        </label>
        <textarea
          id="moduleLevels"
          spellCheck={false}
          className="w-full min-h-[120px] font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border text-text focus:outline-none focus:ring-2 focus:ring-primary/30"
          value={moduleLines}
          onChange={(e) => patchLog({ moduleLevels: parseModuleLevels(e.target.value) })}
          placeholder={t('config.sections.log.moduleLevelsPlaceholder')}
        />
        <p className="text-xs text-text-secondary mt-1">{t('config.sections.log.moduleLevelsHint')}</p>
      </div>
    </FieldGroup>
  )
}
