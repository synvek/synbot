import React from 'react'
import { NavLink } from 'react-router-dom'

const Sidebar: React.FC = () => {
  const navItems = [
    { path: '/', label: 'Overview', icon: '📊' },
    { path: '/chat', label: 'Chat', icon: '💬' },
    { path: '/channels', label: 'Channels', icon: '📡' },
    { path: '/sessions', label: 'Sessions', icon: '👥' },
    { path: '/cron', label: 'Cron Jobs', icon: '⏰' },
    { path: '/roles', label: 'Roles', icon: '👤' },
    { path: '/skills', label: 'Skills', icon: '🎯' },
    { path: '/config', label: 'Config', icon: '⚙️' },
    { path: '/logs', label: 'Logs', icon: '📝' },
  ]

  return (
    <aside className="w-64 bg-white shadow-sm overflow-y-auto">
      <nav className="p-4">
        <ul className="space-y-2">
          {navItems.map((item) => (
            <li key={item.path}>
              <NavLink
                to={item.path}
                end={item.path === '/'}
                className={({ isActive }) =>
                  `flex items-center gap-3 px-4 py-2 rounded-lg transition-colors ${
                    isActive
                      ? 'bg-blue-50 text-blue-600 font-medium'
                      : 'text-gray-700 hover:bg-gray-50'
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
