import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Security from './Security';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api/client';
import type {
  PolicyExplain,
  SecurityEventsResponse,
  SecurityOverview,
} from '../types/api';

vi.mock('../api/client', () => ({
  apiClient: {
    securityOverview: vi.fn(),
    listSecurityEvents: vi.fn(),
    clearSecurityEvents: vi.fn(),
    explainPolicy: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const overviewFixture = (over: Partial<SecurityOverview> = {}): SecurityOverview => ({
  profile: 'safe',
  platform: {
    os: 'macos',
    version: '14.5',
    arch: 'aarch64',
    supported: true,
    recommended_app_sandbox: 'seatbelt',
    recommended_tool_sandbox: 'docker',
  },
  workspace: '/home/u/work',
  app_sandbox: { platform: 'seatbelt', work_dir: '/home/u/work' },
  tool_sandbox: {
    backend: 'gvisor-docker',
    backend_explicit: false,
    exec_kind: 'docker',
    image: 'synbot/sandbox:latest',
    network: { enabled: true, hosts: ['api.example.com'], ports: ['443'] },
    filesystem: { readonly: ['/usr'], writable: ['/work'], hidden: [] },
    resources: { max_memory: '2g', max_cpu: '2', max_disk: '5g' },
  },
  exec: {
    timeout_secs: 120,
    restrict_to_workspace: true,
    permissions_enabled: true,
    default_level: 'require_approval',
    rule_count: 2,
    approval_timeout_secs: 300,
    deny_patterns: ['rm -rf /'],
    allow_patterns: ['ls*'],
  },
  secrets: { masking: true, patterns: 9 },
  plugins: { configured: 3, packaged: 2, legacy: 1 },
  audit: {
    capacity: 500,
    retained: 1,
    dropped: 0,
    counts: [{ kind: 'command_blocked', count: 1 }],
  },
  ...over,
});

const trailFixture = (over: Partial<SecurityEventsResponse> = {}): SecurityEventsResponse => ({
  events: [
    {
      id: 7,
      at: '2026-01-02T03:04:05Z',
      kind: 'command_blocked',
      outcome: 'blocked',
      subject: 'rm -rf /',
      detail: 'command matches deny pattern rm -rf /',
    },
  ],
  counts: [{ kind: 'command_blocked', count: 1 }],
  capacity: 500,
  retained: 1,
  dropped: 0,
  ...over,
});

const verdictFixture = (over: Partial<PolicyExplain> = {}): PolicyExplain => ({
  command: 'rm -rf build',
  final: 'blocked',
  blocked_by_pattern: null,
  permissions_enabled: true,
  level: 'deny',
  matched_rule: { pattern: 'rm -rf*', level: 'deny', description: 'never delete recursively' },
  default_applied: false,
  approval_timeout_secs: 300,
  ...over,
});

const renderPage = () =>
  render(
    <I18nProvider>
      <Security />
    </I18nProvider>
  );

describe('Security', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.securityOverview.mockResolvedValue(overviewFixture());
    mocked.listSecurityEvents.mockResolvedValue(trailFixture());
    mocked.clearSecurityEvents.mockResolvedValue(1);
    mocked.explainPolicy.mockResolvedValue(verdictFixture());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the effective exec policy and the security profile', async () => {
    renderPage();

    expect(await screen.findByText('Safe')).toBeInTheDocument();
    expect(screen.getByText('require approval')).toBeInTheDocument();
    expect(screen.getByText(/2 rules/)).toBeInTheDocument();
    expect(screen.getByText('120s')).toBeInTheDocument();
    expect(screen.getByText('300s')).toBeInTheDocument();
    // The pattern chip and the sample event's subject both read `rm -rf /`.
    expect(screen.getAllByText('rm -rf /').length).toBeGreaterThan(0);
    expect(screen.getByText('ls*')).toBeInTheDocument();
    // Workspace row and the app sandbox work dir both show the same path.
    expect(screen.getAllByText('/home/u/work').length).toBeGreaterThan(0);
  });

  it('shows the sandbox backend, image and limits', async () => {
    renderPage();

    expect(await screen.findByText('gvisor-docker')).toBeInTheDocument();
    expect(screen.getByText(/built-in default/)).toBeInTheDocument();
    expect(screen.getByText('synbot/sandbox:latest')).toBeInTheDocument();
    expect(screen.getByText('api.example.com')).toBeInTheDocument();
    expect(screen.getByText('443')).toBeInTheDocument();
    expect(screen.getByText('/usr')).toBeInTheDocument();
    expect(screen.getByText(/2g/)).toBeInTheDocument();
    expect(screen.getByText('seatbelt')).toBeInTheDocument();
  });

  it('warns when exec is not sandboxed at all', async () => {
    mocked.securityOverview.mockResolvedValue(
      overviewFixture({ tool_sandbox: null, app_sandbox: null })
    );
    renderPage();

    expect(await screen.findByText(/No tool sandbox configured/)).toBeInTheDocument();
  });

  it('explains a blocked command with the rule that matched', async () => {
    renderPage();

    const input = await screen.findByPlaceholderText('e.g. rm -rf build');
    fireEvent.change(input, { target: { value: '  rm -rf build  ' } });
    fireEvent.click(screen.getByRole('button', { name: /^Explain$|^试算$/ }));

    await waitFor(() => expect(mocked.explainPolicy).toHaveBeenCalledWith('rm -rf build'));
    expect(await screen.findByText('Blocked')).toBeInTheDocument();
    expect(screen.getByText('rm -rf*')).toBeInTheDocument();
    expect(screen.getByText('never delete recursively')).toBeInTheDocument();
  });

  it('explains a command rejected by the deny patterns', async () => {
    mocked.explainPolicy.mockResolvedValue(
      verdictFixture({
        final: 'blocked',
        blocked_by_pattern: 'command matches deny pattern rm -rf /',
        level: 'allow',
        matched_rule: null,
        default_applied: true,
      })
    );
    renderPage();

    fireEvent.change(await screen.findByPlaceholderText('e.g. rm -rf build'), {
      target: { value: 'rm -rf /' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Explain$|^试算$/ }));

    expect(await screen.findByText(/command matches deny pattern/)).toBeInTheDocument();
    expect(screen.getByText(/No rule matched/)).toBeInTheDocument();
  });

  it('lists audit events with their kind, outcome and subject', async () => {
    mocked.listSecurityEvents.mockResolvedValue(
      trailFixture({
        events: [
          {
            id: 9,
            at: '2026-01-02T03:04:05Z',
            kind: 'sandbox_required',
            outcome: 'needs_user',
            subject: '/etc/passwd',
            detail: 'exec needs a sandbox',
          },
        ],
        counts: [{ kind: 'sandbox_required', count: 1 }],
      })
    );
    renderPage();

    expect(await screen.findByText('/etc/passwd')).toBeInTheDocument();
    expect(screen.getByText('Sandbox required')).toBeInTheDocument();
    expect(screen.getByText('needs approval')).toBeInTheDocument();
    expect(screen.getByText('exec needs a sandbox')).toBeInTheDocument();
  });

  it('filters the trail by kind', async () => {
    mocked.listSecurityEvents.mockResolvedValue(
      trailFixture({
        counts: [
          { kind: 'command_blocked', count: 2 },
          { kind: 'path_escape', count: 1 },
        ],
      })
    );
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: /Path escape · 1/ }));

    await waitFor(() =>
      expect(mocked.listSecurityEvents).toHaveBeenLastCalledWith('path_escape')
    );
  });

  it('clears the trail only after the confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderPage();

    fireEvent.click(await screen.findByRole('button', { name: /^Clear trail$|^清空轨迹$/ }));
    expect(confirm).toHaveBeenCalled();
    expect(mocked.clearSecurityEvents).not.toHaveBeenCalled();

    confirm.mockReturnValue(true);
    mocked.listSecurityEvents.mockResolvedValue(trailFixture({ events: [], counts: [], retained: 0 }));
    fireEvent.click(screen.getByRole('button', { name: /^Clear trail$|^清空轨迹$/ }));

    await waitFor(() => expect(mocked.clearSecurityEvents).toHaveBeenCalled());
    expect(await screen.findByText(/Cleared 1 events/)).toBeInTheDocument();
    await waitFor(() => expect(mocked.listSecurityEvents).toHaveBeenCalledTimes(2));
  });

  it('shows the empty hint when nothing has been stopped', async () => {
    mocked.listSecurityEvents.mockResolvedValue(trailFixture({ events: [], counts: [], retained: 0 }));
    renderPage();

    expect(await screen.findByText(/Nothing blocked yet/)).toBeInTheDocument();
  });

  it('surfaces load errors', async () => {
    mocked.securityOverview.mockRejectedValue(new Error('boom'));
    renderPage();

    expect(await screen.findByText('boom')).toBeInTheDocument();
  });
});
