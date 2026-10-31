import { ChevronRight } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { CodeBlock, StatusDot } from '../../ui'
import type { StatusTone } from '../../ui'
import { formatDuration, summarize } from './format'

interface ToolRowProps {
  name: string
  status?: 'running' | 'success' | 'failure'
  args?: string
  result?: string
  durationMs?: number
}

const STATUS_TONE: Record<'running' | 'success' | 'failure', StatusTone> = {
  running: 'running',
  success: 'success',
  failure: 'error',
}

/**
 * A tool call is one 28px line: state, name, a clipped argument summary, elapsed time. The payload
 * only exists while the row is open, which is what keeps a forty-call turn scannable.
 */
export function ToolRow({ name, status = 'success', args, result, durationMs }: ToolRowProps) {
  const { t } = useI18n()
  const statusLabel =
    status === 'running'
      ? t('chat.toolRunning', 'running')
      : status === 'failure'
        ? t('chat.toolFailed', 'failed')
        : t('chat.toolDone', 'done')

  return (
    <details className="group" open={status === 'failure'}>
      <summary className="flex h-7 cursor-pointer list-none items-center gap-2 rounded-control px-1 text-micro text-ink-2 transition-colors duration-fast hover:bg-surface-2 [&::-webkit-details-marker]:hidden">
        <StatusDot tone={STATUS_TONE[status]} />
        <span className="shrink-0 font-mono text-ink">{name}</span>
        {args && <span className="min-w-0 flex-1 truncate font-mono text-ink-3">{summarize(args)}</span>}
        <span className="sr-only">{statusLabel}</span>
        {durationMs !== undefined && (
          <span className="tabular shrink-0 font-mono text-ink-3">{formatDuration(durationMs)}</span>
        )}
        <ChevronRight
          className="h-3 w-3 shrink-0 text-ink-3 transition-transform duration-fast group-open:rotate-90"
          aria-hidden="true"
        />
      </summary>
      {(args || result) && (
        <div className="space-y-1.5 py-2 pl-6">
          {args && (
            <CodeBlock title={t('chat.toolArgs', 'Arguments')} copyable={false} maxHeight={200}>
              {args}
            </CodeBlock>
          )}
          {result && (
            <CodeBlock title={t('chat.toolOutput', 'Output')} maxHeight={320}>
              {result}
            </CodeBlock>
          )}
        </div>
      )}
    </details>
  )
}
