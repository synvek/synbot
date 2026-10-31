import Markdown from '../../components/Markdown'
import { useI18n } from '../../i18n/I18nContext'
import { ReasoningRow } from './ReasoningRow'
import { TurnMeta } from './UserTurn'

interface AssistantTurnProps {
  content: string
  reasoning?: string
  timestamp: string
  streaming?: boolean
}

/**
 * Assistant text is plain content on the page: no container, no fill, so long answers keep the
 * full 760px measure and code, tables and prose read as one document.
 */
export function AssistantTurn({ content, reasoning, timestamp, streaming = false }: AssistantTurnProps) {
  const { t } = useI18n()

  return (
    <div className="flex flex-col gap-1.5">
      {reasoning && <ReasoningRow text={reasoning} label={t('chat.reasoning', 'Reasoning')} />}
      <div className="text-body text-ink">
        <Markdown content={content} />
        {streaming && (
          <span className="mt-1 block h-4 w-px animate-dot-breathe bg-accent" aria-hidden="true" />
        )}
      </div>
      <TurnMeta label={t('chat.assistant', 'Assistant')} timestamp={timestamp} />
    </div>
  )
}
