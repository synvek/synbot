import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Workflows from './Workflows';
import { I18nProvider } from '../i18n/I18nContext';
import { ToastProvider } from '../ui';
import { apiClient } from '../api/client';
import type { WorkflowDetail, WorkflowStepView, WorkflowSummary } from '../types/api';

vi.mock('../api/client', () => ({
  apiClient: {
    listWorkflows: vi.fn(),
    getWorkflow: vi.fn(),
    runWorkflow: vi.fn(),
    cancelWorkflow: vi.fn(),
    deleteWorkflow: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const SESSION = 'telegram:42';

const run = (over: Partial<WorkflowSummary> = {}): WorkflowSummary => ({
  sessionKey: SESSION,
  workflowId: 'wf-1',
  name: 'Ship the release',
  description: 'tag and publish',
  status: 'waiting_user_input',
  currentStepIndex: 2,
  stepsTotal: 5,
  stepsCompleted: 2,
  createdAt: '2026-10-01T10:00:00Z',
  updatedAt: '2026-10-01T10:05:00Z',
  canResume: true,
  ...over,
});

const step = (over: Partial<WorkflowStepView> = {}): WorkflowStepView => ({
  id: 'gather',
  type: 'llm',
  description: 'Collect the requirements',
  status: 'completed',
  output: 'requirements gathered',
  ...over,
});

const detail = (over: Partial<WorkflowDetail> = {}): WorkflowDetail => ({
  ...run(),
  inputs: { target: 'v1.2.3' },
  userInputTimeoutSecs: 600,
  steps: [
    step(),
    step({ id: 'approve', type: 'user_input', status: 'current', output: null }),
    step({ id: 'tag', status: 'pending', output: null }),
  ],
  ...over,
});

const renderPage = () =>
  render(
    <I18nProvider>
      <ToastProvider>
        <Workflows />
      </ToastProvider>
    </I18nProvider>
  );

const RUN_ROW = /Ship the release/;
const RESUME = /^Resume$|^继续运行$/;
const RESTART = /^Restart from step 1$|^从头重跑$/;
const DELETE = /^Delete record$|^删除记录$/;
const CANCEL = /^Cancel$|^取消$/;
const CONFIRM = /^Continue$|^继续$/;
const CLOSE = /^Close$|^关闭$/;

describe('Workflows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.listWorkflows.mockResolvedValue([run()]);
    mocked.getWorkflow.mockResolvedValue(detail());
    mocked.runWorkflow.mockResolvedValue('resumed');
    mocked.cancelWorkflow.mockResolvedValue(run({ status: 'cancelled', canResume: false }));
    mocked.deleteWorkflow.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lists runs with their status and progress', async () => {
    renderPage();

    expect(await screen.findByText(RUN_ROW)).toBeInTheDocument();
    expect(screen.getByText(SESSION)).toBeInTheDocument();
    expect(await screen.findByText(/Waiting for input|等待用户输入/)).toBeInTheDocument();
    // `stepsCompleted/stepsTotal` is the progress the row shows next to the status.
    expect(screen.getByText('2/5')).toBeInTheDocument();
  });

  it('shows the empty hint when no run was persisted', async () => {
    mocked.listWorkflows.mockResolvedValue([]);
    renderPage();

    expect(await screen.findByText(/No workflows yet|还没有工作流/)).toBeInTheDocument();
  });

  it('surfaces load errors', async () => {
    mocked.listWorkflows.mockRejectedValue(new Error('boom'));
    renderPage();

    expect(await screen.findByText('boom')).toBeInTheDocument();
  });

  it('opens a run and renders its steps with outputs', async () => {
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));

    await waitFor(() => expect(mocked.getWorkflow).toHaveBeenCalledWith(SESSION));
    // The row renders `<index>. <id>` as separate nodes, so match on the containing element.
    expect(await screen.findByText(/approve/)).toBeInTheDocument();
    // Completed steps keep their output collapsed until the reader asks for it.
    fireEvent.click(screen.getByRole('button', { name: /Show output|查看输出/ }));
    expect(screen.getByText('requirements gathered')).toBeInTheDocument();
    expect(screen.getAllByText(/This step has not produced output yet|该步骤尚未产生输出/)).toHaveLength(2);
    // Inputs come from the persisted state, not from the chat history.
    expect(screen.getByText('target')).toBeInTheDocument();
    expect(screen.getByText('v1.2.3')).toBeInTheDocument();
  });

  it('resumes an unfinished run and refreshes the list', async () => {
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));
    fireEvent.click(await screen.findByRole('button', { name: RESUME }));

    await waitFor(() => expect(mocked.runWorkflow).toHaveBeenCalledWith(SESSION, false));
    await waitFor(() => expect(mocked.listWorkflows).toHaveBeenCalledTimes(2));
  });

  it('offers a restart instead of a resume once the run finished', async () => {
    mocked.listWorkflows.mockResolvedValue([run({ status: 'completed', canResume: false })]);
    mocked.getWorkflow.mockResolvedValue(
      detail({ status: 'completed', canResume: false, currentStepIndex: 3 })
    );
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));

    expect(await screen.findByRole('button', { name: RESTART })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: RESUME })).not.toBeInTheDocument();
    // A finished run cannot be cancelled either — there is nothing left to stop.
    expect(screen.queryByRole('button', { name: CANCEL })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: RESTART }));
    // Destructive actions go through a confirmation dialog, never a native confirm().
    fireEvent.click(await screen.findByRole('button', { name: CONFIRM }));

    await waitFor(() => expect(mocked.runWorkflow).toHaveBeenCalledWith(SESSION, true));
  });

  it('cancels a running workflow', async () => {
    // The action reloads the list, so the second listing reports the new status.
    mocked.listWorkflows
      .mockResolvedValueOnce([run()])
      .mockResolvedValue([run({ status: 'cancelled', canResume: false })]);
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));
    fireEvent.click(await screen.findByRole('button', { name: CANCEL }));

    await waitFor(() => expect(mocked.cancelWorkflow).toHaveBeenCalledWith(SESSION));
    expect(await screen.findByText(/^Cancelled$|^已取消$/)).toBeInTheDocument();
  });

  it('only deletes after the confirmation is accepted', async () => {
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));
    fireEvent.click(await screen.findByRole('button', { name: DELETE }));
    expect(mocked.deleteWorkflow).not.toHaveBeenCalled();

    // Dismissing the dialog leaves the record alone.
    fireEvent.click(screen.getByRole('button', { name: CLOSE }));
    expect(mocked.deleteWorkflow).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: DELETE }));
    fireEvent.click(await screen.findByRole('button', { name: CONFIRM }));

    await waitFor(() => expect(mocked.deleteWorkflow).toHaveBeenCalledWith(SESSION));
  });

  it('keeps the error from a rejected action without dropping the list', async () => {
    mocked.runWorkflow.mockRejectedValue({
      response: { data: { error: 'Conflict: the agent is busy with another turn' } },
    });
    renderPage();

    fireEvent.click(await screen.findByText(RUN_ROW));
    fireEvent.click(await screen.findByRole('button', { name: RESUME }));

    expect(
      await screen.findByText(/the agent is busy with another turn/)
    ).toBeInTheDocument();
    // Still in the list (and now in the detail header too).
    expect(screen.getAllByText(RUN_ROW).length).toBeGreaterThanOrEqual(1);
  });
});
