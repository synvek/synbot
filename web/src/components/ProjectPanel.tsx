import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Folder,
  FolderPlus,
  File as FileIcon,
  GitBranch,
  RefreshCw,
  RotateCcw,
  Search,
  X,
} from 'lucide-react';
import { apiClient } from '../api';
import { isDesktop, onOpenProject, pickFolder } from '../desktop/bridge';
import type {
  CheckpointFile,
  CheckpointView,
  GitStatus,
  Project,
  ProjectFileContent,
  ProjectFileEntry,
  SymbolHit,
  TaskItem,
  TaskStatus,
} from '../types/api';
import { useI18n } from '../i18n/I18nContext';
import DeliverablePanel from './DeliverablePanel';

export interface ChangedFile {
  path: string;
  kind: string;
}

interface ProjectPanelProps {
  /** Folder the agent works in right now (`null` = global workspace). */
  project: Project | null;
  /** A turn is running: switching folders only affects the next turn. */
  running: boolean;
  /** Files touched by the most recent turn. */
  changedFiles: ChangedFile[];
  /** Turn id of the most recent turn, used to load its checkpoint (diff + undo). */
  turnId: string | null;
  /** A file the chat asked to inspect, e.g. clicked in the turn summary. */
  focusFile: { turnId: string; path: string } | null;
  /** Called after a successful undo so the caller can refresh its own view. */
  onUndone?: (turnId: string) => void;
  /** Bumped by the socket on `plan_updated`, so the board reflects agent-written plans. */
  planVersion?: number;
  onSelect: (projectId: string | null) => void;
}

function formatBytes(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Right-hand workbench panel: which folder the agent works in, its file tree, and what the last
 * turn changed. Folder switching goes through the agent socket so the server owns the binding.
 */
export default function ProjectPanel({
  project,
  running,
  changedFiles,
  turnId,
  focusFile,
  onUndone,
  planVersion = 0,
  onSelect,
}: ProjectPanelProps) {
  const { t } = useI18n();
  const [projects, setProjects] = useState<Project[]>([]);
  const [dirPath, setDirPath] = useState('');
  const [entries, setEntries] = useState<ProjectFileEntry[]>([]);
  const [reloadToken, setReloadToken] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openPath, setOpenPath] = useState('');
  const [opening, setOpening] = useState(false);
  const [tab, setTab] = useState<'files' | 'changes' | 'tasks' | 'delivery'>('files');
  const [preview, setPreview] = useState<ProjectFileContent | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [checkpoint, setCheckpoint] = useState<CheckpointView | null>(null);
  const [diffPath, setDiffPath] = useState<string | null>(null);
  const [undoing, setUndoing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [git, setGit] = useState<GitStatus | null>(null);
  const [gitPanel, setGitPanel] = useState(false);
  const [gitDiff, setGitDiff] = useState<{ path: string; text: string } | null>(null);
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [tasksPath, setTasksPath] = useState<string | null>(null);
  const [taskBusy, setTaskBusy] = useState<string | null>(null);
  /** Symbol lookup over the repo index; `null` = the box is empty, so the tree shows. */
  const [symbolQuery, setSymbolQuery] = useState('');
  const [symbolHits, setSymbolHits] = useState<SymbolHit[] | null>(null);
  const [symbolBusy, setSymbolBusy] = useState(false);
  /** Bumped after an explicit re-index so the current query runs again. */
  const [symbolVersion, setSymbolVersion] = useState(0);
  /** Line to centre the preview on (set when a symbol hit is opened). */
  const [previewLine, setPreviewLine] = useState<number | null>(null);
  const requestIdRef = useRef(0);

  const refreshProjects = useCallback(async () => {
    try {
      setProjects(await apiClient.getProjects());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refreshProjects();
  }, [refreshProjects]);

  // Load the directory listing whenever the folder (or the browsed subdirectory) changes.
  useEffect(() => {
    if (!project) {
      setEntries([]);
      setDirPath('');
      return;
    }
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    apiClient
      .getProjectFiles(project.id, dirPath)
      .then((dir) => {
        if (requestIdRef.current === requestId) setEntries(dir.entries);
      })
      .catch((e) => {
        if (requestIdRef.current === requestId) {
          setError(e instanceof Error ? e.message : String(e));
        }
      })
      .finally(() => {
        if (requestIdRef.current === requestId) setLoading(false);
      });
  }, [project, dirPath, reloadToken]);

  // Load the latest turn's checkpoint: per-file diffs plus the undo entry point. A missing
  // checkpoint is normal (turn only ran `exec`, or the file was too large to snapshot).
  useEffect(() => {
    setCheckpoint(null);
    setDiffPath(null);
    setNotice(null);
    if (!turnId) return;
    let active = true;
    apiClient
      .getCheckpoint(turnId)
      .then((view) => {
        if (active) setCheckpoint(view);
      })
      .catch(() => {
        // No checkpoint for this turn: the "changes" tab falls back to the event list.
      });
    return () => {
      active = false;
    };
  }, [turnId]);

  // A click in the chat's turn summary opens that file's diff here.
  useEffect(() => {
    if (!focusFile) return;
    setTab('changes');
    setDiffPath(focusFile.path);
  }, [focusFile]);

  const handleUndo = async () => {
    if (!checkpoint) return;
    const confirmed = window.confirm(
      t(
        'project.undoConfirm',
        `撤销本轮对 ${checkpoint.files.length} 个文件的改动？此操作会直接写回磁盘。`
      )
    );
    if (!confirmed) return;
    setUndoing(true);
    setError(null);
    try {
      const report = await apiClient.undoCheckpoint(checkpoint.id);
      const skipped = report.skipped.length;
      setNotice(
        skipped > 0
          ? t(
              'project.undoPartial',
              `已恢复 ${report.restored.length} 个，跳过 ${skipped} 个`
            )
          : t('project.undoDone', `已恢复 ${report.restored.length} 个文件`)
      );
      setReloadToken((value) => value + 1);
      setCheckpoint(await apiClient.getCheckpoint(checkpoint.id));
      setDiffPath(null);
      onUndone?.(checkpoint.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setUndoing(false);
    }
  };

  // Repository state for the strip under the project path; refreshed with the file listing.
  useEffect(() => {
    setGit(null);
    setGitDiff(null);
    let active = true;
    apiClient
      .getGitStatus(project?.id)
      .then((status) => {
        if (active) setGit(status);
      })
      .catch(() => {
        // Not a repository (or git missing): the strip simply does not render.
      });
    return () => {
      active = false;
    };
  }, [project, reloadToken]);

  // Plan/task board: same file the agent writes with `plan_write`. Reloaded when the folder
  // changes, when the user hits refresh, and whenever the socket reports a new plan.
  useEffect(() => {
    let active = true;
    apiClient
      .getTasks(project?.id)
      .then((board) => {
        if (!active) return;
        setTasks(board.tasks);
        setTasksPath(board.path ?? null);
      })
      .catch(() => {
        // No board yet (agent never planned): the tab renders the empty state.
        if (active) setTasks([]);
      });
    return () => {
      active = false;
    };
  }, [project, reloadToken, planVersion]);

  // Click cycles pending → in_progress → completed → pending; cancelled restarts at pending.
  const cycleTask = async (task: TaskItem) => {
    const next: TaskStatus =
      task.status === 'pending'
        ? 'in_progress'
        : task.status === 'in_progress'
          ? 'completed'
          : 'pending';
    setTaskBusy(task.id);
    setError(null);
    try {
      const board = await apiClient.updateTask(task.id, next, project?.id);
      setTasks(board.tasks);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskBusy(null);
    }
  };

  // Symbol lookup over the repo index: debounced, and only once the query is meaningful.
  // The backend builds the index on first use, so this works before the agent ever ran.
  useEffect(() => {
    const query = symbolQuery.trim();
    if (query.length < 2) {
      setSymbolHits(null);
      setSymbolBusy(false);
      return;
    }
    let active = true;
    setSymbolBusy(true);
    const timer = window.setTimeout(() => {
      apiClient
        .searchSymbols(query, project?.id)
        .then((result) => {
          if (active) setSymbolHits(result.hits);
        })
        .catch(() => {
          if (active) setSymbolHits([]);
        })
        .finally(() => {
          if (active) setSymbolBusy(false);
        });
    }, 250);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [symbolQuery, project, symbolVersion]);

  const openSymbol = (hit: SymbolHit) => {
    void openFile(hit.path, hit.line);
  };

  const reindex = async () => {
    setError(null);
    setSymbolBusy(true);
    try {
      const stats = await apiClient.refreshRepoIndex(project?.id);
      setNotice(
        t('project.reindexed', {
          files: stats.files,
          symbols: stats.symbols,
        })
      );
      setSymbolVersion((value) => value + 1);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSymbolBusy(false);
    }
  };

  const openGitDiff = async (path: string) => {
    try {
      setGitDiff({ path, text: await apiClient.getGitDiff(path, false, project?.id) });
    } catch (e) {
      setGitDiff({ path, text: e instanceof Error ? e.message : String(e) });
    }
  };

  const openChange = (file: CheckpointFile) => {
    if (file.hunks.length > 0 || file.skipped) {
      setDiffPath(file.display_path);
      return;
    }
    // Nothing to diff (e.g. an empty new file): show the content when the project is open.
    if (project) void openFile(file.display_path);
  };

  const shownFiles: CheckpointFile[] = checkpoint?.files ?? [];
  const diffFile = diffPath ? shownFiles.find((f) => f.display_path === diffPath) : undefined;
  const doneTasks = tasks.filter((task) => task.status === 'completed').length;

  const openProjectPath = useCallback(
    async (rawPath: string) => {
      const path = rawPath.trim();
      if (!path) return;
      setOpening(true);
      setError(null);
      try {
        const opened = await apiClient.openProject(path);
        await refreshProjects();
        setOpenPath('');
        setDirPath('');
        onSelect(opened.id);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setOpening(false);
      }
    },
    [onSelect, refreshProjects]
  );

  const handleOpen = async (event: React.FormEvent) => {
    event.preventDefault();
    await openProjectPath(openPath);
  };

  // The tray menu's "Open Project…" sends the folder it picked as a DOM event.
  useEffect(() => onOpenProject((path) => void openProjectPath(path)), [openProjectPath]);

  /** Desktop only: the path comes from the native folder picker instead of typing. */
  const handleBrowse = async () => {
    const path = await pickFolder();
    if (path) await openProjectPath(path);
  };

  const handleClose = async () => {
    if (!project) return;
    try {
      await apiClient.closeProject(project.id);
      await refreshProjects();
      onSelect(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const openFile = async (path: string, line: number | null = null) => {
    if (!project) return;
    setPreviewLine(line);
    setPreviewError(null);
    try {
      setPreview(await apiClient.getProjectFile(project.id, path));
    } catch (e) {
      setPreviewError(e instanceof Error ? e.message : String(e));
    }
  };

  const segments = dirPath ? dirPath.split('/') : [];

  // Windowed view for symbol jumps: a 512KB file renders ~120 lines instead of thousands.
  const previewWindow = useMemo(() => {
    if (!preview || previewLine == null) return null;
    const lines = preview.content.split('\n');
    const focus = Math.min(Math.max(previewLine, 1), Math.max(lines.length, 1));
    const start = Math.max(0, focus - 1 - 60);
    const end = Math.min(lines.length, focus + 60);
    return { lines: lines.slice(start, end), start, total: lines.length, focus };
  }, [preview, previewLine]);

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden bg-surface">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-line">
        <span className="text-sm font-semibold text-ink">{t('project.title', '工作区')}</span>
        <button
          type="button"
          onClick={() => {
            setReloadToken((value) => value + 1);
            void refreshProjects();
          }}
          className="ml-auto p-1 rounded text-ink-2 hover:text-ink hover:bg-surface-2"
          title={t('project.refresh', '刷新')}
        >
          <RefreshCw size={14} />
        </button>
      </div>

      <div className="px-3 py-2 space-y-2 border-b border-line">
        <select
          aria-label={t('project.workspaceSelect', '工作区')}
          value={project?.id ?? ''}
          onChange={(e) => onSelect(e.target.value || null)}
          className="w-full text-sm px-2 py-1.5 rounded border border-line bg-surface-2 text-ink"
        >
          <option value="">{t('project.globalWorkspace', '默认工作区（~/.synbot/workspace）')}</option>
          {projects.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name} — {item.path}
            </option>
          ))}
        </select>

        {project && (
          <div className="text-xs text-ink-2 space-y-0.5">
            <div className="truncate font-mono" title={project.path}>
              {project.path}
            </div>
            <div className="flex items-center gap-2">
              {project.git_head && (
                <span className="flex items-center gap-1">
                  <GitBranch size={11} />
                  {project.git_head.slice(0, 7)}
                </span>
              )}
              {project.rules.length > 0 && (
                <span>{t('project.ruleCount', { count: project.rules.length })}</span>
              )}
              <button
                type="button"
                onClick={() => void handleClose()}
                className="ml-auto p-0.5 rounded hover:text-error"
                title={t('project.close', '移除')}
              >
                <X size={12} />
              </button>
            </div>
            {running && (
              <div className="text-[11px] text-ink-2">
                {t('project.switchNextTurn', '切换将在下一轮生效')}
              </div>
            )}
            {git?.is_repo && (
              <button
                type="button"
                onClick={() => setGitPanel(true)}
                title={git.files.map((f) => `${f.status} ${f.path}`).join('\n')}
                className="flex items-center gap-1 text-[11px] text-ink-2 hover:text-ink"
              >
                <GitBranch size={11} />
                <span className="truncate">{git.branch ?? 'HEAD'}</span>
                {git.ahead > 0 && <span>↑{git.ahead}</span>}
                {git.behind > 0 && <span>↓{git.behind}</span>}
                <span className="ml-1">
                  {git.files.length > 0
                    ? t('project.gitChanged', `${git.files.length} 个未提交`)
                    : t('project.gitClean', '干净')}
                </span>
              </button>
            )}
          </div>
        )}

        <form onSubmit={handleOpen} className="flex gap-1">
          <input
            value={openPath}
            onChange={(e) => setOpenPath(e.target.value)}
            placeholder={t('project.openPathPlaceholder', '输入文件夹路径…')}
            className="flex-1 min-w-0 text-xs px-2 py-1 rounded border border-line bg-surface-2 text-ink focus:outline-none focus:ring-1 focus:ring-accent"
          />
          {isDesktop() && (
            <button
              type="button"
              onClick={handleBrowse}
              disabled={opening}
              className="p-1.5 rounded border border-line text-ink-2 hover:text-ink disabled:opacity-50"
              title={t('project.browse', '选择文件夹…')}
            >
              <Folder size={13} />
            </button>
          )}
          <button
            type="submit"
            disabled={opening || !openPath.trim()}
            className="p-1.5 rounded border border-line text-ink-2 hover:text-ink disabled:opacity-50"
            title={t('project.open', '打开文件夹')}
          >
            <FolderPlus size={13} />
          </button>
        </form>

        {error && <div className="text-xs text-error break-all">{error}</div>}
      </div>

      {/* Content-width tabs in a scroll strip: the four English labels do not fit 320px at equal widths. */}
      <div className="flex shrink-0 overflow-x-auto border-b border-line text-small">
        <button
          type="button"
          onClick={() => setTab('files')}
          className={`shrink-0 whitespace-nowrap px-2.5 py-1.5 ${
            tab === 'files' ? 'border-b-2 border-primary text-ink' : 'text-ink-2'
          }`}
        >
          {t('project.files', '文件')}
        </button>
        <button
          type="button"
          onClick={() => setTab('changes')}
          className={`shrink-0 whitespace-nowrap px-2.5 py-1.5 ${
            tab === 'changes' ? 'border-b-2 border-primary text-ink' : 'text-ink-2'
          }`}
        >
          {t('project.changes', '本轮改动')}
          {changedFiles.length > 0 && ` (${changedFiles.length})`}
        </button>
        <button
          type="button"
          onClick={() => setTab('tasks')}
          className={`shrink-0 whitespace-nowrap px-2.5 py-1.5 ${
            tab === 'tasks' ? 'border-b-2 border-primary text-ink' : 'text-ink-2'
          }`}
        >
          {t('project.tasks', '任务')}
          {tasks.length > 0 && ` ${doneTasks}/${tasks.length}`}
        </button>
        <button
          type="button"
          onClick={() => setTab('delivery')}
          className={`shrink-0 whitespace-nowrap px-2.5 py-1.5 ${
            tab === 'delivery' ? 'border-b-2 border-primary text-ink' : 'text-ink-2'
          }`}
        >
          {t('project.deliverables', '交付物')}
        </button>
      </div>

      {tab === 'files' ? (
        project ? (
          <>
            <div className="flex items-center gap-1 px-2 pt-2">
              <div className="relative flex-1 min-w-0">
                <Search
                  size={12}
                  className="absolute left-1.5 top-1.5 text-ink-2 pointer-events-none"
                />
                <input
                  value={symbolQuery}
                  onChange={(e) => setSymbolQuery(e.target.value)}
                  placeholder={t('project.symbolSearch', '搜索符号（函数/类型/类）')}
                  className="w-full text-xs pl-6 pr-2 py-1 rounded border border-line bg-surface-2 text-ink focus:outline-none focus:ring-1 focus:ring-accent"
                />
              </div>
              <button
                type="button"
                onClick={() => void reindex()}
                disabled={symbolBusy}
                title={t('project.reindex', '重建索引')}
                className="p-1.5 rounded border border-line text-ink-2 hover:text-ink disabled:opacity-50"
              >
                <RefreshCw size={12} />
              </button>
            </div>
            {notice && <div className="px-3 py-1 text-[11px] text-success">{notice}</div>}
            <div className="flex items-center gap-1 px-2 py-1 text-xs text-ink-2 overflow-x-auto">
              <button
                type="button"
                onClick={() => setDirPath('')}
                className="hover:text-ink"
              >
                {project.name}
              </button>
              {segments.map((segment, index) => (
                <span key={`${segment}-${index}`} className="flex items-center gap-1">
                  <span className="opacity-50">/</span>
                  <button
                    type="button"
                    onClick={() => setDirPath(segments.slice(0, index + 1).join('/'))}
                    className="hover:text-ink"
                  >
                    {segment}
                  </button>
                </span>
              ))}
            </div>
            <ul className="flex-1 overflow-y-auto text-sm">
              {symbolHits !== null ? (
                <>{/* symbol search results replace the tree while the box has a query */}
                  {symbolBusy && (
                    <li className="px-3 py-2 text-xs text-ink-2">{t('common.loading')}</li>
                  )}
                  {!symbolBusy && symbolHits.length === 0 && (
                    <li className="px-3 py-2 text-xs text-ink-2">
                      {t('project.symbolsEmpty', '没有匹配的符号')}
                    </li>
                  )}
                  {symbolHits.map((hit, index) => (
                    <li key={`${hit.path}-${hit.line}-${index}`}>
                      <button
                        type="button"
                        onClick={() => openSymbol(hit)}
                        className="w-full flex items-start gap-2 px-3 py-1 text-left hover:bg-surface-2"
                      >
                        <span className="uppercase text-[10px] text-ink-2 w-11 shrink-0 pt-0.5">
                          {hit.kind}
                        </span>
                        <span className="min-w-0">
                          <span className="block truncate text-ink text-xs">
                            {hit.container ? `${hit.container}::${hit.name}` : hit.name}
                          </span>
                          <span className="block truncate text-[10px] font-mono text-ink-2">
                            {hit.path}:{hit.line}
                          </span>
                        </span>
                      </button>
                    </li>
                  ))}
                </>
              ) : (
                <>
              {loading && (
                <li className="px-3 py-2 text-xs text-ink-2">{t('common.loading')}</li>
              )}
              {!loading && entries.length === 0 && (
                <li className="px-3 py-2 text-xs text-ink-2">
                  {t('project.emptyFolder', '空文件夹')}
                </li>
              )}
              {entries.map((entry) => (
                <li key={entry.path}>
                  <button
                    type="button"
                    onClick={() =>
                      entry.is_dir ? setDirPath(entry.path) : void openFile(entry.path)
                    }
                    className="w-full flex items-center gap-2 px-3 py-1 text-left hover:bg-surface-2"
                  >
                    {entry.is_dir ? (
                      <Folder size={13} className="text-accent shrink-0" />
                    ) : (
                      <FileIcon size={13} className="text-ink-2 shrink-0" />
                    )}
                    <span className="truncate text-ink">{entry.name}</span>
                    {!entry.is_dir && (
                      <span className="ml-auto text-[10px] text-ink-2 shrink-0">
                        {formatBytes(entry.size)}
                      </span>
                    )}
                  </button>
                </li>
              ))}
                </>
              )}
            </ul>
          </>
        ) : (
          <div className="p-3 text-xs text-ink-2">{t('project.noProject', '未打开文件夹，本轮运行在默认工作区。')}</div>
        )
      ) : tab === 'changes' ? (
        <div className="flex flex-col flex-1 min-h-0">
          {checkpoint && checkpoint.files.length > 0 && (
            <div className="flex items-center gap-2 px-3 py-1.5 border-b border-line text-[11px] text-ink-2">
              <span className="text-success">+{checkpoint.additions}</span>
              <span className="text-error">-{checkpoint.deletions}</span>
              {checkpoint.undone_at ? (
                <span className="ml-auto">{t('project.alreadyUndone', '已撤销')}</span>
              ) : (
                <button
                  type="button"
                  onClick={() => void handleUndo()}
                  disabled={undoing || running}
                  title={t('project.undoHint', '把本轮写过的文件恢复到本轮开始前')}
                  className="ml-auto flex items-center gap-1 px-1.5 py-0.5 rounded border border-line hover:text-ink disabled:opacity-50"
                >
                  <RotateCcw size={11} />
                  {undoing ? t('project.undoing', '撤销中…') : t('project.undo', '撤销本轮')}
                </button>
              )}
            </div>
          )}
          {notice && <div className="px-3 py-1 text-[11px] text-success">{notice}</div>}
          <ul className="flex-1 overflow-y-auto text-xs">
            {shownFiles.length > 0 ? (
              shownFiles.map((file) => (
                <li key={file.path}>
                  <button
                    type="button"
                    onClick={() => openChange(file)}
                    className="w-full flex items-center gap-2 px-3 py-1 text-left hover:bg-surface-2"
                  >
                    <span className="uppercase text-[10px] text-ink-2 w-10 shrink-0">
                      {file.change.slice(0, 7)}
                    </span>
                    <span className="truncate font-mono text-ink">{file.display_path}</span>
                    <span className="ml-auto shrink-0 text-[10px]">
                      {file.additions > 0 && (
                        <span className="text-success">+{file.additions}</span>
                      )}
                      {file.deletions > 0 && (
                        <span className="text-error ml-1">-{file.deletions}</span>
                      )}
                    </span>
                  </button>
                </li>
              ))
            ) : changedFiles.length === 0 ? (
              <li className="px-3 py-2 text-ink-2">
                {t('project.noChanges', '最近一轮没有改动文件')}
              </li>
            ) : (
              // No checkpoint (turn only ran `exec`, or files were too large): keep the event list.
              changedFiles.map((file) => (
                <li key={file.path}>
                  <button
                    type="button"
                    onClick={() => project && void openFile(file.path)}
                    className="w-full flex items-center gap-2 px-3 py-1 text-left hover:bg-surface-2"
                  >
                    <span className="uppercase text-[10px] text-ink-2 w-10 shrink-0">
                      {file.kind}
                    </span>
                    <span className="truncate font-mono text-ink">{file.path}</span>
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      ) : tab === 'tasks' ? (
        <div className="flex flex-col flex-1 min-h-0">
          <div className="flex items-center gap-2 px-3 py-1.5 border-b border-line text-[11px] text-ink-2">
            <span>
              {t('project.taskProgress', { done: doneTasks, total: tasks.length })}
            </span>
            <span className="ml-auto truncate font-mono" title={tasksPath ?? ''}>
              {tasksPath ? tasksPath.split('/').slice(-2).join('/') : ''}
            </span>
          </div>
          <ul className="flex-1 overflow-y-auto text-xs">
            {tasks.length === 0 ? (
              <li className="px-3 py-2 text-ink-2">
                {t('project.noTasks', '还没有任务计划。让 agent 先列出步骤（plan_write）再动手。')}
              </li>
            ) : (
              tasks.map((task) => (
                <li key={task.id}>
                  <button
                    type="button"
                    onClick={() => void cycleTask(task)}
                    disabled={taskBusy === task.id}
                    title={task.notes ?? t('project.taskHint', '点击切换状态')}
                    className="w-full flex items-start gap-2 px-3 py-1 text-left hover:bg-surface-2 disabled:opacity-50"
                  >
                    <span
                      className={`mt-0.5 w-3 h-3 shrink-0 rounded-sm border ${
                        task.status === 'completed'
                          ? 'bg-success border-success'
                          : task.status === 'in_progress'
                            ? 'bg-accent/40 border-primary'
                            : task.status === 'cancelled'
                              ? 'bg-border border-line'
                              : 'border-line'
                      }`}
                    />
                    <span
                      className={`truncate ${
                        task.status === 'completed' || task.status === 'cancelled'
                          ? 'text-ink-2 line-through'
                          : 'text-ink'
                      }`}
                    >
                      {task.title}
                    </span>
                    <span className="ml-auto shrink-0 text-[10px] text-ink-2">
                      {t(`project.taskStatus.${task.status}`, task.status)}
                    </span>
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      ) : project ? (
        <DeliverablePanel project={project} />
      ) : (
        <div className="p-3 text-xs text-ink-2">
          {t('project.noProject', '未打开文件夹，本轮运行在默认工作区。')}
        </div>
      )}

      {diffFile && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-6">
          <div className="flex flex-col w-full max-w-5xl max-h-[85vh] rounded-lg bg-surface border border-line overflow-hidden">
            <div className="flex items-center gap-2 px-3 py-2 border-b border-line">
              <span className="text-xs uppercase text-ink-2">{diffFile.change}</span>
              <span className="text-sm font-mono text-ink truncate">
                {diffFile.display_path}
                {diffFile.renamed_to && ` → ${diffFile.renamed_to}`}
              </span>
              <span className="text-[11px]">
                <span className="text-success">+{diffFile.additions}</span>{' '}
                <span className="text-error">-{diffFile.deletions}</span>
              </span>
              <button
                type="button"
                onClick={() => setDiffPath(null)}
                className="ml-auto p-1 rounded text-ink-2 hover:text-ink"
              >
                <X size={16} />
              </button>
            </div>
            {diffFile.skipped ? (
              <div className="p-4 text-sm text-ink-2">
                {t('project.notSnapshotted', '未快照')}：{diffFile.skipped}
              </div>
            ) : (
              <div className="flex-1 overflow-auto bg-surface-2 font-mono text-[11px] leading-5">
                {diffFile.hunks.map((hunk, hunkIndex) => (
                  <div key={`${hunk.old_start}-${hunkIndex}`}>
                    <div className="px-3 py-0.5 text-accent bg-surface">
                      @@ -{hunk.old_start} +{hunk.new_start} @@
                    </div>
                    {hunk.lines.map((line, lineIndex) => (
                      <div
                        key={lineIndex}
                        className={
                          line.kind === 'add'
                            ? 'bg-success/10 text-ink'
                            : line.kind === 'del'
                              ? 'bg-error/10 text-ink'
                              : 'text-ink-2'
                        }
                      >
                        <span className="inline-block w-8 text-right pr-1 opacity-50 select-none">
                          {line.old_no ?? ''}
                        </span>
                        <span className="inline-block w-8 text-right pr-2 opacity-50 select-none">
                          {line.new_no ?? ''}
                        </span>
                        <span className="inline-block w-3 select-none">
                          {line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}
                        </span>
                        <span className="whitespace-pre-wrap break-all">{line.text}</span>
                      </div>
                    ))}
                  </div>
                ))}
                {diffFile.hunks.length === 0 && (
                  <div className="p-3 text-ink-2">
                    {t('project.noDiff', '没有可显示的差异')}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {gitPanel && git?.is_repo && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-6">
          <div className="flex flex-col w-full max-w-4xl max-h-[80vh] rounded-lg bg-surface border border-line overflow-hidden">
            <div className="flex items-center gap-2 px-3 py-2 border-b border-line">
              <GitBranch size={14} className="text-ink-2" />
              <span className="text-sm font-mono text-ink">
                {git.branch ?? 'HEAD'}
                {git.head ? ` @ ${git.head}` : ''}
              </span>
              <span className="text-[11px] text-ink-2">
                {t('project.gitChanged', `${git.files.length} 个未提交`)}
              </span>
              <button
                type="button"
                onClick={() => {
                  setGitPanel(false);
                  setGitDiff(null);
                }}
                className="ml-auto p-1 rounded text-ink-2 hover:text-ink"
              >
                <X size={16} />
              </button>
            </div>
            <div className="flex flex-1 min-h-0">
              <ul className="w-56 shrink-0 overflow-y-auto border-r border-line text-xs">
                {git.files.length === 0 && (
                  <li className="px-3 py-2 text-ink-2">
                    {t('project.gitClean', '干净')}
                  </li>
                )}
                {git.files.map((file, index) => (
                  <li key={`${file.path}-${file.staged}-${index}`}>
                    <button
                      type="button"
                      onClick={() => void openGitDiff(file.path)}
                      className={`w-full flex items-center gap-2 px-3 py-1 text-left hover:bg-surface-2 ${
                        gitDiff?.path === file.path ? 'bg-surface-2' : ''
                      }`}
                    >
                      <span className="uppercase text-[10px] text-ink-2 w-5 shrink-0">
                        {file.status}
                      </span>
                      <span className="truncate font-mono text-ink">{file.path}</span>
                      {file.staged && <span className="ml-auto text-[10px] text-success">staged</span>}
                    </button>
                  </li>
                ))}
              </ul>
              <pre className="flex-1 overflow-auto p-3 text-[11px] leading-5 font-mono bg-surface-2 text-ink whitespace-pre">
                {gitDiff
                  ? gitDiff.text
                  : t('project.gitPickFile', '选择左侧文件查看未暂存的改动')}
              </pre>
            </div>
          </div>
        </div>
      )}

      {(preview || previewError) && (
        <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-6">
          <div className="flex flex-col w-full max-w-4xl max-h-[80vh] rounded-lg bg-surface border border-line overflow-hidden">
            <div className="flex items-center gap-2 px-3 py-2 border-b border-line">
              <span className="text-sm font-mono text-ink truncate">
                {preview?.path ?? ''}
              </span>
              {preview?.truncated && (
                <span className="text-[10px] text-error shrink-0">
                  {t('project.truncated', '已截断')}
                </span>
              )}
              <button
                type="button"
                onClick={() => {
                  setPreview(null);
                  setPreviewError(null);
                }}
                className="ml-auto p-1 rounded text-ink-2 hover:text-ink"
              >
                <X size={16} />
              </button>
            </div>
            {previewError ? (
              <div className="p-4 text-sm text-error">{previewError}</div>
            ) : previewWindow ? (
              <div className="flex-1 overflow-auto bg-surface-2 font-mono text-[11px] leading-5">
                <div className="px-3 py-0.5 text-[10px] text-ink-2 bg-surface">
                  {t('project.linePreview', {
                    line: previewWindow.focus,
                    total: previewWindow.total,
                  })}
                </div>
                {previewWindow.lines.map((text, index) => {
                  const number = previewWindow.start + index + 1;
                  return (
                    <div
                      key={number}
                      className={
                        number === previewWindow.focus
                          ? 'bg-accent/15 text-ink'
                          : 'text-ink-2'
                      }
                    >
                      <span className="inline-block w-10 text-right pr-2 opacity-50 select-none">
                        {number}
                      </span>
                      <span className="whitespace-pre-wrap break-all">{text}</span>
                    </div>
                  );
                })}
              </div>
            ) : (
              <pre className="flex-1 overflow-auto p-3 text-xs font-mono text-ink bg-surface-2">
                {preview?.content}
              </pre>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
