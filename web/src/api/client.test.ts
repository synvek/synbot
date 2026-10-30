import { beforeEach, describe, expect, it } from 'vitest';
import { ApiClient } from './client';

const DESKTOP = {
  apiBase: 'http://127.0.0.1:19000',
  wsBase: 'ws://127.0.0.1:19000/',
  token: 'deadbeef',
};

describe('ApiClient transports', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it('keeps WebSocket URLs origin-relative on the web', () => {
    const client = new ApiClient();
    const url = client.wsUrl('/ws/agent');

    expect(client.isDesktop()).toBe(false);
    expect(url).toMatch(/^ws:\/\/[^/]+\/ws\/agent$/);
    expect(url).not.toContain('token=');
  });

  it('switches to the desktop daemon and appends the per-launch token', () => {
    const client = new ApiClient();
    sessionStorage.setItem('auth', 'Basic c3RhbGU=');

    client.configureDesktop(DESKTOP);

    expect(client.isDesktop()).toBe(true);
    expect(client.wsUrl('/ws/terminal')).toBe('ws://127.0.0.1:19000/ws/terminal?token=deadbeef');
    // A leftover Basic header would shadow the token after a reload.
    expect(sessionStorage.getItem('auth')).toBeNull();
  });

  it('accepts paths without a leading slash and keeps existing query strings', () => {
    const client = new ApiClient();
    client.configureDesktop(DESKTOP);

    expect(client.wsUrl('ws/agent?resume=1')).toBe(
      'ws://127.0.0.1:19000/ws/agent?resume=1&token=deadbeef'
    );
  });

  it('drops the token when auth is cleared', () => {
    const client = new ApiClient();
    client.configureDesktop(DESKTOP);
    client.clearAuth();

    expect(client.wsUrl('/ws/agent')).toBe('ws://127.0.0.1:19000/ws/agent');
  });
});
