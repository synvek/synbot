import React from 'react'
import { NavLink } from 'react-router-dom'
import { useI18n } from '../i18n/I18nContext'
import {
  OverviewIcon,
  ChatIcon,
  ChannelsIcon,
  SessionsIcon,
  CronIcon,
  RolesIcon,
  SkillsIcon,
  ConfigIcon,
  LogsIcon
} from './icons'

const Sidebar: React.FC = () => {
  const { t } = useI18n()
  
  const navItems = [
    { path: '/', label: t('sidebar.overview'), icon: OverviewIcon },
    { path: '/chat', label: t('sidebar.chat'), icon: ChatIcon },
    { path: '/channels', label: t('sidebar.channels'), icon: ChannelsIcon },
    { path: '/sessions', label: t('sidebar.sessions'), icon: SessionsIcon },
    { path: '/cron', label: t('sidebar.cron'), icon: CronIcon },
    { path: '/roles', label: t('sidebar.roles'), icon: RolesIcon },
    { path: '/skills', label: t('sidebar.skills'), icon: SkillsIcon },
    { path: '/config', label: t('sidebar.config'), icon: ConfigIcon },
    { path: '/logs', label: t('sidebar.logs'), icon: LogsIcon },
  ]

  return (
    <aside className="w-64 bg-surface shadow-sm overflow-y-auto border-r border-border">
      <nav className="p-4">
        <ul className="space-y-2">
          {navItems.map((item) => (
            <li key={item.path}>
              <NavLink
                to={item.path}
                end={item.path === '/'}
                className={({ isActive }) =>
                  `flex items-center gap-3 px-4 py-2.5 rounded-lg transition-all ${
                    isActive
                      ? 'bg-primary-muted text-primary font-medium'
                      : 'text-text hover:bg-background hover:text-primary'
                  }`
                }
              >
                <item.icon className="w-5 h-5" />
                <span>{item.label}</span>
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
    </aside>
  )
}

export default Sidebar
