import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Plugins from './Plugins';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api/client';
import type { InstalledPlugin, PluginListResponse } from '../types/api';

vi.mock('../api/client', () => ({
  apiClient: {
    listPlugins: vi.fn(),
    installPlugin: vi.fn(),
    uninstallPlugin: vi.fn(),
    setPluginEnabled: vi.fn(),
    listPluginKeys: vi.fn(),
    addPluginKey: vi.fn(),
    removePluginKey: vi.fn(),
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
    expect(screen.getByText(/Signed|已签名/)).toBeInTheDocument();
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
});
