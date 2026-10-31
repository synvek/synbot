import type { ReactNode } from 'react'
import { cn } from './cn'

interface EmptyStateProps {
  title: ReactNode
  body?: ReactNode
  action?: ReactNode
  icon?: ReactNode
  className?: string
  /** `inline` keeps it inside a table or panel instead of taking the page. */
  size?: 'page' | 'inline'
}

/**
 * The empty state is the first-run teacher: a distributed build opens on a console with no
 * channels, no projects and no sessions, so every one of those needs its next action here.
 */
export function EmptyState({ title, body, action, icon, className, size = 'page' }: EmptyStateProps) {
  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center text-center',
        size === 'page' ? 'gap-3 px-6 py-16' : 'gap-2 px-4 py-10',
        className
      )}
    >
      {icon && <div className="text-ink-3" aria-hidden="true">{icon}</div>}
      <p className={cn('font-medium text-ink', size === 'page' ? 'text-section' : 'text-body')}>{title}</p>
      {body && <p className="max-w-sm text-small text-ink-2">{body}</p>}
      {action && <div className="mt-1">{action}</div>}
    </div>
  )
}
