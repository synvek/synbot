import React, { useCallback, useEffect, useRef, useState } from 'react'
import { AxiosError } from 'axios'
import { apiClient } from '../api/client'
import type { ConfigApiPayload, ValidationErrorItem } from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import {
  ConfigSectionBody,
  SECTION_ORDER,
  type ConfigSectionId,
} from '../components/config/sectionRegistry'

type LoadState = 'idle' | 'loading' | 'ready' | 'error'

type ViewMode = 'form' | 'json'

const Config: React.FC = () => {
  const [loadState, setLoadState] = useState<LoadState>('idle')
  const [error, setError] = useState<string | null>(null)
  const [payload, setPayload] = useState<ConfigApiPayload | null>(null)
  const [draftConfig, setDraftConfig] = useState<Record<string, unknown>>({})
  const [jsonText, setJsonText] = useState('')
  const [viewMode, setViewMode] = useState<ViewMode>('form')
  const [activeSection, setActiveSection] = useState<ConfigSectionId>('general')
  const [saveMessage, setSaveMessage] = useState<string | null>(null)
  const [validationErrors, setValidationErrors] = useState<ValidationErrorItem[]>([])
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
      setError(t('config.failedToFetch'))
      console.error(err)
      setLoadState(hasLoadedOnce.current ? 'ready' : 'error')
    }
  }, [t])

  useEffect(() => {
    load()
  }, [load])

  const switchToJson = () => {
    setJsonText(JSON.stringify(draftConfig, null, 2))
    setViewMode('json')
  }

  const switchToForm = () => {
    try {
      const parsed = JSON.parse(jsonText) as unknown
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        setError(t('config.invalidJson'))
        return
      }
      setDraftConfig(parsed as Record<string, unknown>)
      setError(null)
      setViewMode('form')
    } catch {
      setError(t('config.invalidJson'))
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
        setError(t('config.invalidJson'))
        return
      }
    } else {
      toSave = JSON.parse(JSON.stringify(draftConfig))
    }

    try {
      const res = await apiClient.updateConfig(toSave)
      setSaveMessage(t('config.saveSuccess'))
      const cfg = toSave as Record<string, unknown>
      const next: ConfigApiPayload = {
        config: cfg,
        configPath: res.configPath,
        restartNotice: res.restartNotice,
      }
      setPayload(next)
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
          setError(body.error ?? t('config.validationFailed'))
        } else {
          setError(body.error ?? t('config.saveFailed'))
        }
      } else {
        setError(t('config.saveFailed'))
        console.error(err)
      }
    }
  }

  if (loadState === 'loading' || loadState === 'idle') {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-text-secondary">{t('common.loading')}</div>
      </div>
    )
  }

  if (loadState === 'error' && !payload) {
    return (
      <div className="bg-error/10 border border-error/20 rounded-lg p-4">
        <p className="text-error">{error}</p>
      </div>
    )
  }

  return (
    <div>
      <div className="mb-6">
        <h2 className="text-2xl font-bold text-text">{t('config.title')}</h2>
        <p className="text-text-secondary mt-1">{t('config.description')}</p>
        {payload?.configPath && (
          <p className="text-sm text-text-secondary mt-2 font-mono break-all">
            {t('config.filePath')}: {payload.configPath}
          </p>
        )}
      </div>

      <div className="flex flex-wrap gap-2 mb-4">
        {viewMode === 'json' && (
          <button
            type="button"
            onClick={() => {
              try {
                const parsed = JSON.parse(jsonText) as unknown
                setJsonText(JSON.stringify(parsed, null, 2))
                setValidationErrors([])
                setError(null)
              } catch {
                setError(t('config.invalidJson'))
              }
            }}
            className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
          >
            {t('config.formatJson')}
          </button>
        )}
        <button
          type="button"
          onClick={resetToLoaded}
          className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
        >
          {t('config.reset')}
        </button>
        <button
          type="button"
          onClick={save}
          className="px-4 py-2 rounded-lg bg-primary text-white hover:opacity-90 text-sm font-medium"
        >
          {t('config.save')}
        </button>
        <button
          type="button"
          onClick={load}
          className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
        >
          {t('config.reload')}
        </button>
        {viewMode === 'form' ? (
          <button
            type="button"
            onClick={switchToJson}
            className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
          >
            {t('config.advancedJson')}
          </button>
        ) : (
          <button
            type="button"
            onClick={switchToForm}
            className="px-4 py-2 rounded-lg bg-surface border border-border text-text hover:bg-background text-sm"
          >
            {t('config.backToForm')}
          </button>
        )}
      </div>

      {error && (
        <div className="mb-4 bg-error/10 border border-error/20 rounded-lg p-3 text-error text-sm">{error}</div>
      )}

      {saveMessage && (
        <div className="mb-4 bg-success/10 border border-success/20 rounded-lg p-3 text-sm">
          <p className="text-success font-medium">{saveMessage}</p>
          {payload?.restartNotice && (
            <p className="mt-2 text-text-secondary">{payload.restartNotice}</p>
          )}
        </div>
      )}

      {validationErrors.length > 0 && (
        <div className="mb-4 bg-error/10 border border-error/20 rounded-lg p-4">
          <p className="font-medium text-error mb-2">{t('config.validationErrorsTitle')}</p>
          <ul className="list-disc list-inside space-y-1 text-sm text-text">
            {validationErrors.map((e, i) => (
              <li key={`${e.field}-${i}`}>
                <span className="font-mono">{e.field}</span>: {e.constraint}
              </li>
            ))}
          </ul>
        </div>
      )}

      {viewMode === 'form' ? (
        <div className="flex flex-col lg:flex-row gap-6">
          <nav
            className="lg:w-56 shrink-0 border border-border rounded-lg p-2 bg-surface/80 max-h-[70vh] overflow-y-auto"
            aria-label={t('config.sectionNav')}
          >
            <ul className="space-y-1">
              {SECTION_ORDER.map((id) => (
                <li key={id}>
                  <button
                    type="button"
                    onClick={() => setActiveSection(id)}
                    className={`w-full text-left px-3 py-2 rounded-md text-sm ${
                      activeSection === id
                        ? 'bg-primary text-white'
                        : 'text-text-secondary hover:bg-background'
                    }`}
                  >
                    {t(`config.sections.nav.${id}`)}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
          <div className="flex-1 min-w-0 border border-border rounded-lg p-6 bg-surface/40">
            <ConfigSectionBody id={activeSection} draft={draftConfig} setDraft={setDraftConfig} t={t} />
          </div>
        </div>
      ) : (
        <textarea
          value={jsonText}
          onChange={(e) => setJsonText(e.target.value)}
          spellCheck={false}
          className="w-full min-h-[480px] font-mono text-sm p-4 rounded-lg bg-surface border border-border text-text focus:outline-none focus:ring-2 focus:ring-primary/30"
          aria-label={t('config.jsonEditor')}
        />
      )}

      <div className="mt-6 bg-warning/10 border border-warning/20 rounded-lg p-4">
        <p className="text-sm text-warning">{t('config.note')}</p>
      </div>
    </div>
  )
}

export default Config
