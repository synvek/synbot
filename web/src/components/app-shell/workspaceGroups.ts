import type { Project, SessionSummary } from '../../types/api'

/** One workspace and the conversations created inside it. */
export interface WorkspaceGroup {
  /** Project id; `null` is the default workspace (`~/.synbot/workspace`). */
  id: string | null
  /** Registered folder, or `null` for the default workspace. */
  project: Project | null
  sessions: SessionSummary[]
}

/**
 * Conversations nested under the workspace they were created in.
 *
 * Order is the registry's: the default workspace first, then every registered folder in the order
 * the daemon lists them. A conversation whose workspace is no longer listed lands in the default
 * group rather than vanishing from the sidebar.
 */
export function groupSessionsByWorkspace(
  projects: Project[],
  sessions: SessionSummary[]
): WorkspaceGroup[] {
  const groups: WorkspaceGroup[] = [
    { id: null, project: null, sessions: [] },
    ...projects.map((project) => ({ id: project.id, project, sessions: [] as SessionSummary[] })),
  ]
  const byId = new Map<string | null, WorkspaceGroup>(groups.map((group) => [group.id, group]))
  for (const session of sessions) {
    const key = session.project_id && byId.has(session.project_id) ? session.project_id : null
    byId.get(key)!.sessions.push(session)
  }
  return groups
}
