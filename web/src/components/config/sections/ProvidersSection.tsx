import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, SecretField, SelectField, Subheading, TextField } from '../FormPrimitives'
import { asRecord, CONFIG_SECRET_MASK } from '../immutable'

const BUILTIN_KEYS = [
  'anthropic',
  'openai',
  'gemini',
  'openrouter',
  'deepseek',
  'moonshot',
  'kimiCode',
  'ollama',
] as const

function providerEntry(
  prov: Record<string, unknown>,
  key: string,
): { apiKey: string; apiBase: string; apiStyle: string; maxTokensCap: string }
{
  const p = asRecord(prov[key])
  return {
    apiKey: typeof p.apiKey === 'string' ? p.apiKey : '',
    apiBase: p.apiBase != null ? String(p.apiBase) : '',
    apiStyle: typeof p.apiStyle === 'string' ? p.apiStyle : 'openai',
    maxTokensCap: p.maxTokensCap != null ? String(p.maxTokensCap) : '',
  }
}

function patchBuiltin(
  setDraft: SectionProps['setDraft'],
  key: string,
  patch: Record<string, unknown>,
) {
  setDraft((d) => {
    const prov = asRecord(d.providers)
    const cur = asRecord(prov[key])
    return { ...d, providers: { ...prov, [key]: { ...cur, ...patch } } }
  })
}

export const ProvidersSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const prov = asRecord(draft.providers)
  const extra = asRecord(
    typeof prov.extra === 'object' && prov.extra !== null && !Array.isArray(prov.extra)
      ? prov.extra
      : {},
  )
  const extraNames = Object.keys(extra)

  return (
    <FieldGroup>
      {BUILTIN_KEYS.map((key) => {
        const e = providerEntry(prov, key)
        const labelKey = `config.sections.providers.builtin.${key}`
        return (
          <div key={key} className="border border-border rounded-lg p-4 space-y-3">
            <Subheading className="!mt-0 !mb-2">{t(labelKey)}</Subheading>
            <SecretField
              id={`pk-${key}`}
              label={t('config.sections.providers.apiKey')}
              value={e.apiKey || CONFIG_SECRET_MASK}
              leaveUnchangedHint={e.apiKey === CONFIG_SECRET_MASK || e.apiKey === '' ? t('config.secretLeaveUnchanged') : undefined}
              onChange={(v) => patchBuiltin(setDraft, key, { apiKey: v })}
            />
            <TextField
              id={`base-${key}`}
              label={t('config.sections.providers.apiBase')}
              value={e.apiBase}
              onChange={(v) => patchBuiltin(setDraft, key, { apiBase: v.trim() ? v : null })}
            />
            <SelectField
              id={`style-${key}`}
              label={t('config.sections.providers.apiStyle')}
              value={e.apiStyle}
              onChange={(v) => patchBuiltin(setDraft, key, { apiStyle: v })}
              options={[
                { value: 'openai', label: 'openai' },
                { value: 'anthropic', label: 'anthropic' },
              ]}
            />
            <TextField
              id={`cap-${key}`}
              label={t('config.sections.providers.maxTokensCap')}
              value={e.maxTokensCap}
              onChange={(v) =>
                patchBuiltin(setDraft, key, {
                  maxTokensCap: v.trim() ? Number(v) : null,
                })
              }
            />
          </div>
        )
      })}

      <Subheading>{t('config.sections.providers.extra')}</Subheading>
      <p className="text-sm text-text-secondary -mt-2 mb-2">{t('config.sections.providers.extraHint')}</p>
      {extraNames.map((name) => (
        <div key={name} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between items-center">
            <span className="font-mono text-sm font-medium">{name}</span>
            <button
              type="button"
              className="text-sm text-error hover:underline"
              onClick={() => {
                setDraft((d) => {
                  const p = asRecord(d.providers)
                  const ex = { ...asRecord(p.extra) }
                  delete ex[name]
                  return { ...d, providers: { ...p, extra: ex } }
                })
              }}
            >
              {t('common.delete')}
            </button>
          </div>
          <ExtraEntryForm
            name={name}
            entry={asRecord(extra[name])}
            onPatch={(patch) => {
              setDraft((d) => {
                const p = asRecord(d.providers)
                const ex = { ...asRecord(p.extra) }
                ex[name] = { ...asRecord(ex[name]), ...patch }
                return { ...d, providers: { ...p, extra: ex } }
              })
            }}
            t={t}
          />
        </div>
      ))}
      <button
        type="button"
        className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
        onClick={() => {
          const n = window.prompt(t('config.sections.providers.extraPrompt'))
          if (!n?.trim()) return
          const name = n.trim()
          setDraft((d) => {
            const p = asRecord(d.providers)
            const ex = { ...asRecord(p.extra) }
            if (ex[name]) return d
            ex[name] = { apiKey: '', apiBase: null, apiStyle: 'openai', maxTokensCap: null }
            return { ...d, providers: { ...p, extra: ex } }
          })
        }}
      >
        {t('config.sections.providers.addExtra')}
      </button>
    </FieldGroup>
  )
}

const ExtraEntryForm: React.FC<{
  name: string
  entry: Record<string, unknown>
  onPatch: (p: Record<string, unknown>) => void
  t: (k: string) => string
}> = ({ entry, onPatch, t }) => {
  const apiKey = typeof entry.apiKey === 'string' ? entry.apiKey : CONFIG_SECRET_MASK
  const apiBase = entry.apiBase != null ? String(entry.apiBase) : ''
  const apiStyle = typeof entry.apiStyle === 'string' ? entry.apiStyle : 'openai'
  const maxTokensCap = entry.maxTokensCap != null ? String(entry.maxTokensCap) : ''
  return (
    <>
      <SecretField
        id={`ex-key-${name}`}
        label={t('config.sections.providers.apiKey')}
        value={apiKey}
        leaveUnchangedHint={t('config.secretLeaveUnchanged')}
        onChange={(v) => onPatch({ apiKey: v })}
      />
      <TextField
        id={`ex-base-${name}`}
        label={t('config.sections.providers.apiBase')}
        value={apiBase}
        onChange={(v) => onPatch({ apiBase: v.trim() ? v : null })}
      />
      <SelectField
        id={`ex-style-${name}`}
        label={t('config.sections.providers.apiStyle')}
        value={apiStyle}
        onChange={(v) => onPatch({ apiStyle: v })}
        options={[
          { value: 'openai', label: 'openai' },
          { value: 'anthropic', label: 'anthropic' },
        ]}
      />
      <TextField
        id={`ex-cap-${name}`}
        label={t('config.sections.providers.maxTokensCap')}
        value={maxTokensCap}
        onChange={(v) => onPatch({ maxTokensCap: v.trim() ? Number(v) : null })}
      />
    </>
  )
}
