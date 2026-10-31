import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowUp, Square } from 'lucide-react'
import { useI18n } from '../../i18n/I18nContext'
import { Button } from '../../ui'
import type { CommandInfo } from '../../types/api'

interface ComposerProps {
  connected: boolean
  running: boolean
  /** Workspace slash commands, rescanned when the project changes. */
  commands: CommandInfo[]
  onSend: (text: string) => void
  onStop: () => void
}

/** Five lines of text plus the padding: past this the textarea scrolls instead of growing. */
const MAX_HEIGHT = 168

/**
 * The composer is the only place in the console with a filled control: it is where the work
 * starts. Enter sends, Shift+Enter breaks the line, and a leading "/" turns the field into a
 * command picker without leaving the keyboard.
 */
export function Composer({ connected, running, commands, onSend, onStop }: ComposerProps) {
  const { t } = useI18n()
  const [value, setValue] = useState('')
  const [activeIndex, setActiveIndex] = useState(0)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    const node = inputRef.current
    if (!node) return
    node.style.height = 'auto'
    node.style.height = `${Math.min(node.scrollHeight, MAX_HEIGHT)}px`
  }, [value])

  /** "/rev" → "rev"; null when the input is not a single command token being typed. */
  const slashQuery = useMemo(() => {
    if (!value.startsWith('/')) return null
    const rest = value.slice(1)
    return /\s/.test(rest) ? null : rest.toLowerCase()
  }, [value])

  const matches = useMemo(() => {
    if (slashQuery === null) return []
    return commands.filter((command) => command.name.toLowerCase().includes(slashQuery)).slice(0, 6)
  }, [commands, slashQuery])

  useEffect(() => {
    setActiveIndex(0)
  }, [matches.length])

  const canSend = connected && !running && value.trim().length > 0

  const submit = () => {
    if (!canSend) return
    onSend(value.trim())
    setValue('')
  }

  const pickCommand = (name: string) => {
    setValue(`/${name} `)
    inputRef.current?.focus()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // An IME candidate window owns Enter/Arrow keys while composing (zh input).
    if (event.nativeEvent.isComposing) return

    if (matches.length > 0) {
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setActiveIndex((index) => (index + 1) % matches.length)
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        setActiveIndex((index) => (index - 1 + matches.length) % matches.length)
        return
      }
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault()
        pickCommand(matches[activeIndex].name)
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        setValue('')
        return
      }
    }

    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      submit()
    }
  }

  return (
    <div className="border-t border-line bg-surface">
      <div className="mx-auto w-full max-w-transcript px-6 py-3">
        <div className="relative">
          {matches.length > 0 && (
            <div
              role="listbox"
              aria-label={t('chat.commands', 'Commands')}
              className="absolute bottom-full left-0 right-0 mb-2 overflow-hidden rounded-panel border border-line bg-surface shadow-overlay"
            >
              <div className="border-b border-line px-2.5 py-1 text-micro text-ink-3">
                {t('chat.commands', 'Commands')}
              </div>
              {matches.map((command, index) => (
                <button
                  key={command.name}
                  type="button"
                  role="option"
                  aria-selected={index === activeIndex}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => pickCommand(command.name)}
                  className={`flex w-full items-baseline gap-2 px-2.5 py-1 text-left text-small ${
                    index === activeIndex ? 'bg-surface-2' : ''
                  }`}
                >
                  <span className="shrink-0 font-mono text-ink">/{command.name}</span>
                  <span className="truncate text-ink-2">{command.description}</span>
                </button>
              ))}
            </div>
          )}

          <div className="rounded-panel border border-line-strong bg-surface-2 transition-colors duration-fast focus-within:border-accent">
            <label className="sr-only" htmlFor="composer-input">
              {t('chat.typeMessage', 'Message')}
            </label>
            <textarea
              id="composer-input"
              ref={inputRef}
              rows={1}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={connected ? t('chat.typeMessage', 'Message') : t('chat.waitingConnection')}
              disabled={!connected}
              className="block max-h-[168px] w-full resize-none bg-transparent px-3 py-2 text-body text-ink placeholder:text-ink-3 focus:outline-none disabled:cursor-not-allowed"
            />
            <div className="flex items-center gap-2 px-2 pb-2">
              <span className="min-w-0 truncate text-micro text-ink-3">
                {t('chat.composerHint', 'Enter to send · Shift+Enter for a new line · / for commands')}
              </span>
              <div className="ml-auto flex shrink-0 items-center gap-1.5">
                {running ? (
                  <Button
                    variant="danger"
                    size="sm"
                    icon={<Square className="h-3 w-3" />}
                    onClick={onStop}
                  >
                    {t('chat.stop', 'Stop')}
                  </Button>
                ) : (
                  <Button
                    variant="primary"
                    size="sm"
                    icon={<ArrowUp className="h-3.5 w-3.5" />}
                    disabled={!canSend}
                    onClick={submit}
                  >
                    {t('chat.send', 'Send')}
                  </Button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
