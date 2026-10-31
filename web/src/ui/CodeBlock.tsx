import { useState, type ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'
import { cn } from './cn'

interface CodeBlockProps {
  children: string
  /** Shown above the block: a path, a language, a command name. */
  title?: ReactNode
  maxHeight?: number
  copyable?: boolean
  className?: string
  wrap?: boolean
}

export function CodeBlock({ children, title, maxHeight = 320, copyable = true, className, wrap = true }: CodeBlockProps) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(children)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      /* Clipboard denied: the text stays selectable, which is the fallback. */
    }
  }

  return (
    <div className={cn('overflow-hidden rounded-panel border border-line bg-surface-2', className)}>
      {(title || copyable) && (
        <div className="flex items-center justify-between gap-2 border-b border-line px-2.5 py-1">
          <span className="truncate font-mono text-micro text-ink-3">{title}</span>
          {copyable && (
            <button
              type="button"
              onClick={copy}
              aria-label={copied ? 'Copied' : 'Copy'}
              className="inline-flex items-center gap-1 text-micro text-ink-3 transition-colors duration-fast hover:text-ink"
            >
              {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          )}
        </div>
      )}
      <pre
        className={cn(
          'overflow-auto px-2.5 py-2 font-mono text-micro leading-relaxed text-ink',
          wrap ? 'whitespace-pre-wrap break-words' : 'whitespace-pre'
        )}
        style={{ maxHeight }}
      >
        <code>{children}</code>
      </pre>
    </div>
  )
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn('animate-pulse rounded-control bg-surface-2', className)} aria-hidden="true" />
}

/** Vertical scroll container that owns its scrollbar; used by panels and transcript lists. */
export function ScrollArea({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('min-h-0 overflow-y-auto', className)}>{children}</div>
}
