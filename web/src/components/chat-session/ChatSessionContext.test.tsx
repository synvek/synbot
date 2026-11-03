import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { I18nProvider } from '../../i18n/I18nContext'
import type { ChatMessage } from '../../types/websocket'
import { ChatSessionProvider, useChatSession } from './ChatSessionContext'

/**
 * The socket is mocked, not exercised: this suite pins the one behaviour the shell depends on —
 * 「新对话」 goes through `/clear` on the daemon and then drops the local transcript.
 */
const socket = {
  connected: true,
  running: false,
  messages: [] as ChatMessage[],
  send: vi.fn(),
  cancel: vi.fn(),
  sendApprovalResponse: vi.fn(),
  resetMessages: vi.fn(),
  project: null,
  planVersion: 0,
  subagents: [],
  selectProject: vi.fn(),
}

vi.mock('../../hooks/useAgentSocket', () => ({
  useAgentSocket: () => socket,
}))

vi.mock('../../api', () => ({
  apiClient: {
    wsUrl: () => 'ws://test/ws/agent',
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
  const { newConversation } = useChatSession()
  return (
    <button type="button" onClick={newConversation}>
      new
    </button>
  )
}

const message = (id: string, role: 'user' | 'assistant', content: string): ChatMessage => ({
  id,
  role,
  content,
  timestamp: '2026-01-01T00:00:00Z',
})

const renderProvider = async () => {
  render(
    <I18nProvider>
      <ChatSessionProvider>
        <Probe />
      </ChatSessionProvider>
    </I18nProvider>
  )
  // Let the provider's project/command fetches settle inside act, so the suite stays quiet.
  await act(async () => {})
}

const start = () => fireEvent.click(screen.getByRole('button', { name: 'new' }))

describe('ChatSessionContext', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    socket.messages = []
    socket.send.mockClear()
    socket.resetMessages.mockClear()
    vi.spyOn(window, 'confirm').mockReturnValue(true)
  })

  it('clears the daemon session before dropping the local transcript', async () => {
    socket.messages = [message('1', 'user', 'hello'), message('2', 'assistant', 'hi')]
    await renderProvider()

    start()

    await waitFor(() => expect(socket.send).toHaveBeenCalledWith('/clear'))
    expect(socket.resetMessages).toHaveBeenCalledTimes(1)
    expect(socket.send.mock.invocationCallOrder[0]).toBeLessThan(
      socket.resetMessages.mock.invocationCallOrder[0]
    )
  })

  it('asks before discarding a transcript, and does nothing when declined', async () => {
    socket.messages = [message('1', 'user', 'hello')]
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    await renderProvider()

    start()

    expect(window.confirm).toHaveBeenCalledTimes(1)
    expect(socket.send).not.toHaveBeenCalled()
    expect(socket.resetMessages).not.toHaveBeenCalled()
  })

  it('starts over without a prompt when the conversation is already empty', async () => {
    await renderProvider()

    start()

    await waitFor(() => expect(socket.send).toHaveBeenCalledWith('/clear'))
    expect(window.confirm).not.toHaveBeenCalled()
    expect(socket.resetMessages).toHaveBeenCalledTimes(1)
  })
})
