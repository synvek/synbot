import { describe, expect, it } from 'vitest'
import type { Project, SessionSummary } from '../../types/api'
import { groupSessionsByWorkspace } from './workspaceGroups'

const project = (id: string, name = id): Project => ({
  id,
  name,
  path: `/work/${id}`,
  added_at: '2026-01-01T00:00:00Z',
  git_head: null,
  rules: [],
})

const session = (chatId: string, projectId: string | null): SessionSummary => ({
  id: `agent:main:web:dm:${chatId}`,
  chat_id: chatId,
  channel: 'web',
  scope: 'dm',
  identifier: chatId,
  title: chatId,
  archived: false,
  pending_plan_id: null,
  plan_mode: false,
  model_override: null,
  project_id: projectId,
  message_count: 1,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
})

describe('groupSessionsByWorkspace', () => {
  it('nests conversations under the workspace they were created in', () => {
    const groups = groupSessionsByWorkspace(
      [project('a'), project('b')],
      [session('chat-b', 'b'), session('chat-default', null), session('chat-a', 'a')]
    )

    expect(groups.map((group) => group.id)).toEqual([null, 'a', 'b'])
    expect(groups[0].sessions.map((item) => item.chat_id)).toEqual(['chat-default'])
    expect(groups[1].sessions.map((item) => item.chat_id)).toEqual(['chat-a'])
    expect(groups[2].sessions.map((item) => item.chat_id)).toEqual(['chat-b'])
  })

  it('keeps workspaces without conversations, so the row can still create one', () => {
    const groups = groupSessionsByWorkspace([project('empty')], [])

    expect(groups).toHaveLength(2)
    expect(groups[1]).toMatchObject({ id: 'empty', sessions: [] })
  })

  it('falls back to the default group when the workspace is gone', () => {
    // The folder was removed from the list (or projects.json was edited by hand): the
    // conversation must stay reachable instead of disappearing from the sidebar.
    const groups = groupSessionsByWorkspace([], [session('orphan', 'missing')])

    expect(groups).toHaveLength(1)
    expect(groups[0].sessions.map((item) => item.chat_id)).toEqual(['orphan'])
  })
})
