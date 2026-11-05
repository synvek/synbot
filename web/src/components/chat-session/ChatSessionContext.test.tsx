import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { I18nProvider } from '../../i18n/I18nContext'
import type { ChatMessage } from '../../types/websocket'
import type { ProjectInfo } from '../../types/agentEvents'
import { ChatSessionProvider, useChatSession } from './ChatSessionContext'

/**
 * The socket is mocked, not exercised: this suite pins the shell contract around conversations —
 * 「新对话」 mints a session and routes to it, a route id selects that chat, and archiving the open
 * one falls back to the next conversation instead of leaving a dangling transcript.
 */
const socket = {
  connected: true,
  running: false,
  messages: [] as ChatMessage[],
  send: vi.fn(),
  cancel: vi.fn(),
  sendApprovalResponse: vi.fn(),
  resetMessages: vi.fn(),
  project: null as ProjectInfo | null,
  planVersion: 0,
  pendingPlan: null,
  planApproved: false,
  setPlanMode: vi.fn(),
  sendPlanDecision: vi.fn(),
  subagents: [],
  selectProject: vi.fn(),
}

const sockets: Array<{ url: string }> = []

vi.mock('../../hooks/useAgentSocket', () => ({
  useAgentSocket: (options: { url: string }) => {
    sockets.push({ url: options.url })
    return socket
  },
}))

const session = (chatId: string, title: string | null = null) => ({
  id: `agent:main:web:dm:${chatId}`,
  chat_id: chatId,
  channel: 'web',
  scope: 'dm',
  identifier: chatId,
  title,
  archived: false,
  pending_plan_id: null,
  plan_mode: false,
  model_override: null,
  project_id: null,
  message_count: 2,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
})

const api = {
  getSessions: vi.fn(),
  createSession: vi.fn(),
  patchSession: vi.fn(),
  archiveSession: vi.fn(),
}

vi.mock('../../api', () => ({
  apiClient: {
    wsUrl: (path: string) => `ws://test${path}`,
    getSessions: (...args: unknown[]) => api.getSessions(...args),
    createSession: (...args: unknown[]) => api.createSession(...args),
    patchSession: (...args: unknown[]) => api.patchSession(...args),
    archiveSession: (...args: unknown[]) => api.archiveSession(...args),
    getProjects: vi.fn(async () => []),
    getCommands: vi.fn(async () => []),
    openProject: vi.fn(),
    closeProject: vi.fn(),
  },
}))

vi.mock('../../desktop/bridge', () => ({
  notifyDesktop: vi.fn(),
  onOpenProject: () => () => {},
}))

function Probe() {
  const {
    newConversation,
    activeChatId,
    sessions,
    archiveSession,
    decidePlan,
    planMode,
    selectWorkspace,
    workspaceId,
  } = useChatSession()
  const location = useLocation()
  return (
    <div>
      <button type="button" onClick={() => newConversation()}>
        new
      </button>
      <button type="button" onClick={() => newConversation('p-1')}>
        new in workspace
      </button>
      <button type="button" onClick={() => newConversation(null)}>
        new in default
      </button>
      <button type="button" onClick={() => selectWorkspace('p-1')}>
        pick workspace
      </button>
      <button type="button" onClick={() => selectWorkspace(null)}>
        pick default workspace
      </button>
      <button type="button" onClick={() => void archiveSession(activeChatId ?? '')}>
        archive
      </button>
      <button type="button" onClick={() => decidePlan(true)}>
        approve plan
      </button>
      <output data-testid="path">{location.pathname}</output>
      <output data-testid="chat">{activeChatId ?? ''}</output>
      <output data-testid="planMode">{planMode ? 'on' : 'off'}</output>
      <output data-testid="workspace">{workspaceId ?? 'default'}</output>
      <output data-testid="titles">{sessions.map((item) => item.title).join('|')}</output>
    </div>
  )
}

const renderProvider = async (initialPath = '/chat') => {
  render(
    <MemoryRouter initialEntries={[initialPath]}>
      <I18nProvider>
        <ChatSessionProvider>
          <Probe />
        </ChatSessionProvider>
      </I18nProvider>
    </MemoryRouter>
  )
  // Let the provider's project/session fetches settle inside act, so the suite stays quiet.
  await act(async () => {})
}

describe('ChatSessionContext', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    sockets.length = 0
    socket.messages = []
    socket.project = null
    socket.send.mockClear()
    socket.resetMessages.mockClear()
    socket.sendPlanDecision.mockClear()
    socket.selectProject.mockClear()
    api.getSessions.mockReset()
    api.createSession.mockReset()
    api.patchSession.mockReset()
    api.archiveSession.mockReset()
    api.getSessions.mockResolvedValue({ items: [], total: 0, page: 1, page_size: 20 })
    vi.spyOn(window, 'confirm').mockReturnValue(true)
  })

  it('routes an id-less /chat to the most recent conversation', async () => {
    api.getSessions.mockResolvedValue({
      items: [session('w-aaaaaaaaaaaa', 'Latest'), session('w-bbbbbbbbbbbb', 'Older')],
      total: 2,
      page: 1,
      page_size: 20,
    })
    await renderProvider()

    await waitFor(() =>
      expect(screen.getByTestId('path').textContent).toBe('/chat/w-aaaaaaaaaaaa')
    )
    expect(screen.getByTestId('chat').textContent).toBe('w-aaaaaaaaaaaa')
  })

  it('binds the socket to the conversation in the route', async () => {
    api.getSessions.mockResolvedValue({
      items: [session('w-cccccccccccc')],
      total: 1,
      page: 1,
      page_size: 20,
    })
    await renderProvider('/chat/w-cccccccccccc')

    const last = sockets[sockets.length - 1]
    expect(last.url).toBe('ws://test/ws/agent?chat_id=w-cccccccccccc')
  })

  it('mints a new conversation instead of clearing the current one', async () => {
    socket.messages = [
      { id: '1', role: 'user', content: 'hello', timestamp: '2026-01-01T00:00:00Z' },
    ]
    api.createSession.mockResolvedValue({ chat_id: 'w-dddddddddddd', session_id: 'agent:x' })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'new' }))

    await waitFor(() => expect(api.createSession).toHaveBeenCalledTimes(1))
    expect(socket.send).not.toHaveBeenCalled()
    await waitFor(() =>
      expect(screen.getByTestId('path').textContent).toBe('/chat/w-dddddddddddd')
    )
  })

  it('drops the on-screen transcript without a confirm prompt', async () => {
    // The chat being left keeps its history in the sidebar, so starting another one is not a
    // destructive act and must not interrupt with a dialog.
    socket.messages = [
      { id: '1', role: 'user', content: 'hello', timestamp: '2026-01-01T00:00:00Z' },
    ]
    api.createSession.mockResolvedValue({ chat_id: 'w-hhhhhhhhhhhh', session_id: 'agent:x' })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'new' }))

    await waitFor(() => expect(api.createSession).toHaveBeenCalledTimes(1))
    expect(window.confirm).not.toHaveBeenCalled()
    expect(socket.resetMessages).toHaveBeenCalled()
  })

  it('mints a conversation inside the workspace whose + was pressed', async () => {
    api.createSession.mockResolvedValue({ chat_id: 'w-eeeeeeeeeeee', session_id: 'agent:x' })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'new in workspace' }))

    await waitFor(() => expect(api.createSession).toHaveBeenCalledWith(undefined, 'p-1'))
  })

  it('merges the new conversation into the workspace of the open one', async () => {
    socket.project = { id: 'p-9', name: 'repo', path: '/work/repo', added_at: '', git_head: null, rules: [] }
    api.createSession.mockResolvedValue({ chat_id: 'w-ffffffffffff', session_id: 'agent:x' })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'new' }))

    await waitFor(() => expect(api.createSession).toHaveBeenCalledWith(undefined, 'p-9'))
  })

  it('starts in the default workspace when the open chat has none', async () => {
    api.createSession.mockResolvedValue({ chat_id: 'w-gggggggggggg', session_id: 'agent:x' })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'new in default' }))

    await waitFor(() => expect(api.createSession).toHaveBeenCalledWith(undefined, null))
  })

  it('leaves the archived conversation for the next one still listed', async () => {
    const open = session('w-aaaaaaaaaaaa', 'Open')
    api.getSessions.mockResolvedValue({
      items: [open, session('w-bbbbbbbbbbbb', 'Later')],
      total: 2,
      page: 1,
      page_size: 20,
    })
    api.archiveSession.mockResolvedValue(undefined)
    await renderProvider('/chat/w-aaaaaaaaaaaa')

    fireEvent.click(screen.getByRole('button', { name: 'archive' }))

    await waitFor(() => expect(api.archiveSession).toHaveBeenCalledWith('w-aaaaaaaaaaaa'))
    await waitFor(() => expect(screen.getByTestId('path').textContent).toBe('/chat/w-bbbbbbbbbbbb'))
  })

  it('reflects plan mode from the conversation listing', async () => {
    api.getSessions.mockResolvedValue({
      items: [{ ...session('w-cccccccccccc'), plan_mode: true }],
      total: 1,
      page: 1,
      page_size: 20,
    })
    await renderProvider('/chat/w-cccccccccccc')

    expect(screen.getByTestId('planMode').textContent).toBe('on')
  })

  it('forwards a plan decision to the bound socket', async () => {
    api.getSessions.mockResolvedValue({
      items: [session('w-cccccccccccc')],
      total: 1,
      page: 1,
      page_size: 20,
    })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'approve plan' }))

    expect(socket.sendPlanDecision).toHaveBeenCalledWith(true)
  })

  it('re-pins an empty conversation to the workspace picked under the composer', async () => {
    api.getSessions.mockResolvedValue({
      items: [session('w-cccccccccccc')],
      total: 1,
      page: 1,
      page_size: 20,
    })
    await renderProvider('/chat/w-cccccccccccc')
    expect(screen.getByTestId('workspace').textContent).toBe('default')

    fireEvent.click(screen.getByRole('button', { name: 'pick workspace' }))

    expect(socket.selectProject).toHaveBeenCalledWith('p-1')
    // The listing is regrouped on the spot, so the sidebar row moves before the next fetch.
    await waitFor(() => expect(screen.getByTestId('workspace').textContent).toBe('p-1'))
  })

  it('refuses to change the workspace once the transcript has a message', async () => {
    socket.messages = [
      { id: '1', role: 'user', content: 'hello', timestamp: '2026-01-01T00:00:00Z' },
    ]
    api.getSessions.mockResolvedValue({
      items: [session('w-cccccccccccc')],
      total: 1,
      page: 1,
      page_size: 20,
    })
    await renderProvider('/chat/w-cccccccccccc')

    fireEvent.click(screen.getByRole('button', { name: 'pick workspace' }))

    expect(socket.selectProject).not.toHaveBeenCalled()
    expect(screen.getByTestId('workspace').textContent).toBe('default')
  })
})
