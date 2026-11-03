import { Check, Folder, FolderPlus, Loader2 } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { MenuItem, MenuLabel, MenuSeparator, Popover } from '../../ui'
import type { Project } from '../../types/api'
import type { ProjectInfo } from '../../types/agentEvents'
import { requestOpenFolder } from '../../components/app-shell/intents'
import { useShell } from '../../components/app-shell/ShellContext'

interface WorkspaceChipProps {
  project: ProjectInfo | null
  projects: Project[]
  /** A turn is running: the switch lands on the next turn, which the menu says out loud. */
  running: boolean
  onSelect: (projectId: string | null) => void
}

/**
 * The conversation's working folder, top right. It moved out of the inspector: the folder the
 * agent works in belongs to the conversation, not to the file panel you may have closed.
 */
export function WorkspaceChip({ project, projects, running, onSelect }: WorkspaceChipProps) {
  const { t } = useI18n()
  const { setSidebarOpen } = useShell()

  return (
    <Popover
      label={t('shell.workspaces')}
      closeOnSelect
      icon={<Folder className="h-3.5 w-3.5" aria-hidden="true" />}
      text={
        <span className="max-w-[180px] truncate font-mono text-micro text-ink-2">
          {project?.name ?? t('shell.globalWorkspace')}
        </span>
      }
    >
      <MenuLabel>{t('shell.workspaces')}</MenuLabel>
      <MenuItem
        selected={project === null}
        onSelect={() => onSelect(null)}
        aside={project === null ? <Check className="h-3.5 w-3.5 text-accent" /> : undefined}
      >
        {t('shell.globalWorkspace')}
      </MenuItem>
      {projects.map((item) => (
        <MenuItem
          key={item.id}
          selected={project?.id === item.id}
          onSelect={() => onSelect(item.id)}
          aside={
            project?.id === item.id ? (
              <Check className="h-3.5 w-3.5 text-accent" />
            ) : undefined
          }
        >
          {item.name}
        </MenuItem>
      ))}
      {running && (
        <>
          <MenuSeparator />
          <div className="flex items-center gap-2 px-2.5 py-1 text-micro text-ink-3">
            <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
            {t('project.switchNextTurn', '切换将在下一轮生效')}
          </div>
        </>
      )}
      <MenuSeparator />
      {/* The list only ever grows from here: adding a folder is the same act as picking one. */}
      <MenuItem
        icon={<FolderPlus className="h-3.5 w-3.5" aria-hidden="true" />}
        onSelect={() => {
          setSidebarOpen(true)
          requestOpenFolder()
        }}
      >
        {t('shell.openFolder')}
      </MenuItem>
    </Popover>
  )
}
