import { ChevronRight } from 'lucide-react'

interface ReasoningRowProps {
  text: string
  label: string
}

/**
 * Reasoning is an annotation, not content: it collapses to one line and, when opened, reads as a
 * quoted block so it can never be mistaken for the answer.
 */
export function ReasoningRow({ text, label }: ReasoningRowProps) {
  return (
    <details className="group text-micro text-ink-3">
      <summary className="inline-flex cursor-pointer select-none list-none items-center gap-1 rounded-control py-0.5 transition-colors duration-fast hover:text-ink-2 [&::-webkit-details-marker]:hidden">
        <ChevronRight
          className="h-3 w-3 transition-transform duration-fast group-open:rotate-90"
          aria-hidden="true"
        />
        {label}
      </summary>
      <div className="mt-1 whitespace-pre-wrap break-words border-l border-line pl-3 text-small text-ink-2">
        {text}
      </div>
    </details>
  )
}
