import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient } from '../api/client';
import type {
  InstalledPlugin,
  MarketInstallReport,
  MarketPlugin,
  MarketResponse,
  PluginListResponse,
  PluginTrust,
  TrustedKey,
} from '../types/api';
import { useI18n } from '../i18n/I18nContext';

/** Colour classes per trust verdict (also the order they appear in the legend). */
const TRUST_STYLE: Record<PluginTrust, string> = {
  trusted: 'bg-success/10 text-success border-success/40',
  unsigned: 'bg-primary-muted text-primary border-primary/40',
  untrusted: 'bg-error/10 text-error border-error/40',
  tampered: 'bg-error text-white border-error',
};

const TRUST_KEY: Record<PluginTrust, string> = {
  trusted: 'plugins.trustTrusted',
  unsigned: 'plugins.trustUnsigned',
  untrusted: 'plugins.trustUntrusted',
  tampered: 'plugins.trustTampered',
};

type Tab = 'installed' | 'market';

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function shortKey(publicKey: string): string {
  return publicKey.length <= 16 ? publicKey : `${publicKey.slice(0, 8)}…${publicKey.slice(-6)}`;
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const Plugins: React.FC = () => {
  const { t } = useI18n();
  const [tab, setTab] = useState<Tab>('installed');
  const [data, setData] = useState<PluginListResponse | null>(null);
  const [keys, setKeys] = useState<TrustedKey[]>([]);
  const [installPath, setInstallPath] = useState('');
  const [keyName, setKeyName] = useState('');
  const [keyValue, setKeyValue] = useState('');
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [market, setMarket] = useState<MarketResponse | null>(null);
  const [marketQuery, setMarketQuery] = useState('');
  const [marketBusy, setMarketBusy] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [plugins, trusted] = await Promise.all([
        apiClient.listPlugins(),
        apiClient.listPluginKeys(),
      ]);
      setData(plugins);
      setKeys(trusted);
      setError(null);
    } catch (e: unknown) {
      setError(errorText(e));
    }
  }, []);

  /** The market fetches every configured source, so it loads separately from the plugin list. */
  const loadMarket = useCallback(async () => {
    try {
      setMarket(await apiClient.listMarket());
      setError(null);
    } catch (e: unknown) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    load();
    loadMarket();
  }, [load, loadMarket]);

  const run = useCallback(
    async (label: string, action: () => Promise<unknown>, success?: string) => {
      setBusy(label);
      setError(null);
      setNotice(null);
      try {
        await action();
        if (success) setNotice(success);
        await load();
        return true;
      } catch (e: unknown) {
        setError(errorText(e));
        return false;
      } finally {
        setBusy(null);
      }
    },
    [load]
  );

  /** Install or update from the market, then refresh both the list and the market state. */
  const runMarket = useCallback(
    async (label: string, action: () => Promise<MarketInstallReport>) => {
      setMarketBusy(label);
      setError(null);
      setNotice(null);
      try {
        const report = await action();
        if (report.installed.length > 0) {
          const ids = report.installed.map((item) => `${item.id}@${item.version}`).join(', ');
          setNotice(t('plugins.marketInstallOk', { count: report.installed.length, ids }));
        } else if (report.failed.length === 0) {
          setNotice(t('plugins.marketUpToDateAll'));
        }
        if (report.failed.length > 0) {
          const details = report.failed.map((item) => `${item.id}: ${item.error}`).join('; ');
          setError(`${t('plugins.marketInstallFailed')}: ${details}`);
        }
        await Promise.all([load(), loadMarket()]);
      } catch (e: unknown) {
        setError(errorText(e));
      } finally {
        setMarketBusy(null);
      }
    },
    [load, loadMarket, t]
  );

  const install = async () => {
    const path = installPath.trim();
    if (!path) return;
    // Keep the typed path on failure so the user can fix a typo instead of retyping it.
    const ok = await run(
      'install',
      () => apiClient.installPlugin(path),
      t('plugins.installed', '已安装')
    );
    if (ok) setInstallPath('');
  };

  const uninstall = (plugin: InstalledPlugin) => {
    if (!window.confirm(t('plugins.uninstallConfirm', { name: plugin.name }))) return;
    void run(`uninstall:${plugin.id}`, () => apiClient.uninstallPlugin(plugin.id));
  };

  const toggle = (plugin: InstalledPlugin) =>
    void run(`toggle:${plugin.id}`, () =>
      apiClient.setPluginEnabled(plugin.id, !plugin.enabled)
    );

  const installFromMarket = (plugin: MarketPlugin) =>
    void runMarket(`market:${plugin.id}`, () =>
      apiClient.installMarketPlugin(plugin.id, plugin.update_available ? plugin.version : undefined)
    );

  const updateAll = () =>
    void runMarket('market:update-all', () => apiClient.updateMarketPlugins());

  const addKey = async () => {
    if (!keyName.trim() || !keyValue.trim()) return;
    const ok = await run(
      'add-key',
      () => apiClient.addPluginKey(keyName.trim(), keyValue.trim()),
      t('plugins.keyAdded', '已信任该公钥')
    );
    if (ok) {
      setKeyName('');
      setKeyValue('');
    }
  };

  const marketPlugins = market?.plugins ?? [];
  const updateCount = marketPlugins.filter((plugin) => plugin.update_available).length;
  const visibleMarketPlugins = useMemo(() => {
    const query = marketQuery.trim().toLowerCase();
    if (!query) return marketPlugins;
    return marketPlugins.filter((plugin) =>
      [plugin.id, plugin.name, plugin.description, plugin.author, ...plugin.tags]
        .join(' ')
        .toLowerCase()
        .includes(query)
    );
  }, [marketPlugins, marketQuery]);

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-text">{t('plugins.title')}</h2>
        <p className="text-text-secondary mt-1">{t('plugins.description')}</p>
        {data && (
          <p className="text-[11px] text-text-secondary mt-1 font-mono break-all">
            {t('plugins.root', '插件目录')}: {data.root} · {t('plugins.trustStore', '信任库')}:{' '}
            {data.trust_store}
          </p>
        )}
      </div>

      {error && (
        <div className="rounded-lg border border-error/40 bg-error/10 px-4 py-2 text-sm text-error">
          {error}
        </div>
      )}
      {notice && (
        <div className="rounded-lg border border-success/40 bg-success/10 px-4 py-2 text-sm text-success">
          {notice}
        </div>
      )}

      <div className="flex gap-1 border-b border-border">
        {(['installed', 'market'] as Tab[]).map((value) => (
          <button
            key={value}
            type="button"
            onClick={() => setTab(value)}
            className={`px-3 py-1.5 text-sm -mb-px border-b-2 ${
              tab === value
                ? 'border-primary text-primary'
                : 'border-transparent text-text-secondary hover:text-text'
            }`}
          >
            {value === 'installed' ? t('plugins.tabInstalled') : t('plugins.tabMarket')}
            {value === 'market' && updateCount > 0 ? ` (${updateCount})` : ''}
          </button>
        ))}
      </div>

      {tab === 'installed' && (
        <>
          <div className="bg-surface rounded-lg shadow border border-border p-4">
            <h3 className="text-sm font-semibold text-text">
              {t('plugins.installTitle', '安装插件包')}
            </h3>
            <p className="text-xs text-text-secondary mt-1">{t('plugins.installHint')}</p>
            <div className="mt-2 flex flex-wrap gap-2">
              <input
                type="text"
                value={installPath}
                onChange={(e) => setInstallPath(e.target.value)}
                placeholder={t('plugins.installPlaceholder')}
                className="flex-1 min-w-[16rem] px-3 py-1.5 rounded border border-border bg-background text-text text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary"
              />
              <button
                type="button"
                onClick={install}
                disabled={busy !== null || !installPath.trim()}
                className="px-4 py-1.5 rounded bg-primary text-white text-sm hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed"
              >
                {busy === 'install' ? t('common.loading') : t('plugins.install', '安装')}
              </button>
              <button
                type="button"
                onClick={() => void load()}
                className="px-3 py-1.5 rounded border border-border text-sm text-text hover:bg-background"
              >
                {t('common.refresh', '刷新')}
              </button>
            </div>
          </div>

          <section className="space-y-3">
            <div className="flex items-baseline gap-2">
              <h3 className="text-lg font-semibold text-text">{t('plugins.installed')}</h3>
              <span className="text-xs text-text-secondary">
                {t('plugins.count', { count: data?.plugins.length ?? 0 })}
              </span>
            </div>
            {data && data.plugins.length === 0 ? (
              <p className="text-sm text-text-secondary">{t('plugins.empty')}</p>
            ) : (
              (data?.plugins ?? []).map((plugin) => (
                <div
                  key={plugin.id}
                  className={`bg-surface rounded-lg shadow border p-4 space-y-2 ${
                    plugin.loadable ? 'border-border' : 'border-error/50'
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-base font-semibold text-text">{plugin.name}</span>
                    <span className="font-mono text-[11px] text-text-secondary">
                      {plugin.id}@{plugin.version}
                    </span>
                    <span
                      className={`px-1.5 py-0.5 rounded border text-[11px] ${TRUST_STYLE[plugin.trust]}`}
                      title={t('plugins.sha256', 'sha256') + ': ' + plugin.sha256}
                    >
                      {t(TRUST_KEY[plugin.trust])}
                    </span>
                    {!plugin.loadable && (
                      <span className="text-[11px] text-error">{t('plugins.notLoaded')}</span>
                    )}
                    <span className="ml-auto flex items-center gap-2">
                      <label className="flex items-center gap-1 text-xs text-text-secondary">
                        <input
                          type="checkbox"
                          checked={plugin.enabled}
                          disabled={busy !== null}
                          onChange={() => toggle(plugin)}
                        />
                        {plugin.enabled ? t('plugins.enabled') : t('plugins.disabled')}
                      </label>
                      <button
                        type="button"
                        onClick={() => uninstall(plugin)}
                        disabled={busy !== null}
                        className="px-2 py-0.5 rounded border border-border text-xs text-error hover:bg-background disabled:opacity-50"
                      >
                        {busy === `uninstall:${plugin.id}`
                          ? t('common.loading')
                          : t('plugins.uninstall')}
                      </button>
                    </span>
                  </div>

                  {plugin.description && (
                    <p className="text-sm text-text-secondary">{plugin.description}</p>
                  )}

                  <div className="flex flex-wrap items-center gap-1 text-[11px]">
                    <span className="text-text-secondary">{t('plugins.permissions')}:</span>
                    {plugin.permission_summary === 'none' ? (
                      <span className="px-1.5 rounded bg-background text-text-secondary">
                        {t('plugins.permissionNone')}
                      </span>
                    ) : (
                      plugin.permission_summary.split(' ').map((chip) => (
                        <span
                          key={chip}
                          className="px-1.5 rounded bg-background border border-border font-mono text-text-secondary"
                        >
                          {chip}
                        </span>
                      ))
                    )}
                  </div>

                  <div className="flex flex-wrap items-center gap-3 text-[11px] text-text-secondary">
                    <span>
                      {t('plugins.size', '大小')}: {formatBytes(plugin.wasm_bytes)}
                    </span>
                    {plugin.installed_at && (
                      <span>{new Date(plugin.installed_at).toLocaleString()}</span>
                    )}
                    <span className="font-mono truncate max-w-[24rem]" title={plugin.path}>
                      {plugin.path}
                    </span>
                  </div>
                </div>
              ))
            )}
          </section>

          {data && data.legacy.length > 0 && (
            <section className="space-y-2">
              <h3 className="text-lg font-semibold text-text">{t('plugins.legacy')}</h3>
              <p className="text-xs text-text-secondary">{t('plugins.legacyHint')}</p>
              {data.legacy.map((plugin) => (
                <div
                  key={plugin.id}
                  className="flex items-center gap-2 bg-surface rounded-lg border border-border px-4 py-2"
                >
                  <span className="font-mono text-sm text-text">{plugin.id}</span>
                  {plugin.internal && (
                    <span className="px-1.5 rounded bg-background text-[11px] text-text-secondary">
                      internal
                    </span>
                  )}
                  <span className="ml-auto text-xs text-text-secondary">
                    {plugin.enabled ? t('plugins.enabled') : t('plugins.disabled')}
                  </span>
                  <label className="flex items-center gap-1 text-xs text-text-secondary">
                    <input
                      type="checkbox"
                      checked={plugin.enabled}
                      disabled={busy !== null}
                      onChange={() =>
                        void run(`toggle:${plugin.id}`, () =>
                          apiClient.setPluginEnabled(plugin.id, !plugin.enabled)
                        )
                      }
                    />
                    {t('plugins.enabled')}
                  </label>
                </div>
              ))}
            </section>
          )}

          <section className="bg-surface rounded-lg shadow border border-border p-4 space-y-3">
            <div>
              <h3 className="text-sm font-semibold text-text">{t('plugins.keys')}</h3>
              <p className="text-xs text-text-secondary mt-1">{t('plugins.keysHint')}</p>
            </div>
            {keys.length === 0 ? (
              <p className="text-xs text-text-secondary">{t('plugins.keysEmpty')}</p>
            ) : (
              <ul className="space-y-1">
                {keys.map((key) => (
                  <li key={key.name} className="flex items-center gap-2 text-xs">
                    <span className="text-text">{key.name}</span>
                    <span className="font-mono text-text-secondary" title={key.public_key}>
                      {shortKey(key.public_key)}
                    </span>
                    <button
                      type="button"
                      onClick={() =>
                        void run(`key:${key.name}`, () => apiClient.removePluginKey(key.name))
                      }
                      disabled={busy !== null}
                      className="ml-auto px-2 py-0.5 rounded border border-border text-error hover:bg-background disabled:opacity-50"
                    >
                      {t('plugins.removeKey')}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <div className="flex flex-wrap gap-2">
              <input
                type="text"
                value={keyName}
                onChange={(e) => setKeyName(e.target.value)}
                placeholder={t('plugins.keyName')}
                className="w-40 px-3 py-1.5 rounded border border-border bg-background text-text text-sm focus:outline-none focus:ring-2 focus:ring-primary"
              />
              <input
                type="text"
                value={keyValue}
                onChange={(e) => setKeyValue(e.target.value)}
                placeholder={t('plugins.keyPlaceholder')}
                className="flex-1 min-w-[16rem] px-3 py-1.5 rounded border border-border bg-background text-text text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary"
              />
              <button
                type="button"
                onClick={addKey}
                disabled={busy !== null || !keyName.trim() || !keyValue.trim()}
                className="px-4 py-1.5 rounded bg-primary text-white text-sm hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed"
              >
                {t('plugins.addKey')}
              </button>
            </div>
          </section>
        </>
      )}

      {tab === 'market' && (
        <section className="space-y-3">
          <div className="bg-surface rounded-lg shadow border border-border p-4 space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-semibold text-text">{t('plugins.tabMarket')}</h3>
              <span className="text-xs text-text-secondary">
                {t('plugins.count', { count: marketPlugins.length })}
              </span>
              <span className="ml-auto flex flex-wrap items-center gap-2">
                {updateCount > 0 && (
                  <button
                    type="button"
                    onClick={updateAll}
                    disabled={marketBusy !== null}
                    className="px-3 py-1.5 rounded bg-primary text-white text-sm hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed"
                  >
                    {marketBusy === 'market:update-all'
                      ? t('common.loading')
                      : t('plugins.marketUpdateAll', { count: updateCount })}
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void loadMarket()}
                  className="px-3 py-1.5 rounded border border-border text-sm text-text hover:bg-background"
                >
                  {t('common.refresh', '刷新')}
                </button>
              </span>
            </div>
            <p className="text-xs text-text-secondary">{t('plugins.marketHint')}</p>
            {market && market.sources.length > 0 ? (
              <p className="text-[11px] text-text-secondary font-mono break-all">
                {market.sources.join(' · ')}
              </p>
            ) : (
              <p className="text-xs text-warning">{t('plugins.marketNoSources')}</p>
            )}
            <input
              type="text"
              value={marketQuery}
              onChange={(e) => setMarketQuery(e.target.value)}
              placeholder={t('plugins.marketSearch')}
              className="w-full px-3 py-1.5 rounded border border-border bg-background text-text text-sm focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          {market && market.errors.length > 0 && (
            <div className="rounded-lg border border-error/40 bg-error/10 px-4 py-2 text-xs text-error space-y-1">
              <p className="font-semibold">{t('plugins.marketSourceErrors')}</p>
              {market.errors.map((sourceError) => (
                <p key={sourceError.source} className="font-mono break-all">
                  {sourceError.source}: {sourceError.error}
                </p>
              ))}
            </div>
          )}

          {visibleMarketPlugins.length === 0 ? (
            <p className="text-sm text-text-secondary">{t('plugins.marketEmpty')}</p>
          ) : (
            visibleMarketPlugins.map((plugin) => (
              <div
                key={plugin.id}
                className="bg-surface rounded-lg shadow border border-border p-4 space-y-2"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-base font-semibold text-text">{plugin.name}</span>
                  <span className="font-mono text-[11px] text-text-secondary">
                    {plugin.id}@{plugin.version}
                  </span>
                  {plugin.installed_version && !plugin.update_available && (
                    <span className="px-1.5 py-0.5 rounded border border-success/40 bg-success/10 text-[11px] text-success">
                      {t('plugins.marketInstalledAt', { version: plugin.installed_version })}
                    </span>
                  )}
                  {plugin.update_available && (
                    <span className="px-1.5 py-0.5 rounded border border-warning/40 bg-warning/10 text-[11px] text-warning">
                      {t('plugins.marketUpdate', { version: plugin.version })}
                    </span>
                  )}
                  <span className="ml-auto">
                    <button
                      type="button"
                      onClick={() => installFromMarket(plugin)}
                      disabled={
                        marketBusy !== null ||
                        (Boolean(plugin.installed_version) && !plugin.update_available)
                      }
                      className="px-3 py-1 rounded bg-primary text-white text-xs hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed"
                    >
                      {marketBusy === `market:${plugin.id}`
                        ? t('plugins.marketInstalling')
                        : plugin.update_available
                          ? t('plugins.marketUpdate', { version: plugin.version })
                          : plugin.installed_version
                            ? t('plugins.marketUpToDate')
                            : t('plugins.marketInstall')}
                    </button>
                  </span>
                </div>

                {plugin.description && (
                  <p className="text-sm text-text-secondary">{plugin.description}</p>
                )}

                <div className="flex flex-wrap items-center gap-1 text-[11px]">
                  {plugin.author && (
                    <span className="text-text-secondary">{plugin.author}</span>
                  )}
                  {plugin.dependencies.length > 0 && (
                    <>
                      <span className="text-text-secondary">{t('plugins.marketDeps')}:</span>
                      {plugin.dependencies.map((dependency) => (
                        <span
                          key={dependency.id}
                          className="px-1.5 rounded bg-background border border-border font-mono text-text-secondary"
                        >
                          {dependency.id}
                          {dependency.version ? ` ${dependency.version}` : ''}
                        </span>
                      ))}
                    </>
                  )}
                </div>
              </div>
            ))
          )}

          <p className="text-[11px] text-text-secondary">{t('plugins.marketRestartHint')}</p>
        </section>
      )}
    </div>
  );
};

export default Plugins;
