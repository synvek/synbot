import React, { useState } from 'react'
import type { SectionProps } from '../types'
import {
  FieldGroup,
  NumberField,
  SecretField,
  Subheading,
  TextField,
  Toggle,
} from '../FormPrimitives'
import { asArray, asRecord, CONFIG_SECRET_MASK } from '../immutable'

const PROVIDERS = [
  'telegram',
  'discord',
  'feishu',
  'slack',
  'email',
  'matrix',
  'dingtalk',
  'whatsapp',
  'irc',
] as const

type ProviderId = (typeof PROVIDERS)[number]

function channelsRoot(draft: Record<string, unknown>): Record<string, unknown> {
  return asRecord(draft.channels)
}

export const ChannelsSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const [tab, setTab] = useState<ProviderId>('telegram')
  const ch = channelsRoot(draft)

  const setChannels = (next: Record<string, unknown>) => {
    setDraft((d) => ({ ...d, channels: next }))
  }

  const getList = (id: ProviderId): unknown[] => {
    if (id === 'whatsapp' || id === 'irc') {
      const v = ch[id]
      if (v == null) return []
      return asArray(v)
    }
    return asArray(ch[id])
  }

  const setList = (id: ProviderId, list: unknown[]) => {
    if (id === 'whatsapp' || id === 'irc') {
      setChannels({ ...ch, [id]: list.length ? list : null })
    } else {
      setChannels({ ...ch, [id]: list })
    }
  }

  return (
    <FieldGroup>
      <div className="flex flex-wrap gap-1 mb-4 border-b border-border pb-2">
        {PROVIDERS.map((p) => (
          <button
            key={p}
            type="button"
            className={`px-3 py-1.5 rounded-md text-sm font-medium ${
              tab === p
                ? 'bg-primary text-white'
                : 'bg-surface text-text-secondary hover:bg-background'
            }`}
            onClick={() => setTab(p)}
          >
            {t(`config.sections.channels.providers.${p}`)}
          </button>
        ))}
      </div>

      {tab === 'telegram' && (
        <SimpleTokenChannel list={getList('telegram')} setList={(l) => setList('telegram', l)} t={t} />
      )}
      {tab === 'discord' && (
        <SimpleTokenChannel list={getList('discord')} setList={(l) => setList('discord', l)} t={t} />
      )}
      {tab === 'feishu' && (
        <FeishuList list={getList('feishu')} setList={(l) => setList('feishu', l)} t={t} />
      )}
      {tab === 'slack' && (
        <SlackList list={getList('slack')} setList={(l) => setList('slack', l)} t={t} />
      )}
      {tab === 'email' && (
        <EmailList list={getList('email')} setList={(l) => setList('email', l)} t={t} />
      )}
      {tab === 'matrix' && (
        <MatrixList list={getList('matrix')} setList={(l) => setList('matrix', l)} t={t} />
      )}
      {tab === 'dingtalk' && (
        <DingTalkList list={getList('dingtalk')} setList={(l) => setList('dingtalk', l)} t={t} />
      )}
      {tab === 'whatsapp' && (
        <WhatsAppList list={getList('whatsapp')} setList={(l) => setList('whatsapp', l)} t={t} />
      )}
      {tab === 'irc' && (
        <IrcList list={getList('irc')} setList={(l) => setList('irc', l)} t={t} />
      )}
    </FieldGroup>
  )
}

const Allowlist: React.FC<{
  value: unknown[]
  onChange: (v: unknown[]) => void
  t: (k: string) => string
}> = ({ value, onChange, t }) => {
  const rows = value.length ? value : [{ chatId: '', chatAlias: '', myName: '' }]
  return (
    <div className="space-y-2">
      <Subheading className="!mt-2 !mb-1">{t('config.sections.channels.allowlist')}</Subheading>
      {rows.map((row, idx) => {
        const r = asRecord(row)
        return (
          <div key={idx} className="grid grid-cols-1 md:grid-cols-3 gap-2 items-end border border-border/60 rounded p-2">
            <TextField
              id={`al-c-${idx}`}
              label="chatId"
              value={typeof r.chatId === 'string' ? r.chatId : ''}
              onChange={(v) => {
                const next = [...rows]
                next[idx] = { ...asRecord(next[idx]), chatId: v }
                onChange(next)
              }}
            />
            <TextField
              id={`al-a-${idx}`}
              label="chatAlias"
              value={typeof r.chatAlias === 'string' ? r.chatAlias : ''}
              onChange={(v) => {
                const next = [...rows]
                next[idx] = { ...asRecord(next[idx]), chatAlias: v }
                onChange(next)
              }}
            />
            <div className="flex gap-2">
              <div className="flex-1">
                <TextField
                  id={`al-m-${idx}`}
                  label="myName"
                  value={r.myName != null ? String(r.myName) : ''}
                  onChange={(v) => {
                    const next = [...rows]
                    next[idx] = { ...asRecord(next[idx]), myName: v.trim() ? v : null }
                    onChange(next)
                  }}
                />
              </div>
              <button
                type="button"
                className="text-error text-sm px-2"
                onClick={() => onChange(rows.filter((_, i) => i !== idx))}
              >
                ×
              </button>
            </div>
          </div>
        )
      })}
      <button
        type="button"
        className="text-sm text-primary"
        onClick={() => onChange([...rows, { chatId: '', chatAlias: '', myName: null }])}
      >
        + {t('config.sections.channels.addAllowlist')}
      </button>
    </div>
  )
}

const SimpleTokenChannel: React.FC<{
  list: unknown[]
  setList: (l: unknown[]) => void
  t: (k: string) => string
}> = ({ list, setList, t }) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`n-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`e-${idx}`} />
          <SecretField
            label={t('config.sections.channels.token')}
            value={typeof o.token === 'string' ? o.token : CONFIG_SECRET_MASK}
            leaveUnchangedHint={t('config.secretLeaveUnchanged')}
            onChange={(v) => patch({ token: v })}
            id={`tok-${idx}`}
          />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`ea-${idx}`} />
          <TextField label={t('config.sections.channels.groupMyName')} value={o.groupMyName != null ? String(o.groupMyName) : ''} onChange={(v) => patch({ groupMyName: v.trim() ? v : null })} id={`gm-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`st-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`da-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { name: list.length ? 'instance' : 'default', enabled: false, token: '', allowlist: [], enableAllowlist: true, showToolCalls: true, defaultAgent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const FeishuList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`fn-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`fe-${idx}`} />
          <TextField label={t('config.sections.channels.appId')} value={String(o.appId ?? '')} onChange={(v) => patch({ appId: v })} id={`aid-${idx}`} />
          <SecretField label={t('config.sections.channels.appSecret')} value={typeof o.appSecret === 'string' ? o.appSecret : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patch({ appSecret: v })} id={`as-${idx}`} />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`fea-${idx}`} />
          <TextField label={t('config.sections.channels.groupMyName')} value={o.groupMyName != null ? String(o.groupMyName) : ''} onChange={(v) => patch({ groupMyName: v.trim() ? v : null })} id={`fgm-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`fst-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`fda-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { name: 'feishu', enabled: false, appId: '', appSecret: '', allowlist: [], enableAllowlist: true, showToolCalls: true, defaultAgent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const SlackList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`sn-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`se-${idx}`} />
          <SecretField label={t('config.sections.channels.token')} value={typeof o.token === 'string' ? o.token : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patch({ token: v })} id={`stok-${idx}`} />
          <SecretField label={t('config.sections.channels.appToken')} value={typeof o.appToken === 'string' ? o.appToken : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patch({ appToken: v })} id={`sat-${idx}`} />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`sea-${idx}`} />
          <TextField label={t('config.sections.channels.groupMyName')} value={o.groupMyName != null ? String(o.groupMyName) : ''} onChange={(v) => patch({ groupMyName: v.trim() ? v : null })} id={`sgm-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`sst-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`sda-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { name: 'slack', enabled: false, token: '', appToken: '', allowlist: [], enableAllowlist: true, showToolCalls: true, defaultAgent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const EmailList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const imap = asRecord(o.imap)
      const smtp = asRecord(o.smtp)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      const patchImap = (p: Record<string, unknown>) => patch({ imap: { ...imap, ...p } })
      const patchSmtp = (p: Record<string, unknown>) => patch({ smtp: { ...smtp, ...p } })
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`en-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`ee-${idx}`} />
          <Subheading className="!mt-2">IMAP</Subheading>
          <TextField label={t('config.sections.channels.emailHost')} value={String(imap.host ?? '')} onChange={(v) => patchImap({ host: v })} id={`ih-${idx}`} />
          <NumberField label={t('config.sections.channels.emailPort')} value={typeof imap.port === 'number' ? imap.port : 993} onChange={(v) => patchImap({ port: v })} id={`ip-${idx}`} />
          <TextField label={t('config.sections.channels.emailUsername')} value={String(imap.username ?? '')} onChange={(v) => patchImap({ username: v })} id={`iu-${idx}`} />
          <SecretField label={t('config.sections.channels.emailPassword')} value={typeof imap.password === 'string' ? imap.password : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patchImap({ password: v })} id={`ipw-${idx}`} />
          <Toggle label={t('config.sections.channels.emailUseTls')} checked={Boolean(imap.useTls ?? true)} onChange={(v) => patchImap({ useTls: v })} id={`itls-${idx}`} />
          <Subheading className="!mt-2">SMTP</Subheading>
          <TextField label={t('config.sections.channels.emailHost')} value={String(smtp.host ?? '')} onChange={(v) => patchSmtp({ host: v })} id={`sh-${idx}`} />
          <NumberField label={t('config.sections.channels.emailPort')} value={typeof smtp.port === 'number' ? smtp.port : 465} onChange={(v) => patchSmtp({ port: v })} id={`sp-${idx}`} />
          <TextField label={t('config.sections.channels.emailUsername')} value={String(smtp.username ?? '')} onChange={(v) => patchSmtp({ username: v })} id={`su-${idx}`} />
          <SecretField label={t('config.sections.channels.emailPassword')} value={typeof smtp.password === 'string' ? smtp.password : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patchSmtp({ password: v })} id={`spw-${idx}`} />
          <Toggle label={t('config.sections.channels.emailUseTls')} checked={Boolean(smtp.useTls ?? true)} onChange={(v) => patchSmtp({ useTls: v })} id={`stls-${idx}`} />
          <TextField label={t('config.sections.channels.fromSender')} value={String(o.fromSender ?? '')} onChange={(v) => patch({ fromSender: v })} id={`fs-${idx}`} />
          <TextField label={t('config.sections.channels.startTime')} value={String(o.startTime ?? '')} onChange={(v) => patch({ startTime: v })} id={`st-${idx}`} />
          <NumberField label={t('config.sections.channels.pollIntervalSecs')} value={typeof o.pollIntervalSecs === 'number' ? o.pollIntervalSecs : 120} onChange={(v) => patch({ pollIntervalSecs: v })} id={`pi-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`est-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`eda-${idx}`} />
        </div>
      )
    })}
    <button
      type="button"
      className="px-4 py-2 rounded-lg bg-surface border border-border text-sm"
      onClick={() =>
        setList([
          ...list,
          {
            name: 'email',
            enabled: false,
            imap: { host: '', port: 993, username: '', password: '', useTls: true },
            smtp: { host: '', port: 465, username: '', password: '', useTls: true },
            fromSender: '',
            startTime: '',
            pollIntervalSecs: 120,
            showToolCalls: true,
            defaultAgent: 'main',
          },
        ])
      }
    >
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const MatrixList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`mn-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`me-${idx}`} />
          <TextField label={t('config.sections.channels.homeserverUrl')} value={String(o.homeserverUrl ?? '')} onChange={(v) => patch({ homeserverUrl: v })} id={`mh-${idx}`} />
          <TextField label={t('config.sections.channels.username')} value={String(o.username ?? '')} onChange={(v) => patch({ username: v })} id={`mu-${idx}`} />
          <SecretField label={t('config.sections.channels.password')} value={typeof o.password === 'string' ? o.password : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patch({ password: v })} id={`mpw-${idx}`} />
          <TextField label={t('config.sections.channels.accessToken')} value={o.accessToken != null ? String(o.accessToken) : ''} onChange={(v) => patch({ accessToken: v.trim() ? v : null })} id={`mat-${idx}`} />
          <TextField label={t('config.sections.channels.storePath')} value={String(o.storePath ?? '')} onChange={(v) => patch({ storePath: v })} id={`msp-${idx}`} />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`mea-${idx}`} />
          <TextField label={t('config.sections.channels.groupMyName')} value={o.groupMyName != null ? String(o.groupMyName) : ''} onChange={(v) => patch({ groupMyName: v.trim() ? v : null })} id={`mgm-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`mst-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`mda-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { name: 'matrix', enabled: false, homeserverUrl: '', username: '', password: '', accessToken: null, storePath: '', allowlist: [], enableAllowlist: true, showToolCalls: true, defaultAgent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const DingTalkList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`dn-${idx}`} />
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`de-${idx}`} />
          <TextField label={t('config.sections.channels.clientId')} value={String(o.clientId ?? '')} onChange={(v) => patch({ clientId: v })} id={`dcid-${idx}`} />
          <SecretField label={t('config.sections.channels.clientSecret')} value={typeof o.clientSecret === 'string' ? o.clientSecret : CONFIG_SECRET_MASK} leaveUnchangedHint={t('config.secretLeaveUnchanged')} onChange={(v) => patch({ clientSecret: v })} id={`dcs-${idx}`} />
          <TextField label="appKey" value={o.appKey != null ? String(o.appKey) : ''} onChange={(v) => patch({ appKey: v.trim() ? v : null })} id={`dak-${idx}`} />
          <TextField label="appSecret" value={o.appSecret != null ? String(o.appSecret) : ''} onChange={(v) => patch({ appSecret: v.trim() ? v : null })} id={`das-${idx}`} />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`dea-${idx}`} />
          <Toggle label={t('config.sections.channels.showToolCalls')} checked={Boolean(o.showToolCalls ?? true)} onChange={(v) => patch({ showToolCalls: v })} id={`dst-${idx}`} />
          <TextField label={t('config.sections.channels.defaultAgent')} value={String(o.defaultAgent ?? 'main')} onChange={(v) => patch({ defaultAgent: v })} id={`dda-${idx}`} />
          <TextField label={t('config.sections.channels.robotCode')} value={String(o.robotCode ?? '')} onChange={(v) => patch({ robotCode: v })} id={`drc-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { name: 'dingtalk', enabled: false, clientId: '', clientSecret: '', allowlist: [], enableAllowlist: true, showToolCalls: true, defaultAgent: 'main', robotCode: '' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const WhatsAppList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`we-${idx}`} />
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`wn-${idx}`} />
          <TextField label={t('config.sections.channels.sessionDir')} value={String(o.sessionDir ?? '')} onChange={(v) => patch({ sessionDir: v })} id={`wsd-${idx}`} />
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <TextField label={t('config.sections.channels.agent')} value={String(o.agent ?? 'main')} onChange={(v) => patch({ agent: v })} id={`wa-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { enabled: false, name: 'whatsapp', sessionDir: '', allowlist: [], agent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)

const IrcList: React.FC<{ list: unknown[]; setList: (l: unknown[]) => void; t: (k: string) => string }> = ({
  list,
  setList,
  t,
}) => (
  <div className="space-y-4">
    {list.map((item, idx) => {
      const o = asRecord(item)
      const patch = (p: Record<string, unknown>) => {
        const next = [...list]
        next[idx] = { ...asRecord(next[idx]), ...p }
        setList(next)
      }
      return (
        <div key={idx} className="border border-border rounded-lg p-4 space-y-3">
          <div className="flex justify-between">
            <span className="font-medium text-sm">{t('config.sections.channels.instance')} {idx + 1}</span>
            <button type="button" className="text-sm text-error" onClick={() => setList(list.filter((_, i) => i !== idx))}>
              {t('common.delete')}
            </button>
          </div>
          <Toggle label={t('common.enabled')} checked={Boolean(o.enabled)} onChange={(v) => patch({ enabled: v })} id={`ie-${idx}`} />
          <TextField label={t('config.sections.channels.name')} value={String(o.name ?? '')} onChange={(v) => patch({ name: v })} id={`in-${idx}`} />
          <TextField label={t('config.sections.channels.ircServer')} value={o.server != null ? String(o.server) : ''} onChange={(v) => patch({ server: v.trim() ? v : null })} id={`isrv-${idx}`} />
          <NumberField label={t('config.sections.channels.ircPort')} value={typeof o.port === 'number' ? o.port : 6697} onChange={(v) => patch({ port: v })} id={`ipt-${idx}`} />
          <TextField label={t('config.sections.channels.ircNickname')} value={o.nickname != null ? String(o.nickname) : ''} onChange={(v) => patch({ nickname: v.trim() ? v : null })} id={`inick-${idx}`} />
          <div>
            <label className="block text-sm font-medium text-text mb-1">{t('config.sections.channels.ircChannels')}</label>
            <textarea
              className="w-full font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border"
              value={asArray<string>(o.channels).join('\n')}
              onChange={(e) =>
                patch({
                  channels: e.target.value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean),
                })
              }
            />
          </div>
          <Toggle label={t('config.sections.channels.ircUseTls')} checked={Boolean(o.useTls ?? true)} onChange={(v) => patch({ useTls: v })} id={`itls2-${idx}`} />
          <div>
            <label className="block text-sm font-medium text-text mb-1" htmlFor={`ipw2-${idx}`}>
              {t('config.sections.channels.password')}
            </label>
            <input
              id={`ipw2-${idx}`}
              type="password"
              autoComplete="new-password"
              className="w-full px-3 py-2 rounded-lg bg-surface border border-border text-text text-sm"
              value={o.password != null ? String(o.password) : ''}
              onChange={(e) =>
                patch({
                  password: e.target.value.trim() ? e.target.value : null,
                })
              }
            />
            <p className="text-xs text-text-secondary mt-1">{t('config.secretLeaveUnchanged')}</p>
          </div>
          <Allowlist value={asArray(o.allowlist)} onChange={(v) => patch({ allowlist: v })} t={t} />
          <Toggle label={t('config.sections.channels.enableAllowlist')} checked={Boolean(o.enableAllowlist ?? true)} onChange={(v) => patch({ enableAllowlist: v })} id={`iea-${idx}`} />
          <TextField label={t('config.sections.channels.agent')} value={String(o.agent ?? 'main')} onChange={(v) => patch({ agent: v })} id={`ia-${idx}`} />
        </div>
      )
    })}
    <button type="button" className="px-4 py-2 rounded-lg bg-surface border border-border text-sm" onClick={() => setList([...list, { enabled: false, name: 'irc', server: null, port: 6697, nickname: null, channels: [], useTls: true, password: null, allowlist: [], enableAllowlist: true, agent: 'main' }])}>
      {t('config.sections.channels.addInstance')}
    </button>
  </div>
)
