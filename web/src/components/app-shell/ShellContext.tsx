import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'

interface ShellContextValue {
  /** Rail is icon-only. Persisted in `localStorage: shell-rail`. */
  railCollapsed: boolean
  toggleRail: () => void
  /** Focus mode hides the rail and the chat inspector; ⌘. toggles it. */
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

const RAIL_KEY = 'shell-rail'

export function ShellProvider({ children }: { children: ReactNode }) {
  const [railCollapsed, setRailCollapsed] = useState<boolean>(() => localStorage.getItem(RAIL_KEY) === 'collapsed')
  const [focusMode, setFocusMode] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)

  const toggleRail = useCallback(() => {
    setRailCollapsed((current) => {
      const next = !current
      localStorage.setItem(RAIL_KEY, next ? 'collapsed' : 'expanded')
      return next
    })
  }, [])

  const toggleFocusMode = useCallback(() => setFocusMode((current) => !current), [])

  // ⌘K / Ctrl+K opens the palette, ⌘. toggles focus mode. Both are ignored while typing in a
  // field so a chord never eats text.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const typing =
        target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable
      const meta = event.metaKey || event.ctrlKey
      if (!meta) return
      if (event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setPaletteOpen((open) => !open)
      } else if (event.key === '.' && !typing) {
        event.preventDefault()
        setFocusMode((current) => !current)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  const value = useMemo(
    () => ({ railCollapsed, toggleRail, focusMode, toggleFocusMode, paletteOpen, setPaletteOpen }),
    [railCollapsed, toggleRail, focusMode, toggleFocusMode, paletteOpen]
  )

  return <ShellContext.Provider value={value}>{children}</ShellContext.Provider>
}
