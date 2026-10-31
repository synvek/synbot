interface NoticeRowProps {
  content: string
}

/**
 * System notes (`/clear`, workflow progress) sit on the reading axis as a left-ruled line — the
 * one shape in the transcript that is neither an answer nor a tool.
 */
export function NoticeRow({ content }: NoticeRowProps) {
  return (
    <div className="whitespace-pre-wrap break-words border-l border-line pl-3 text-micro text-ink-2">
      {content}
    </div>
  )
}
