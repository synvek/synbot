import { useI18n } from '../../i18n/I18nContext'
import { formatClock } from './format'

interface TurnMetaProps {
  /** Who spoke: "You", "Assistant". */
  label: string
  timestamp: string
}

/** The one metadata line under a message: speaker, clock. Quiet, never a chip. */
export function TurnMeta({ label, timestamp }: TurnMetaProps) {
  return (
    <div className="flex items-center gap-1.5 text-micro text-ink-3">
      <span className="font-medium">{label}</span>
      <span aria-hidden="true">·</span>
      <time className="tabular" dateTime={timestamp}>
        {formatClock(timestamp)}
      </time>
    </div>
  )
}

interface UserTurnProps {
  content: string
  timestamp: string
}

/**
 * The user's turn is an inset block, not a bubble: same left edge as everything else so a long
 * answer and the question that produced it stay on one reading axis.
 */
export function UserTurn({ content, timestamp }: UserTurnProps) {
  const { t } = useI18n()

  return (
    <div className="flex flex-col gap-1.5">
      <div className="whitespace-pre-wrap break-words rounded-panel border border-line bg-surface-2 px-3.5 py-2.5 text-body text-ink">
        {content}
      </div>
      <TurnMeta label={t('chat.user', 'You')} timestamp={timestamp} />
    </div>
  )
}
