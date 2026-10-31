import { useI18n } from '../../i18n/I18nContext'
import { Badge } from '../../ui'
import { FAILED_TURN_REASONS, WARNING_TURN_REASONS, formatDuration } from './format'

interface TurnSummaryProps {
  durationMs?: number
  totalTokens?: number
  reason?: string
  files: { path: string; kind: string }[]
  turnId?: string
  onOpenFile: (turnId: string, path: string) => void
}

/**
 * The end of a turn: one rule, one metric line, and the files it touched as openable chips.
 * Everything a finished turn is accountable for, in two rows.
 */
export function TurnSummary({
  durationMs,
  totalTokens,
  reason,
  files,
  turnId,
  onOpenFile,
}: TurnSummaryProps) {
  const { t } = useI18n()
  const failed = FAILED_TURN_REASONS.includes(reason ?? '')
  const warning = WARNING_TURN_REASONS.includes(reason ?? '')
  const outcome = reason && reason !== 'completed' ? t(`chat.turnReason.${reason}`, reason) : null

  return (
    <div
      className="flex flex-col gap-2 border-t border-line pt-2"
      title={reason ? t(`chat.turnReasonHint.${reason}`, '') : undefined}
    >
      <div className="tabular flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-micro text-ink-3">
        <span>{formatDuration(durationMs)}</span>
        {totalTokens !== undefined && (
          <span>{t('chat.tokens', { total: totalTokens.toLocaleString() })}</span>
        )}
        {files.length > 0 && <span>{t('chat.filesChanged', { count: files.length })}</span>}
        {outcome && (
          <Badge tone={failed ? 'danger' : warning ? 'warning' : 'neutral'}>{outcome}</Badge>
        )}
      </div>
      {files.length > 0 && (
        <ul className="flex flex-wrap gap-1">
          {files.map((file) => (
            <li key={file.path} className="max-w-full">
              <button
                type="button"
                disabled={!turnId}
                onClick={() => turnId && onOpenFile(turnId, file.path)}
                title={t('chat.viewDiff', 'View diff')}
                className="inline-flex max-w-full items-center gap-1 rounded-control border border-line bg-surface px-1.5 py-0.5 font-mono text-micro text-ink-2 transition-colors duration-fast hover:border-line-strong hover:text-ink disabled:cursor-default"
              >
                <span className="shrink-0 text-ink-3">{file.kind}</span>
                <span className="truncate">{file.path}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
