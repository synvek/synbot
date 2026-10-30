import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, SecretField, SelectField, Subheading, TextField } from '../FormPrimitives'
import { asRecord, CONFIG_SECRET_MASK } from '../immutable'
import { useProviderCatalog } from '../useProviderCatalog'
import type { ProviderMetadata } from '../../../types/api'

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
  // Metadata comes from GET /api/providers so the form never restates what the daemon already
  // knows (wire format, default endpoint, whether a key is set, who selected the provider).
  // `null` means "not loaded" or "older daemon": the form then behaves exactly as before.
  const catalog = useProviderCatalog()
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
        const meta = catalog?.providers.find((p) => p.key === key)
        return (
          <div key={key} className="border border-border rounded-lg p-4 space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Subheading className="!mt-0 !mb-0">{t(labelKey)}</Subheading>
              {meta && <ProviderBadges meta={meta} t={t} />}
            </div>
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
              placeholder={meta?.defaultBase}
              onChange={(v) => patchBuiltin(setDraft, key, { apiBase: v.trim() ? v : null })}
            />
            {meta && (
              <p className="text-xs text-text-secondary -mt-1">
                <span>
                  {e.apiBase
                    ? t('config.sections.providers.baseOverride')
                    : t('config.sections.providers.baseDefault')}
                </span>
                {' '}
                <span className="font-mono">{meta.apiBaseEffective}</span>
                {' · '}
                <span>
                  {meta.embeddings
                    ? t('config.sections.providers.embeddingsYes')
                    : t('config.sections.providers.embeddingsNo')}
                </span>
                {' · '}
                <a
                  href={meta.docsUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="text-primary hover:underline"
                >
                  {t('config.sections.providers.modelsLink')}
                </a>
              </p>
            )}
            {meta ? (
              // `apiStyle` is ignored for built-ins (the client is chosen by name), so showing an
              // editable select here would invite the user to change a value nothing reads.
              <p className="text-xs text-text-secondary -mt-1">
                {t('config.sections.providers.styleFixed')}{' '}
                <span className="font-mono">{meta.apiStyle}</span>
              </p>
            ) : (
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
            )}
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
      {extraNames.map((name) => {
        const extraMeta = catalog?.extra.find((e) => e.name === name)
        return (
          <div key={name} className="border border-border rounded-lg p-4 space-y-3">
            <div className="flex flex-wrap justify-between items-center gap-2">
              <span className="font-mono text-sm font-medium">{name}</span>
              <div className="flex flex-wrap items-center gap-2">
                {extraMeta &&
                  (extraMeta.configured ? (
                    <Badge tone="ok">{t('config.sections.providers.configured')}</Badge>
                  ) : (
                    <Badge tone="warn">{t('config.sections.providers.notConfigured')}</Badge>
                  ))}
                {extraMeta && extraMeta.usedBy.length > 0 && (
                  <Badge>
                    {t('config.sections.providers.usedBy')}:{' '}
                    {extraMeta.usedBy
                      .map((u) => (u === 'main' ? t('config.sections.providers.mainRole') : u))
                      .join(', ')}
                  </Badge>
                )}
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
        )
      })}
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

const Badge: React.FC<{ tone?: 'ok' | 'warn' | 'info'; children: React.ReactNode }> = ({
  tone = 'info',
  children,
}) => {
  const cls =
    tone === 'ok'
      ? 'bg-success/10 text-success border-success/20'
      : tone === 'warn'
        ? 'bg-warning/10 text-warning border-warning/20'
        : 'bg-primary/10 text-primary border-primary/20'
  return <span className={`px-2 py-0.5 rounded-full border text-xs ${cls}`}>{children}</span>
}

/**
 * Per-provider state from `/api/providers`: whether a key is set, whether the runtime is local,
 * and which agent selects it. Rendered only once the catalog has loaded.
 */
const ProviderBadges: React.FC<{ meta: ProviderMetadata; t: (k: string) => string }> = ({
  meta,
  t,
}) => (
  <div className="flex flex-wrap items-center gap-2">
    {meta.configured ? (
      <Badge tone="ok">{t('config.sections.providers.configured')}</Badge>
    ) : (
      <Badge tone="warn">{t('config.sections.providers.notConfigured')}</Badge>
    )}
    {meta.local && <Badge>{t('config.sections.providers.local')}</Badge>}
    {meta.usedBy.length > 0 && (
      <Badge>
        {t('config.sections.providers.usedBy')}:{' '}
        {meta.usedBy
          .map((u) => (u === 'main' ? t('config.sections.providers.mainRole') : u))
          .join(', ')}
      </Badge>
    )}
  </div>
)
