import { useCallback, useState } from 'react'
import { Check, Circle, ListChecks, Loader2, X } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { Badge, Button } from '../../ui'
import type { PendingPlan, PlanStep } from '../../types/agentEvents'

interface PlanCardProps {
  plan: PendingPlan
  /** Approved: the turn that executes the plan is starting. */
  approved: boolean
  onDecide: (approve: boolean) => void
}

function StepIcon({ status }: { status: PlanStep['status'] }) {
  switch (status) {
    case 'completed':
      return <Check className="h-3 w-3 shrink-0 text-success" aria-hidden="true" />
    case 'in_progress':
      return <Loader2 className="h-3 w-3 shrink-0 animate-spin text-accent" aria-hidden="true" />
    case 'cancelled':
      return <X className="h-3 w-3 shrink-0 text-ink-3" aria-hidden="true" />
    default:
      return <Circle className="h-3 w-3 shrink-0 text-ink-3" aria-hidden="true" />
  }
}

/**
 * A plan-mode turn is a read-only dry run, so its output is a decision, not a transcript line: the
 * steps the agent intends to take, the two ways out, and nothing running until one is chosen.
 *
 * The card outlives the click on purpose — after approval it says the plan is executing until the
 * turn that runs it starts.
 */
export function PlanCard({ plan, approved, onDecide }: PlanCardProps) {
  const { t } = useI18n()
  const [decided, setDecided] = useState<'approve' | 'cancel' | null>(null)
  const decidedHere = decided !== null
  const running = approved || decided === 'approve'

  const decide = useCallback(
    (approve: boolean) => {
      if (decidedHere) return
      setDecided(approve ? 'approve' : 'cancel')
      onDecide(approve)
    },
    [decidedHere, onDecide]
  )

  return (
    <div className="my-1 rounded-panel border border-line border-l-2 border-l-accent bg-surface p-3">
      <div className="flex items-center gap-2">
        <ListChecks className="h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" />
        <h3 className="text-small font-medium text-ink">{t('chat.planTitle', '计划待确认')}</h3>
        <span className="tabular ml-auto font-mono text-micro text-ink-3">
          {t('chat.planStepCount', { count: plan.steps.length })}
        </span>
      </div>

      {plan.summary && (
        <p className="mt-2 whitespace-pre-wrap break-words text-small text-ink-2">{plan.summary}</p>
      )}

      <ol className="mt-2 space-y-1">
        {plan.steps.map((step, index) => (
          <li key={step.id} className="flex items-baseline gap-2 text-small">
            <span className="tabular w-4 shrink-0 text-right font-mono text-micro text-ink-3">
              {index + 1}
            </span>
            <StepIcon status={step.status} />
            <span
              className={
                step.status === 'cancelled' ? 'text-ink-3 line-through' : 'break-words text-ink-2'
              }
            >
              {step.title}
            </span>
          </li>
        ))}
      </ol>

      <div className="mt-3">
        {running ? (
          <Badge tone="neutral">
            <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
            {t('chat.planRunning', '执行中…')}
          </Badge>
        ) : decided === 'cancel' ? (
          <Badge tone="neutral">{t('chat.planCancelled', '已取消')}</Badge>
        ) : (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="primary" size="sm" icon={<Check className="h-3.5 w-3.5" />} onClick={() => decide(true)}>
              {t('chat.planApprove', '批准执行')}
            </Button>
            <Button variant="secondary" size="sm" icon={<X className="h-3.5 w-3.5" />} onClick={() => decide(false)}>
              {t('chat.planCancel', '取消')}
            </Button>
            <span className="text-micro text-ink-3">
              {t('chat.planHint', '批准前不会修改任何文件')}
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
