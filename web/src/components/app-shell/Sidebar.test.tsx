import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { I18nProvider } from '../../i18n/I18nContext'
import type { Project, SessionSummary } from '../../types/api'
import { Sidebar } from './Sidebar'

/**
 * The sidebar is now a tree: one row per workspace, its conversations underneath, and the
 * workspace's own actions on that row. These tests pin the parts that used to be a separate
 * 「workspaces」 column plus a flat 「history」 list.
 */
const project = (id: string, name: string): Project => ({
  id,
  name,
  path: `/work/${name}`,
  added_at: '2026-01-01T00:00:00Z',
  git_head: null,
  rules: [],
})

const session = (chatId: string, projectId: string | null, title: string): SessionSummary => ({
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
  project_id: projectId,
  message_count: 2,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
})

const state = {
  workspaceId: null as string | null,
  projects: [] as Project[],
  running: false,
  sessions: [] as SessionSummary[],
  activeChatId: null as string | null,
  newConversation: vi.fn(),
  addWorkspace: vi.fn(),
  renameWorkspace: vi.fn(),
  removeWorkspace: vi.fn(),
  selectSession: vi.fn(),
  renameSession: vi.fn(),
  archiveSession: vi.fn(),
  refreshSessions: vi.fn(),
}

vi.mock('../chat-session/ChatSessionContext', () => ({
  useChatSession: () => state,
}))

vi.mock('./ShellContext', () => ({
  useShell: () => ({
    sidebarCollapsed: false,
    sidebarOpen: true,
    toggleSidebar: vi.fn(),
    setSidebarOpen: vi.fn(),
    paletteOpen: false,
    setPaletteOpen: vi.fn(),
    focusMode: false,
    toggleFocusMode: vi.fn(),
  }),
}))

// The footer menus and the tray bridge are not what this suite is about.
vi.mock('./MoreMenu', () => ({ MoreMenu: () => null }))
vi.mock('./AccountMenu', () => ({ AccountMenu: () => null }))
vi.mock('./DaemonStatus', () => ({ DaemonStatus: () => null }))

const renderSidebar = async () => {
  render(
    <MemoryRouter>
      <I18nProvider>
        <Sidebar />
      </I18nProvider>
    </MemoryRouter>
  )
  // Locales arrive through a dynamic import: query only after a translated label exists.
  await screen.findByText(/Workspaces|工作区/)
}

const WORKSPACE_MENU = /工作区操作|Workspace actions/

/** The 「+」 of one workspace row — it carries the workspace name in its accessible name. */
const plusFor = (workspace: string) => {
  const button = screen
    .getAllByRole('button', { name: /新建对话|New chat in/ })
    .find((element) => (element.getAttribute('aria-label') ?? '').includes(workspace))
  if (!button) {
    const labels = screen
      .getAllByRole('button')
      .map((element) => element.getAttribute('aria-label'))
    throw new Error(`No + button for workspace ${workspace}; saw ${labels.join(' | ')}`)
  }
  return button
}

describe('Sidebar workspace tree', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    state.projects = [project('p-1', 'repo-one'), project('p-2', 'repo-two')]
    state.sessions = [
      session('c-1', 'p-1', 'Alpha chat'),
      session('c-2', null, 'Default chat'),
    ]
    state.activeChatId = 'c-1'
    state.workspaceId = 'p-1'
    state.newConversation.mockReset()
    state.removeWorkspace.mockReset()
    vi.spyOn(window, 'confirm').mockReturnValue(true)
  })

  it('nests each conversation under the workspace it belongs to', async () => {
    await renderSidebar()

    expect(screen.getByText('Default chat')).toBeTruthy()
    expect(screen.getByText('Alpha chat')).toBeTruthy()
    // Every registered workspace keeps its row even without conversations of its own.
    expect(screen.getByText('repo-two')).toBeTruthy()
  })

  it('creates conversations inside the workspace whose + was pressed', async () => {
    await renderSidebar()

    fireEvent.click(plusFor('repo-one'))

    expect(state.newConversation).toHaveBeenCalledWith('p-1')
  })

  it('opens the workspace menu with reveal, rename and remove', async () => {
    await renderSidebar()

    fireEvent.click(screen.getAllByRole('button', { name: WORKSPACE_MENU })[0])
    const menu = screen.getByRole('menu', { name: WORKSPACE_MENU })

    expect(within(menu).getByRole('menuitem', { name: /Open folder|打开文件夹/ })).toBeTruthy()
    expect(within(menu).getByRole('menuitem', { name: /Rename|重命名/ })).toBeTruthy()
    expect(within(menu).getByRole('menuitem', { name: /Remove|删除/ })).toBeTruthy()
  })

  it('removes a workspace only after a confirmation', async () => {
    await renderSidebar()

    fireEvent.click(screen.getAllByRole('button', { name: WORKSPACE_MENU })[0])
    const menu = screen.getByRole('menu', { name: WORKSPACE_MENU })
    fireEvent.click(within(menu).getByRole('menuitem', { name: /Remove|删除/ }))

    await waitFor(() => expect(state.removeWorkspace).toHaveBeenCalledWith('p-1'))
    expect(window.confirm).toHaveBeenCalled()
  })
})
