/**
 * Locale-aware date and duration formatting for the console.
 *
 * The i18n provider keeps `<html lang>` in sync ("zh-CN" or "en"), which is the single source of
 * truth a non-React helper can read. Call sites used plain `toLocaleString()`, so a Chinese UI
 * printed `2/11/2026, 5:24:18 PM` — same data, wrong language.
 */

function uiLocale(): string {
  return document.documentElement.lang === 'zh-CN' ? 'zh-CN' : 'en-US'
}

function parse(value: string | number | Date | null | undefined): Date | null {
  if (value === null || value === undefined || value === '') return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

/** Date + time, as a table cell or detail row shows it. `fallback` covers null and bad input. */
export function formatDateTime(
  value: string | number | Date | null | undefined,
  fallback = '—'
): string {
  const date = parse(value)
  return date ? date.toLocaleString(uiLocale()) : fallback
}

/**
 * Compact date + time for table cells: "Feb 11, 5:24 PM" / "2月11日 17:24".
 * The full form wraps to two lines inside a fixed-width column, which breaks row rhythm.
 */
export function formatShortDateTime(
  value: string | number | Date | null | undefined,
  fallback = '—'
): string {
  const date = parse(value)
  if (!date) return fallback
  const sameYear = date.getFullYear() === new Date().getFullYear()
  return date.toLocaleString(uiLocale(), {
    ...(sameYear ? {} : { year: 'numeric' }),
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/** Clock only: log lines, audit rows, message metadata. */
export function formatClock(value: string | number | Date | null | undefined, fallback = ''): string {
  const date = parse(value)
  return date ? date.toLocaleTimeString(uiLocale()) : fallback
}

/** Date only, for grouped headings. */
export function formatDate(value: string | number | Date | null | undefined, fallback = '—'): string {
  const date = parse(value)
  return date ? date.toLocaleDateString(uiLocale()) : fallback
}

/** Seconds → "3d 4h 12m" / "3 天 4 小时 12 分"; the console never shows raw uptime seconds. */
export function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const zh = uiLocale() === 'zh-CN'
  const day = zh ? '天' : 'd'
  const hour = zh ? '小时' : 'h'
  const minute = zh ? '分' : 'm'
  const glue = zh ? ' ' : ' '
  if (days > 0) return [days + day, hours + hour, minutes + minute].join(glue)
  if (hours > 0) return [hours + hour, minutes + minute].join(glue)
  return minutes + minute
}
