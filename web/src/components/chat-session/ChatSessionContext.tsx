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
import { useLocation, useNavigate } from 'react-router-dom'
import { apiClient } from '../../api'
import { notifyDesktop, onOpenProject } from '../../desktop/bridge'
import { useAgentSocket } from '../../hooks/useAgentSocket'
import { useI18n } from '../../i18n/I18nContext'
import type { CommandInfo, ModelOverride, Project, SessionSummary } from '../../types/api'
import type { PendingPlan, ProjectInfo, SubagentInfo } from '../../types/agentEvents'
import type { ChatMessage } from '../../types/websocket'

/** How many past conversations the sidebar lists. */
const RECENT_LIMIT = 20

interface ChatSessionValue {
  /** Chat the socket is bound to (`null` until the first conversation resolves). */
  activeChatId: string | null
  /** Recent web conversations, newest first — the sidebar's history column. */
  sessions: SessionSummary[]
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
  /** Model pinned to the open conversation (`null` = the configured default). */
  modelOverride: ModelOverride | null
  /** Pin a model to the open chat, or drop the pin with `null`. */
  setModelOverride: (model: ModelOverride | null) => Promise<void>
  /** Plan of the open conversation waiting for a decision, if any. */
  pendingPlan: PendingPlan | null
  /** The plan was approved and its turn is spinning up. */
  planApproved: boolean
  /** The open conversation plans before it writes. */
  planMode: boolean
  /** Arm or disarm plan mode for the next turn. */
  setPlanMode: (enabled: boolean) => void
  /** Run the pending plan, or drop it without executing anything. */
  decidePlan: (approve: boolean) => void
  send: (content: string) => void
  cancel: () => void
  approve: (requestId: string, approved: boolean) => void
  selectProject: (projectId: string | null) => void
  /** Register a folder (if needed) and bind the conversation to it. */
  openProject: (path: string) => Promise<Project>
  closeProject: (projectId: string) => Promise<void>
  refreshProjects: () => Promise<void>
  refreshSessions: () => Promise<void>
  /** Switch to another conversation; the socket reconnects and replays its history. */
  selectSession: (chatId: string) => void
  /** Mint a conversation, then navigate to it. Resolves with the new chat id. */
  createSession: (title?: string) => Promise<string>
  renameSession: (chatId: string, title: string) => Promise<void>
  /** Hide a conversation from the lists; its history stays on disk. */
  archiveSession: (chatId: string) => Promise<void>
  /** Start over in a brand-new conversation. */
  newConversation: () => void
}

const ChatSessionContext = createContext<ChatSessionValue | undefined>(undefined)

export function useChatSession() {
  const context = useContext(ChatSessionContext)
  if (!context) throw new Error('useChatSession must be used within a ChatSessionProvider')
  return context
}

/** `/chat/:chatId` → chat id. Admin routes return `null` and leave the active chat alone. */
function chatIdFromPath(pathname: string): string | null {
  const match = /^\/chat\/([^/]+)\/?$/.exec(pathname)
  return match ? decodeURIComponent(match[1]) : null
}

/**
 * Owns the console's conversations: one live socket, one list of recent chats, and the routing
 * that ties them together.
 *
 * It lives in the shell rather than on the chat page for two reasons: the sidebar issues
 * conversation commands (new conversation, switch chat, switch workspace) on every route, and a
 * turn that finishes while the operator is reading a log still has to raise its desktop
 * notification. The route is the source of truth for *which* chat is open, but the id is kept in
 * state so leaving `/chat` for an admin page does not tear the socket down mid-turn.
 */
export function ChatSessionProvider({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  const navigate = useNavigate()
  const location = useLocation()

  const routeChatId = useMemo(() => chatIdFromPath(location.pathname), [location.pathname])
  const [activeChatId, setActiveChatId] = useState<string | null>(routeChatId)

  useEffect(() => {
    if (routeChatId) setActiveChatId(routeChatId)
  }, [routeChatId])

  // Same-origin on the web, absolute loopback URL plus the desktop token in the app. The chat id
  // rides along as a query parameter because the server has to bind events and history before the
  // first `hello` — see the protocol note in `src/web/handlers/ws_agent.rs`.
  const wsUrl = useMemo(
    () =>
      apiClient.wsUrl(
        activeChatId ? `/ws/agent?chat_id=${encodeURIComponent(activeChatId)}` : '/ws/agent'
      ),
    [activeChatId]
  )

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
    pendingPlan,
    planApproved,
    setPlanMode: sendPlanMode,
    sendPlanDecision,
    subagents,
    selectProject,
  } = useAgentSocket({ url: wsUrl, autoConnect: activeChatId !== null })

  const [commands, setCommands] = useState<CommandInfo[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  /** Distinguishes "no conversations yet" from "the first listing has not landed". */
  const [sessionsLoaded, setSessionsLoaded] = useState(false)
  /** A pin chosen here but not yet reflected by the listing, scoped to the chat it was made in. */
  const [pendingPin, setPendingPin] = useState<{
    chatId: string
    model: ModelOverride | null
  } | null>(null)

  const storedModel = sessions.find((item) => item.chat_id === activeChatId)?.model_override ?? null
  const modelOverride = pendingPin && pendingPin.chatId === activeChatId ? pendingPin.model : storedModel

  /**
   * Plan mode is a per-conversation switch the daemon owns (it clears it once a plan is proposed),
   * so the pill shows a local pin, then the listing, and never a stale `true` next to a plan card.
   */
  const [planModePin, setPlanModePin] = useState<{ chatId: string; enabled: boolean } | null>(null)
  const storedPlanMode = sessions.find((item) => item.chat_id === activeChatId)?.plan_mode ?? false
  const planMode = pendingPlan
    ? false
    : planModePin && planModePin.chatId === activeChatId
      ? planModePin.enabled
      : storedPlanMode

  // Proposing the plan is what ends plan mode server-side; follow it here as well so the pill and
  // the card never disagree.
  useEffect(() => {
    if (!pendingPlan || !activeChatId) return
    setPlanModePin({ chatId: activeChatId, enabled: false })
  }, [pendingPlan, activeChatId])

  const setPlanMode = useCallback(
    (enabled: boolean) => {
      if (!activeChatId || !connected) return
      setPlanModePin({ chatId: activeChatId, enabled })
      sendPlanMode(enabled)
    },
    [activeChatId, connected, sendPlanMode]
  )

  const refreshProjects = useCallback(async () => {
    try {
      setProjects(await apiClient.getProjects())
    } catch {
      // A failed listing keeps the previous one: the sidebar still shows what it knew.
    }
  }, [])

  const refreshSessions = useCallback(async () => {
    try {
      setSessions((await apiClient.getSessions(1, RECENT_LIMIT, 'web')).items)
    } catch {
      // Same contract as projects: the last known list stays on screen.
    } finally {
      setSessionsLoaded(true)
    }
  }, [])

  const createSession = useCallback(
    async (title?: string) => {
      const created = await apiClient.createSession(title)
      setActiveChatId(created.chat_id)
      navigate(`/chat/${encodeURIComponent(created.chat_id)}`)
      await refreshSessions()
      return created.chat_id
    },
    [navigate, refreshSessions]
  )

  const selectSession = useCallback(
    (chatId: string) => {
      // Changing the id swaps the socket URL: the old socket closes, the new one seeds history and
      // replays from seq 0. A turn left behind keeps running server-side.
      if (chatId !== activeChatId) setActiveChatId(chatId)
      navigate(`/chat/${encodeURIComponent(chatId)}`)
    },
    [activeChatId, navigate]
  )

  const renameSession = useCallback(
    async (chatId: string, title: string) => {
      const trimmed = title.trim()
      if (!trimmed) return
      setSessions((prev) =>
        prev.map((item) => (item.chat_id === chatId ? { ...item, title: trimmed } : item))
      )
      try {
        await apiClient.patchSession(chatId, { title: trimmed })
      } finally {
        await refreshSessions()
      }
    },
    [refreshSessions]
  )

  const setModelOverride = useCallback(
    async (model: ModelOverride | null) => {
      if (!activeChatId) return
      const chatId = activeChatId
      // The chip updates on the spot: the pin is what the *next* turn reads, so a list refetch
      // racing behind it must not flip the label back for a frame.
      setPendingPin({ chatId, model })
      try {
        await apiClient.patchSession(
          chatId,
          model ? { model_override: model } : { clear_model_override: true }
        )
      } catch (error) {
        console.error('Failed to pin the conversation model:', error)
        setPendingPin(null)
        return
      }
      await refreshSessions()
      setPendingPin((current) =>
        current && current.chatId === chatId && current.model === model ? null : current
      )
    },
    [activeChatId, refreshSessions]
  )

  const archiveSession = useCallback(
    async (chatId: string) => {
      await apiClient.archiveSession(chatId)
      const remaining = sessions.filter((item) => item.chat_id !== chatId)
      setSessions(remaining)
      if (chatId === activeChatId) {
        // The open conversation just disappeared from the list: land on the next one instead of
        // leaving the transcript bound to a chat the sidebar no longer shows.
        if (remaining.length > 0) selectSession(remaining[0].chat_id)
        else await createSession()
      }
      await refreshSessions()
    },
    [activeChatId, createSession, refreshSessions, selectSession, sessions]
  )

  // Landing on `/chat` without an id resolves to the most recent conversation, minting one on a
  // fresh install. This is also what makes a reload keep the operator where they were when the
  // route had no id to remember.
  const creatingRef = useRef(false)
  const onChatIndex = location.pathname === '/chat' || location.pathname === '/chat/'
  useEffect(() => {
    if (!onChatIndex || !sessionsLoaded) return
    if (sessions.length > 0) {
      navigate(`/chat/${encodeURIComponent(sessions[0].chat_id)}`, { replace: true })
      return
    }
    if (creatingRef.current) return
    creatingRef.current = true
    void createSession()
      .catch((error) => console.error('Failed to create a conversation:', error))
      .finally(() => {
        creatingRef.current = false
      })
  }, [onChatIndex, sessionsLoaded, sessions, navigate, createSession])

  useEffect(() => {
    void refreshProjects()
    void refreshSessions()
  }, [refreshProjects, refreshSessions])

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

  // A new conversation is a new chat id, so nothing has to be cleared on the daemon: the old one
  // keeps its history and stays one click away in the sidebar.
  const newConversation = useCallback(() => {
    if (messages.length > 0 && !window.confirm(t('shell.newChatConfirm'))) return
    // The transcript belongs to the chat being left; dropping it now avoids a frame of the old
    // conversation under the new URL while the socket reconnects.
    resetMessages()
    void createSession().catch((error) => console.error('Failed to create a conversation:', error))
  }, [createSession, messages.length, resetMessages, t])

  // The desktop tray's "Add Workspace…" sends the folder it picked as a DOM event.
  useEffect(() => onOpenProject((path) => void openProject(path)), [openProject])

  // The pin rides along on the message, so a switch made mid-conversation applies to the very
  // next turn without waiting for the session list to catch up.
  const sendWithModel = useCallback(
    (content: string) => send(content, modelOverride),
    [send, modelOverride]
  )

  const value = useMemo<ChatSessionValue>(
    () => ({
      activeChatId,
      sessions,
      connected,
      running,
      messages,
      project,
      planVersion,
      subagents,
      commands,
      projects,
      modelOverride,
      setModelOverride,
      pendingPlan,
      planApproved,
      planMode,
      setPlanMode,
      decidePlan: sendPlanDecision,
      send: sendWithModel,
      cancel,
      approve: sendApprovalResponse,
      selectProject,
      openProject,
      closeProject,
      refreshProjects,
      refreshSessions,
      selectSession,
      createSession,
      renameSession,
      archiveSession,
      newConversation,
    }),
    [
      activeChatId,
      sessions,
      connected,
      running,
      messages,
      project,
      planVersion,
      subagents,
      commands,
      projects,
      modelOverride,
      setModelOverride,
      pendingPlan,
      planApproved,
      planMode,
      setPlanMode,
      sendPlanDecision,
      sendWithModel,
      cancel,
      sendApprovalResponse,
      selectProject,
      openProject,
      closeProject,
      refreshProjects,
      refreshSessions,
      selectSession,
      createSession,
      renameSession,
      archiveSession,
      newConversation,
    ]
  )

  return <ChatSessionContext.Provider value={value}>{children}</ChatSessionContext.Provider>
}
