import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { useChatSession } from '../chat-session/ChatSessionContext'

interface ShellContextValue {
  /** Sidebar is icon-only. Persisted in `localStorage: shell-rail`. */
  sidebarCollapsed: boolean
  toggleSidebar: () => void
  /**
   * Sidebar as an overlay. Only meaningful below `lg`, where the sidebar would otherwise squeeze
   * the transcript; the wide layout ignores it.
   */
  sidebarOpen: boolean
  setSidebarOpen: (open: boolean) => void
  /** Focus mode hides the sidebar and the chat inspector; ⌘. toggles it. */
  focusMode: boolean
  toggleFocusMode: () => void
  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void
}

const ShellContext = createContext<ShellContextValue | undefined>(undefined)

export function useShell() {
  const context = useContext(ShellContext)
  if (!context) throw new Error('useShell must be used within a ShellProvider')
  return context
}

const SIDEBAR_KEY = 'shell-rail'

export function ShellProvider({ children }: { children: ReactNode }) {
  const { newConversation } = useChatSession()
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(
    () => localStorage.getItem(SIDEBAR_KEY) === 'collapsed'
  )
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const [focusMode, setFocusMode] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((current) => {
      const next = !current
      localStorage.setItem(SIDEBAR_KEY, next ? 'collapsed' : 'expanded')
      return next
    })
  }, [])

  const toggleFocusMode = useCallback(() => setFocusMode((current) => !current), [])

  // ⌘K opens the palette, ⌘N starts a new conversation, ⌘. toggles focus mode. All three are
  // ignored while typing in a field so a chord never eats text.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const typing =
        target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable
      const meta = event.metaKey || event.ctrlKey
      if (!meta || typing) return
      const key = event.key.toLowerCase()
      if (key === 'k') {
        event.preventDefault()
        setPaletteOpen((open) => !open)
      } else if (key === 'n') {
        event.preventDefault()
        newConversation()
      } else if (event.key === '.') {
        event.preventDefault()
        setFocusMode((current) => !current)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [newConversation])

  const value = useMemo(
    () => ({
      sidebarCollapsed,
      toggleSidebar,
      sidebarOpen,
      setSidebarOpen,
      focusMode,
      toggleFocusMode,
      paletteOpen,
      setPaletteOpen,
    }),
    [sidebarCollapsed, toggleSidebar, sidebarOpen, focusMode, toggleFocusMode, paletteOpen]
  )

  return <ShellContext.Provider value={value}>{children}</ShellContext.Provider>
}
