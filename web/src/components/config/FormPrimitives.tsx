import React, { useEffect, useState } from 'react'
import { Field, Input, Select, Switch } from '../../ui'
import { CONFIG_SECRET_MASK } from './immutable'

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
    /** Id of a `<datalist>` with suggestions; the field still accepts free text. */
    list?: string
    hint?: React.ReactNode
  }
> = ({ id, label, value, onChange, placeholder, disabled, list, hint, className = '' }) => (
  <Field label={label} htmlFor={id} hint={hint} className={className}>
    <Input
      id={id}
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      list={list}
      disabled={disabled}
    />
  </Field>
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
  <Field label={label} htmlFor={id} className={className}>
    <Input
      id={id}
      type="number"
      className="tabular font-mono"
      value={Number.isFinite(value) ? value : 0}
      onChange={(e) => onChange(Number(e.target.value))}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
    />
  </Field>
)

export const Toggle: React.FC<
  FieldProps & {
    checked: boolean
    onChange: (v: boolean) => void
    disabled?: boolean
  }
> = ({ id, label, checked, onChange, disabled, className = '' }) => (
  <div className={`flex min-h-8 items-center gap-2 ${className}`}>
    <Switch id={id} label={label} checked={checked} onChange={onChange} disabled={disabled} />
    <span className="text-small text-ink-2">{label}</span>
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
  <Field label={label} htmlFor={id} className={className}>
    <Select id={id} value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </Select>
  </Field>
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
    <Field
      label={label}
      htmlFor={id}
      hint={leaveUnchangedHint && value === CONFIG_SECRET_MASK && !editing ? leaveUnchangedHint : undefined}
      className={className}
    >
      <Input
        id={id}
        type="password"
        autoComplete="new-password"
        className="font-mono"
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
    </Field>
  )
}

export const Subheading: React.FC<{ children: React.ReactNode; className?: string }> = ({
  children,
  className = '',
}) => (
  <h3
    className={`mb-2 mt-5 text-micro font-medium uppercase tracking-wide text-ink-3 ${className}`}
  >
    {children}
  </h3>
)

export const FieldGroup: React.FC<{ children: React.ReactNode; className?: string }> = ({
  children,
  className = '',
}) => <div className={`space-y-3 ${className}`}>{children}</div>
