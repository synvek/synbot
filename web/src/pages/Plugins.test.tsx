import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Plugins from './Plugins';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api/client';
import type {
  InstalledPlugin,
  MarketPlugin,
  MarketResponse,
  PluginListResponse,
} from '../types/api';

vi.mock('../api/client', () => ({
  apiClient: {
    listPlugins: vi.fn(),
    installPlugin: vi.fn(),
    uninstallPlugin: vi.fn(),
    setPluginEnabled: vi.fn(),
    listPluginKeys: vi.fn(),
    addPluginKey: vi.fn(),
    removePluginKey: vi.fn(),
    listMarket: vi.fn(),
    installMarketPlugin: vi.fn(),
    updateMarketPlugins: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const plugin = (over: Partial<InstalledPlugin> = {}): InstalledPlugin => ({
  id: 'demo',
  name: 'Demo plugin',
  version: '1.2.3',
  description: 'does demo things',
  author: 'acme',
  homepage: null,
  permissions: { filesystem: { read: true, write: true, paths: [] }, network: { hosts: [] }, config: true },
  permission_summary: 'fs:rw',
  entry: 'demo.wasm',
  trust: 'unsigned',
  sha256: 'a'.repeat(64),
  wasm_bytes: 2048,
  path: '/tmp/plugins/demo',
  installed_at: '2026-01-01T00:00:00Z',
  enabled: true,
  loadable: true,
  ...over,
});

const listing = (over: Partial<PluginListResponse> = {}): PluginListResponse => ({
  plugins: [plugin()],
  legacy: [],
  trust_keys: 0,
  trust_store: '/home/u/.synbot/plugin_keys.json',
  root: '/home/u/.synbot/plugins',
  ...over,
});

const marketPlugin = (over: Partial<MarketPlugin> = {}): MarketPlugin => ({
  id: 'demo',
  name: 'Demo plugin',
  version: '1.2.3',
  description: 'does demo things',
  author: 'acme',
  homepage: null,
  tags: [],
  manifest_url: '/tmp/demo/plugin.json',
  entry_url: null,
  sha256: 'a'.repeat(64),
  files: [],
  dependencies: [],
  source: '/tmp/index.json',
  installed_version: null,
  update_available: false,
  ...over,
});

const marketListing = (over: Partial<MarketResponse> = {}): MarketResponse => ({
  sources: ['/tmp/index.json'],
  plugins: [marketPlugin()],
  errors: [],
  ...over,
});

// The tab is suffixed with the pending update count, e.g. `Market (1)`.
const MARKET_TAB = /^(Market|市场)( \(\d+\))?$/;

const renderPage = () =>
  render(
    <I18nProvider>
      <Plugins />
    </I18nProvider>
  );

describe('Plugins', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.listPlugins.mockResolvedValue(listing());
    mocked.listPluginKeys.mockResolvedValue([]);
    mocked.uninstallPlugin.mockResolvedValue();
    mocked.setPluginEnabled.mockResolvedValue();
    mocked.installPlugin.mockResolvedValue(plugin());
    mocked.addPluginKey.mockResolvedValue({ name: 'publisher', public_key: 'KEY' });
    mocked.removePluginKey.mockResolvedValue();
    mocked.listMarket.mockResolvedValue(marketListing());
    mocked.installMarketPlugin.mockResolvedValue({
      installed: [{ id: 'demo', version: '1.2.3', trust: 'unsigned', dependencies: [] }],
      failed: [],
    });
    mocked.updateMarketPlugins.mockResolvedValue({ installed: [], failed: [] });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders a package with its trust verdict, permissions and size', async () => {
    mocked.listPlugins.mockResolvedValue(
      listing({
        plugins: [
          plugin({
            trust: 'trusted',
            permission_summary: 'fs:rw net:api.example.com',
            wasm_bytes: 3 * 1024 * 1024,
          }),
        ],
      })
    );
    renderPage();

    expect(await screen.findByText('Demo plugin')).toBeInTheDocument();
    expect(screen.getByText('demo@1.2.3')).toBeInTheDocument();
    // Locale files load lazily, so the translated trust badge can land a tick later.
    expect(await screen.findByText(/Signed|已签名/)).toBeInTheDocument();
    expect(screen.getByText('fs:rw')).toBeInTheDocument();
    expect(screen.getByText('net:api.example.com')).toBeInTheDocument();
    expect(screen.getByText(/3\.0 MB/)).toBeInTheDocument();
    expect(screen.getByText('does demo things')).toBeInTheDocument();
  });

  it('flags packages that failed the integrity check', async () => {
    mocked.listPlugins.mockResolvedValue(
      listing({ plugins: [plugin({ trust: 'tampered', loadable: false })] })
    );
    renderPage();

    expect(await screen.findByText(/Tampered|已被篡改/)).toBeInTheDocument();
    expect(screen.getByText(/Integrity check failed|完整性校验失败/)).toBeInTheDocument();
  });

  it('shows the empty hint when nothing is installed', async () => {
    mocked.listPlugins.mockResolvedValue(listing({ plugins: [] }));
    renderPage();

    expect(
      await screen.findByText(/No plugin packages installed yet|还没有安装任何插件包/)
    ).toBeInTheDocument();
  });

  it('only uninstalls after the confirmation is accepted', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: /^Uninstall$|^卸载$/ }));
    expect(confirm).toHaveBeenCalled();
    expect(mocked.uninstallPlugin).not.toHaveBeenCalled();

    confirm.mockReturnValue(true);
    fireEvent.click(screen.getByRole('button', { name: /^Uninstall$|^卸载$/ }));

    await waitFor(() => expect(mocked.uninstallPlugin).toHaveBeenCalledWith('demo'));
    await waitFor(() => expect(mocked.listPlugins).toHaveBeenCalledTimes(2));
  });

  it('toggles loading without uninstalling', async () => {
    renderPage();

    fireEvent.click(await screen.findByRole('checkbox'));

    await waitFor(() => expect(mocked.setPluginEnabled).toHaveBeenCalledWith('demo', false));
    await waitFor(() => expect(mocked.listPlugins).toHaveBeenCalledTimes(2));
  });

  it('installs the package directory that was typed in', async () => {
    renderPage();

    const input = await screen.findByPlaceholderText('/path/to/plugin-package');
    fireEvent.change(input, { target: { value: '  /tmp/pkg  ' } });
    fireEvent.click(screen.getByRole('button', { name: /^Install$|^安装$/ }));

    await waitFor(() => expect(mocked.installPlugin).toHaveBeenCalledWith('/tmp/pkg'));
    await waitFor(() => expect(input).toHaveValue(''));
  });

  it('trusts a publisher key and clears the form', async () => {
    renderPage();

    fireEvent.change(await screen.findByPlaceholderText(/^(Name|名称)$/), {
      target: { value: 'publisher' },
    });
    fireEvent.change(screen.getByPlaceholderText(/base64/), {
      target: { value: ' KEY ' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Add key$|^添加公钥$/ }));

    await waitFor(() => expect(mocked.addPluginKey).toHaveBeenCalledWith('publisher', 'KEY'));
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/base64/)).toHaveValue('')
    );
  });

  it('lists config-only plugins separately from packages', async () => {
    mocked.listPlugins.mockResolvedValue(
      listing({ legacy: [{ id: 'legacy-tool', enabled: false, internal: false }] })
    );
    renderPage();

    expect(await screen.findByText(/Config-only plugins|直接配置的插件/)).toBeInTheDocument();
    expect(screen.getByText('legacy-tool')).toBeInTheDocument();
  });

  it('surfaces load errors', async () => {
    mocked.listPlugins.mockRejectedValue(new Error('boom'));
    renderPage();

    expect(await screen.findByText('boom')).toBeInTheDocument();
  });

  it('installs a plugin from the market tab', async () => {
    mocked.listMarket.mockResolvedValue(
      marketListing({
        plugins: [marketPlugin({ dependencies: [{ id: 'base', version: '>=1.0.0' }] })],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));
    expect(await screen.findByText('demo@1.2.3')).toBeInTheDocument();
    expect(screen.getByText('base >=1.0.0')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^Install$|^安装$/ }));

    await waitFor(() => expect(mocked.installMarketPlugin).toHaveBeenCalledWith('demo', undefined));
    expect(
      await screen.findByText(/Installed 1 plugin\(s\)|已安装 1 个插件/)
    ).toBeInTheDocument();
  });

  it('pins the version when the market offers an update', async () => {
    mocked.listMarket.mockResolvedValue(
      marketListing({
        plugins: [marketPlugin({ installed_version: '1.0.0', update_available: true })],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));
    const updateButtons = await screen.findAllByRole('button', { name: /Update to 1\.2\.3|更新到 1\.2\.3/ });
    fireEvent.click(updateButtons[0]);

    await waitFor(() => expect(mocked.installMarketPlugin).toHaveBeenCalledWith('demo', '1.2.3'));
  });

  it('updates everything from the market header', async () => {
    mocked.listMarket.mockResolvedValue(
      marketListing({
        plugins: [marketPlugin({ installed_version: '1.0.0', update_available: true })],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));
    fireEvent.click(
      await screen.findByRole('button', { name: /Update all|全部更新/ })
    );

    await waitFor(() => expect(mocked.updateMarketPlugins).toHaveBeenCalled());
    expect(
      await screen.findByText(/Everything is up to date|所有插件都是最新版本/)
    ).toBeInTheDocument();
  });

  it('surfaces broken market sources next to the working entries', async () => {
    mocked.listMarket.mockResolvedValue(
      marketListing({
        plugins: [],
        errors: [{ source: 'https://example.com/index.json', error: 'HTTP 404' }],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));

    expect(await screen.findByText(/Market source failed|市场源读取失败/)).toBeInTheDocument();
    expect(
      screen.getByText(/https:\/\/example\.com\/index\.json: HTTP 404/)
    ).toBeInTheDocument();
  });

  it('asks for a market source when none is configured', async () => {
    mocked.listMarket.mockResolvedValue({ sources: [], plugins: [], errors: [] });
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));

    expect(
      await screen.findByText(/No market source configured|还没有配置市场源/)
    ).toBeInTheDocument();
  });

  it('filters market entries by the search box', async () => {
    mocked.listMarket.mockResolvedValue(
      marketListing({
        plugins: [
          marketPlugin(),
          marketPlugin({ id: 'other', name: 'Other tool', version: '0.1.0' }),
        ],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: MARKET_TAB }));
    fireEvent.change(
      await screen.findByPlaceholderText(/Search name, id or tag|搜索名称/),
      { target: { value: 'other' } }
    );

    expect(screen.queryByText('demo@1.2.3')).not.toBeInTheDocument();
    expect(screen.getByText('other@0.1.0')).toBeInTheDocument();
  });
});
