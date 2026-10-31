import { useEffect, useRef, type ReactNode } from 'react'
import { X } from 'lucide-react'
import { IconButton } from './Button'
import { cn } from './cn'

interface DialogProps {
  open: boolean
  onClose: () => void
  title: ReactNode
  description?: ReactNode
  children?: ReactNode
  footer?: ReactNode
  /** `sm` for confirmations, `md` for forms, `lg` for content viewers. */
  width?: 'sm' | 'md' | 'lg'
  /** Accessible name for the corner close button; the caller passes the translated one. */
  closeLabel?: string
}

const WIDTH_CLASS = { sm: 'max-w-[400px]', md: 'max-w-[520px]', lg: 'max-w-[760px]' }

/**
 * A native <dialog> in modal mode: the browser owns the top layer, the focus trap, Esc, the
 * backdrop, and returning focus to whatever opened it. We only style the inside.
 */
export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 'md',
  closeLabel = 'Close',
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null)
  const titleId = useRef(`dialog-title-${Math.random().toString(36).slice(2, 9)}`)

  useEffect(() => {
    const node = ref.current
    if (!node) return
    if (open && !node.open) {
      // Older WebKit and test environments ship <dialog> without the modal API; the attribute
      // fallback still renders, it just does not trap focus.
      if (typeof node.showModal === 'function') node.showModal()
      else node.setAttribute('open', '')
    }
    if (!open && node.open) {
      if (typeof node.close === 'function') node.close()
      else node.removeAttribute('open')
    }
  }, [open])

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId.current}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClose={() => {
        if (open) onClose()
      }}
      className={cn('w-[calc(100vw-32px)] p-0', WIDTH_CLASS[width])}
    >
      <div className="flex items-start justify-between gap-4 border-b border-line px-4 py-3">
        <div className="min-w-0">
          <h2 id={titleId.current} className="text-section font-semibold text-ink">
            {title}
          </h2>
          {description && <p className="mt-0.5 text-small text-ink-2">{description}</p>}
        </div>
        <IconButton label={closeLabel} size="sm" onClick={onClose}>
          <X className="h-4 w-4" />
        </IconButton>
      </div>
      {children && <div className="max-h-[60vh] overflow-y-auto px-4 py-4">{children}</div>}
      {footer && <div className="flex justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>}
    </dialog>
  )
}
