import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import {
  Check,
  Folder,
  FolderPlus,
  History,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Search,
} from 'lucide-react'
import { apiClient } from '../../api'
import { isDesktop, pickFolder } from '../../desktop/bridge'
import { useI18n } from '../../i18n/I18nContext'
import { Button, IconButton, cn } from '../../ui'
import { formatShortDateTime } from '../../utils/datetime'
import type { SessionSummary } from '../../types/api'
import { useChatSession } from '../chat-session/ChatSessionContext'
import { AccountMenu } from './AccountMenu'
import { DaemonStatus } from './DaemonStatus'
import { MoreMenu } from './MoreMenu'
import { OPEN_FOLDER_EVENT } from './intents'
import { useShell } from './ShellContext'

/** One row of the sidebar: 28px tall, icon + label, one accent mark for the current choice. */
const ROW =
  'flex h-7 w-full items-center gap-2 rounded-control px-2 text-small transition-colors duration-fast'

/** How many recent web sessions the history column lists. */
const HISTORY_LIMIT = 8

/**
 * The left column is the conversation's context, not a product map: what folder the agent works
 * in, what came before, and a way to start over. Everything administrative lives in 「更多」 at
 * the bottom, settings in the account menu beside it — both are set-once surfaces.
 *
 * Below `lg` the same element becomes an overlay drawer, so the transcript keeps its full width
 * on a laptop screen instead of being squeezed between a rail and an inspector.
 */
export function Sidebar() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { sidebarCollapsed, sidebarOpen, toggleSidebar, setSidebarOpen, setPaletteOpen } = useShell()
  const {
    project,
    projects,
    running,
    newConversation,
    selectProject,
    openProject,
  } = useChatSession()

  const compact = sidebarCollapsed
  const [history, setHistory] = useState<SessionSummary[]>([])
  const [openFolder, setOpenFolder] = useState(false)
  const [openPath, setOpenPath] = useState('')
  const [opening, setOpening] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadHistory = useCallback(async () => {
    try {
      const page = await apiClient.getSessions(1, HISTORY_LIMIT, 'web')
      setHistory(page.items)
    } catch {
      // History is a convenience: a failed listing leaves the last one on screen.
    }
  }, [])

  useEffect(() => {
    void loadHistory()
  }, [loadHistory])

  // The palette's 「打开文件夹」 just reveals this form: the picker and the error handling stay here.
  useEffect(() => {
    const onIntent = () => {
      setSidebarOpen(true)
      setOpenFolder(true)
    }
    window.addEventListener(OPEN_FOLDER_EVENT, onIntent)
    return () => window.removeEventListener(OPEN_FOLDER_EVENT, onIntent)
  }, [setSidebarOpen])

  // A finished turn is when a new session row first exists, so refresh on the falling edge.
  const wasRunning = useRef(false)
  useEffect(() => {
    if (wasRunning.current && !running) void loadHistory()
    wasRunning.current = running
  }, [running, loadHistory])

  const submitPath = async (path: string) => {
    const trimmed = path.trim()
    if (!trimmed) return
    setOpening(true)
    setError(null)
    try {
      await openProject(trimmed)
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
          onClick={newConversation}
          title={t('shell.newChatShortcut')}
        >
          <span className={cn(compact && 'lg:hidden')}>{t('shell.newChat')}</span>
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-2">
        <SidebarLabel compact={compact}>{t('shell.workspaces')}</SidebarLabel>
        <ul className="space-y-0.5">
          <li>
            <button
              type="button"
              onClick={() => selectProject(null)}
              title={t('shell.globalWorkspace')}
              className={cn(
                ROW,
                compact && 'lg:justify-center lg:px-0',
                project === null
                  ? 'bg-surface-2 font-medium text-ink'
                  : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
              )}
            >
              <Folder className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span className={cn('min-w-0 flex-1 truncate text-left', compact && 'lg:hidden')}>
                {t('shell.globalWorkspace')}
              </span>
              {project === null && <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden="true" />}
            </button>
          </li>
          {projects.map((item) => {
            const active = project?.id === item.id
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => selectProject(item.id)}
                  title={item.path}
                  className={cn(
                    ROW,
                    compact && 'lg:justify-center lg:px-0',
                    active
                      ? 'bg-surface-2 font-medium text-ink'
                      : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
                  )}
                >
                  <Folder className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                  <span className={cn('min-w-0 flex-1 truncate text-left', compact && 'lg:hidden')}>
                    {item.name}
                  </span>
                  {active && <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden="true" />}
                </button>
              </li>
            )
          })}
          <li>
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
                  placeholder={t('project.openPathPlaceholder', '输入文件夹路径…')}
                  className="w-full rounded-control border border-line-strong bg-surface px-2 py-1 text-small text-ink placeholder:text-ink-3 focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                />
                <div className="flex items-center gap-1">
                  {isDesktop() && (
                    <IconButton label={t('project.browse', '选择文件夹…')} size="sm" onClick={handleBrowse}>
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
                    {t('shell.openFolder')}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setOpenFolder(false)}>
                    {t('common.cancel')}
                  </Button>
                </div>
                {error && <p className="text-micro text-error break-all">{error}</p>}
              </form>
            ) : (
              <button
                type="button"
                onClick={() => setOpenFolder(true)}
                title={t('shell.openFolder')}
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
          </li>
        </ul>

        <SidebarLabel compact={compact} className="mt-3">
          {t('shell.history')}
        </SidebarLabel>
        <ul className="space-y-0.5">
          {history.length === 0 ? (
            <li className={cn('px-2 py-1 text-micro text-ink-3', compact && 'lg:hidden')}>
              {t('shell.noHistory')}
            </li>
          ) : (
            history.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => navigate(`/sessions?id=${encodeURIComponent(item.id)}`)}
                  title={formatShortDateTime(item.updated_at)}
                  className={cn(ROW, compact && 'lg:justify-center lg:px-0', 'text-ink-2 hover:bg-surface-2 hover:text-ink')}
                >
                  <History className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                  <span className={cn('min-w-0 flex-1 truncate text-left', compact && 'lg:hidden')}>
                    {formatShortDateTime(item.updated_at)}
                  </span>
                  {!compact && (
                    <span className="tabular shrink-0 text-micro text-ink-3">{item.message_count}</span>
                  )}
                </button>
              </li>
            ))
          )}
        </ul>
      </div>

      <div className="shrink-0 space-y-0.5 border-t border-line p-1.5">
        <MoreMenu compact={compact} />
        <AccountMenu compact={compact} />
        <DaemonStatus compact={compact} />
      </div>
    </nav>
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
