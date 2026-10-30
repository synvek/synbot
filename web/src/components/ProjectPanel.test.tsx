import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import ProjectPanel from './ProjectPanel';
import { I18nProvider } from '../i18n/I18nContext';
import { apiClient } from '../api';
import type { CheckpointView } from '../types/api';

vi.mock('../api', () => ({
  apiClient: {
    getProjects: vi.fn(),
    getProjectFiles: vi.fn(),
    getProjectFile: vi.fn(),
    getProjectDeliverables: vi.fn(),
    getDeliverablePreview: vi.fn(),
    downloadDeliverable: vi.fn(),
    getCheckpoint: vi.fn(),
    undoCheckpoint: vi.fn(),
    getGitStatus: vi.fn(),
    getGitDiff: vi.fn(),
    getTasks: vi.fn(),
    updateTask: vi.fn(),
    searchSymbols: vi.fn(),
    refreshRepoIndex: vi.fn(),
    openProject: vi.fn(),
    closeProject: vi.fn(),
  },
}));

const mocked = vi.mocked(apiClient, true);

const checkpoint: CheckpointView = {
  id: 'turn-1',
  created_at: '2026-01-01T00:00:00Z',
  workspace: '/tmp/proj',
  undone_at: null,
  undo_skipped: [],
  additions: 1,
  deletions: 1,
  files: [
    {
      path: '/tmp/proj/a.ts',
      display_path: 'a.ts',
      change: 'modified',
      renamed_to: null,
      skipped: null,
      additions: 1,
      deletions: 1,
      hunks: [
        {
          old_start: 1,
          new_start: 1,
          lines: [
            { kind: 'del', old_no: 1, text: 'const a = 1;' },
            { kind: 'add', new_no: 1, text: 'const a = 2;' },
          ],
        },
      ],
    },
  ],
};

const renderPanel = (turnId: string | null) =>
  render(
    <I18nProvider>
      <ProjectPanel
        project={null}
        running={false}
        changedFiles={[]}
        turnId={turnId}
        focusFile={null}
        onSelect={() => {}}
      />
    </I18nProvider>
  );

/** The changes tab label differs per language, and the provider loads locales asynchronously. */
const changesTab = () => screen.getByRole('button', { name: /Changed|本轮改动/ });

describe('ProjectPanel checkpoint view', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocked.getProjects.mockResolvedValue([]);
    mocked.getProjectFiles.mockResolvedValue({ path: '', entries: [] });
    mocked.getGitStatus.mockResolvedValue({
      is_repo: false,
      branch: null,
      ahead: 0,
      behind: 0,
      head: null,
      files: [],
    });
    mocked.getTasks.mockResolvedValue({ tasks: [], updated_at: null });
    mocked.searchSymbols.mockResolvedValue({ hits: [], files: 0 });
  });

  it('lists a turn’s files with line counts and shows the diff on click', async () => {
    mocked.getCheckpoint.mockResolvedValue(checkpoint);
    renderPanel('turn-1');

    fireEvent.click(changesTab());
    const fileRow = await screen.findByRole('button', { name: /a\.ts/ });
    expect(screen.getAllByText('+1').length).toBeGreaterThan(0);

    fireEvent.click(fileRow);
    // Both sides of the hunk are rendered, deletions first.
    const removed = await screen.findByText('const a = 1;');
    const added = screen.getByText('const a = 2;');
    expect(removed).toBeTruthy();
    expect(added).toBeTruthy();
    expect(mocked.getCheckpoint).toHaveBeenCalledWith('turn-1');
  });

  it('undoes the turn and keeps the panel open for review', async () => {
    mocked.getCheckpoint.mockResolvedValue(checkpoint);
    mocked.undoCheckpoint.mockResolvedValue({
      turn_id: 'turn-1',
      restored: ['a.ts'],
      skipped: [],
      errors: [],
    });
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const onUndone = vi.fn();
    render(
      <I18nProvider>
        <ProjectPanel
          project={null}
          running={false}
          changedFiles={[]}
          turnId="turn-1"
          focusFile={null}
          onUndone={onUndone}
          onSelect={() => {}}
        />
      </I18nProvider>
    );

    fireEvent.click(changesTab());
    const undo = await screen.findByRole('button', { name: /Undo turn|撤销本轮/ });
    fireEvent.click(undo);

    await waitFor(() => expect(mocked.undoCheckpoint).toHaveBeenCalledWith('turn-1'));
    await waitFor(() => expect(onUndone).toHaveBeenCalledWith('turn-1'));
  });

  it('falls back to file_changed events when a turn has no checkpoint', async () => {
    mocked.getCheckpoint.mockRejectedValue(new Error('checkpoint not found'));
    render(
      <I18nProvider>
        <ProjectPanel
          project={null}
          running={false}
          changedFiles={[{ path: 'src/lib.rs', kind: 'modified' }]}
          turnId="turn-2"
          focusFile={null}
          onSelect={() => {}}
        />
      </I18nProvider>
    );

    fireEvent.click(changesTab());
    expect(await screen.findByText('src/lib.rs')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Undo turn|撤销本轮/ })).toBeNull();
  });

  it('renders the task board and ticks a task from the panel', async () => {
    mocked.getTasks.mockResolvedValue({
      tasks: [
        {
          id: 't1',
          title: 'read the code',
          status: 'pending',
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-01-01T00:00:00Z',
        },
        {
          id: 't2',
          title: 'write tests',
          status: 'completed',
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-01-01T00:00:00Z',
        },
      ],
      updated_at: '2026-01-01T00:00:00Z',
      path: '/tmp/proj/.synbot/plans/tasks.json',
    });
    mocked.updateTask.mockResolvedValue({
      tasks: [
        {
          id: 't1',
          title: 'read the code',
          status: 'in_progress',
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-01-02T00:00:00Z',
        },
      ],
      updated_at: '2026-01-02T00:00:00Z',
    });
    renderPanel(null);

    fireEvent.click(screen.getByRole('button', { name: /Tasks|任务/ }));
    const first = await screen.findByRole('button', { name: /read the code/ });
    expect(screen.getByText('write tests')).toBeTruthy();

    fireEvent.click(first);
    await waitFor(() => expect(mocked.updateTask).toHaveBeenCalledWith('t1', 'in_progress', undefined));
    // The board is replaced by the server response, not patched locally.
    await waitFor(() => expect(screen.queryByText('write tests')).toBeNull());
  });

  it('searches symbols and jumps the preview to the hit line', async () => {
    mocked.searchSymbols.mockResolvedValue({
      hits: [
        {
          path: 'src/agent.rs',
          line: 42,
          column: 5,
          kind: 'fn',
          name: 'run_turn',
          container: 'AgentLoop',
          signature: 'pub async fn run_turn(&self)',
          score: 1020,
        },
      ],
      files: 12,
    });
    mocked.getProjectFile.mockResolvedValue({
      path: 'src/agent.rs',
      content: ['fn a() {}', 'fn b() {}', 'fn c() {}'].join('\n'),
      truncated: false,
    });

    render(
      <I18nProvider>
        <ProjectPanel
          project={{
            id: 'p1',
            name: 'proj',
            path: '/tmp/proj',
            added_at: '2026-01-01T00:00:00Z',
            git_head: null,
            rules: [],
          }}
          running={false}
          changedFiles={[]}
          turnId={null}
          focusFile={null}
          onSelect={() => {}}
        />
      </I18nProvider>
    );

    fireEvent.change(
      screen.getByPlaceholderText(/Search symbols|搜索符号/),
      { target: { value: 'run_turn' } }
    );
    const hit = await screen.findByRole('button', { name: /AgentLoop::run_turn/ });
    expect(mocked.searchSymbols).toHaveBeenCalledWith('run_turn', 'p1');

    fireEvent.click(hit);
    await waitFor(() => expect(mocked.getProjectFile).toHaveBeenCalledWith('p1', 'src/agent.rs'));
    // The preview is windowed around the hit, so the line number is visible next to the code.
    expect(await screen.findByText(/line 3 of 3|第 3 行/)).toBeTruthy();
  });

  it('lists generated documents in the deliverables tab', async () => {
    mocked.getProjectDeliverables.mockResolvedValue({
      path: '',
      truncated: false,
      scanned: 2,
      entries: [
        {
          name: 'report.docx',
          path: 'out/report.docx',
          kind: 'docx',
          size: 2048,
          modified_ms: 1_700_000_000_000,
        },
      ],
    });

    render(
      <I18nProvider>
        <ProjectPanel
          project={{
            id: 'p1',
            name: 'proj',
            path: '/tmp/proj',
            added_at: '2026-01-01T00:00:00Z',
            git_head: null,
            rules: [],
          }}
          running={false}
          changedFiles={[]}
          turnId={null}
          focusFile={null}
          onSelect={() => {}}
        />
      </I18nProvider>
    );

    fireEvent.click(screen.getByRole('button', { name: /Deliverables|交付物/ }));
    expect(await screen.findByText('report.docx')).toBeInTheDocument();
    expect(mocked.getProjectDeliverables).toHaveBeenCalledWith('p1');
  });
});
