import React, { useEffect } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { ThemeProvider } from '../contexts/ThemeContext'
import { ToastProvider } from '../ui'
import { useI18n } from '../i18n/I18nContext'
import { ChatSessionProvider } from './chat-session/ChatSessionContext'
import { CommandPaletteHost } from './app-shell/CommandPaletteHost'
import { Sidebar } from './app-shell/Sidebar'
import { ShellProvider, useShell } from './app-shell/ShellContext'
import { cn } from '../ui'

/**
 * The shell is a sidebar and the work surface, nothing above them. Chat owns the whole viewport;
 * every other page scrolls inside 24px gutters.
 */
function ShellFrame() {
  const location = useLocation()
  const { t } = useI18n()
  const { focusMode, sidebarOpen, setSidebarOpen } = useShell()
  const isChat = location.pathname.startsWith('/chat')

  // A drawer that survives navigation would cover the page the operator just asked for.
  useEffect(() => {
    setSidebarOpen(false)
  }, [location.pathname, setSidebarOpen])

  return (
    <div className="flex h-screen bg-background text-ink">
      {!focusMode && (
        <>
          <Sidebar />
          {sidebarOpen && (
            <button
              type="button"
              aria-label={t('shell.closeSidebar', '关闭侧栏')}
              onClick={() => setSidebarOpen(false)}
              className="fixed inset-0 z-30 bg-black/40 lg:hidden"
            />
          )}
        </>
      )}
      <main
        className={cn(
          'flex min-h-0 min-w-0 flex-1 flex-col',
          isChat ? 'overflow-hidden' : 'overflow-y-auto p-6'
        )}
      >
        <Outlet />
      </main>
      <CommandPaletteHost />
    </div>
  )
}

const Layout: React.FC = () => {
  const { t } = useI18n()

  return (
    <ThemeProvider>
      <ToastProvider dismissLabel={t('common.close')}>
        <ChatSessionProvider>
          <ShellProvider>
            <ShellFrame />
          </ShellProvider>
        </ChatSessionProvider>
      </ToastProvider>
    </ThemeProvider>
  )
}

export default Layout
