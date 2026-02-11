import React from 'react'
import { NavLink } from 'react-router-dom'
import { useI18n } from '../i18n/I18nContext'

const Sidebar: React.FC = () => {
  const { t } = useI18n()
  
  const navItems = [
    { path: '/', label: t('sidebar.overview'), icon: '📊' },
    { path: '/chat', label: t('sidebar.chat'), icon: '💬' },
    { path: '/channels', label: t('sidebar.channels'), icon: '📡' },
    { path: '/sessions', label: t('sidebar.sessions'), icon: '👥' },
    { path: '/cron', label: t('sidebar.cron'), icon: '⏰' },
    { path: '/roles', label: t('sidebar.roles'), icon: '👤' },
    { path: '/skills', label: t('sidebar.skills'), icon: '🎯' },
    { path: '/config', label: t('sidebar.config'), icon: '⚙️' },
    { path: '/logs', label: t('sidebar.logs'), icon: '📝' },
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
                <span className="text-xl">{item.icon}</span>
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
