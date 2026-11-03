import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, Toggle } from '../FormPrimitives'
import { asRecord } from '../immutable'

/**
 * Desktop shell behaviour. The daemon ignores `desktop.*`; the Tauri shell reads it straight from
 * `config.json` on every window close, so a saved change applies without a restart.
 */
export const DesktopSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const desktop = asRecord(draft.desktop)
  const closeToTray = Boolean(desktop.closeToTray ?? false)

  const patchDesktop = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      desktop: { ...asRecord(d.desktop), ...patch },
    }))
  }

  return (
    <FieldGroup>
      <Toggle
        id="desktopCloseToTray"
        label={t('config.sections.desktop.closeToTray')}
        checked={closeToTray}
        onChange={(v) => patchDesktop({ closeToTray: v })}
      />
      <p className="text-sm text-ink-2 -mt-2">{t('config.sections.desktop.closeToTrayHint')}</p>
    </FieldGroup>
  )
}
