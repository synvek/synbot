import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import TeamPanel from './TeamPanel';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api';
import type { SubagentInfo } from '../types/agentEvents';

vi.mock('../api', () => ({
  apiClient: {
    listSubagents: vi.fn(),
    cancelSubagent: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const agent = (over: Partial<SubagentInfo>): SubagentInfo => ({
  id: 'sa-1',
  label: 'explore auth',
  state: 'running',
  created_at: '2026-01-01T00:00:00Z',
  ...over,
});

const renderPanel = (live: SubagentInfo[] = []) =>
  render(
    <I18nProvider>
      <TeamPanel live={live} />
    </I18nProvider>
  );

/** Labels differ per language and the provider loads locales asynchronously. */
const stopAll = () => screen.queryByRole('button', { name: /Stop all|全部停止/ });

describe('TeamPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.listSubagents.mockResolvedValue({ agents: [], running: 0 });
    mocked.cancelSubagent.mockResolvedValue(1);
  });

  it('lists the pool snapshot with role, state and activity', async () => {
    mocked.listSubagents.mockResolvedValue({
      agents: [agent({ role: 'reviewer', activity: 'tool: exec' })],
      running: 1,
    });
    renderPanel();

    expect(await screen.findByText('explore auth')).toBeInTheDocument();
    expect(screen.getByText(/reviewer/)).toBeInTheDocument();
    expect(screen.getByText('tool: exec')).toBeInTheDocument();
    expect(stopAll()).toBeInTheDocument();
  });

  it('shows the empty hint when nobody has been spawned', async () => {
    renderPanel();

    expect(await screen.findByText(/No subagents yet|还没有子代理/)).toBeInTheDocument();
    expect(stopAll()).toBeNull();
  });

  it('lets a live event override a stale snapshot entry', async () => {
    mocked.listSubagents.mockResolvedValue({ agents: [agent({})], running: 1 });
    const { rerender } = renderPanel();

    expect(await screen.findByText('explore auth')).toBeInTheDocument();
    expect(stopAll()).toBeInTheDocument();

    rerender(
      <I18nProvider>
        <TeamPanel
          live={[
            agent({ state: 'completed', finished_at: '2026-01-01T00:00:05Z', activity: 'done' }),
          ]}
        />
      </I18nProvider>
    );

    await waitFor(() => expect(stopAll()).toBeNull());
    // Counts come from the merged view: one teammate, none running any more.
    expect(
      await screen.findByText(/0 running \/ 1 total|运行中 0 \/ 共 1/)
    ).toBeInTheDocument();
  });

  it('cancels a single teammate and refreshes the pool', async () => {
    mocked.listSubagents.mockResolvedValue({ agents: [agent({})], running: 1 });
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /^Stop$|^停止$/ }));

    await waitFor(() => expect(mocked.cancelSubagent).toHaveBeenCalledWith('sa-1', false));
    await waitFor(() => expect(mocked.listSubagents).toHaveBeenCalledTimes(2));
  });

  it('cancels every running teammate at once', async () => {
    mocked.listSubagents.mockResolvedValue({
      agents: [agent({}), agent({ id: 'sa-2', label: 'write docs' })],
      running: 2,
    });
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: /Stop all|全部停止/ }));

    await waitFor(() => expect(mocked.cancelSubagent).toHaveBeenCalledWith('all', true));
  });
});
