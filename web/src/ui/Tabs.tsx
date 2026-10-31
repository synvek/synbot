import { useRef, type ReactNode } from 'react'
import { cn } from './cn'

export interface TabItem {
  id: string
  label: ReactNode
  /** Optional count (teammates running, failed jobs…). */
  count?: number
}

interface TabsProps {
  items: TabItem[]
  value: string
  onChange: (id: string) => void
  className?: string
  /** `underline` for page-level tabs, `quiet` for panel tabs. */
  variant?: 'underline' | 'quiet'
}

export function Tabs({ items, value, onChange, className, variant = 'underline' }: TabsProps) {
  const refs = useRef<Array<HTMLButtonElement | null>>([])

  const move = (index: number, delta: number) => {
    const next = (index + delta + items.length) % items.length
    refs.current[next]?.focus()
    onChange(items[next].id)
  }

  return (
    <div
      role="tablist"
      className={cn('flex items-center gap-1', variant === 'underline' && 'border-b border-line', className)}
    >
      {items.map((item, index) => {
        const active = item.id === value
        return (
          <button
            key={item.id}
            ref={(node) => {
              refs.current[index] = node
            }}
            role="tab"
            type="button"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(item.id)}
            onKeyDown={(event) => {
              if (event.key === 'ArrowRight') {
                event.preventDefault()
                move(index, 1)
              } else if (event.key === 'ArrowLeft') {
                event.preventDefault()
                move(index, -1)
              }
            }}
            className={cn(
              'relative inline-flex items-center gap-1.5 whitespace-nowrap text-small transition-colors duration-fast',
              variant === 'underline'
                ? cn(
                    '-mb-px h-9 border-b-2 px-1',
                    active
                      ? 'border-accent font-medium text-ink'
                      : 'border-transparent text-ink-2 hover:text-ink'
                  )
                : cn(
                    'h-7 rounded-control px-2',
                    active ? 'bg-surface-2 font-medium text-ink' : 'text-ink-2 hover:bg-surface-2 hover:text-ink'
                  )
            )}
          >
            {item.label}
            {item.count !== undefined && item.count > 0 && (
              <span className="tabular font-mono text-[11px] text-ink-3">{item.count}</span>
            )}
          </button>
        )
      })}
    </div>
  )
}

interface SegmentedControlProps<T extends string> {
  options: Array<{ value: T; label: ReactNode; icon?: ReactNode }>
  value: T
  onChange: (value: T) => void
  className?: string
  ariaLabel?: string
}

/** Two to four mutually exclusive choices that apply immediately (no Save). */
export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  className,
  ariaLabel,
}: SegmentedControlProps<T>) {
  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className={cn('inline-flex items-center gap-0.5 rounded-control border border-line bg-surface-2 p-0.5', className)}
    >
      {options.map((option) => {
        const active = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex h-6 items-center gap-1 rounded-[4px] px-2 text-micro transition-colors duration-fast',
              active ? 'bg-surface font-medium text-ink' : 'text-ink-2 hover:text-ink'
            )}
          >
            {option.icon}
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
