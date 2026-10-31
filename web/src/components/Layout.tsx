import React from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import { ThemeProvider } from '../contexts/ThemeContext'
import { ToastProvider } from '../ui'
import { useI18n } from '../i18n/I18nContext'
import { AppBar } from './app-shell/AppBar'
import { CommandPaletteHost } from './app-shell/CommandPaletteHost'
import { Rail } from './app-shell/Rail'
import { ShellProvider, useShell } from './app-shell/ShellContext'
import { cn } from '../ui'

/** Chat owns the whole viewport below the bar; every other page scrolls inside 24px gutters. */
function ShellFrame() {
  const location = useLocation()
  const { focusMode } = useShell()
  const isChat = location.pathname.startsWith('/chat')

  return (
    <div className="flex h-screen flex-col bg-background text-ink">
      <AppBar />
      <div className="flex min-h-0 flex-1">
        {!focusMode && <Rail />}
        <main
          className={cn(
            'flex min-h-0 min-w-0 flex-1 flex-col',
            isChat ? 'overflow-hidden' : 'overflow-y-auto p-6'
          )}
        >
          <Outlet />
        </main>
      </div>
      <CommandPaletteHost />
    </div>
  )
}

const Layout: React.FC = () => {
  const { t } = useI18n()

  return (
    <ThemeProvider>
      <ToastProvider dismissLabel={t('common.close')}>
        <ShellProvider>
          <ShellFrame />
        </ShellProvider>
      </ToastProvider>
    </ThemeProvider>
  )
}

export default Layout
