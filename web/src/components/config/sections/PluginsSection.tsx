import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup } from '../FormPrimitives'
import { JsonObjectEditor } from '../JsonObjectEditor'
import { asRecord } from '../immutable'

export const PluginsSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const plugins = asRecord(
    draft.plugins && typeof draft.plugins === 'object' && !Array.isArray(draft.plugins)
      ? draft.plugins
      : {},
  )
  const keys = Object.keys(plugins)

  const setPlugins = (next: Record<string, unknown>) => {
    setDraft((d) => ({ ...d, plugins: next }))
  }

  return (
    <FieldGroup>
      <p className="text-sm text-text-secondary mb-4">{t('config.sections.plugins.hint')}</p>
      {keys.map((name) => (
        <div key={name} className="border border-border rounded-lg p-4 mb-4">
          <div className="flex justify-between items-center mb-2">
            <span className="font-mono text-sm font-medium">{name}</span>
            <button
              type="button"
              className="text-sm text-error"
              onClick={() => {
                const next = { ...plugins }
                delete next[name]
                setPlugins(next)
              }}
            >
              {t('common.delete')}
            </button>
          </div>
          <JsonObjectEditor
            label={t('config.sections.plugins.pluginJson')}
            value={plugins[name]}
            onChange={(v) => setPlugins({ ...plugins, [name]: v })}
          />
        </div>
      ))}
      <button
        type="button"
        className="px-4 py-2 rounded-lg bg-surface border border-border text-sm"
        onClick={() => {
          const n = window.prompt(t('config.sections.plugins.promptName'))
          if (!n?.trim()) return
          const key = n.trim()
          if (key in plugins) return
          setPlugins({ ...plugins, [key]: {} })
        }}
      >
        {t('config.sections.plugins.add')}
      </button>
    </FieldGroup>
  )
}
