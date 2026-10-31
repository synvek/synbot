import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react'
import { Check, Info, TriangleAlert, X } from 'lucide-react'
import { cn } from './cn'

export type ToastTone = 'success' | 'error' | 'info'

interface Toast {
  id: number
  message: ReactNode
  tone: ToastTone
}

interface ToastContextValue {
  toast: (message: ReactNode, tone?: ToastTone) => void
}

const ToastContext = createContext<ToastContextValue | undefined>(undefined)

export function useToast() {
  const context = useContext(ToastContext)
  if (!context) throw new Error('useToast must be used within a ToastProvider')
  return context
}

const TONE_ICON = {
  success: <Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />,
  error: <TriangleAlert className="h-3.5 w-3.5 text-error" aria-hidden="true" />,
  info: <Info className="h-3.5 w-3.5 text-accent" aria-hidden="true" />,
}

/**
 * Confirmations for actions that leave no visible trace (saved, copied, deleted). Anything that
 * changes a row in place should just change the row; a toast is not a substitute for state.
 */
export function ToastProvider({ children, dismissLabel = 'Dismiss' }: { children: ReactNode; dismissLabel?: string }) {
  const [toasts, setToasts] = useState<Toast[]>([])

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((item) => item.id !== id))
  }, [])

  const toast = useCallback(
    (message: ReactNode, tone: ToastTone = 'success') => {
      const id = Date.now() + Math.random()
      setToasts((current) => [...current.slice(-2), { id, message, tone }])
      window.setTimeout(() => dismiss(id), 4000)
    },
    [dismiss]
  )

  const value = useMemo(() => ({ toast }), [toast])

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div
        className="pointer-events-none fixed bottom-4 right-4 z-[60] flex flex-col items-end gap-2"
        role="status"
        aria-live="polite"
      >
        {toasts.map((item) => (
          <div
            key={item.id}
            className={cn(
              'pointer-events-auto flex max-w-[360px] items-start gap-2 rounded-panel border border-line bg-surface px-3 py-2 shadow-overlay'
            )}
          >
            <span className="mt-0.5">{TONE_ICON[item.tone]}</span>
            <span className="flex-1 text-small text-ink">{item.message}</span>
            <button
              type="button"
              aria-label={dismissLabel}
              onClick={() => dismiss(item.id)}
              className="text-ink-3 transition-colors duration-fast hover:text-ink"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  )
}
