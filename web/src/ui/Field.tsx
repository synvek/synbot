import {
  forwardRef,
  useId,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from 'react'
import { cn } from './cn'

/**
 * One field = label + control + hint or error. The label is wired to the control with an id, so
 * the whole console gets clickable labels for free; the error replaces the hint in place rather
 * than stacking a third line.
 */
interface FieldProps {
  label: ReactNode
  htmlFor?: string
  hint?: ReactNode
  error?: ReactNode
  required?: boolean
  className?: string
  children: ReactNode
}

export function Field({ label, htmlFor, hint, error, required, className, children }: FieldProps) {
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <label htmlFor={htmlFor} className="text-small font-medium text-ink">
        {label}
        {required && (
          <span className="ml-0.5 text-error" aria-hidden="true">
            *
          </span>
        )}
      </label>
      {children}
      {error ? (
        <p className="text-micro text-error" role="alert">
          {error}
        </p>
      ) : hint ? (
        <p className="text-micro text-ink-3">{hint}</p>
      ) : null}
    </div>
  )
}

const CONTROL_CLASS =
  'w-full rounded-control border border-line-strong bg-surface px-2.5 text-body text-ink ' +
  'placeholder:text-ink-3 transition-colors duration-fast ease-out ' +
  'hover:border-ink-3 disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-ink-3 ' +
  'aria-[invalid=true]:border-error'

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...rest }, ref) {
    return <input ref={ref} className={cn(CONTROL_CLASS, 'h-8', className)} {...rest} />
  }
)

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  function Textarea({ className, rows = 3, ...rest }, ref) {
    return (
      <textarea
        ref={ref}
        rows={rows}
        className={cn(CONTROL_CLASS, 'resize-y py-1.5 leading-relaxed', className)}
        {...rest}
      />
    )
  }
)

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  function Select({ className, children, ...rest }, ref) {
    return (
      <select ref={ref} className={cn(CONTROL_CLASS, 'h-8 pr-1.5', className)} {...rest}>
        {children}
      </select>
    )
  }
)

interface SwitchProps {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  disabled?: boolean
  id?: string
  className?: string
}

export function Switch({ checked, onChange, label, disabled, id, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      id={id}
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        'relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-pill border transition-colors duration-base',
        checked ? 'border-accent bg-accent' : 'border-line-strong bg-surface-2',
        'disabled:cursor-not-allowed disabled:opacity-50',
        className
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          'block h-3 w-3 rounded-full bg-white shadow-sm transition-transform duration-base ease-out',
          checked ? 'translate-x-4' : 'translate-x-0.5'
        )}
      />
    </button>
  )
}

interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label: ReactNode
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox(
  { label, className, id, ...rest },
  ref
) {
  const generated = useId()
  const inputId = id ?? generated
  return (
    <label htmlFor={inputId} className={cn('flex items-center gap-2 text-small text-ink', className)}>
      <input
        ref={ref}
        id={inputId}
        type="checkbox"
        className="h-3.5 w-3.5 rounded-sm border-line-strong accent-[var(--color-accent)]"
        {...rest}
      />
      {label}
    </label>
  )
})

interface RadioProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label: ReactNode
}

export const Radio = forwardRef<HTMLInputElement, RadioProps>(function Radio(
  { label, className, id, ...rest },
  ref
) {
  const generated = useId()
  const inputId = id ?? generated
  return (
    <label htmlFor={inputId} className={cn('flex items-center gap-2 text-small text-ink', className)}>
      <input
        ref={ref}
        id={inputId}
        type="radio"
        className="h-3.5 w-3.5 border-line-strong accent-[var(--color-accent)]"
        {...rest}
      />
      {label}
    </label>
  )
})
