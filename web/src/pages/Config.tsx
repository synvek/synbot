import { useCallback, useEffect, useRef, useState } from 'react'
import { AxiosError } from 'axios'
import { Braces, Check, Copy, ListFilter, RotateCcw, Save, TriangleAlert } from 'lucide-react'
import { apiClient } from '../api/client'
import type { ConfigApiPayload, ValidationErrorItem } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { Button, PageHeader, SegmentedControl, cn } from '../ui'
import {
  ConfigSectionBody,
  SECTION_ORDER,
  type ConfigSectionId,
} from '../components/config/sectionRegistry'

type LoadState = 'idle' | 'loading' | 'ready' | 'error'
type ViewMode = 'form' | 'json'

export default function Config() {
  const [loadState, setLoadState] = useState<LoadState>('idle')
  const [error, setError] = useState<string | null>(null)
  const [payload, setPayload] = useState<ConfigApiPayload | null>(null)
  const [draftConfig, setDraftConfig] = useState<Record<string, unknown>>({})
  const [jsonText, setJsonText] = useState('')
  const [viewMode, setViewMode] = useState<ViewMode>('form')
  const [activeSection, setActiveSection] = useState<ConfigSectionId>('general')
  const [saveMessage, setSaveMessage] = useState<string | null>(null)
  const [validationErrors, setValidationErrors] = useState<ValidationErrorItem[]>([])
  const [copied, setCopied] = useState(false)
  const { t } = useI18n()
  const hasLoadedOnce = useRef(false)

  const load = useCallback(async () => {
    try {
      setLoadState('loading')
      setError(null)
      setSaveMessage(null)
      setValidationErrors([])
      const data = await apiClient.getConfig()
      setPayload(data)
      const cfg = data.config as Record<string, unknown>
      setDraftConfig(cfg)
      setJsonText(JSON.stringify(cfg, null, 2))
      hasLoadedOnce.current = true
      setLoadState('ready')
    } catch (err) {
      setError(t('config.failedToFetch', 'Failed to load the configuration'))
      console.error(err)
      setLoadState(hasLoadedOnce.current ? 'ready' : 'error')
    }
  }, [t])

  useEffect(() => {
    void load()
  }, [load])

  const switchToJson = () => {
    setJsonText(JSON.stringify(draftConfig, null, 2))
    setViewMode('json')
  }

  const switchToForm = () => {
    try {
      const parsed = JSON.parse(jsonText) as unknown
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        setError(t('config.invalidJson', 'The JSON is not a configuration object'))
        return
      }
      setDraftConfig(parsed as Record<string, unknown>)
      setError(null)
      setViewMode('form')
    } catch {
      setError(t('config.invalidJson', 'The JSON is not a configuration object'))
    }
  }

  const formatJson = () => {
    try {
      setJsonText(JSON.stringify(JSON.parse(jsonText), null, 2))
      setValidationErrors([])
      setError(null)
    } catch {
      setError(t('config.invalidJson', 'The JSON is not a configuration object'))
    }
  }

  const copyJson = async () => {
    try {
      await navigator.clipboard.writeText(jsonText)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      /* Clipboard denied: the text stays selectable, which is the fallback. */
    }
  }

  const resetToLoaded = () => {
    if (!payload?.config) return
    const cfg = payload.config as Record<string, unknown>
    setDraftConfig(cfg)
    setJsonText(JSON.stringify(cfg, null, 2))
    setValidationErrors([])
    setSaveMessage(null)
    setError(null)
  }

  const save = async () => {
    setSaveMessage(null)
    setValidationErrors([])
    setError(null)

    let toSave: unknown
    if (viewMode === 'json') {
      try {
        toSave = JSON.parse(jsonText)
      } catch {
        setError(t('config.invalidJson', 'The JSON is not a configuration object'))
        return
      }
    } else {
      toSave = JSON.parse(JSON.stringify(draftConfig))
    }

    try {
      const res = await apiClient.updateConfig(toSave)
      setSaveMessage(t('config.saveSuccess', 'Configuration saved'))
      const cfg = toSave as Record<string, unknown>
      setPayload({ config: cfg, configPath: res.configPath, restartNotice: res.restartNotice })
      setDraftConfig(cfg)
      setJsonText(JSON.stringify(cfg, null, 2))
    } catch (err) {
      if (err instanceof AxiosError && err.response?.status === 400) {
        const body = err.response.data as {
          data?: { validationErrors?: ValidationErrorItem[] }
          error?: string
        }
        const list = body.data?.validationErrors ?? []
        if (list.length > 0) {
          setValidationErrors(list)
          setError(body.error ?? t('config.validationFailed', 'The configuration was rejected'))
        } else {
          setError(body.error ?? t('config.saveFailed', 'Could not save the configuration'))
        }
      } else {
        setError(t('config.saveFailed', 'Could not save the configuration'))
        console.error(err)
      }
    }
  }

  if (loadState === 'loading' || loadState === 'idle') {
    return <p className="py-10 text-center text-small text-ink-2">{t('common.loading', 'Loading…')}</p>
  }

  if (loadState === 'error' && !payload) {
    return (
      <div>
        <PageHeader title={t('config.title', 'Configuration')} />
        <div className="rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>
      </div>
    )
  }

  return (
    <div className="flex min-h-full flex-col">
      <PageHeader
        title={t('config.title', 'Configuration')}
        description={t('config.description', 'Everything the daemon reads at start-up')}
        actions={
          <SegmentedControl
            ariaLabel={t('config.sections.title', 'View')}
            value={viewMode}
            onChange={(next) => (next === 'json' ? switchToJson() : switchToForm())}
            options={[
              { value: 'form', label: t('config.formView', 'Form'), icon: <ListFilter className="h-3 w-3" /> },
              { value: 'json', label: t('config.jsonEditor', 'JSON'), icon: <Braces className="h-3 w-3" /> },
            ]}
          />
        }
      />

      {payload?.configPath && (
        <p className="-mt-1 mb-3 truncate font-mono text-micro text-ink-3">
          {t('config.filePath', 'File')}: {payload.configPath}
        </p>
      )}

      {error && <div className="mb-3 rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}

      {validationErrors.length > 0 && (
        <div className="mb-3 rounded-panel border px-3 py-2">
          <p className="mb-1 flex items-center gap-1.5 text-small font-medium text-error">
            <TriangleAlert className="h-3.5 w-3.5" aria-hidden="true" />
            {t('config.validationErrorsTitle', 'The configuration was rejected')}
          </p>
          <ul className="space-y-0.5">
            {validationErrors.map((item, index) => (
              <li key={`${item.field}-${index}`} className="text-micro text-ink-2">
                <span className="font-mono text-ink">{item.field}</span>: {item.constraint}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="min-h-0 flex-1">
        {viewMode === 'form' ? (
          <div className="flex flex-col gap-4 lg:flex-row">
            <nav
              aria-label={t('config.sectionNav', 'Sections')}
              className="shrink-0 overflow-y-auto lg:w-52 lg:max-h-[calc(100vh-220px)]"
            >
              <ul className="flex flex-row gap-1 overflow-x-auto lg:flex-col lg:overflow-visible">
                {SECTION_ORDER.map((id) => (
                  <li key={id}>
                    <button
                      type="button"
                      onClick={() => setActiveSection(id)}
                      aria-current={activeSection === id ? 'true' : undefined}
                      className={cn(
                        'flex h-7 w-full items-center whitespace-nowrap rounded-control px-2 text-left text-small transition-colors duration-fast',
                        activeSection === id
                          ? 'bg-surface-2 font-medium text-ink'
                          : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
                      )}
                    >
                      {t(`config.sections.nav.${id}`)}
                    </button>
                  </li>
                ))}
              </ul>
            </nav>
            <div className="min-w-0 flex-1 rounded-panel border border-line px-4 py-3">
              <ConfigSectionBody id={activeSection} draft={draftConfig} setDraft={setDraftConfig} t={t} />
            </div>
          </div>
        ) : (
          <div className="overflow-hidden rounded-panel border border-line">
            <div className="flex items-center justify-between gap-2 border-b border-line px-2.5 py-1">
              <span className="truncate font-mono text-micro text-ink-3">
                {payload?.configPath ?? 'config'}
              </span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={formatJson}
                  className="text-micro text-ink-3 transition-colors duration-fast hover:text-ink"
                >
                  {t('config.formatJson', 'Format')}
                </button>
                <button
                  type="button"
                  onClick={() => void copyJson()}
                  aria-label={t('common.copy', 'Copy')}
                  className="text-ink-3 transition-colors duration-fast hover:text-ink"
                >
                  {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
                </button>
              </div>
            </div>
            <textarea
              value={jsonText}
              onChange={(event) => setJsonText(event.target.value)}
              spellCheck={false}
              aria-label={t('config.jsonEditor', 'JSON editor')}
              className="block max-h-[calc(100vh-300px)] min-h-[420px] w-full resize-y bg-surface-2 px-2.5 py-2 font-mono text-micro leading-relaxed text-ink outline-none"
            />
          </div>
        )}
      </div>

      {/* Sticky action bar: sits at the bottom of the viewport on short forms, follows the scroll on long ones. */}
      <div className="sticky bottom-0 -mx-6 mt-4 flex shrink-0 flex-wrap items-center gap-2 border-t border-line bg-background px-6 py-2">
        <p className="mr-auto min-w-0 text-micro text-ink-2">
          {saveMessage ? <span className="text-success">{saveMessage}</span> : null}
          {saveMessage && payload?.restartNotice ? ' · ' : null}
          {saveMessage ? payload?.restartNotice : null}
          {!saveMessage && t('config.note', 'Changes take effect after the daemon restarts.')}
        </p>
        <Button
          variant="ghost"
          size="sm"
          icon={<RotateCcw className="h-3.5 w-3.5" />}
          onClick={resetToLoaded}
        >
          {t('config.reset', 'Reset')}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => void load()}>
          {t('config.reload', 'Reload')}
        </Button>
        <Button variant="primary" size="sm" icon={<Save className="h-3.5 w-3.5" />} onClick={() => void save()}>
          {t('config.save', 'Save')}
        </Button>
      </div>
    </div>
  )
}
