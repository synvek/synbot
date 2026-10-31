import { ArrowDown, MessageSquare } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useI18n } from '../../i18n/I18nContext'
import { EmptyState } from '../../ui'
import type { ChatMessage } from '../../types/websocket'
import { ApprovalCard } from './ApprovalCard'
import { AssistantTurn } from './AssistantTurn'
import { NoticeRow } from './NoticeRow'
import { ToolRow } from './ToolRow'
import { TurnSummary } from './TurnSummary'
import { UserTurn } from './UserTurn'

interface MessageStreamProps {
  messages: ChatMessage[]
  connected: boolean
  hasProject: boolean
  onApprove: (requestId: string) => void
  onReject: (requestId: string) => void
  onOpenFile: (turnId: string, path: string) => void
}

/** Distance from the bottom that still counts as "following the stream". */
const STICK_THRESHOLD = 48

/**
 * One column, one reading axis: every message type renders as a row on the same left edge. The
 * stream follows new content only while the reader is already at the bottom, so scrolling back
 * through a running turn is never yanked away.
 */
export function MessageStream({
  messages,
  connected,
  hasProject,
  onApprove,
  onReject,
  onOpenFile,
}: MessageStreamProps) {
  const { t } = useI18n()
  const scrollRef = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  const [atBottom, setAtBottom] = useState(true)

  const handleScroll = useCallback(() => {
    const node = scrollRef.current
    if (!node) return
    const distance = node.scrollHeight - node.scrollTop - node.clientHeight
    stick.current = distance <= STICK_THRESHOLD
    setAtBottom(stick.current)
  }, [])

  useEffect(() => {
    const node = scrollRef.current
    if (node && stick.current) node.scrollTop = node.scrollHeight
  }, [messages])

  const jumpToLatest = () => {
    const node = scrollRef.current
    if (!node) return
    node.scrollTop = node.scrollHeight
    stick.current = true
    setAtBottom(true)
  }

  const empty = !connected
    ? {
        title: t('chat.emptyOffline', 'Not connected'),
        body: t('chat.emptyOfflineHint', 'Waiting for the Synbot daemon on this host.'),
      }
    : !hasProject
      ? {
          title: t('chat.emptyNoProject', 'No workspace selected'),
          body: t('chat.emptyNoProjectHint', 'Pick a folder in the workspace panel to scope the agent.'),
        }
      : {
          title: t('chat.emptyTitle', 'Nothing here yet'),
          body: t('chat.emptyHint', 'Ask about the workspace, or type / for the available commands.'),
        }

  return (
    <div className="relative min-h-0 flex-1">
      <div ref={scrollRef} onScroll={handleScroll} className="h-full overflow-y-auto">
        <div className="mx-auto flex w-full max-w-transcript flex-col gap-5 px-6 py-5">
          {messages.length === 0 ? (
            <EmptyState
              title={empty.title}
              body={empty.body}
              icon={<MessageSquare className="h-6 w-6" aria-hidden="true" />}
            />
          ) : (
            messages.map((message) => {
              switch (message.role) {
                case 'user':
                  return <UserTurn key={message.id} content={message.content} timestamp={message.timestamp} />
                case 'assistant':
                  return (
                    <AssistantTurn
                      key={message.id}
                      content={message.content}
                      reasoning={message.reasoning}
                      timestamp={message.timestamp}
                      streaming={message.streaming}
                    />
                  )
                case 'approval':
                  return message.approvalRequest ? (
                    <ApprovalCard
                      key={message.id}
                      request={message.approvalRequest}
                      onApprove={onApprove}
                      onReject={onReject}
                      result={message.approvalResult}
                    />
                  ) : null
                case 'tool_call':
                case 'tool_result':
                  return (
                    <ToolRow
                      key={message.id}
                      name={message.toolName ?? message.content}
                      status={message.toolStatus}
                      args={message.toolArgs}
                      result={message.toolResult}
                      durationMs={message.toolDurationMs}
                    />
                  )
                case 'notice':
                  return <NoticeRow key={message.id} content={message.content} />
                case 'turn':
                  return (
                    <TurnSummary
                      key={message.id}
                      durationMs={message.turnDurationMs}
                      totalTokens={message.turnTokens?.total}
                      reason={message.turnReason}
                      files={message.turnFiles ?? []}
                      turnId={message.turnId}
                      onOpenFile={onOpenFile}
                    />
                  )
                default:
                  return null
              }
            })
          )}
        </div>
      </div>

      {!atBottom && messages.length > 0 && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="absolute bottom-3 left-1/2 inline-flex -translate-x-1/2 items-center gap-1.5 rounded-pill border border-line bg-surface px-2.5 py-1 text-micro text-ink-2 shadow-overlay transition-colors duration-fast hover:text-ink"
        >
          <ArrowDown className="h-3 w-3" aria-hidden="true" />
          {t('chat.jumpToLatest', 'Jump to latest')}
        </button>
      )}
    </div>
  )
}
