import { useId, useState, type ReactNode } from 'react'
import { cn } from './cn'

interface TooltipProps {
  label: ReactNode
  children: ReactNode
  side?: 'top' | 'bottom'
  className?: string
}

/**
 * Hover/focus hint. It never carries information that exists nowhere else: an icon-only control
 * still needs its `aria-label`, and a truncated path is still readable after expanding.
 */
export function Tooltip({ label, children, side = 'bottom', className }: TooltipProps) {
  const [visible, setVisible] = useState(false)
  const id = useId()

  return (
    <span
      className={cn('relative inline-flex', className)}
      onMouseEnter={() => setVisible(true)}
      onMouseLeave={() => setVisible(false)}
      onFocus={() => setVisible(true)}
      onBlur={() => setVisible(false)}
      aria-describedby={visible ? id : undefined}
    >
      {children}
      {visible && (
        <span
          role="tooltip"
          id={id}
          className={cn(
            'pointer-events-none absolute left-1/2 z-50 -translate-x-1/2 whitespace-nowrap rounded-control border border-line bg-surface px-2 py-1',
            'text-micro text-ink shadow-overlay',
            side === 'bottom' ? 'top-[calc(100%+6px)]' : 'bottom-[calc(100%+6px)]'
          )}
        >
          {label}
        </span>
      )}
    </span>
  )
}
