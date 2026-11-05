import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowUp, Check, Folder, FolderPlus, Home, ListChecks, Square } from 'lucide-react'
import { requestOpenFolder } from '../../components/app-shell/intents'
import { useI18n } from '../../i18n/I18nContext'
import { Button, MenuItem, MenuLabel, MenuSeparator, Popover } from '../../ui'
import type { CommandInfo, Project } from '../../types/api'

interface ComposerProps {
  connected: boolean
  running: boolean
  /** Workspace slash commands, rescanned when the project changes. */
  commands: CommandInfo[]
  /** The next turn plans before it writes. */
  planMode: boolean
  onTogglePlanMode: (enabled: boolean) => void
  onSend: (text: string) => void
  onStop: () => void
  /** Folder this conversation is pinned to (`null` = the default workspace). */
  workspace: { name: string; path: string } | null
  /** Workspace id of the open conversation, for the picker's check mark. */
  workspaceId: string | null
  /** Registered folders, so a new conversation can start in any of them. */
  workspaces: Project[]
  /** Show the workspace line: only worth the space before the first message exists. */
  showWorkspace: boolean
  onSelectWorkspace: (projectId: string | null) => void
}

/** Five lines of text plus the padding: past this the textarea scrolls instead of growing. */
const MAX_HEIGHT = 168

/**
 * The composer is the only place in the console with a filled control: it is where the work
 * starts. Enter sends, Shift+Enter breaks the line, and a leading "/" turns the field into a
 * command picker without leaving the keyboard.
 */
export function Composer({
  connected,
  running,
  commands,
  planMode,
  onTogglePlanMode,
  onSend,
  onStop,
  workspace,
  workspaceId,
  workspaces,
  showWorkspace,
  onSelectWorkspace,
}: ComposerProps) {
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

          <div className="overflow-hidden rounded-panel border border-line-strong bg-surface-2 transition-colors duration-fast focus-within:border-accent">
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
              placeholder={
                connected
                  ? planMode
                    ? t('chat.planModePlaceholder', '描述任务，我会先给出计划供你确认')
                    : t('chat.typeMessage', 'Message')
                  : t('chat.waitingConnection')
              }
              disabled={!connected}
              className="block max-h-[168px] w-full resize-none bg-transparent px-3 py-2 text-body text-ink placeholder:text-ink-3 focus:outline-none disabled:cursor-not-allowed"
            />
            <div className="flex items-center gap-2 px-2 pb-2">
              <button
                type="button"
                aria-pressed={planMode}
                disabled={!connected}
                onClick={() => onTogglePlanMode(!planMode)}
                title={t('chat.planModeHint', '开启后本轮只读：先给出计划，批准前不修改任何文件')}
                className={`inline-flex shrink-0 items-center gap-1 rounded-pill border px-2 py-0.5 text-micro transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-60 ${
                  planMode
                    ? 'border-primary/20 bg-accent/10 text-accent'
                    : 'border-line text-ink-3 hover:text-ink-2'
                }`}
              >
                <ListChecks className="h-3 w-3" aria-hidden="true" />
                {t('chat.planMode', '计划模式')}
              </button>
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

          {/* The conversation's workspace, named where the work starts. Until the first message
              exists it is a picker — the folder is decided here, not in the header — and after
              that it is a caption that would only repeat what the work already says. It sits
              outside the bordered box: the menu has to escape that overflow. */}
          {showWorkspace && (
            <Popover
              label={t('chat.selectWorkspace', '选择工作区')}
              side="top"
              align="start"
              size="row"
              closeOnSelect
              disabled={!connected}
              triggerClassName="mt-1"
              icon={<Folder className="h-3 w-3 shrink-0" aria-hidden="true" />}
              text={
                <span className="flex min-w-0 flex-1 items-center gap-1.5">
                  {workspace ? (
                    <>
                      <span className="shrink-0 font-medium text-ink-2">{workspace.name}</span>
                      <span className="min-w-0 truncate font-mono">{workspace.path}</span>
                    </>
                  ) : (
                    <span className="min-w-0 truncate">
                      {t('chat.workspaceDefaultHint', '默认工作区 · 点击选择工作目录')}
                    </span>
                  )}
                </span>
              }
            >
              <MenuLabel>{t('chat.selectWorkspace', '选择工作区')}</MenuLabel>
              <p className="px-2.5 pb-1 text-micro text-ink-3">
                {t('chat.workspaceFixed', '发出首条消息后不可更改')}
              </p>
              <MenuItem
                icon={<Home className="h-3.5 w-3.5" />}
                selected={workspaceId === null}
                onSelect={() => onSelectWorkspace(null)}
                aside={
                  workspaceId === null ? (
                    <Check className="h-3 w-3 text-accent" aria-hidden="true" />
                  ) : undefined
                }
              >
                {t('shell.globalWorkspace', '默认工作区')}
              </MenuItem>
              {workspaces.map((item) => (
                <MenuItem
                  key={item.id}
                  icon={<Folder className="h-3.5 w-3.5" />}
                  selected={item.id === workspaceId}
                  onSelect={() => onSelectWorkspace(item.id)}
                  aside={
                    <span className="flex shrink-0 items-center gap-1.5">
                      {item.id === workspaceId && (
                        <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden="true" />
                      )}
                      <span className="max-w-[45%] truncate font-mono text-micro text-ink-3">
                        {item.path}
                      </span>
                    </span>
                  }
                >
                  {item.name}
                </MenuItem>
              ))}
              <MenuSeparator />
              <MenuItem icon={<FolderPlus className="h-3.5 w-3.5" />} onSelect={requestOpenFolder}>
                {t('shell.openFolder', '添加工作区…')}
              </MenuItem>
            </Popover>
          )}
        </div>
      </div>
    </div>
  )
}
