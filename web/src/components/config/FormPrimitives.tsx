import React, { useEffect, useState } from 'react'
import { CONFIG_SECRET_MASK } from './immutable'

const inputClass =
  'w-full px-3 py-2 rounded-lg bg-surface border border-border text-text text-sm focus:outline-none focus:ring-2 focus:ring-primary/30'
const labelClass = 'block text-sm font-medium text-text mb-1'

type FieldProps = {
  id?: string
  label: string
  className?: string
}

export const TextField: React.FC<
  FieldProps & {
    value: string
    onChange: (v: string) => void
    placeholder?: string
    disabled?: boolean
  }
> = ({ id, label, value, onChange, placeholder, disabled, className = '' }) => (
  <div className={className}>
    <label htmlFor={id} className={labelClass}>
      {label}
    </label>
    <input
      id={id}
      type="text"
      className={inputClass}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      disabled={disabled}
    />
  </div>
)

export const NumberField: React.FC<
  FieldProps & {
    value: number
    onChange: (v: number) => void
    min?: number
    max?: number
    step?: number
    disabled?: boolean
  }
> = ({ id, label, value, onChange, min, max, step, disabled, className = '' }) => (
  <div className={className}>
    <label htmlFor={id} className={labelClass}>
      {label}
    </label>
    <input
      id={id}
      type="number"
      className={inputClass}
      value={Number.isFinite(value) ? value : 0}
      onChange={(e) => onChange(Number(e.target.value))}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
    />
  </div>
)

export const Toggle: React.FC<
  FieldProps & {
    checked: boolean
    onChange: (v: boolean) => void
    disabled?: boolean
  }
> = ({ id, label, checked, onChange, disabled, className = '' }) => (
  <div className={`flex items-center gap-2 ${className}`}>
    <input
      id={id}
      type="checkbox"
      className="rounded border-border text-primary focus:ring-primary/30"
      checked={checked}
      onChange={(e) => onChange(e.target.checked)}
      disabled={disabled}
    />
    <label htmlFor={id} className="text-sm text-text cursor-pointer">
      {label}
    </label>
  </div>
)

export const SelectField: React.FC<
  FieldProps & {
    value: string
    onChange: (v: string) => void
    options: { value: string; label: string }[]
    disabled?: boolean
  }
> = ({ id, label, value, onChange, options, disabled, className = '' }) => (
  <div className={className}>
    <label htmlFor={id} className={labelClass}>
      {label}
    </label>
    <select
      id={id}
      className={inputClass}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  </div>
)

/**
 * For masked secrets from GET /api/config: local edit until blur, then commit MASK or new secret.
 */
export const SecretField: React.FC<
  FieldProps & {
    value: string
    onChange: (v: string) => void
    placeholder?: string
    leaveUnchangedHint?: string
  }
> = ({ id, label, value, onChange, placeholder, leaveUnchangedHint, className = '' }) => {
  const [editing, setEditing] = useState(false)
  const [local, setLocal] = useState('')

  useEffect(() => {
    if (!editing) setLocal(value === CONFIG_SECRET_MASK ? '' : value)
  }, [value, editing])

  const display = editing ? local : value === CONFIG_SECRET_MASK ? CONFIG_SECRET_MASK : value

  return (
    <div className={className}>
      <label htmlFor={id} className={labelClass}>
        {label}
      </label>
      <input
        id={id}
        type="password"
        autoComplete="new-password"
        className={inputClass}
        value={display}
        onChange={(e) => {
          const v = e.target.value
          setLocal(v)
          if (editing) {
            /* commit live so draft stays consistent if user saves without blur */
            onChange(v.trim() === '' ? CONFIG_SECRET_MASK : v)
          }
        }}
        onFocus={() => {
          setEditing(true)
          setLocal(value === CONFIG_SECRET_MASK ? '' : value)
        }}
        onBlur={() => {
          setEditing(false)
          const next = local.trim() === '' ? CONFIG_SECRET_MASK : local
          onChange(next)
        }}
        placeholder={placeholder}
      />
      {leaveUnchangedHint && value === CONFIG_SECRET_MASK && !editing && (
        <p className="mt-1 text-xs text-text-secondary">{leaveUnchangedHint}</p>
      )}
    </div>
  )
}

export const Subheading: React.FC<{ children: React.ReactNode; className?: string }> = ({
  children,
  className = '',
}) => <h3 className={`text-lg font-semibold text-text mt-6 mb-3 ${className}`}>{children}</h3>

export const FieldGroup: React.FC<{ children: React.ReactNode; className?: string }> = ({
  children,
  className = '',
}) => <div className={`space-y-4 ${className}`}>{children}</div>
