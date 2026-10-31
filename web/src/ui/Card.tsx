import type { ReactNode } from 'react'
import { cn } from './cn'

interface CardProps {
  children: ReactNode
  className?: string
  /** Panels are flat by default: a 1px line, never a shadow. */
  tone?: 'surface' | 'plain'
}

export function Card({ children, className, tone = 'surface' }: CardProps) {
  return (
    <div
      className={cn(
        'rounded-panel border border-line',
        tone === 'surface' ? 'bg-surface' : 'bg-transparent',
        className
      )}
    >
      {children}
    </div>
  )
}

interface SectionHeaderProps {
  title: ReactNode
  description?: ReactNode
  actions?: ReactNode
  className?: string
}

/** A title row inside a card or a section: 16/600, one optional action on the right. */
export function SectionHeader({ title, description, actions, className }: SectionHeaderProps) {
  return (
    <div className={cn('flex items-start justify-between gap-4', className)}>
      <div className="min-w-0">
        <h2 className="text-section font-semibold text-ink">{title}</h2>
        {description && <p className="mt-0.5 text-small text-ink-2">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  )
}

export type StatTone = 'neutral' | 'success' | 'warning' | 'danger' | 'accent'

const STAT_VALUE_CLASS: Record<StatTone, string> = {
  neutral: 'text-ink',
  success: 'text-success',
  warning: 'text-warning',
  danger: 'text-error',
  accent: 'text-accent',
}

interface StatTileProps {
  label: ReactNode
  value: ReactNode
  hint?: ReactNode
  tone?: StatTone
  icon?: ReactNode
  /** Right-hand slot, e.g. a sparkline or a status dot. */
  aside?: ReactNode
  className?: string
}

/** A single number with its label. The number is mono-tabular so a changing value does not jitter. */
export function StatTile({ label, value, hint, tone = 'neutral', icon, aside, className }: StatTileProps) {
  return (
    <div className={cn('flex flex-col gap-2 px-4 py-3', className)}>
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-micro uppercase tracking-wide text-ink-3">
          {icon}
          {label}
        </span>
        {aside}
      </div>
      <span className={cn('tabular font-mono text-[22px] leading-none tracking-tight', STAT_VALUE_CLASS[tone])}>
        {value}
      </span>
      {hint && <span className="text-micro text-ink-3">{hint}</span>}
    </div>
  )
}

interface SparklineProps {
  values: number[]
  className?: string
  label?: string
}

/** A 32×16 trend mark. No axes, no labels: it only has to show a direction next to a number. */
export function Sparkline({ values, className, label }: SparklineProps) {
  if (values.length === 0) return null
  const max = Math.max(...values, 1)
  const min = Math.min(...values, 0)
  const span = max - min || 1
  const points = values
    .map((value, index) => {
      const x = values.length === 1 ? 0 : (index / (values.length - 1)) * 32
      const y = 15 - ((value - min) / span) * 14
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(' ')
  return (
    <svg
      width={32}
      height={16}
      viewBox="0 0 32 16"
      role={label ? 'img' : 'presentation'}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      className={cn('text-accent', className)}
    >
      <polyline
        points={points}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.25}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}
