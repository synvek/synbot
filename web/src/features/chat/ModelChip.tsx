import { useEffect, useMemo, useState } from 'react'
import { Check, Cpu, Loader2 } from 'lucide-react'
import { apiClient } from '../../api'
import { useI18n } from '../../i18n/I18nContext'
import { Button, MenuItem, MenuLabel, MenuSeparator, Popover } from '../../ui'
import type { ModelOverride, ProviderCatalog } from '../../types/api'

interface ModelChipProps {
  /** Model pinned to this conversation, or `null` for the configured default. */
  value: ModelOverride | null
  /** A turn is running: the switch lands on the next one, which the panel says out loud. */
  running: boolean
  onChange: (model: ModelOverride | null) => void | Promise<void>
}

/**
 * Which model answers in this conversation, top right next to the workspace chip.
 *
 * The list comes from `GET /api/providers` instead of a hard-coded catalog: the provider half is a
 * fact about this instance (what has a key), while the model id is free text because providers
 * publish new ids far more often than Synbot ships releases. Picking nothing keeps the default.
 */
export function ModelChip({ value, running, onChange }: ModelChipProps) {
  const { t } = useI18n()
  const [catalog, setCatalog] = useState<ProviderCatalog | null>(null)

  useEffect(() => {
    let active = true
    apiClient
      .listProviders()
      .then((loaded) => {
        if (active) setCatalog(loaded)
      })
      .catch(() => {
        // A failed listing only costs the picker its options; the default model still answers.
      })
    return () => {
      active = false
    }
  }, [])

  const label = value ? `${value.provider}/${value.model}` : t('chat.modelDefault', '默认模型')

  return (
    <Popover
      label={t('chat.model', '模型')}
      icon={<Cpu className="h-3.5 w-3.5" aria-hidden="true" />}
      text={<span className="max-w-[200px] truncate font-mono text-micro text-ink-2">{label}</span>}
    >
      <ModelPicker
        value={value}
        catalog={catalog}
        running={running}
        onChange={(next) => void onChange(next)}
      />
    </Popover>
  )
}

function ModelPicker({
  value,
  catalog,
  running,
  onChange,
}: {
  value: ModelOverride | null
  catalog: ProviderCatalog | null
  running: boolean
  onChange: (model: ModelOverride | null) => void
}) {
  const { t } = useI18n()
  const fallback = catalog?.defaults ?? { provider: null, model: null }
  const [provider, setProvider] = useState(value?.provider ?? fallback.provider ?? '')
  const [model, setModel] = useState(value?.model ?? '')
  const [error, setError] = useState<string | null>(null)

  // Only providers that can answer right now (a key is set, or it is a local runtime), plus the
  // pinned one when it has since lost its key: opening the panel must never rewrite the pin.
  const options = useMemo(() => {
    const list = [
      ...(catalog?.providers ?? [])
        .filter((item) => item.configured)
        .map((item) => ({ value: item.key, label: `${item.label} (${item.key})` })),
      ...(catalog?.extra ?? [])
        .filter((item) => item.configured)
        .map((item) => ({ value: item.name, label: item.name })),
    ]
    if (value && !list.some((item) => item.value === value.provider)) {
      list.push({ value: value.provider, label: value.provider })
    }
    return list
  }, [catalog, value])
  const isDefault = value === null
  const defaultLabel = fallback.provider && fallback.model
    ? `${fallback.provider}/${fallback.model}`
    : t('chat.modelDefault', '默认模型')

  const apply = () => {
    const trimmedProvider = provider.trim()
    const trimmedModel = model.trim()
    if (!trimmedProvider || !trimmedModel) {
      setError(t('chat.modelIncomplete', '请选择 Provider 并填写模型 ID'))
      return
    }
    setError(null)
    onChange({ provider: trimmedProvider, model: trimmedModel })
  }

  return (
    <div className="w-72 px-1 pb-1">
      <MenuLabel>{t('chat.model', '模型')}</MenuLabel>
      <MenuItem
        selected={isDefault}
        onSelect={() => onChange(null)}
        aside={isDefault ? <Check className="h-3.5 w-3.5 text-accent" /> : undefined}
      >
        <span className="font-mono text-micro">{defaultLabel}</span>
      </MenuItem>

      <MenuSeparator />
      <div className="space-y-1.5 px-1.5">
        <label className="block text-micro text-ink-3" htmlFor="model-chip-provider">
          {t('chat.modelProvider', 'Provider')}
        </label>
        <select
          id="model-chip-provider"
          value={provider}
          onChange={(event) => setProvider(event.target.value)}
          className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 text-small text-ink focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
        >
          <option value="">{t('chat.modelPickProvider', '选择 Provider')}</option>
          {options.map((item) => (
            <option key={item.value} value={item.value}>
              {item.label}
            </option>
          ))}
        </select>

        <label className="block text-micro text-ink-3" htmlFor="model-chip-model">
          {t('chat.modelId', '模型 ID')}
        </label>
        <input
          id="model-chip-model"
          value={model}
          onChange={(event) => setModel(event.target.value)}
          placeholder={fallback.model ?? 'gpt-4o'}
          className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 font-mono text-small text-ink placeholder:text-ink-3 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
        />

        <Button size="sm" className="w-full" onClick={apply} disabled={!provider.trim() || !model.trim()}>
          {t('chat.modelApply', '在此对话使用')}
        </Button>
        {error && <p className="text-micro text-error">{error}</p>}
        {running && (
          <p className="flex items-center gap-1.5 text-micro text-ink-3">
            <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
            {t('chat.modelSwitchNextTurn', '切换将在下一轮生效')}
          </p>
        )}
      </div>
    </div>
  )
}
