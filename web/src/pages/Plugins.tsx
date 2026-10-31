import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { apiClient } from '../api/client'
import type {
  InstalledPlugin,
  MarketInstallReport,
  MarketPlugin,
  MarketResponse,
  PluginListResponse,
  PluginTrust,
  TrustedKey,
} from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatDateTime } from '../utils/datetime'
import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  Field,
  Input,
  PageHeader,
  type BadgeTone,
} from '../ui'

/** Badge tone per trust verdict. A tampered package is the only one that earns a filled chip. */
const TRUST_TONE: Record<PluginTrust, BadgeTone> = {
  trusted: 'success',
  unsigned: 'accent',
  untrusted: 'danger',
  tampered: 'danger',
}

const TRUST_KEY: Record<PluginTrust, string> = {
  trusted: 'plugins.trustTrusted',
  unsigned: 'plugins.trustUnsigned',
  untrusted: 'plugins.trustUntrusted',
  tampered: 'plugins.trustTampered',
}

type Tab = 'installed' | 'market'

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function shortKey(publicKey: string): string {
  return publicKey.length <= 16 ? publicKey : `${publicKey.slice(0, 8)}…${publicKey.slice(-6)}`
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

function Chips({ values }: { values: string[] }) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {values.map((value) => (
        <span
          key={value}
          className="rounded-control bg-surface-2 px-1.5 py-0.5 font-mono text-micro text-ink-2"
        >
          {value}
        </span>
      ))}
    </div>
  )
}

export default function Plugins() {
  const { t } = useI18n()
  const [tab, setTab] = useState<Tab>('installed')
  const [data, setData] = useState<PluginListResponse | null>(null)
  const [keys, setKeys] = useState<TrustedKey[]>([])
  const [installPath, setInstallPath] = useState('')
  const [keyName, setKeyName] = useState('')
  const [keyValue, setKeyValue] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [market, setMarket] = useState<MarketResponse | null>(null)
  const [marketQuery, setMarketQuery] = useState('')
  const [marketBusy, setMarketBusy] = useState<string | null>(null)
  const [pendingUninstall, setPendingUninstall] = useState<InstalledPlugin | null>(null)

  const load = useCallback(async () => {
    try {
      const [plugins, trusted] = await Promise.all([apiClient.listPlugins(), apiClient.listPluginKeys()])
      setData(plugins)
      setKeys(trusted)
      setError(null)
    } catch (e: unknown) {
      setError(errorText(e))
    }
  }, [])

  /** The market fetches every configured source, so it loads separately from the plugin list. */
  const loadMarket = useCallback(async () => {
    try {
      setMarket(await apiClient.listMarket())
      setError(null)
    } catch (e: unknown) {
      setError(errorText(e))
    }
  }, [])

  useEffect(() => {
    void load()
    void loadMarket()
  }, [load, loadMarket])

  const run = useCallback(
    async (label: string, action: () => Promise<unknown>, success?: string) => {
      setBusy(label)
      setError(null)
      setNotice(null)
      try {
        await action()
        if (success) setNotice(success)
        await load()
        return true
      } catch (e: unknown) {
        setError(errorText(e))
        return false
      } finally {
        setBusy(null)
      }
    },
    [load]
  )

  /** Install or update from the market, then refresh both the list and the market state. */
  const runMarket = useCallback(
    async (label: string, action: () => Promise<MarketInstallReport>) => {
      setMarketBusy(label)
      setError(null)
      setNotice(null)
      try {
        const report = await action()
        if (report.installed.length > 0) {
          const ids = report.installed.map((item) => `${item.id}@${item.version}`).join(', ')
          setNotice(t('plugins.marketInstallOk', { count: report.installed.length, ids }))
        } else if (report.failed.length === 0) {
          setNotice(t('plugins.marketUpToDateAll'))
        }
        if (report.failed.length > 0) {
          const details = report.failed.map((item) => `${item.id}: ${item.error}`).join('; ')
          setError(`${t('plugins.marketInstallFailed')}: ${details}`)
        }
        await Promise.all([load(), loadMarket()])
      } catch (e: unknown) {
        setError(errorText(e))
      } finally {
        setMarketBusy(null)
      }
    },
    [load, loadMarket, t]
  )

  const install = async () => {
    const path = installPath.trim()
    if (!path) return
    // Keep the typed path on failure so the user can fix a typo instead of retyping it.
    const ok = await run('install', () => apiClient.installPlugin(path), t('plugins.installed', 'Installed'))
    if (ok) setInstallPath('')
  }

  const uninstall = async (plugin: InstalledPlugin) => {
    setPendingUninstall(null)
    await run(`uninstall:${plugin.id}`, () => apiClient.uninstallPlugin(plugin.id))
  }

  const toggle = (plugin: InstalledPlugin) =>
    void run(`toggle:${plugin.id}`, () => apiClient.setPluginEnabled(plugin.id, !plugin.enabled))

  const installFromMarket = (plugin: MarketPlugin) =>
    void runMarket(`market:${plugin.id}`, () =>
      apiClient.installMarketPlugin(plugin.id, plugin.update_available ? plugin.version : undefined)
    )

  const updateAll = () => void runMarket('market:update-all', () => apiClient.updateMarketPlugins())

  const addKey = async () => {
    if (!keyName.trim() || !keyValue.trim()) return
    const ok = await run(
      'add-key',
      () => apiClient.addPluginKey(keyName.trim(), keyValue.trim()),
      t('plugins.keyAdded', 'Key trusted')
    )
    if (ok) {
      setKeyName('')
      setKeyValue('')
    }
  }

  const marketPlugins = market?.plugins ?? []
  const updateCount = marketPlugins.filter((plugin) => plugin.update_available).length
  const visibleMarketPlugins = useMemo(() => {
    const query = marketQuery.trim().toLowerCase()
    if (!query) return marketPlugins
    return marketPlugins.filter((plugin) =>
      [plugin.id, plugin.name, plugin.description, plugin.author, ...plugin.tags]
        .join(' ')
        .toLowerCase()
        .includes(query)
    )
  }, [marketPlugins, marketQuery])

  const installed = data?.plugins ?? []

  return (
    <div className="space-y-4">
      <PageHeader
        title={t('plugins.title', 'Plugins')}
        count={installed.length}
        description={t('plugins.description', 'WASM packages the agent can call as tools')}
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon={<RefreshCw className="h-3.5 w-3.5" />}
            onClick={() => {
              void load()
              void loadMarket()
            }}
          >
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {data && (
        <p className="-mt-1 break-all font-mono text-micro text-ink-3">
          {t('plugins.root', 'Plugins')}: {data.root} · {t('plugins.trustStore', 'Trust store')}:{' '}
          {data.trust_store}
        </p>
      )}

      {error && <div className="rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}
      {notice && <div className="rounded-panel border px-3 py-2 text-small tone-success">{notice}</div>}

      <div role="tablist" className="flex gap-1 border-b border-line">
        {(['installed', 'market'] as Tab[]).map((value) => (
          <button
            key={value}
            type="button"
            role="tab"
            onClick={() => setTab(value)}
            aria-selected={tab === value}
            className={
              tab === value
                ? '-mb-px border-b-2 border-accent px-2 py-1.5 text-small font-medium text-ink'
                : '-mb-px border-b-2 border-transparent px-2 py-1.5 text-small text-ink-2 transition-colors duration-fast hover:text-ink'
            }
          >
            {value === 'installed' ? t('plugins.tabInstalled', 'Installed') : t('plugins.tabMarket', 'Market')}
            {value === 'market' && updateCount > 0 ? ` (${updateCount})` : ''}
          </button>
        ))}
      </div>

      {tab === 'installed' && (
        <div className="space-y-4">
          <div className="rounded-panel border border-line p-3">
            <h2 className="text-section font-semibold text-ink">{t('plugins.installTitle', 'Install a package')}</h2>
            <p className="mb-2 mt-0.5 text-small text-ink-2">{t('plugins.installHint', 'Point at a directory that contains plugin.json.')}</p>
            <div className="flex flex-wrap items-end gap-2">
              <Field label={t('plugins.installPlaceholder', '/path/to/plugin-package')} htmlFor="plugin-path" className="min-w-[16rem] flex-1">
                <Input
                  id="plugin-path"
                  value={installPath}
                  onChange={(event) => setInstallPath(event.target.value)}
                  placeholder={t('plugins.installPlaceholder', '/path/to/plugin-package')}
                  className="font-mono"
                />
              </Field>
              <Button
                variant="primary"
                size="sm"
                onClick={() => void install()}
                disabled={busy !== null || !installPath.trim()}
                loading={busy === 'install'}
              >
                {t('plugins.install', 'Install')}
              </Button>
            </div>
          </div>

          <section>
            <div className="mb-2 flex items-baseline gap-2">
              <h2 className="text-section font-semibold text-ink">{t('plugins.installed', 'Installed')}</h2>
              <span className="text-micro text-ink-3">{t('plugins.count', { count: installed.length })}</span>
            </div>

            {installed.length === 0 ? (
              <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
                {t('plugins.empty', 'No plugin packages installed yet')}
              </p>
            ) : (
              <ul className="overflow-hidden rounded-panel border border-line">
                {installed.map((plugin) => (
                  <li
                    key={plugin.id}
                    className={
                      plugin.loadable
                        ? 'space-y-1.5 border-b border-line px-3 py-2.5 last:border-b-0'
                        : 'space-y-1.5 border-b border-l-2 border-line border-l-error px-3 py-2.5 last:border-b-0'
                    }
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-body font-medium text-ink">{plugin.name}</span>
                      <span className="font-mono text-micro text-ink-3">
                        {plugin.id}@{plugin.version}
                      </span>
                      <Badge tone={TRUST_TONE[plugin.trust]}>{t(TRUST_KEY[plugin.trust])}</Badge>
                      {!plugin.loadable && (
                        <span className="text-micro text-error">{t('plugins.notLoaded', 'Integrity check failed')}</span>
                      )}
                      <span className="ml-auto flex items-center gap-2">
                        <Checkbox
                          checked={plugin.enabled}
                          disabled={busy !== null}
                          onChange={() => toggle(plugin)}
                          label={
                            <span className="text-micro">
                              {plugin.enabled ? t('plugins.enabled', 'Enabled') : t('plugins.disabled', 'Disabled')}
                            </span>
                          }
                        />
                        <Button
                          variant="danger"
                          size="sm"
                          disabled={busy !== null}
                          loading={busy === `uninstall:${plugin.id}`}
                          onClick={() => setPendingUninstall(plugin)}
                        >
                          {t('plugins.uninstall', 'Uninstall')}
                        </Button>
                      </span>
                    </div>

                    {plugin.description && <p className="text-small text-ink-2">{plugin.description}</p>}

                    <div className="flex flex-wrap items-center gap-1">
                      <span className="text-micro text-ink-3">{t('plugins.permissions', 'Permissions')}:</span>
                      {plugin.permission_summary === 'none' ? (
                        <span className="text-micro text-ink-3">{t('plugins.permissionNone', 'none')}</span>
                      ) : (
                        <Chips values={plugin.permission_summary.split(' ')} />
                      )}
                    </div>

                    <div className="flex flex-wrap items-center gap-3 text-micro text-ink-3">
                      <span className="tabular">
                        {t('plugins.size', 'Size')}: {formatBytes(plugin.wasm_bytes)}
                      </span>
                      {plugin.installed_at && (
                        <span className="tabular">{formatDateTime(plugin.installed_at)}</span>
                      )}
                      <span className="max-w-[24rem] truncate font-mono" title={plugin.path}>
                        {plugin.path}
                      </span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {data && data.legacy.length > 0 && (
            <section>
              <h2 className="text-section font-semibold text-ink">{t('plugins.legacy', 'Config-only plugins')}</h2>
              <p className="mb-2 mt-0.5 text-small text-ink-2">{t('plugins.legacyHint', 'Declared in the configuration file; no package on disk.')}</p>
              <ul className="overflow-hidden rounded-panel border border-line">
                {data.legacy.map((plugin) => (
                  <li
                    key={plugin.id}
                    className="flex items-center gap-2 border-b border-line px-3 py-1.5 last:border-b-0"
                  >
                    <span className="font-mono text-small text-ink">{plugin.id}</span>
                    {plugin.internal && <Badge tone="neutral">internal</Badge>}
                    <span className="ml-auto text-micro text-ink-3">
                      {plugin.enabled ? t('plugins.enabled', 'Enabled') : t('plugins.disabled', 'Disabled')}
                    </span>
                    <Checkbox
                      checked={plugin.enabled}
                      disabled={busy !== null}
                      onChange={() =>
                        void run(`toggle:${plugin.id}`, () =>
                          apiClient.setPluginEnabled(plugin.id, !plugin.enabled)
                        )
                      }
                      label={<span className="sr-only">{t('common.toggle', 'Toggle')}</span>}
                    />
                  </li>
                ))}
              </ul>
            </section>
          )}

          <section className="rounded-panel border border-line p-3">
            <h2 className="text-section font-semibold text-ink">{t('plugins.keys', 'Trusted keys')}</h2>
            <p className="mb-2 mt-0.5 text-small text-ink-2">{t('plugins.keysHint', 'A package signed by a trusted key loads without a warning.')}</p>
            {keys.length === 0 ? (
              <p className="text-small text-ink-3">{t('plugins.keysEmpty', 'No publisher key trusted yet.')}</p>
            ) : (
              <ul className="mb-2 space-y-1">
                {keys.map((key) => (
                  <li key={key.name} className="flex items-center gap-2 text-small">
                    <span className="text-ink">{key.name}</span>
                    <span className="truncate font-mono text-micro text-ink-3" title={key.public_key}>
                      {shortKey(key.public_key)}
                    </span>
                    <Button
                      variant="danger"
                      size="sm"
                      className="ml-auto"
                      disabled={busy !== null}
                      onClick={() => void run(`key:${key.name}`, () => apiClient.removePluginKey(key.name))}
                    >
                      {t('plugins.removeKey', 'Remove key')}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            <div className="flex flex-wrap items-end gap-2">
              <Field label={t('plugins.keyName', 'Name')} htmlFor="plugin-key-name" className="w-40">
                <Input
                  id="plugin-key-name"
                  value={keyName}
                  onChange={(event) => setKeyName(event.target.value)}
                  placeholder={t('plugins.keyName', 'Name')}
                />
              </Field>
              <Field
                label={t('plugins.keyValue', 'Public key')}
                htmlFor="plugin-key-value"
                className="min-w-[16rem] flex-1"
              >
                <Input
                  id="plugin-key-value"
                  value={keyValue}
                  onChange={(event) => setKeyValue(event.target.value)}
                  placeholder={t('plugins.keyPlaceholder', 'base64 public key')}
                  className="font-mono"
                />
              </Field>
              <Button
                variant="primary"
                size="sm"
                onClick={() => void addKey()}
                disabled={busy !== null || !keyName.trim() || !keyValue.trim()}
                loading={busy === 'add-key'}
              >
                {t('plugins.addKey', 'Add key')}
              </Button>
            </div>
          </section>
        </div>
      )}

      {tab === 'market' && (
        <div className="space-y-3">
          <div className="rounded-panel border border-line p-3">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-section font-semibold text-ink">{t('plugins.tabMarket', 'Market')}</h2>
              <span className="text-micro text-ink-3">{t('plugins.count', { count: marketPlugins.length })}</span>
              <div className="ml-auto flex flex-wrap items-center gap-2">
                {updateCount > 0 && (
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={updateAll}
                    disabled={marketBusy !== null}
                    loading={marketBusy === 'market:update-all'}
                  >
                    {t('plugins.marketUpdateAll', { count: updateCount })}
                  </Button>
                )}
                <Button variant="ghost" size="sm" onClick={() => void loadMarket()}>
                  {t('common.refresh', 'Refresh')}
                </Button>
              </div>
            </div>
            <p className="mb-1 mt-0.5 text-small text-ink-2">{t('plugins.marketHint', 'Indexes are fetched from the configured sources.')}</p>
            {market && market.sources.length > 0 ? (
              <p className="break-all font-mono text-micro text-ink-3">{market.sources.join(' · ')}</p>
            ) : (
              <p className="text-small text-warning">{t('plugins.marketNoSources', 'No market source configured')}</p>
            )}
            <div className="mt-2">
              <Field label={t('plugins.marketSearch', 'Search')} htmlFor="market-search">
                <Input
                  id="market-search"
                  value={marketQuery}
                  onChange={(event) => setMarketQuery(event.target.value)}
                  placeholder={t('plugins.marketSearch', 'Search name, id or tag')}
                />
              </Field>
            </div>
          </div>

          {market && market.errors.length > 0 && (
            <div className="space-y-0.5 rounded-panel border px-3 py-2 tone-danger">
              <p className="text-small font-medium">{t('plugins.marketSourceErrors', 'Market source failed')}</p>
              {market.errors.map((sourceError) => (
                <p key={sourceError.source} className="break-all font-mono text-micro">
                  {sourceError.source}: {sourceError.error}
                </p>
              ))}
            </div>
          )}

          {visibleMarketPlugins.length === 0 ? (
            <p className="rounded-panel border border-line px-4 py-10 text-center text-small text-ink-2">
              {t('plugins.marketEmpty', 'Nothing to install from the configured sources.')}
            </p>
          ) : (
            <ul className="overflow-hidden rounded-panel border border-line">
              {visibleMarketPlugins.map((plugin) => {
                const installedHere = Boolean(plugin.installed_version) && !plugin.update_available
                return (
                  <li key={plugin.id} className="space-y-1.5 border-b border-line px-3 py-2.5 last:border-b-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-body font-medium text-ink">{plugin.name}</span>
                      <span className="font-mono text-micro text-ink-3">
                        {plugin.id}@{plugin.version}
                      </span>
                      {installedHere && (
                        <Badge tone="success">
                          {t('plugins.marketInstalledAt', { version: plugin.installed_version ?? '' })}
                        </Badge>
                      )}
                      {plugin.update_available && (
                        <Badge tone="warning">{t('plugins.marketUpdate', { version: plugin.version })}</Badge>
                      )}
                      <span className="ml-auto">
                        <Button
                          variant={installedHere ? 'ghost' : 'primary'}
                          size="sm"
                          onClick={() => installFromMarket(plugin)}
                          disabled={marketBusy !== null || installedHere}
                          loading={marketBusy === `market:${plugin.id}`}
                        >
                          {plugin.update_available
                            ? t('plugins.marketUpdate', { version: plugin.version })
                            : plugin.installed_version
                              ? t('plugins.marketUpToDate', 'Up to date')
                              : t('plugins.marketInstall', 'Install')}
                        </Button>
                      </span>
                    </div>

                    {plugin.description && <p className="text-small text-ink-2">{plugin.description}</p>}

                    <div className="flex flex-wrap items-center gap-1 text-micro text-ink-3">
                      {plugin.author && <span>{plugin.author}</span>}
                      {plugin.dependencies.length > 0 && (
                        <>
                          <span>{t('plugins.marketDeps', 'Depends on')}:</span>
                          <Chips
                            values={plugin.dependencies.map(
                              (dependency) => `${dependency.id}${dependency.version ? ` ${dependency.version}` : ''}`
                            )}
                          />
                        </>
                      )}
                    </div>
                  </li>
                )
              })}
            </ul>
          )}

          <p className="text-micro text-ink-3">{t('plugins.marketRestartHint', 'Restart the daemon to load new packages.')}</p>
        </div>
      )}

      {pendingUninstall && (
        <Dialog
          open
          onClose={() => setPendingUninstall(null)}
          title={t('plugins.uninstall', 'Uninstall')}
          width="sm"
          closeLabel={t('common.close', 'Close')}
          footer={
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setPendingUninstall(null)}>
                {t('common.cancel', 'Cancel')}
              </Button>
              <Button variant="danger" size="sm" onClick={() => void uninstall(pendingUninstall)}>
                {t('common.confirm', 'Confirm')}
              </Button>
            </div>
          }
        >
          <p className="text-small text-ink-2">
            {t('plugins.uninstallConfirm', { name: pendingUninstall.name })}
          </p>
        </Dialog>
      )}
    </div>
  )
}
