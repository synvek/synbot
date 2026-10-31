import type { ReactNode } from 'react'
import { cn } from './cn'

export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'

const TONE_CLASS: Record<BadgeTone, string> = {
  neutral: 'border-line bg-surface-2 text-ink-2',
  accent: 'tone-accent',
  success: 'tone-success',
  warning: 'tone-warning',
  danger: 'tone-danger',
  info: 'tone-info',
}

interface BadgeProps {
  children: ReactNode
  tone?: BadgeTone
  /** Square corners for counts inside a header; pills for states. */
  shape?: 'pill' | 'square'
  className?: string
}

/** A state chip. One per row is the rule; a wall of coloured chips is not a status system. */
export function Badge({ children, tone = 'neutral', shape = 'pill', className }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 border px-1.5 py-0.5 text-micro font-medium leading-4',
        shape === 'pill' ? 'rounded-pill' : 'rounded-control',
        TONE_CLASS[tone],
        className
      )}
    >
      {children}
    </span>
  )
}

export type StatusTone = 'idle' | 'running' | 'success' | 'warning' | 'error'

const DOT_CLASS: Record<StatusTone, string> = {
  idle: 'bg-ink-3',
  running: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-error',
}

interface StatusDotProps {
  tone: StatusTone
  /** Breathing dot for live work; static otherwise. Reduced motion stops the breathing. */
  pulse?: boolean
  label?: string
  className?: string
}

export function StatusDot({ tone, pulse = false, label, className }: StatusDotProps) {
  return (
    <span
      role={label ? 'img' : undefined}
      aria-label={label}
      title={label}
      className={cn(
        'inline-block h-1.5 w-1.5 shrink-0 rounded-full',
        DOT_CLASS[tone],
        tone === 'running' || pulse ? 'animate-dot-breathe' : '',
        className
      )}
    />
  )
}

export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        'inline-flex h-5 min-w-5 items-center justify-center rounded-[4px] border border-line-strong bg-surface-2 px-1',
        'font-mono text-[11px] leading-none text-ink-2',
        className
      )}
    >
      {children}
    </kbd>
  )
}
