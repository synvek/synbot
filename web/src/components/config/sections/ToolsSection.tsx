import React from 'react'
import type { SectionProps } from '../types'
import {
  FieldGroup,
  NumberField,
  SecretField,
  SelectField,
  Subheading,
  TextField,
  Toggle,
} from '../FormPrimitives'
import { asArray, asRecord } from '../immutable'

export const ToolsSection: React.FC<SectionProps> = ({ draft, setDraft, t }) => {
  const tools = asRecord(draft.tools)

  const patchTools = (patch: Record<string, unknown>) => {
    setDraft((d) => ({
      ...d,
      tools: { ...asRecord(d.tools), ...patch },
    }))
  }

  const exec = asRecord(tools.exec)
  const webT = asRecord(tools.web)
  const browser = asRecord(tools.browser)
  const gen = asRecord(tools.generation)
  const image = asRecord(gen.image)
  const video = asRecord(gen.video)
  const speech = asRecord(gen.speech)
  const mcp = tools.mcp ? asRecord(tools.mcp) : null
  const servers = mcp ? asArray<unknown>(mcp.servers) : []

  const perm = asRecord(exec.permissions)
  const rules = asArray<unknown>(perm.rules)

  return (
    <FieldGroup>
      <Subheading>{t('config.sections.tools.exec')}</Subheading>
      <NumberField
        id="execTimeout"
        label={t('config.sections.tools.execTimeout')}
        value={typeof exec.timeoutSecs === 'number' ? exec.timeoutSecs : 300}
        min={1}
        onChange={(v) => patchTools({ exec: { ...exec, timeoutSecs: v } })}
      />
      <Toggle
        id="execRtw"
        label={t('config.sections.tools.restrictToWorkspace')}
        checked={Boolean(exec.restrictToWorkspace)}
        onChange={(v) => patchTools({ exec: { ...exec, restrictToWorkspace: v } })}
      />
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="denyPat">
          {t('config.sections.tools.denyPatterns')}
        </label>
        <textarea
          id="denyPat"
          className="w-full min-h-[100px] font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border text-text"
          value={asArray<string>(exec.denyPatterns).join('\n')}
          onChange={(e) =>
            patchTools({
              exec: {
                ...exec,
                denyPatterns: e.target.value.split(/\r?\n/).filter((x) => x.length > 0),
              },
            })
          }
        />
      </div>
      <div>
        <label className="block text-sm font-medium text-text mb-1" htmlFor="allowPat">
          {t('config.sections.tools.allowPatterns')}
        </label>
        <textarea
          id="allowPat"
          className="w-full min-h-[80px] font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border text-text"
          value={
            exec.allowPatterns == null
              ? ''
              : asArray<string>(exec.allowPatterns).join('\n')
          }
          onChange={(e) => {
            const lines = e.target.value.split(/\r?\n/).filter((x) => x.length > 0)
            patchTools({
              exec: {
                ...exec,
                allowPatterns: lines.length ? lines : null,
              },
            })
          }}
        />
      </div>

      <Subheading>{t('config.sections.tools.permissions')}</Subheading>
      <Toggle
        id="permEn"
        label={t('config.sections.tools.permEnabled')}
        checked={Boolean(perm.enabled)}
        onChange={(v) =>
          patchTools({
            exec: {
              ...exec,
              permissions: { ...perm, enabled: v },
            },
          })
        }
      />
      <SelectField
        id="permDef"
        label={t('config.sections.tools.permDefaultLevel')}
        value={typeof perm.defaultLevel === 'string' ? perm.defaultLevel : 'require_approval'}
        onChange={(v) =>
          patchTools({
            exec: {
              ...exec,
              permissions: { ...perm, defaultLevel: v },
            },
          })
        }
        options={[
          { value: 'allow', label: 'allow' },
          { value: 'require_approval', label: 'require_approval' },
          { value: 'deny', label: 'deny' },
        ]}
      />
      <NumberField
        id="permAppr"
        label={t('config.sections.tools.approvalTimeoutSecs')}
        value={typeof perm.approvalTimeoutSecs === 'number' ? perm.approvalTimeoutSecs : 300}
        min={1}
        onChange={(v) =>
          patchTools({
            exec: {
              ...exec,
              permissions: { ...perm, approvalTimeoutSecs: v },
            },
          })
        }
      />
      {rules.map((r, idx) => {
        const row = asRecord(r)
        return (
          <div key={idx} className="border border-border rounded p-3 space-y-2">
            <div className="flex justify-between">
              <span className="text-sm font-medium">{t('config.sections.tools.rule')} {idx + 1}</span>
              <button
                type="button"
                className="text-sm text-error"
                onClick={() => {
                  const next = rules.filter((_, i) => i !== idx)
                  patchTools({
                    exec: {
                      ...exec,
                      permissions: { ...perm, rules: next },
                    },
                  })
                }}
              >
                {t('common.delete')}
              </button>
            </div>
            <TextField
              id={`pr-${idx}`}
              label={t('config.sections.tools.rulePattern')}
              value={typeof row.pattern === 'string' ? row.pattern : ''}
              onChange={(v) => {
                const next = [...rules]
                next[idx] = { ...asRecord(next[idx]), pattern: v }
                patchTools({
                  exec: {
                    ...exec,
                    permissions: { ...perm, rules: next },
                  },
                })
              }}
            />
            <SelectField
              id={`pl-${idx}`}
              label={t('config.sections.tools.ruleLevel')}
              value={typeof row.level === 'string' ? row.level : 'require_approval'}
              onChange={(v) => {
                const next = [...rules]
                next[idx] = { ...asRecord(next[idx]), level: v }
                patchTools({
                  exec: {
                    ...exec,
                    permissions: { ...perm, rules: next },
                  },
                })
              }}
              options={[
                { value: 'allow', label: 'allow' },
                { value: 'require_approval', label: 'require_approval' },
                { value: 'deny', label: 'deny' },
              ]}
            />
            <TextField
              id={`pd-${idx}`}
              label={t('config.sections.tools.ruleDescription')}
              value={row.description != null ? String(row.description) : ''}
              onChange={(v) => {
                const next = [...rules]
                next[idx] = {
                  ...asRecord(next[idx]),
                  description: v.trim() ? v : null,
                }
                patchTools({
                  exec: {
                    ...exec,
                    permissions: { ...perm, rules: next },
                  },
                })
              }}
            />
          </div>
        )
      })}
      <button
        type="button"
        className="px-3 py-1.5 rounded-lg bg-surface border border-border text-sm"
        onClick={() =>
          patchTools({
            exec: {
              ...exec,
              permissions: {
                ...perm,
                rules: [...rules, { pattern: '', level: 'require_approval', description: null }],
              },
            },
          })
        }
      >
        {t('config.sections.tools.addRule')}
      </button>

      <Subheading>{t('config.sections.tools.webSearch')}</Subheading>
      <SecretField
        id="braveKey"
        label={t('config.sections.tools.braveApiKey')}
        value={typeof webT.braveApiKey === 'string' ? webT.braveApiKey : ''}
        leaveUnchangedHint={t('config.secretLeaveUnchanged')}
        onChange={(v) => patchTools({ web: { ...webT, braveApiKey: v } })}
      />
      <SecretField
        id="tavKey"
        label={t('config.sections.tools.tavilyApiKey')}
        value={typeof webT.tavilyApiKey === 'string' ? webT.tavilyApiKey : ''}
        leaveUnchangedHint={t('config.secretLeaveUnchanged')}
        onChange={(v) => patchTools({ web: { ...webT, tavilyApiKey: v } })}
      />
      <SelectField
        id="searchBack"
        label={t('config.sections.tools.searchBackend')}
        value={typeof webT.searchBackend === 'string' ? webT.searchBackend : 'duckDuckGo'}
        onChange={(v) => patchTools({ web: { ...webT, searchBackend: v } })}
        options={[
          { value: 'duckDuckGo', label: 'duckDuckGo' },
          { value: 'searxNG', label: 'searxNG' },
          { value: 'brave', label: 'brave' },
          { value: 'tavily', label: 'tavily' },
          { value: 'firecrawl', label: 'firecrawl' },
        ]}
      />
      <TextField
        id="searxUrl"
        label={t('config.sections.tools.searxngUrl')}
        value={typeof webT.searxngUrl === 'string' ? webT.searxngUrl : ''}
        onChange={(v) => patchTools({ web: { ...webT, searxngUrl: v } })}
      />
      <SecretField
        id="fcKey"
        label={t('config.sections.tools.firecrawlApiKey')}
        value={typeof webT.firecrawlApiKey === 'string' ? webT.firecrawlApiKey : ''}
        leaveUnchangedHint={t('config.secretLeaveUnchanged')}
        onChange={(v) => patchTools({ web: { ...webT, firecrawlApiKey: v } })}
      />
      <NumberField
        id="searchCount"
        label={t('config.sections.tools.searchCount')}
        value={typeof webT.searchCount === 'number' ? webT.searchCount : 5}
        min={1}
        onChange={(v) => patchTools({ web: { ...webT, searchCount: v } })}
      />

      <Subheading>{t('config.sections.tools.browser')}</Subheading>
      <Toggle
        id="brEn"
        label={t('config.sections.tools.browserEnabled')}
        checked={Boolean(browser.enabled ?? true)}
        onChange={(v) => patchTools({ browser: { ...browser, enabled: v } })}
      />
      <TextField
        id="brExe"
        label={t('config.sections.tools.browserExecutable')}
        value={typeof browser.executable === 'string' ? browser.executable : 'agent-browser'}
        onChange={(v) => patchTools({ browser: { ...browser, executable: v } })}
      />
      <NumberField
        id="brTo"
        label={t('config.sections.tools.browserTimeout')}
        value={typeof browser.timeoutSecs === 'number' ? browser.timeoutSecs : 30}
        min={1}
        onChange={(v) => patchTools({ browser: { ...browser, timeoutSecs: v } })}
      />

      <Subheading>{t('config.sections.tools.generationImage')}</Subheading>
      <Toggle
        id="imgEn"
        label={t('config.sections.tools.enabled')}
        checked={Boolean(image.enabled)}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, enabled: v } } })
        }
      />
      <TextField
        id="imgProv"
        label={t('config.sections.tools.genProvider')}
        value={typeof image.provider === 'string' ? image.provider : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, provider: v } } })
        }
      />
      <TextField
        id="imgOut"
        label={t('config.sections.tools.genOutputDir')}
        value={typeof image.outputDir === 'string' ? image.outputDir : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, outputDir: v } } })
        }
      />
      <TextField
        id="imgModel"
        label={t('config.sections.tools.genModel')}
        value={typeof image.model === 'string' ? image.model : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, model: v } } })
        }
      />
      <TextField
        id="imgSize"
        label={t('config.sections.tools.imageSize')}
        value={typeof image.size === 'string' ? image.size : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, size: v } } })
        }
      />
      <TextField
        id="imgQual"
        label={t('config.sections.tools.imageQuality')}
        value={typeof image.quality === 'string' ? image.quality : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, image: { ...image, quality: v } } })
        }
      />

      <Subheading>{t('config.sections.tools.generationVideo')}</Subheading>
      <Toggle
        id="vidEn"
        label={t('config.sections.tools.enabled')}
        checked={Boolean(video.enabled)}
        onChange={(v) =>
          patchTools({ generation: { ...gen, video: { ...video, enabled: v } } })
        }
      />
      <TextField
        id="vidProv"
        label={t('config.sections.tools.genProvider')}
        value={typeof video.provider === 'string' ? video.provider : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, video: { ...video, provider: v } } })
        }
      />
      <TextField
        id="vidOut"
        label={t('config.sections.tools.genOutputDir')}
        value={typeof video.outputDir === 'string' ? video.outputDir : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, video: { ...video, outputDir: v } } })
        }
      />
      <TextField
        id="vidModel"
        label={t('config.sections.tools.genModel')}
        value={typeof video.model === 'string' ? video.model : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, video: { ...video, model: v } } })
        }
      />

      <Subheading>{t('config.sections.tools.generationSpeech')}</Subheading>
      <Toggle
        id="spEn"
        label={t('config.sections.tools.enabled')}
        checked={Boolean(speech.enabled)}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, enabled: v } } })
        }
      />
      <TextField
        id="spProv"
        label={t('config.sections.tools.genProvider')}
        value={typeof speech.provider === 'string' ? speech.provider : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, provider: v } } })
        }
      />
      <TextField
        id="spOut"
        label={t('config.sections.tools.genOutputDir')}
        value={typeof speech.outputDir === 'string' ? speech.outputDir : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, outputDir: v } } })
        }
      />
      <TextField
        id="spModel"
        label={t('config.sections.tools.genModel')}
        value={typeof speech.model === 'string' ? speech.model : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, model: v } } })
        }
      />
      <TextField
        id="spVoice"
        label={t('config.sections.tools.speechVoice')}
        value={typeof speech.voice === 'string' ? speech.voice : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, voice: v } } })
        }
      />
      <TextField
        id="spFmt"
        label={t('config.sections.tools.speechFormat')}
        value={typeof speech.format === 'string' ? speech.format : ''}
        onChange={(v) =>
          patchTools({ generation: { ...gen, speech: { ...speech, format: v } } })
        }
      />

      <Subheading>{t('config.sections.tools.mcp')}</Subheading>
      <Toggle
        id="mcpOn"
        label={t('config.sections.tools.mcpEnabled')}
        checked={mcp != null}
        onChange={(v) => {
          if (v) patchTools({ mcp: { servers: [] } })
          else patchTools({ mcp: null })
        }}
      />
      {mcp &&
        servers.map((s, idx) => {
          const srv = asRecord(s)
          return (
            <div key={idx} className="border border-border rounded p-3 space-y-2">
              <div className="flex justify-between">
                <span className="text-sm font-medium">
                  {t('config.sections.tools.mcpServer')} {idx + 1}
                </span>
                <button
                  type="button"
                  className="text-sm text-error"
                  onClick={() => {
                    const next = servers.filter((_, i) => i !== idx)
                    patchTools({ mcp: { servers: next } })
                  }}
                >
                  {t('common.delete')}
                </button>
              </div>
              <TextField
                id={`mcp-id-${idx}`}
                label="id"
                value={typeof srv.id === 'string' ? srv.id : ''}
                onChange={(v) => {
                  const next = [...servers]
                  next[idx] = { ...asRecord(next[idx]), id: v }
                  patchTools({ mcp: { servers: next } })
                }}
              />
              <SelectField
                id={`mcp-tr-${idx}`}
                label={t('config.sections.tools.mcpTransport')}
                value={typeof srv.transport === 'string' ? srv.transport : 'stdio'}
                onChange={(v) => {
                  const next = [...servers]
                  next[idx] = { ...asRecord(next[idx]), transport: v }
                  patchTools({ mcp: { servers: next } })
                }}
                options={[
                  { value: 'stdio', label: 'stdio' },
                  { value: 'sse', label: 'sse' },
                ]}
              />
              <TextField
                id={`mcp-cmd-${idx}`}
                label={t('config.sections.tools.mcpCommand')}
                value={typeof srv.command === 'string' ? srv.command : ''}
                onChange={(v) => {
                  const next = [...servers]
                  next[idx] = { ...asRecord(next[idx]), command: v }
                  patchTools({ mcp: { servers: next } })
                }}
              />
              <div>
                <label className="block text-sm font-medium text-text mb-1">{t('config.sections.tools.mcpArgs')}</label>
                <textarea
                  className="w-full font-mono text-sm px-3 py-2 rounded-lg bg-surface border border-border"
                  value={asArray<string>(srv.args).join('\n')}
                  onChange={(e) => {
                    const next = [...servers]
                    next[idx] = {
                      ...asRecord(next[idx]),
                      args: e.target.value.split(/\r?\n/).filter((l) => l.length > 0),
                    }
                    patchTools({ mcp: { servers: next } })
                  }}
                />
              </div>
              <TextField
                id={`mcp-url-${idx}`}
                label={t('config.sections.tools.mcpUrl')}
                value={typeof srv.url === 'string' ? srv.url : ''}
                onChange={(v) => {
                  const next = [...servers]
                  next[idx] = { ...asRecord(next[idx]), url: v }
                  patchTools({ mcp: { servers: next } })
                }}
              />
              <TextField
                id={`mcp-pfx-${idx}`}
                label={t('config.sections.tools.mcpToolPrefix')}
                value={srv.toolNamePrefix != null ? String(srv.toolNamePrefix) : ''}
                onChange={(v) => {
                  const next = [...servers]
                  next[idx] = {
                    ...asRecord(next[idx]),
                    toolNamePrefix: v.trim() ? v : null,
                  }
                  patchTools({ mcp: { servers: next } })
                }}
              />
            </div>
          )
        })}
      {mcp && (
        <button
          type="button"
          className="px-3 py-1.5 rounded-lg bg-surface border border-border text-sm"
          onClick={() =>
            patchTools({
              mcp: {
                servers: [
                  ...servers,
                  {
                    id: 'new',
                    transport: 'stdio',
                    command: '',
                    args: [],
                    url: '',
                    toolNamePrefix: null,
                  },
                ],
              },
            })
          }
        >
          {t('config.sections.tools.addMcpServer')}
        </button>
      )}
    </FieldGroup>
  )
}
