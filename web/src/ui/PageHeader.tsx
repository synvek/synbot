import type { ReactNode } from 'react'
import { cn } from './cn'

interface PageHeaderProps {
  title: ReactNode
  /** Count of the rows the page manages, rendered as a quiet square chip. */
  count?: number | string
  description?: ReactNode
  actions?: ReactNode
  className?: string
  /** Stick to the top of the scroll container on long pages. */
  sticky?: boolean
}

/**
 * The one page title in the console: 20/600, a count, and at most one primary action. Everything
 * else on the row is a ghost control.
 */
export function PageHeader({ title, count, description, actions, className, sticky = false }: PageHeaderProps) {
  return (
    <div
      className={cn(
        'flex flex-wrap items-start justify-between gap-3 pb-3',
        sticky && 'sticky top-0 z-20 -mx-6 border-b border-line bg-background px-6 py-3',
        className
      )}
    >
      <div className="min-w-0">
        <div className="flex items-baseline gap-2">
          <h1 className="truncate text-page font-semibold text-ink">{title}</h1>
          {count !== undefined && (
            <span className="tabular rounded-control bg-surface-2 px-1.5 py-0.5 font-mono text-micro text-ink-2">
              {count}
            </span>
          )}
        </div>
        {description && <p className="mt-0.5 text-small text-ink-2">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  )
}
