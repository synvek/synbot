import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  OPEN_PROJECT_EVENT,
  bootstrapDesktop,
  isDesktop,
  notifyDesktop,
  onOpenProject,
  pickFolder,
} from './bridge';

type Internals = { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> };

function installTauri(invoke: Internals['invoke']) {
  (window as unknown as { __TAURI_INTERNALS__?: Internals }).__TAURI_INTERNALS__ = { invoke };
}

function removeTauri() {
  delete (window as unknown as { __TAURI_INTERNALS__?: Internals }).__TAURI_INTERNALS__;
}

afterEach(removeTauri);

describe('desktop bridge', () => {
  it('is inert in a browser', async () => {
    expect(isDesktop()).toBe(false);
    expect(await bootstrapDesktop()).toBeNull();
    expect(await pickFolder()).toBeNull();
    // Notifications must not throw when there is no shell to talk to.
    await expect(notifyDesktop('title', 'body')).resolves.toBeUndefined();
  });

  it('bootstraps the daemon address and token over IPC', async () => {
    const invoke = vi.fn().mockResolvedValue({
      apiBase: 'http://127.0.0.1:18888',
      wsBase: 'ws://127.0.0.1:18888',
      token: 'abc',
      version: '0.12.0',
      platform: 'macos',
    });
    installTauri(invoke);

    expect(isDesktop()).toBe(true);
    await expect(bootstrapDesktop()).resolves.toMatchObject({ apiBase: 'http://127.0.0.1:18888' });
    expect(invoke).toHaveBeenCalledWith('desktop_bootstrap', undefined);
  });

  it('normalises the folder picker result', async () => {
    installTauri(vi.fn().mockResolvedValueOnce('/tmp/project').mockResolvedValueOnce(null));

    await expect(pickFolder()).resolves.toBe('/tmp/project');
    await expect(pickFolder()).resolves.toBeNull();
  });

  it('swallows notification failures', async () => {
    const invoke = vi.fn().mockRejectedValue(new Error('permission denied'));
    installTauri(invoke);

    await expect(notifyDesktop('title', 'body')).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith('desktop_notify', { title: 'title', body: 'body' });
  });

  it('forwards tray "open project" events and unsubscribes', () => {
    const handler = vi.fn();
    const unsubscribe = onOpenProject(handler);

    window.dispatchEvent(new CustomEvent(OPEN_PROJECT_EVENT, { detail: '/tmp/from-tray' }));
    expect(handler).toHaveBeenCalledWith('/tmp/from-tray');

    // Empty payloads are ignored: the tray only sends paths it actually picked.
    window.dispatchEvent(new CustomEvent(OPEN_PROJECT_EVENT, { detail: '   ' }));
    expect(handler).toHaveBeenCalledTimes(1);

    unsubscribe();
    window.dispatchEvent(new CustomEvent(OPEN_PROJECT_EVENT, { detail: '/tmp/later' }));
    expect(handler).toHaveBeenCalledTimes(1);
  });
});
