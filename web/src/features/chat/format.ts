import { formatClock as clock } from '../../utils/datetime'

/** Turn reasons that mean the turn failed. */
export const FAILED_TURN_REASONS = ['error', 'max_tool_errors']

/** Stopped early on purpose (budget exhausted, no progress): a warning, not a failure. */
export const WARNING_TURN_REASONS = ['max_iterations', 'no_progress']

/** Compact duration for tool rows and turn summaries: 840ms, 3.2s, 1m 04s. */
export function formatDuration(ms?: number): string {
  if (ms === undefined) return ''
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  const minutes = Math.floor(ms / 60_000)
  const seconds = Math.round((ms % 60_000) / 1000)
  return `${minutes}m ${String(seconds).padStart(2, '0')}s`
}

/** Clock time for a message; the locale follows the interface language. */
export function formatClock(iso: string): string {
  return clock(iso)
}

/** One line of a tool payload: collapsed whitespace, clipped so the row never wraps. */
export function summarize(payload: string, limit = 120): string {
  const line = payload.replace(/\s+/g, ' ').trim()
  return line.length > limit ? `${line.slice(0, limit)}…` : line
}
