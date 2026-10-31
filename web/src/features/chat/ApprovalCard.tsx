import { useCallback, useState } from 'react'
import { Check, Lock, X } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { formatClock } from '../../utils/datetime'
import { Badge, Button, CodeBlock } from '../../ui'
import type { ApprovalRequest as ApprovalRequestType } from '../../types/websocket'

interface ApprovalCardProps {
  request: ApprovalRequestType
  onApprove: (requestId: string) => void
  onReject: (requestId: string) => void
  result?: {
    approved: boolean
    message: string
    /** Set when another surface (phone, another tab) answered or the request timed out. */
    responder?: string
  }
}

/**
 * An approval is the one thing in a transcript that blocks the agent, so it is the one card that
 * gets a coloured rule: warning on the left, the command verbatim, two decisions, nothing else.
 */
export function ApprovalCard({ request, onApprove, onReject, result }: ApprovalCardProps) {
  const { t } = useI18n()
  const [responded, setResponded] = useState(!!result)

  const handleApprove = useCallback(() => {
    onApprove(request.id)
    setResponded(true)
  }, [onApprove, request.id])

  const handleReject = useCallback(() => {
    onReject(request.id)
    setResponded(true)
  }, [onReject, request.id])

  const settled = responded || !!result
  /** Clicked here but the daemon has not echoed the receipt yet. */
  const awaiting = responded && !result
  const outcome = result?.responder
    ? t('approval.resolvedElsewhere', {
        responder: result.responder,
        outcome: result.approved ? t('approval.approved', 'Approved') : t('approval.rejected', 'Rejected'),
      })
    : result?.message ||
      (result?.approved ? t('approval.approved', 'Approved') : t('approval.rejected', 'Rejected'))

  return (
    <div className="my-1 rounded-panel border border-line border-l-2 border-l-warning bg-surface p-3">
      <div className="flex items-center gap-2">
        <Lock className="h-3.5 w-3.5 shrink-0 text-warning" aria-hidden="true" />
        <h3 className="text-small font-medium text-ink">{t('approval.title', 'Command approval')}</h3>
        <span className="tabular ml-auto font-mono text-micro text-ink-3">
          {formatClock(request.timestamp)} · {request.timeout_secs}s
        </span>
      </div>

      <div className="mt-2.5 space-y-2">
        <CodeBlock title={t('approval.command', 'Command')} maxHeight={160}>
          {request.command}
        </CodeBlock>

        <div className="flex flex-wrap items-baseline gap-x-2 text-micro">
          <span className="text-ink-3">{t('approval.workingDir', 'Working directory')}</span>
          <span className="break-all font-mono text-ink-2">{request.working_dir}</span>
        </div>

        <div className="text-small text-ink-2">
          <span className="mr-2 text-micro text-ink-3">{t('approval.context', 'Context')}</span>
          <span className="whitespace-pre-wrap break-words">{request.context}</span>
        </div>
      </div>

      <div className="mt-3">
        {settled ? (
          <div className="flex items-center gap-2">
            {awaiting ? (
              <Badge tone="neutral">{t('approval.pending', 'Waiting for the daemon…')}</Badge>
            ) : (
              <Badge tone={result?.approved ? 'success' : 'danger'}>{outcome}</Badge>
            )}
          </div>
        ) : (
          <div className="flex items-center gap-2">
            <Button variant="primary" size="sm" icon={<Check className="h-3.5 w-3.5" />} onClick={handleApprove}>
              {t('approval.approve', 'Approve')}
            </Button>
            <Button variant="secondary" size="sm" icon={<X className="h-3.5 w-3.5" />} onClick={handleReject}>
              {t('approval.reject', 'Reject')}
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
