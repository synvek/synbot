import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { MessagesSquare, PanelLeft, PanelRight } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { IconButton, SegmentedControl, StatusDot } from '../../ui'
import type { CommandInfo, ModelOverride, Project } from '../../types/api'
import type { ChatMessage } from '../../types/websocket'
import type { PendingPlan, ProjectInfo, SubagentInfo } from '../../types/agentEvents'
import ProjectPanel from '../../components/ProjectPanel'
import { useShell } from '../../components/app-shell/ShellContext'
import { Composer } from './Composer'
import { MessageStream } from './MessageStream'
import { Inspector } from './Inspector'
import { ModelChip } from './ModelChip'

// xterm is ~340KB: load it only when the terminal tab is opened for the first time.
const TerminalPanel = lazy(() => import('../../components/TerminalPanel'))
const TeamPanel = lazy(() => import('../../components/TeamPanel'))

type View = 'chat' | 'terminal' | 'team'

interface ChatSurfaceProps {
  connected: boolean
  running: boolean
  messages: ChatMessage[]
  project: ProjectInfo | null
  /** Workspace id of the open conversation (`null` = the default workspace). */
  workspaceId: string | null
  /** Registered folders, so a renamed workspace shows its new label. */
  projects: Project[]
  /** Model pinned to this conversation, for the chip next to the workspace. */
  model: ModelOverride | null
  /** Plan-mode proposal waiting for a decision (`null` when none). */
  pendingPlan: PendingPlan | null
  /** The plan was approved and its turn is starting. */
  planApproved: boolean
  /** The next turn plans before it writes. */
  planMode: boolean
  planVersion: number
  subagents: SubagentInfo[]
  commands: CommandInfo[]
  onSend: (text: string) => void
  onStop: () => void
  onApprove: (requestId: string, approved: boolean) => void
  onDecidePlan: (approve: boolean) => void
  onTogglePlanMode: (enabled: boolean) => void
  onSelectModel: (model: ModelOverride | null) => void
  /** Re-pin the conversation's workspace; the composer only offers it while it is empty. */
  onSelectWorkspace: (projectId: string | null) => void
}

/**
 * The workbench: transcript in the middle, tabs for the shell the agent is running in, and one
 * panel on the right that owns everything file-shaped. Its header is now the app's only bar —
 * the view switch and the panel toggle live there. The conversation's workspace is not a control
 * in that bar: it lives under the composer, where it is a picker until the first message and a
 * caption after that. Nothing here knows the WebSocket protocol; it renders what the page hands it.
 */
export function ChatSurface({
  connected,
  running,
  messages,
  project,
  workspaceId,
  projects,
  model,
  pendingPlan,
  planApproved,
  planMode,
  planVersion,
  subagents,
  commands,
  onSend,
  onStop,
  onApprove,
  onDecidePlan,
  onTogglePlanMode,
  onSelectModel,
  onSelectWorkspace,
}: ChatSurfaceProps) {
  const { t } = useI18n()
  const { focusMode, setSidebarOpen } = useShell()
  const [view, setView] = useState<View>('chat')
  const [terminalMounted, setTerminalMounted] = useState(false)
  const [teamMounted, setTeamMounted] = useState(false)
  const [inspectorOpen, setInspectorOpen] = useState(true)
  /** File the user clicked in a turn summary; the project panel opens its diff. */
  const [focusFile, setFocusFile] = useState<{ turnId: string; path: string } | null>(null)

  /** Files reported by the most recent finished turn (`file_changed` events). */
  const lastTurnFiles = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i]
      if (message.role === 'turn') return message.turnFiles ?? []
    }
    return []
  }, [messages])

  /** Id of the most recent finished turn — the key for its checkpoint (diff + undo). */
  const lastTurnId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i]
      if (message.role === 'turn') return message.turnId ?? null
    }
    return null
  }, [messages])

  /** Teammates still working — shown next to the team tab so a background fan-out is visible. */
  const runningTeammates = useMemo(
    () => subagents.filter((agent) => agent.state === 'running').length,
    [subagents]
  )

  /**
   * Current workspace, resolved through the registered list so a rename shows up without a
   * reconnect — and through the id rather than the socket's echo, so a pick in the composer is
   * reflected the moment it is made.
   */
  const workspace = useMemo(() => {
    if (!workspaceId) return null
    const registered = projects.find((item) => item.id === workspaceId)
    if (registered) return { name: registered.name, path: registered.path }
    return project && project.id === workspaceId
      ? { name: project.name, path: project.path }
      : null
  }, [workspaceId, project, projects])

  // ⌘B / Ctrl+B folds the files panel; it is the only chord the workbench owns.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'b') {
        event.preventDefault()
        setInspectorOpen((open) => !open)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const openFile = (turnId: string, path: string) => {
    setFocusFile({ turnId, path })
  }

  return (
    <div className="flex h-full min-h-0">
      {/* The workbench has no visible page title; screen readers still need one. */}
      <h1 className="sr-only">{t('sidebar.chat', 'Chat')}</h1>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-3">
          {/* Below lg the sidebar is a drawer, so this is where it is opened from. */}
          <IconButton
            className="lg:hidden"
            size="sm"
            label={t('shell.openSidebar')}
            onClick={() => setSidebarOpen(true)}
          >
            <PanelLeft className="h-4 w-4" aria-hidden="true" />
          </IconButton>
          <SegmentedControl<View>
            ariaLabel={t('chat.views', 'Views')}
            value={view}
            onChange={(next) => {
              if (next === 'terminal') setTerminalMounted(true)
              if (next === 'team') setTeamMounted(true)
              setView(next)
            }}
            options={[
              { value: 'chat', label: t('chat.tabChat', 'Chat') },
              { value: 'terminal', label: t('chat.tabTerminal', 'Terminal') },
              { value: 'team', label: t('chat.tabTeam', 'Team') },
            ]}
          />
          <div className="ml-auto flex min-w-0 items-center gap-1">
            {running && (
              <span className="mr-1 hidden items-center gap-1.5 text-micro text-ink-2 sm:flex">
                <StatusDot tone="running" />
                {t('chat.toolRunning', 'running')}
              </span>
            )}
            {runningTeammates > 0 && view !== 'team' && (
              <span className="tabular flex items-center gap-1.5 text-micro text-ink-3">
                <MessagesSquare className="h-3 w-3" aria-hidden="true" />
                {runningTeammates}
              </span>
            )}
            <ModelChip value={model} running={running} onChange={onSelectModel} />
            {!focusMode && (
              <IconButton
                size="sm"
                // Below `lg` the inspector is hidden altogether, so the toggle would be dead.
                className="hidden lg:inline-flex"
                label={
                  inspectorOpen
                    ? t('chat.hideInspector', 'Hide files panel')
                    : t('chat.showInspector', 'Show files panel')
                }
                aria-pressed={inspectorOpen}
                onClick={() => setInspectorOpen((open) => !open)}
              >
                <PanelRight className="h-4 w-4" aria-hidden="true" />
              </IconButton>
            )}
          </div>
        </div>

        {!connected && (
          <div className="flex shrink-0 items-center gap-2 border-b border-line bg-surface-2 px-3 py-1 text-micro text-ink-2">
            <StatusDot tone="error" />
            {t('chat.offline', 'Connection lost — reconnecting to the daemon')}
          </div>
        )}

        <div className={view === 'chat' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
          <MessageStream
            messages={messages}
            connected={connected}
            hasProject={workspaceId !== null}
            pendingPlan={pendingPlan}
            planApproved={planApproved}
            onApprove={(requestId) => onApprove(requestId, true)}
            onReject={(requestId) => onApprove(requestId, false)}
            onDecidePlan={onDecidePlan}
            onOpenFile={openFile}
          />
          <Composer
            connected={connected}
            running={running}
            commands={commands}
            planMode={planMode}
            onTogglePlanMode={onTogglePlanMode}
            onSend={onSend}
            onStop={onStop}
            workspace={workspace}
            workspaceId={workspaceId}
            workspaces={projects}
            showWorkspace={messages.length === 0}
            onSelectWorkspace={onSelectWorkspace}
          />
        </div>

        {/* Mounted on first use and only hidden afterwards, so switching views keeps the shell. */}
        <div className={view === 'terminal' ? 'flex min-h-0 flex-1' : 'hidden'}>
          {terminalMounted && (
            <Suspense
              fallback={<div className="p-3 text-micro text-ink-3">{t('common.loading', 'Loading...')}</div>}
            >
              <TerminalPanel
                projectId={project?.id ?? null}
                active={view === 'terminal'}
                onClose={() => setView('chat')}
              />
            </Suspense>
          )}
        </div>

        <div className={view === 'team' ? 'flex min-h-0 flex-1' : 'hidden'}>
          {teamMounted && (
            <Suspense
              fallback={<div className="p-3 text-micro text-ink-3">{t('common.loading', 'Loading...')}</div>}
            >
              <TeamPanel live={subagents} />
            </Suspense>
          )}
        </div>
      </div>

      {inspectorOpen && !focusMode && (
        <Inspector label={t('chat.workspacePanel', 'Files panel')}>
          <ProjectPanel
            project={project}
            running={running}
            changedFiles={lastTurnFiles}
            turnId={lastTurnId}
            focusFile={focusFile}
            planVersion={planVersion}
            onUndone={() => setFocusFile(null)}
          />
        </Inspector>
      )}
    </div>
  )
}
