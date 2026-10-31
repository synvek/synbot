import { useEffect, useRef, useState, type ReactNode } from 'react'
import { ChevronDown } from 'lucide-react'
import { cn } from './cn'

interface PopoverProps {
  /** Accessible name of the trigger. */
  label: string
  icon?: ReactNode
  /** Text shown next to the label; omit for an icon-only trigger. */
  text?: ReactNode
  align?: 'start' | 'end'
  className?: string
  children: ReactNode
}

/**
 * A menu that closes on Esc or an outside click and hands focus back to its trigger. Hand-rolled
 * on purpose: a popover is ~200 lines of behaviour, and adding a focus-management dependency for
 * three menus would cost more than it saves.
 */
export function Popover({ label, icon, text, align = 'end', className, children }: PopoverProps) {
  const [open, setOpen] = useState(false)
  const wrapperRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: MouseEvent) => {
      if (!wrapperRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        setOpen(false)
        triggerRef.current?.focus()
      }
    }
    document.addEventListener('mousedown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    // Move into the menu so keyboard users do not have to tab from the trigger.
    const first = panelRef.current?.querySelector<HTMLElement>('[role="menuitem"],button')
    first?.focus()
    return () => {
      document.removeEventListener('mousedown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [open])

  return (
    <div ref={wrapperRef} className={cn('relative', className)}>
      <button
        ref={triggerRef}
        type="button"
        aria-label={text ? undefined : label}
        title={text ? undefined : label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={cn(
          'inline-flex items-center gap-1.5 rounded-control text-small text-ink-2',
          'transition-colors duration-fast hover:bg-surface-2 hover:text-ink',
          text ? 'h-8 px-2' : 'h-8 w-8 justify-center'
        )}
      >
        {icon}
        {text}
        <ChevronDown className="h-3 w-3 text-ink-3" aria-hidden="true" />
      </button>
      {open && (
        <div
          ref={panelRef}
          role="menu"
          aria-label={label}
          className={cn(
            'absolute z-50 mt-1 min-w-[200px] overflow-hidden rounded-panel border border-line bg-surface py-1 shadow-overlay',
            align === 'end' ? 'right-0' : 'left-0'
          )}
        >
          {children}
        </div>
      )}
    </div>
  )
}

interface MenuItemProps {
  children: ReactNode
  onSelect: () => void
  icon?: ReactNode
  /** Right-hand slot: a swatch, a shortcut, a check mark. */
  aside?: ReactNode
  selected?: boolean
  danger?: boolean
}

export function MenuItem({ children, onSelect, icon, aside, selected, danger }: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onSelect}
      className={cn(
        'flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-small transition-colors duration-fast',
        danger ? 'text-error hover:bg-surface-2' : 'text-ink hover:bg-surface-2',
        selected && 'font-medium'
      )}
    >
      {icon && <span className="text-ink-3" aria-hidden="true">{icon}</span>}
      <span className="flex-1 truncate">{children}</span>
      {aside}
    </button>
  )
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-2.5 pb-1 pt-2 text-micro font-medium uppercase tracking-wide text-ink-3">
      {children}
    </div>
  )
}

export function MenuSeparator() {
  return <div role="separator" className="my-1 border-t border-line" />
}
