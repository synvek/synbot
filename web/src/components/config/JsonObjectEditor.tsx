import React, { useEffect, useState } from 'react'

type Props = {
  label: string
  value: unknown
  onChange: (v: unknown) => void
  nullable?: boolean
  hint?: string
}

/** Edit arbitrary JSON object (or null) with validation on blur. */
export const JsonObjectEditor: React.FC<Props> = ({ label, value, onChange, nullable, hint }) => {
  const [text, setText] = useState('')
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    if (value === null || value === undefined) {
      setText('')
    } else {
      try {
        setText(JSON.stringify(value, null, 2))
      } catch {
        setText(String(value))
      }
    }
    setErr(null)
  }, [value])

  const apply = () => {
    const t = text.trim()
    if (!t) {
      if (nullable) {
        onChange(null)
        setErr(null)
        return
      }
      setErr('Empty')
      return
    }
    try {
      const parsed = JSON.parse(t) as unknown
      onChange(parsed)
      setErr(null)
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'Invalid JSON')
    }
  }

  return (
    <div>
      <div className="flex justify-between items-center mb-1">
        <label className="text-sm font-medium text-text">{label}</label>
        {nullable && (
          <button
            type="button"
            className="text-xs text-text-secondary hover:text-text"
            onClick={() => onChange(null)}
          >
            Clear
          </button>
        )}
      </div>
      {hint && <p className="text-xs text-text-secondary mb-1">{hint}</p>}
      <textarea
        spellCheck={false}
        className="w-full min-h-[200px] font-mono text-xs px-3 py-2 rounded-lg bg-surface border border-border text-text focus:outline-none focus:ring-2 focus:ring-primary/30"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={apply}
      />
      {err && <p className="text-xs text-error mt-1">{err}</p>}
    </div>
  )
}
