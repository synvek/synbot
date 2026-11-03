import { MoreHorizontal } from 'lucide-react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useI18n } from '../../i18n/I18nContext'
import { MenuItem, MenuLabel, Popover } from '../../ui'
import { adminNavGroups } from './nav'

interface MoreMenuProps {
  /** Icon-only sidebar: the trigger loses its label but keeps its accessible name. */
  compact?: boolean
}

/**
 * The admin surface, folded into one corner. Eleven pages that are configured once and then
 * visited occasionally do not deserve a permanent column: they live here, grouped exactly as the
 * old rail grouped them, and ⌘K reaches them without opening the menu at all.
 */
export function MoreMenu({ compact = false }: MoreMenuProps) {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { pathname } = useLocation()

  return (
    <Popover
      label={t('shell.more')}
      align="start"
      side="top"
      closeOnSelect
      className="w-full"
      triggerClassName={compact ? undefined : 'w-full justify-start'}
      icon={<MoreHorizontal className="h-4 w-4" aria-hidden="true" />}
      text={
        compact ? undefined : (
          <span className="min-w-0 flex-1 truncate text-left">{t('shell.more')}</span>
        )
      }
    >
      <MenuLabel>{t('shell.adminPages')}</MenuLabel>
      {adminNavGroups.map((group) => (
        <div key={group.id}>
          <div className="px-2.5 pb-0.5 pt-1.5 text-micro text-ink-3">{t(group.labelKey)}</div>
          {group.items.map((item) => {
            const Icon = item.icon
            const active = pathname === item.path || pathname.startsWith(`${item.path}/`)
            return (
              <MenuItem
                key={item.path}
                icon={<Icon className="h-3.5 w-3.5" aria-hidden="true" />}
                selected={active}
                onSelect={() => navigate(item.path)}
                aside={active ? <span className="h-1 w-1 rounded-full bg-accent" aria-hidden="true" /> : undefined}
              >
                {t(item.labelKey)}
              </MenuItem>
            )
          })}
        </div>
      ))}
    </Popover>
  )
}
