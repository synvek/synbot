import { NavLink } from 'react-router-dom'
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { IconButton, Tooltip, cn } from '../../ui'
import { navGroups } from './nav'
import { useShell } from './ShellContext'

/**
 * The rail: four groups, 32px rows, one accent mark on the active row. Collapsed it keeps the
 * icons and the tooltip, so an operator working on a narrow screen loses no destination.
 */
export function Rail() {
  const { t } = useI18n()
  const { railCollapsed, toggleRail } = useShell()
  // The console is served by the daemon itself, so the address it lives on is the daemon's.
  const host = typeof window === 'undefined' ? '' : window.location.host

  return (
    <nav
      aria-label={t('shell.groups')}
      className={cn(
        'flex shrink-0 flex-col border-r border-line bg-surface transition-[width] duration-base ease-out',
        railCollapsed ? 'w-16' : 'w-[232px]'
      )}
    >
      <div className="flex-1 overflow-y-auto px-2 py-2">
        {navGroups.map((group) => (
          <div key={group.id} className="mb-3 last:mb-0">
            {!railCollapsed && (
              <div className="px-2 pb-1 pt-1 text-[11px] font-medium uppercase tracking-wide text-ink-3">
                {t(group.labelKey)}
              </div>
            )}
            <ul className="space-y-0.5">
              {group.items.map((item) => {
                const Icon = item.icon
                const link = (
                  <NavLink
                    to={item.path}
                    end={item.path === '/'}
                    className={({ isActive }) =>
                      cn(
                        'group relative flex h-8 items-center gap-2 rounded-control text-small transition-colors duration-fast',
                        railCollapsed ? 'justify-center px-0' : 'px-2',
                        isActive
                          ? 'bg-surface-2 font-medium text-ink'
                          : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
                      )
                    }
                  >
                    {({ isActive }) => (
                      <>
                        {isActive && (
                          <span
                            aria-hidden="true"
                            className="absolute -left-2 top-1.5 bottom-1.5 w-[2px] rounded-full bg-accent"
                          />
                        )}
                        <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
                        {!railCollapsed && <span className="truncate">{t(item.labelKey)}</span>}
                      </>
                    )}
                  </NavLink>
                )
                return (
                  <li key={item.path}>
                    {railCollapsed ? (
                      <Tooltip label={t(item.labelKey)}>{link}</Tooltip>
                    ) : (
                      link
                    )}
                  </li>
                )
              })}
            </ul>
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between border-t border-line px-2 py-1.5">
        {!railCollapsed && (
          <span className="truncate font-mono text-[11px] text-ink-3">{host}</span>
        )}
        <IconButton
          label={t('shell.toggleSidebar')}
          size="sm"
          onClick={toggleRail}
          className={railCollapsed ? 'mx-auto' : ''}
        >
          {railCollapsed ? (
            <PanelLeftOpen className="h-4 w-4" />
          ) : (
            <PanelLeftClose className="h-4 w-4" />
          )}
        </IconButton>
      </div>
    </nav>
  )
}
