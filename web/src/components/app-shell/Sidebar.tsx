import { useEffect, useMemo, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import {
  Archive,
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderPlus,
  MessageSquare,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  Pencil,
  Plus,
  Search,
  Trash2,
} from 'lucide-react'
import { isDesktop, openFolderInOs, pickFolder } from '../../desktop/bridge'
import { useI18n } from '../../i18n/I18nContext'
import { Button, IconButton, MenuItem, MenuSeparator, Popover, cn } from '../../ui'
import { formatShortDateTime } from '../../utils/datetime'
import type { Project, SessionSummary } from '../../types/api'
import { useChatSession } from '../chat-session/ChatSessionContext'
import { AccountMenu } from './AccountMenu'
import { DaemonStatus } from './DaemonStatus'
import { MoreMenu } from './MoreMenu'
import { OPEN_FOLDER_EVENT } from './intents'
import { useShell } from './ShellContext'
import { groupSessionsByWorkspace, type WorkspaceGroup } from './workspaceGroups'

/** One row of the sidebar: 28px tall, icon + label, one accent mark for the current choice. */
const ROW =
  'flex h-7 w-full items-center gap-2 rounded-control px-2 text-small transition-colors duration-fast'

/** Sidebar label of a conversation: its title, or when it has none its last activity. */
function sessionLabel(item: SessionSummary, fallback: string): string {
  return item.title?.trim() || fallback
}

/** Collapse state is keyed by project id; the default workspace has no id. */
function groupKey(id: string | null): string {
  return id ?? 'default'
}

/**
 * The left column is the conversation's context, not a product map: every workspace the daemon
 * knows, with its conversations nested underneath. A conversation belongs to exactly one
 * workspace for its whole life, so there is nothing to switch — the rows are a tree, and the
 * actions that shape it (new conversation, open folder, rename, remove) sit on the folder row.
 *
 * Everything administrative lives in 「更多」 at the bottom, settings in the account menu beside
 * it — both are set-once surfaces. Below `lg` the same element becomes an overlay drawer, so the
 * transcript keeps its full width on a laptop screen.
 */
export function Sidebar() {
  const { t } = useI18n()
  const { sidebarCollapsed, sidebarOpen, toggleSidebar, setSidebarOpen, setPaletteOpen } = useShell()
  const {
    workspaceId,
    projects,
    running,
    sessions,
    activeChatId,
    newConversation,
    addWorkspace,
    renameWorkspace,
    removeWorkspace,
    selectSession,
    renameSession,
    archiveSession,
    refreshSessions,
  } = useChatSession()

  const compact = sidebarCollapsed
  const [openFolder, setOpenFolder] = useState(false)
  const [openPath, setOpenPath] = useState('')
  const [opening, setOpening] = useState(false)
  const [error, setError] = useState<string | null>(null)
  /** Conversation being renamed in place; the row turns into an input while it is set. */
  const [renamingId, setRenamingId] = useState<string | null>(null)
  const [draftTitle, setDraftTitle] = useState('')
  /** Workspace being renamed in place; the folder row turns into an input while it is set. */
  const [renamingWorkspace, setRenamingWorkspace] = useState<{ id: string; draft: string } | null>(
    null
  )
  /** Folded groups, by project id. Everything starts expanded. */
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({})

  const groups = useMemo(
    () => groupSessionsByWorkspace(projects, sessions),
    [projects, sessions]
  )

  // The palette's 「添加工作区」 just reveals this form: the picker and the error handling stay here.
  useEffect(() => {
    const onIntent = () => {
      setSidebarOpen(true)
      setOpenFolder(true)
    }
    window.addEventListener(OPEN_FOLDER_EVENT, onIntent)
    return () => window.removeEventListener(OPEN_FOLDER_EVENT, onIntent)
  }, [setSidebarOpen])

  // A finished turn is when a new session row first exists (and its title finally exists), so
  // refresh on the falling edge.
  const wasRunning = useRef(false)
  useEffect(() => {
    if (wasRunning.current && !running) void refreshSessions()
    wasRunning.current = running
  }, [running, refreshSessions])

  // Following a link into a folded workspace should reveal the conversation it opens.
  const activeProjectId = workspaceId
  useEffect(() => {
    if (!activeChatId) return
    const key = groupKey(activeProjectId)
    setCollapsedGroups((prev) => (prev[key] ? { ...prev, [key]: false } : prev))
  }, [activeChatId, activeProjectId])

  const toggleGroup = (id: string | null) => {
    const key = groupKey(id)
    setCollapsedGroups((prev) => ({ ...prev, [key]: !prev[key] }))
  }

  const startRename = (item: SessionSummary) => {
    setRenamingId(item.chat_id)
    setDraftTitle(item.title ?? '')
  }

  const submitRename = async (event?: React.FormEvent) => {
    event?.preventDefault()
    const chatId = renamingId
    if (!chatId) return
    setRenamingId(null)
    try {
      await renameSession(chatId, draftTitle)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  /** Archive is reversible (the file stays on disk), but the row does disappear: ask first. */
  const handleArchive = async (item: SessionSummary) => {
    const label = sessionLabel(item, t('shell.untitledChat', '未命名对话'))
    if (!window.confirm(t('shell.archiveConfirm', { title: label }))) return
    try {
      await archiveSession(item.chat_id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const submitWorkspaceRename = async (event?: React.FormEvent) => {
    event?.preventDefault()
    const pending = renamingWorkspace
    if (!pending) return
    setRenamingWorkspace(null)
    try {
      await renameWorkspace(pending.id, pending.draft)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  /** Removing a workspace only drops the registration: the folder and every chat file survive. */
  const handleRemoveWorkspace = async (workspace: Project) => {
    if (!window.confirm(t('shell.removeWorkspaceConfirm', { name: workspace.name }))) return
    try {
      await removeWorkspace(workspace.id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const submitPath = async (path: string) => {
    const trimmed = path.trim()
    if (!trimmed) return
    setOpening(true)
    setError(null)
    try {
      await addWorkspace(trimmed)
      setOpenPath('')
      setOpenFolder(false)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setOpening(false)
    }
  }

  const handleOpen = async (event: React.FormEvent) => {
    event.preventDefault()
    await submitPath(openPath)
  }

  /** Desktop only: the path comes from the native folder picker instead of typing. */
  const handleBrowse = async () => {
    const path = await pickFolder()
    if (path) await submitPath(path)
  }

  const renderGroup = (group: WorkspaceGroup) => {
    const key = groupKey(group.id)
    const collapsed = collapsedGroups[key] ?? false
    const isCurrent = activeProjectId === group.id
    const workspace = group.project
    const label = workspace?.name ?? t('shell.globalWorkspace')
    const renaming = workspace && renamingWorkspace?.id === workspace.id

    return (
      <li key={key}>
        {renaming ? (
          <form onSubmit={submitWorkspaceRename} className={cn('px-1 pb-1', compact && 'lg:hidden')}>
            <input
              autoFocus
              value={renamingWorkspace.draft}
              maxLength={80}
              aria-label={t('shell.renameWorkspace', '重命名工作区')}
              onChange={(event) =>
                setRenamingWorkspace({ id: renamingWorkspace.id, draft: event.target.value })
              }
              onBlur={() => void submitWorkspaceRename()}
              onKeyDown={(event) => {
                if (event.key === 'Escape') setRenamingWorkspace(null)
              }}
              placeholder={t('shell.renameWorkspacePlaceholder', '工作区名称')}
              className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 text-small text-ink placeholder:text-ink-3 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
            />
          </form>
        ) : (
          <div
            className={cn(
              'group/ws',
              ROW,
              'pr-1',
              compact && 'lg:justify-center lg:px-0',
              isCurrent ? 'text-ink' : 'text-ink-2',
              'hover:bg-surface-2'
            )}
          >
            <button
              type="button"
              onClick={() => toggleGroup(group.id)}
              aria-expanded={!collapsed}
              title={workspace?.path ?? t('shell.globalWorkspace')}
              className={cn(
                'flex min-w-0 flex-1 items-center gap-1.5 text-left',
                compact && 'lg:justify-center'
              )}
            >
              {collapsed ? (
                <ChevronRight
                  className={cn('h-3 w-3 shrink-0 text-ink-3', compact && 'lg:hidden')}
                  aria-hidden="true"
                />
              ) : (
                <ChevronDown
                  className={cn('h-3 w-3 shrink-0 text-ink-3', compact && 'lg:hidden')}
                  aria-hidden="true"
                />
              )}
              <Folder
                className={cn('h-3.5 w-3.5 shrink-0', isCurrent && 'text-accent')}
                aria-hidden="true"
              />
              <span
                className={cn(
                  'min-w-0 flex-1 truncate text-left',
                  isCurrent && 'font-medium',
                  compact && 'lg:hidden'
                )}
              >
                {label}
              </span>
              {!compact && group.sessions.length > 0 && (
                <span className="tabular shrink-0 text-micro text-ink-3 group-hover/ws:hidden">
                  {group.sessions.length}
                </span>
              )}
            </button>
            {/* Every workspace can mint a conversation inside itself; only registered folders
                have a folder to open, rename, or drop. */}
            <span className={cn('flex shrink-0 items-center gap-0.5', compact && 'lg:hidden')}>
              <IconButton
                size="sm"
                label={t('shell.newChatHere', { workspace: label })}
                onClick={() => newConversation(group.id)}
              >
                <Plus className="h-3 w-3" aria-hidden="true" />
              </IconButton>
              {workspace && (
                <WorkspaceMenu
                  workspace={workspace}
                  onRename={() =>
                    setRenamingWorkspace({ id: workspace.id, draft: workspace.name })
                  }
                  onRemove={() => void handleRemoveWorkspace(workspace)}
                />
              )}
            </span>
          </div>
        )}

        {!collapsed && (
          <ul className={cn('space-y-0.5', compact && 'lg:hidden')}>
            {group.sessions.length === 0 && group.id === null && (
              <li className={cn('px-2 pb-1 text-micro text-ink-3', compact && 'lg:hidden')}>
                {t('shell.noHistory')}
              </li>
            )}
            {group.sessions.map((item) => {
              const active = item.chat_id === activeChatId
              const itemLabel = sessionLabel(item, formatShortDateTime(item.updated_at))
              return (
                <li key={item.chat_id}>
                  {renamingId === item.chat_id ? (
                    <form
                      onSubmit={submitRename}
                      className={cn('px-1 pb-1 pl-6', compact && 'lg:hidden')}
                    >
                      <input
                        autoFocus
                        value={draftTitle}
                        maxLength={80}
                        aria-label={t('shell.renameChat', '重命名对话')}
                        onChange={(event) => setDraftTitle(event.target.value)}
                        onBlur={() => void submitRename()}
                        onKeyDown={(event) => {
                          if (event.key === 'Escape') setRenamingId(null)
                        }}
                        placeholder={t('shell.renamePlaceholder', '对话标题')}
                        className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 text-small text-ink placeholder:text-ink-3 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                      />
                    </form>
                  ) : (
                    <div
                      className={cn(
                        'group/chat relative',
                        ROW,
                        'pl-7 pr-1',
                        active
                          ? 'bg-surface-2 font-medium text-ink'
                          : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => selectSession(item.chat_id)}
                        title={itemLabel}
                        aria-current={active || undefined}
                        className="flex min-w-0 flex-1 items-center gap-2 text-left"
                      >
                        <MessageSquare className="h-3 w-3 shrink-0 text-ink-3" aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{itemLabel}</span>
                      </button>
                      {!compact && (
                        <span className="tabular shrink-0 text-micro text-ink-3 group-hover/chat:hidden">
                          {item.message_count}
                        </span>
                      )}
                      {!compact && (
                        <span className="absolute right-1 top-1/2 hidden -translate-y-1/2 items-center gap-0.5 group-hover/chat:flex">
                          <IconButton
                            size="sm"
                            className="h-6 w-6"
                            label={t('shell.renameChat', '重命名对话')}
                            onClick={() => startRename(item)}
                          >
                            <Pencil className="h-3 w-3" aria-hidden="true" />
                          </IconButton>
                          <IconButton
                            size="sm"
                            className="h-6 w-6"
                            label={t('shell.archiveChat', '归档对话')}
                            onClick={() => void handleArchive(item)}
                          >
                            <Archive className="h-3 w-3" aria-hidden="true" />
                          </IconButton>
                        </span>
                      )}
                    </div>
                  )}
                </li>
              )
            })}
          </ul>
        )}
      </li>
    )
  }

  return (
    <nav
      aria-label={t('shell.sidebar')}
      className={cn(
        'flex shrink-0 flex-col border-r border-line bg-surface',
        // Below `lg` the column is a drawer that slides over the transcript; at `lg` and above it
        // is a static column, so the translate class is cancelled out.
        'fixed inset-y-0 left-0 z-40 transition-transform duration-base ease-out',
        'lg:static lg:z-auto lg:translate-x-0',
        sidebarOpen ? 'translate-x-0 shadow-overlay' : '-translate-x-full',
        compact ? 'w-[264px] lg:w-14' : 'w-[264px]'
      )}
    >
      <div className="flex h-12 shrink-0 items-center gap-1 px-2">
        {compact && (
          <IconButton
            label={t('shell.expandSidebar')}
            aria-expanded={false}
            onClick={toggleSidebar}
            className="mx-auto hidden lg:inline-flex"
          >
            <PanelLeftOpen className="h-4 w-4" aria-hidden="true" />
          </IconButton>
        )}
        <Link
          to="/chat"
          className={cn(
            'flex min-w-0 items-center gap-2 rounded-control px-1 py-1',
            compact && 'lg:hidden'
          )}
          aria-label={t('app.title')}
        >
          <span
            aria-hidden="true"
            className="grid h-6 w-6 shrink-0 place-items-center rounded-[7px] bg-ink font-mono text-[12px] font-semibold leading-none text-background"
          >
            S
          </span>
          <span className="truncate text-body font-semibold tracking-tight text-ink">Synbot</span>
        </Link>
        <div className={cn('ml-auto flex items-center gap-0.5', compact && 'lg:hidden')}>
          <IconButton label={t('shell.search')} onClick={() => setPaletteOpen(true)}>
            <Search className="h-4 w-4" aria-hidden="true" />
          </IconButton>
          <IconButton label={t('shell.collapseSidebar')} aria-expanded onClick={toggleSidebar}>
            <PanelLeftClose className="h-4 w-4" aria-hidden="true" />
          </IconButton>
        </div>
      </div>

      <div className={cn('px-2 pb-1 pt-1', compact && 'lg:px-1.5')}>
        <Button
          variant="primary"
          className={cn('w-full', compact && 'lg:px-0')}
          icon={<Plus className="h-3.5 w-3.5" aria-hidden="true" />}
          onClick={() => newConversation()}
          title={t('shell.newChatShortcut')}
        >
          <span className={cn(compact && 'lg:hidden')}>{t('shell.newChat')}</span>
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-2">
        <SidebarLabel compact={compact}>{t('shell.workspaces')}</SidebarLabel>
        <ul className="space-y-0.5">{groups.map(renderGroup)}</ul>

        <div className="mt-1">
          {openFolder ? (
            <form onSubmit={handleOpen} className={cn('space-y-1 px-1 pb-1', compact && 'lg:hidden')}>
              <label className="sr-only" htmlFor="sidebar-open-path">
                {t('shell.openFolder')}
              </label>
              <input
                id="sidebar-open-path"
                autoFocus
                value={openPath}
                onChange={(event) => setOpenPath(event.target.value)}
                placeholder={t('project.openPathPlaceholder', '工作区文件夹路径…')}
                className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 text-small text-ink placeholder:text-ink-3 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
              />
              <div className="flex items-center gap-1">
                {isDesktop() && (
                  <IconButton
                    label={t('project.browse', '选择文件夹…')}
                    size="sm"
                    onClick={handleBrowse}
                  >
                    <Folder className="h-3.5 w-3.5" aria-hidden="true" />
                  </IconButton>
                )}
                <Button
                  type="submit"
                  size="sm"
                  variant="secondary"
                  loading={opening}
                  disabled={!openPath.trim()}
                  className="flex-1"
                >
                  {t('shell.addWorkspace', '添加')}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setOpenFolder(false)}>
                  {t('common.cancel')}
                </Button>
              </div>
              {error && <p className="break-all text-micro text-error">{error}</p>}
            </form>
          ) : (
            <button
              type="button"
              onClick={() => setOpenFolder(true)}
              title={t('shell.openFolderHint', '把本地文件夹加入工作区列表，再用它的 + 新建对话')}
              className={cn(
                ROW,
                compact && 'lg:justify-center lg:px-0',
                'text-ink-2 hover:bg-surface-2 hover:text-ink'
              )}
            >
              <FolderPlus className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span className={cn('min-w-0 flex-1 truncate text-left', compact && 'lg:hidden')}>
                {t('shell.openFolder')}
              </span>
            </button>
          )}
        </div>
      </div>

      <div className="shrink-0 space-y-0.5 border-t border-line p-1.5">
        <MoreMenu compact={compact} />
        <AccountMenu compact={compact} />
        <DaemonStatus compact={compact} />
      </div>
    </nav>
  )
}

/** The `⋯` menu of a workspace row: reveal, rename, or drop it from the list. */
function WorkspaceMenu({
  workspace,
  onRename,
  onRemove,
}: {
  workspace: Project
  onRename: () => void
  onRemove: () => void
}) {
  const { t } = useI18n()
  const desktop = isDesktop()

  return (
    <Popover
      label={t('shell.workspaceMenu', '工作区操作')}
      align="start"
      chevron={false}
      size="sm"
      closeOnSelect
      icon={<MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
    >
      <MenuItem
        icon={<FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />}
        disabled={!desktop}
        onSelect={() => void openFolderInOs(workspace.path)}
        aside={
          desktop ? undefined : <span className="text-micro text-ink-3">{t('shell.desktopOnly')}</span>
        }
      >
        {t('shell.openFolderMenu', '打开文件夹…')}
      </MenuItem>
      <MenuSeparator />
      <MenuItem icon={<Pencil className="h-3.5 w-3.5" aria-hidden="true" />} onSelect={onRename}>
        {t('shell.renameWorkspace', '重命名')}
      </MenuItem>
      <MenuItem
        danger
        icon={<Trash2 className="h-3.5 w-3.5" aria-hidden="true" />}
        onSelect={onRemove}
      >
        {t('shell.removeWorkspace', '从列表删除')}
      </MenuItem>
    </Popover>
  )
}

function SidebarLabel({
  children,
  compact,
  className,
}: {
  children: React.ReactNode
  compact: boolean
  className?: string
}) {
  return (
    <div
      className={cn(
        'px-2 pb-1 pt-1 text-[11px] font-medium uppercase tracking-wide text-ink-3',
        compact && 'lg:hidden',
        className
      )}
    >
      {children}
    </div>
  )
}
