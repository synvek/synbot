import React from 'react'
import type { SectionProps } from '../types'
import { FieldGroup, NumberField, SecretField, Subheading, TextField, Toggle } from '../FormPrimitives'
import { asRecord, CONFIG_SECRET_MASK } from '../immutable'

export const WebSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const web = asRecord(draft.web)
  const enabled = Boolean(web.enabled ?? false)
  const port = typeof web.port === 'number' ? web.port : 18888
  const host = typeof web.host === 'string' ? web.host : '127.0.0.1'
  const auth = web.auth ? asRecord(web.auth) : null
  const username = typeof auth?.username === 'string' ? auth.username : ''
  const password =
    auth && typeof auth.password === 'string' ? auth.password : auth ? CONFIG_SECRET_MASK : ''
  const corsOrigins = Array.isArray(web.corsOrigins) ? (web.corsOrigins as string[]).join('\n') : ''
  const showToolCalls = Boolean(web.showToolCalls ?? true)

  const patchWeb = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      web: { ...asRecord(d.web), ...patch },
    }))
  }

  const setAuth = (u: string, p: string) => {
    const tu = u.trim()
    if (!tu) {
      patchWeb({ auth: null })
      return
    }
    patchWeb({ auth: { username: tu, password: p } })
  }

  return (
    <FieldGroup>
      <Toggle
        id="webEnabled"
        label={t('config.sections.web.enabled')}
        checked={enabled}
        onChange={(v) => patchWeb({ enabled: v })}
      />
      <NumberField
        id="webPort"
        label={t('config.sections.web.port')}
        value={port}
        min={1}
        max={65535}
        onChange={(v) => patchWeb({ port: v })}
      />
      <TextField
        id="webHost"
        label={t('config.sections.web.host')}
        value={host}
        onChange={(v) => patchWeb({ host: v })}
      />
      <Toggle
        id="webShowToolCalls"
        label={t('config.sections.web.showToolCalls')}
        checked={showToolCalls}
        onChange={(v) => patchWeb({ showToolCalls: v })}
      />
      <Subheading>{t('config.sections.web.auth')}</Subheading>
      <p className="text-sm text-text-secondary -mt-2 mb-2">{t('config.sections.web.authHint')}</p>
      <TextField
        id="webAuthUser"
        label={t('config.sections.web.authUsername')}
        value={username}
        onChange={(v) => setAuth(v, password)}
      />
      <SecretField
        id="webAuthPass"
        label={t('config.sections.web.authPassword')}
        value={password}
        leaveUnchangedHint={auth ? t('config.secretLeaveUnchanged') : undefined}
        onChange={(v) => setAuth(username, v)}
      />
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="corsOrigins">
          {t('config.sections.web.corsOrigins')}
        </label>
        <textarea
          id="corsOrigins"
          spellCheck={false}
          className="w-full min-h-[88px] font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border text-text focus:outline-none focus:ring-2 focus:ring-primary/30"
          value={corsOrigins}
          onChange={(e) =>
            patchWeb({
              corsOrigins: e.target.value
                .split(/\r?\n/)
                .map((s) => s.trim())
                .filter(Boolean),
            })
          }
          placeholder={t('config.sections.web.corsOriginsPlaceholder')}
        />
        <p className="text-xs text-text-secondary mt-1">{t('config.sections.web.corsOriginsHint')}</p>
      </div>
    </FieldGroup>
  )
}
