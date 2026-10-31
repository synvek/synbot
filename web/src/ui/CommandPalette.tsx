import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Search } from 'lucide-react'
import { cn } from './cn'
import { Kbd } from './Badge'

export interface CommandPaletteItem {
  id: string
  label: string
  /** Optional section heading; items keep the order they arrive in. */
  group?: string
  /** Right-hand hint: a route, a shortcut, a scheme name. */
  hint?: string
  /** Extra words that should match, e.g. the English route when the UI is Chinese. */
  keywords?: string
  icon?: ReactNode
  run: () => void
}

interface CommandPaletteProps {
  open: boolean
  onClose: () => void
  items: CommandPaletteItem[]
  placeholder?: string
  emptyLabel?: ReactNode
  /** Label of the shortcut chip shown on the right of the input. */
  shortcut?: string
}

/**
 * ⌘K palette. The host decides what exists (navigate, switch project, switch accent, send a
 * slash command); this only filters, groups and handles the keyboard contract: ↑↓ move, ↵ runs,
 * Esc closes, focus returns where it came from.
 */
export function CommandPalette({
  open,
  onClose,
  items,
  placeholder = 'Search',
  emptyLabel = 'No results',
  shortcut = 'Esc',
}: CommandPaletteProps) {
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const restoreRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (!open) return
    restoreRef.current = document.activeElement as HTMLElement | null
    setQuery('')
    setActive(0)
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0)
    return () => {
      window.clearTimeout(timer)
      restoreRef.current?.focus?.()
    }
  }, [open])

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return items
    return items.filter((item) =>
      `${item.label} ${item.keywords ?? ''} ${item.hint ?? ''}`.toLowerCase().includes(needle)
    )
  }, [items, query])

  useEffect(() => {
    setActive(0)
  }, [query])

  // Keep the highlighted row inside the list while arrowing through it.
  useEffect(() => {
    const node = listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)
    node?.scrollIntoView({ block: 'nearest' })
  }, [active])

  if (!open) return null

  const runAt = (index: number) => {
    const item = filtered[index]
    if (!item) return
    onClose()
    item.run()
  }

  let lastGroup: string | undefined

  return (
    <div
      className="fixed inset-0 z-[70] flex items-start justify-center bg-black/40 px-4 pt-[12vh]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        ref={containerRef}
        role="dialog"
        aria-modal="true"
        aria-label={placeholder}
        className="w-full max-w-[560px] overflow-hidden rounded-panel border border-line bg-surface shadow-overlay"
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault()
            onClose()
          } else if (event.key === 'Tab') {
            // No `showModal()` here, so the palette traps Tab itself: focus never escapes behind it.
            const focusables = Array.from(
              containerRef.current?.querySelectorAll<HTMLElement>('input, button') ?? []
            ).filter((node) => !node.hasAttribute('disabled'))
            if (focusables.length === 0) return
            const first = focusables[0]
            const last = focusables[focusables.length - 1]
            const current = document.activeElement
            if (event.shiftKey && (current === first || !containerRef.current?.contains(current))) {
              event.preventDefault()
              last.focus()
            } else if (!event.shiftKey && current === last) {
              event.preventDefault()
              first.focus()
            }
          } else if (event.key === 'ArrowDown') {
            event.preventDefault()
            setActive((index) => (filtered.length === 0 ? 0 : (index + 1) % filtered.length))
          } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            setActive((index) =>
              filtered.length === 0 ? 0 : (index - 1 + filtered.length) % filtered.length
            )
          } else if (event.key === 'Enter') {
            event.preventDefault()
            runAt(active)
          }
        }}
      >
        <div className="flex items-center gap-2 border-b border-line px-3">
          <Search className="h-3.5 w-3.5 text-ink-3" aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={placeholder}
            aria-label={placeholder}
            role="combobox"
            aria-expanded="true"
            aria-controls="command-palette-listbox"
            aria-autocomplete="list"
            aria-activedescendant={filtered[active] ? `command-palette-${filtered[active].id}` : undefined}
            className="h-10 flex-1 bg-transparent text-body text-ink placeholder:text-ink-3 focus:outline-none"
          />
          <Kbd>{shortcut}</Kbd>
        </div>
        <div
          ref={listRef}
          id="command-palette-listbox"
          role="listbox"
          aria-label={placeholder}
          className="max-h-[52vh] overflow-y-auto py-1"
        >
          {filtered.length === 0 ? (
            <p className="px-3 py-6 text-center text-small text-ink-2">{emptyLabel}</p>
          ) : (
            filtered.map((item, index) => {
              const showGroup = item.group && item.group !== lastGroup
              lastGroup = item.group
              return (
                <div key={item.id}>
                  {showGroup && (
                    <div className="px-3 pb-1 pt-2 text-micro font-medium uppercase tracking-wide text-ink-3">
                      {item.group}
                    </div>
                  )}
                  <button
                    type="button"
                    id={`command-palette-${item.id}`}
                    role="option"
                    aria-selected={index === active}
                    tabIndex={-1}
                    data-index={index}
                    onMouseMove={() => setActive(index)}
                    onClick={() => runAt(index)}
                    className={cn(
                      'flex w-full items-center gap-2 px-3 py-1.5 text-left text-small transition-colors duration-fast',
                      index === active ? 'bg-surface-2 text-ink' : 'text-ink-2'
                    )}
                  >
                    {item.icon && <span className="text-ink-3" aria-hidden="true">{item.icon}</span>}
                    <span className="flex-1 truncate">{item.label}</span>
                    {item.hint && (
                      <span className="shrink-0 truncate font-mono text-micro text-ink-3">{item.hint}</span>
                    )}
                  </button>
                </div>
              )
            })
          )}
        </div>
      </div>
    </div>
  )
}
