import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { apiClient } from '../../api'
import { notifyDesktop, onOpenProject } from '../../desktop/bridge'
import { useAgentSocket } from '../../hooks/useAgentSocket'
import { useI18n } from '../../i18n/I18nContext'
import type { CommandInfo, Project } from '../../types/api'
import type { ProjectInfo, SubagentInfo } from '../../types/agentEvents'
import type { ChatMessage } from '../../types/websocket'

interface ChatSessionValue {
  connected: boolean
  running: boolean
  messages: ChatMessage[]
  /** Folder the agent works in right now (`null` = the global workspace). */
  project: ProjectInfo | null
  planVersion: number
  subagents: SubagentInfo[]
  /** Workspace slash commands for the composer picker. */
  commands: CommandInfo[]
  /** Folders registered with the daemon, for the sidebar's workspace list. */
  projects: Project[]
  send: (content: string) => void
  cancel: () => void
  approve: (requestId: string, approved: boolean) => void
  selectProject: (projectId: string | null) => void
  /** Register a folder (if needed) and bind the conversation to it. */
  openProject: (path: string) => Promise<Project>
  closeProject: (projectId: string) => Promise<void>
  refreshProjects: () => Promise<void>
  /**
   * Start over: `/clear` on the daemon (which drops the session history and workflow state)
   * plus a local transcript reset, so the next message opens a fresh conversation.
   */
  newConversation: () => void
}

const ChatSessionContext = createContext<ChatSessionValue | undefined>(undefined)

export function useChatSession() {
  const context = useContext(ChatSessionContext)
  if (!context) throw new Error('useChatSession must be used within a ChatSessionProvider')
  return context
}

/**
 * Owns the one agent conversation the console has.
 *
 * It lives in the shell rather than on the chat page for two reasons: the sidebar issues
 * conversation commands (new conversation, switch workspace) on every route, and a turn that
 * finishes while the operator is reading a log still has to raise its desktop notification.
 * The socket itself is untouched — `useAgentSocket` is the same hook the chat page always used.
 */
export function ChatSessionProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  // Same-origin on the web, absolute loopback URL plus the desktop token in the app.
  const wsUrl = useMemo(() => apiClient.wsUrl('/ws/agent'), [])
  const {
    connected,
    running,
    messages,
    send,
    cancel,
    sendApprovalResponse,
    resetMessages,
    project,
    planVersion,
    subagents,
    selectProject,
  } = useAgentSocket({ url: wsUrl, autoConnect: true })

  const [commands, setCommands] = useState<CommandInfo[]>([])
  const [projects, setProjects] = useState<Project[]>([])

  const refreshProjects = useCallback(async () => {
    try {
      setProjects(await apiClient.getProjects())
    } catch {
      // A failed listing keeps the previous one: the sidebar still shows what it knew.
    }
  }, [])

  useEffect(() => {
    void refreshProjects()
  }, [refreshProjects])

  // Workspace commands follow the selected folder: switching projects rescans `.synbot/commands`.
  useEffect(() => {
    let active = true
    apiClient
      .getCommands(project?.id)
      .then((list) => {
        if (active) setCommands(list)
      })
      .catch(() => {
        if (active) setCommands([])
      })
    return () => {
      active = false
    }
  }, [project])

  /**
   * Desktop only: a turn that finishes while the window is in the background raises a system
   * notification, so long runs do not have to be watched.
   */
  const wasRunning = useRef(false)
  useEffect(() => {
    if (wasRunning.current && !running && document.hidden) {
      const lastReply = [...messages].reverse().find((message) => message.role === 'assistant')
      const preview = (lastReply?.content ?? '').replace(/\s+/g, ' ').trim().slice(0, 200)
      void notifyDesktop(
        t('desktop.turnFinishedTitle', 'Synbot 已完成'),
        preview || t('desktop.turnFinishedBody', '本轮已结束，回到 Synbot 查看结果。')
      )
    }
    wasRunning.current = running
  }, [running, messages, t])

  const openProject = useCallback(
    async (rawPath: string) => {
      const opened = await apiClient.openProject(rawPath.trim())
      await refreshProjects()
      selectProject(opened.id)
      return opened
    },
    [refreshProjects, selectProject]
  )

  const closeProject = useCallback(
    async (projectId: string) => {
      await apiClient.closeProject(projectId)
      await refreshProjects()
      selectProject(null)
    },
    [refreshProjects, selectProject]
  )

  // `/clear` also abandons the running turn, so a busy conversation is safe to reset.
  const newConversation = useCallback(() => {
    if (messages.length > 0 && !window.confirm(t('shell.newChatConfirm'))) return
    send('/clear')
    resetMessages()
  }, [messages.length, resetMessages, send, t])

  // The desktop tray's "Open Project…" sends the folder it picked as a DOM event.
  useEffect(() => onOpenProject((path) => void openProject(path)), [openProject])

  const value = useMemo<ChatSessionValue>(
    () => ({
      connected,
      running,
      messages,
      project,
      planVersion,
      subagents,
      commands,
      projects,
      send,
      cancel,
      approve: sendApprovalResponse,
      selectProject,
      openProject,
      closeProject,
      refreshProjects,
      newConversation,
    }),
    [
      connected,
      running,
      messages,
      project,
      planVersion,
      subagents,
      commands,
      projects,
      send,
      cancel,
      sendApprovalResponse,
      selectProject,
      openProject,
      closeProject,
      refreshProjects,
      newConversation,
    ]
  )

  return <ChatSessionContext.Provider value={value}>{children}</ChatSessionContext.Provider>
}
