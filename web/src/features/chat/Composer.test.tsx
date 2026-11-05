import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent, within } from '@testing-library/react'
import { I18nProvider } from '../../i18n/I18nContext'
import type { Project } from '../../types/api'
import { Composer } from './Composer'

/**
 * The workspace line under the input is the only place a conversation's folder is chosen: it is a
 * picker while the transcript is empty and gone afterwards, so the suite pins both halves.
 */
const repo: Project = {
  id: 'p-1',
  name: 'repo',
  path: '/work/repo',
  added_at: '2026-01-01T00:00:00Z',
  git_head: null,
  rules: [],
}

const docs: Project = { ...repo, id: 'p-2', name: 'docs', path: '/work/docs' }

const renderComposer = (overrides: Partial<React.ComponentProps<typeof Composer>> = {}) => {
  const props = {
    connected: true,
    running: false,
    commands: [],
    planMode: false,
    onTogglePlanMode: vi.fn(),
    onSend: vi.fn(),
    onStop: vi.fn(),
    workspace: { name: 'repo', path: '/work/repo' },
    workspaceId: 'p-1',
    workspaces: [repo, docs],
    showWorkspace: true,
    onSelectWorkspace: vi.fn(),
    ...overrides,
  }
  render(
    <I18nProvider>
      <Composer {...props} />
    </I18nProvider>
  )
  return props
}

/** The caption names the folder, so it is the trigger whose accessible name carries the path. */
const openPicker = () => {
  fireEvent.click(screen.getByRole('button', { name: /\/work\/repo/ }))
  return screen.getByRole('menu')
}

describe('Composer workspace picker', () => {
  it('names the pinned folder under the input', () => {
    renderComposer()

    expect(screen.getByText('/work/repo')).toBeInTheDocument()
  })

  it('disappears once the conversation has something to say', () => {
    renderComposer({ showWorkspace: false })

    expect(screen.queryByText('/work/repo')).not.toBeInTheDocument()
  })

  it('starts the conversation in the folder that was picked', async () => {
    const props = renderComposer()

    const menu = openPicker()
    fireEvent.click(await within(menu).findByRole('menuitem', { name: /docs/ }))

    expect(props.onSelectWorkspace).toHaveBeenCalledWith('p-2')
  })

  it('falls back to the default workspace when that row is picked', async () => {
    const props = renderComposer()

    const menu = openPicker()
    fireEvent.click(
      await within(menu).findByRole('menuitem', { name: /默认工作区|Default workspace/ })
    )

    expect(props.onSelectWorkspace).toHaveBeenCalledWith(null)
  })
})
