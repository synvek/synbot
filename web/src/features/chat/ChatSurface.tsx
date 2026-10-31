import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { MessagesSquare, PanelRight } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { IconButton, SegmentedControl, StatusDot } from '../../ui'
import type { CommandInfo } from '../../types/api'
import type { ChatMessage } from '../../types/websocket'
import type { ProjectInfo, SubagentInfo } from '../../types/agentEvents'
import ProjectPanel from '../../components/ProjectPanel'
import { useShell } from '../../components/app-shell/ShellContext'
import { Composer } from './Composer'
import { MessageStream } from './MessageStream'
import { Inspector } from './Inspector'

// xterm is ~340KB: load it only when the terminal tab is opened for the first time.
const TerminalPanel = lazy(() => import('../../components/TerminalPanel'))
const TeamPanel = lazy(() => import('../../components/TeamPanel'))

type View = 'chat' | 'terminal' | 'team'

interface ChatSurfaceProps {
  connected: boolean
  running: boolean
  messages: ChatMessage[]
  project: ProjectInfo | null
  planVersion: number
  subagents: SubagentInfo[]
  commands: CommandInfo[]
  onSend: (text: string) => void
  onStop: () => void
  onApprove: (requestId: string, approved: boolean) => void
  onSelectProject: (projectId: string | null) => void
}

/**
 * The workbench: transcript in the middle, tabs for the shell the agent is running in, and one
 * panel on the right that owns everything file-shaped. Nothing here knows the WebSocket protocol;
 * it renders what the page hands it.
 */
export function ChatSurface({
  connected,
  running,
  messages,
  project,
  planVersion,
  subagents,
  commands,
  onSend,
  onStop,
  onApprove,
  onSelectProject,
}: ChatSurfaceProps) {
  const { t } = useI18n()
  const { focusMode } = useShell()
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

  // ⌘B / Ctrl+B folds the workspace panel; it is the only chord the workbench owns.
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
        <div className="flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">
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
          {running && (
            <span className="flex items-center gap-1.5 text-micro text-ink-2">
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
          <div className="ml-auto flex min-w-0 items-center gap-2">
            {project && (
              <span className="hidden truncate font-mono text-micro text-ink-3 sm:block" title={project.path}>
                {project.name}
              </span>
            )}
            {!focusMode && (
              <IconButton
                size="sm"
                label={
                  inspectorOpen
                    ? t('chat.hideInspector', 'Hide workspace panel')
                    : t('chat.showInspector', 'Show workspace panel')
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
            hasProject={!!project}
            onApprove={(requestId) => onApprove(requestId, true)}
            onReject={(requestId) => onApprove(requestId, false)}
            onOpenFile={openFile}
          />
          <Composer
            connected={connected}
            running={running}
            commands={commands}
            onSend={onSend}
            onStop={onStop}
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
        <Inspector label={t('chat.workspacePanel', 'Workspace panel')}>
          <ProjectPanel
            project={project}
            running={running}
            changedFiles={lastTurnFiles}
            turnId={lastTurnId}
            focusFile={focusFile}
            planVersion={planVersion}
            onSelect={onSelectProject}
          />
        </Inspector>
      )}
    </div>
  )
}
