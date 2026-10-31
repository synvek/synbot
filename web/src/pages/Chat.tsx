import { useState, useRef, useEffect, useMemo, lazy, Suspense } from 'react';
import { useAgentSocket } from '../hooks/useAgentSocket';
import { useI18n } from '../i18n/I18nContext';
import { notifyDesktop } from '../desktop/bridge';
import { apiClient } from '../api';
import type { CommandInfo } from '../types/api';
import ApprovalRequest from '../components/ApprovalRequest';
import Markdown from '../components/Markdown';
import ProjectPanel from '../components/ProjectPanel';

// xterm is ~340KB: load it only when the terminal tab is opened for the first time.
const TerminalPanel = lazy(() => import('../components/TerminalPanel'));
const TeamPanel = lazy(() => import('../components/TeamPanel'));

function formatDuration(ms?: number) {
  if (ms === undefined) return '';
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
}

/** Turn reasons that mean the turn failed. */
const FAILED_TURN_REASONS = ['error', 'max_tool_errors'];

/** Stopped early on purpose (budget exhausted, no progress): a warning, not a failure. */
const WARNING_TURN_REASONS = ['max_iterations', 'no_progress'];

export default function Chat() {
  const [input, setInput] = useState('');
  /** File the user clicked in a turn summary; the project panel opens its diff. */
  const [focusFile, setFocusFile] = useState<{ turnId: string; path: string } | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  /** Workspace slash-command templates for the composer picker. */
  const [commands, setCommands] = useState<CommandInfo[]>([]);
  /** Chat or the workbench shell (the terminal/team tabs mount lazily and stay alive across switches). */
  const [view, setView] = useState<'chat' | 'terminal' | 'team'>('chat');
  const [terminalMounted, setTerminalMounted] = useState(false);
  const [teamMounted, setTeamMounted] = useState(false);
  const { t } = useI18n();

  // Same-origin on the web, absolute loopback URL plus the desktop token in the app.
  const wsUrl = useMemo(() => apiClient.wsUrl('/ws/agent'), []);
  const {
    connected,
    running,
    messages,
    send,
    cancel,
    sendApprovalResponse,
    sessionId,
    project,
    planVersion,
    subagents,
    selectProject,
  } = useAgentSocket({
    url: wsUrl,
    autoConnect: true,
  });

  /**
   * Desktop only: a turn that finishes while the window is in the background raises a system
   * notification, so long runs do not have to be watched.
   */
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running && document.hidden) {
      const lastReply = [...messages].reverse().find((message) => message.role === 'assistant');
      const preview = (lastReply?.content ?? '').replace(/\s+/g, ' ').trim().slice(0, 200);
      void notifyDesktop(
        t('desktop.turnFinishedTitle', 'Synbot 已完成'),
        preview || t('desktop.turnFinishedBody', '本轮已结束，回到 Synbot 查看结果。')
      );
    }
    wasRunning.current = running;
  }, [running, messages, t]);

  /** Files reported by the most recent finished turn (`file_changed` events). */
  const lastTurnFiles = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (message.role === 'turn') return message.turnFiles ?? [];
    }
    return [];
  }, [messages]);

  /** Teammates still working — shown next to the team tab so a background fan-out is visible. */
  const runningTeammates = useMemo(
    () => subagents.filter((agent) => agent.state === 'running').length,
    [subagents]
  );

  /** Id of the most recent finished turn — the key for its checkpoint (diff + undo). */
  const lastTurnId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i -= 1) {
      const message = messages[i];
      if (message.role === 'turn') return message.turnId ?? null;
    }
    return null;
  }, [messages]);

  // Workspace commands follow the selected folder: switching projects rescans `.synbot/commands`.
  useEffect(() => {
    let active = true;
    apiClient
      .getCommands(project?.id)
      .then((list) => {
        if (active) setCommands(list);
      })
      .catch(() => {
        if (active) setCommands([]);
      });
    return () => {
      active = false;
    };
  }, [project]);

  /** "/rev" → "rev"; null when the input is not a single command token being typed. */
  const slashQuery = useMemo(() => {
    if (!input.startsWith('/')) return null;
    const rest = input.slice(1);
    return /\s/.test(rest) ? null : rest.toLowerCase();
  }, [input]);

  const matchingCommands = useMemo(() => {
    if (slashQuery === null) return [];
    return commands.filter((command) => command.name.toLowerCase().includes(slashQuery)).slice(0, 6);
  }, [commands, slashQuery]);

  const pickCommand = (name: string) => {
    setInput(`/${name} `);
    inputRef.current?.focus();
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!input.trim() || !connected) {
      return;
    }

    send(input);
    setInput('');
  };

  return (
    <div className="flex flex-col h-[calc(100vh-8rem)]">
      <div className="mb-4 flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <h1 className="text-2xl font-bold text-text">{t('chat.title')}</h1>
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${connected ? 'bg-success' : 'bg-error'}`}
          />
          <span className="text-sm text-text-secondary">
            {connected ? t('common.connected') : t('common.disconnected')}
          </span>
        </div>
        {project && (
          <span className="text-sm text-text-secondary truncate max-w-[16rem]" title={project.path}>
            {t('chat.workspace', '工作区')}: {project.name}
          </span>
        )}
        {sessionId && (
          <span className="text-sm text-text-secondary">
            {t('chat.session')}: {sessionId.slice(0, 8)}...
          </span>
        )}
      </div>

      <div className="flex-1 flex gap-4 min-h-0">
        <div className="flex-1 min-w-0 bg-surface rounded-lg shadow overflow-hidden flex flex-col border border-border">
        <div className="flex items-center gap-1 px-3 py-1.5 border-b border-border text-xs shrink-0">
          <button
            type="button"
            onClick={() => setView('chat')}
            className={`px-2 py-0.5 rounded ${
              view === 'chat' ? 'text-text bg-background' : 'text-text-secondary hover:text-text'
            }`}
          >
            {t('chat.tabChat', '对话')}
          </button>
          <button
            type="button"
            onClick={() => {
              setTerminalMounted(true);
              setView('terminal');
            }}
            className={`px-2 py-0.5 rounded ${
              view === 'terminal' ? 'text-text bg-background' : 'text-text-secondary hover:text-text'
            }`}
          >
            {t('chat.tabTerminal', '终端')}
          </button>
          <button
            type="button"
            onClick={() => {
              setTeamMounted(true);
              setView('team');
            }}
            className={`px-2 py-0.5 rounded ${
              view === 'team' ? 'text-text bg-background' : 'text-text-secondary hover:text-text'
            }`}
          >
            {t('chat.tabTeam', '团队')}
            {runningTeammates > 0 && (
              <span className="ml-1 text-primary">{runningTeammates}</span>
            )}
          </button>
          {running && (
            <span className="ml-auto text-[11px] text-primary">{t('chat.toolRunning', '运行中')}</span>
          )}
        </div>
        <div
          className={`flex-1 overflow-y-auto p-4 space-y-4 ${
            view === 'chat' ? '' : 'hidden'
          }`}
        >
          {messages.length === 0 ? (
            <div className="text-center text-text-secondary mt-8">
              <p>{t('chat.noMessages')}</p>
            </div>
          ) : (
            messages.map((message) => (
              <div
                key={message.id}
                className={`flex ${
                  message.role === 'user' ? 'justify-end' : 'justify-start'
                }`}
              >
                {message.role === 'approval' && message.approvalRequest ? (
                  <ApprovalRequest
                    request={message.approvalRequest}
                    onApprove={(requestId) => sendApprovalResponse(requestId, true)}
                    onReject={(requestId) => sendApprovalResponse(requestId, false)}
                    result={message.approvalResult}
                  />
                ) : message.role === 'tool_call' || message.role === 'tool_result' ? (
                  <div className="max-w-[85%] rounded-lg border border-border bg-tool-execution text-text-secondary text-sm overflow-hidden">
                    <div className="flex items-center gap-2 px-3 py-1.5">
                      <span
                        className={`w-1.5 h-1.5 rounded-full ${
                          message.toolStatus === 'running'
                            ? 'bg-primary animate-pulse'
                            : message.toolStatus === 'failure'
                            ? 'bg-error'
                            : 'bg-success'
                        }`}
                      />
                      <span className="font-medium text-text break-all">
                        {message.toolName ?? message.content}
                      </span>
                      <span className="text-xs opacity-70">
                        {message.toolStatus === 'running'
                          ? t('chat.toolRunning', '运行中')
                          : message.toolStatus === 'failure'
                          ? t('chat.toolFailed', '失败')
                          : t('chat.toolDone', '完成')}
                      </span>
                      {message.toolDurationMs !== undefined && (
                        <span className="ml-auto text-xs opacity-70">
                          {formatDuration(message.toolDurationMs)}
                        </span>
                      )}
                    </div>
                    {(message.toolArgs || message.toolResult) && (
                      <div className="px-3 pb-2 space-y-1">
                        {message.toolArgs && (
                          <details>
                            <summary className="cursor-pointer select-none text-xs opacity-70">
                              {t('chat.toolArgs', '参数')}
                            </summary>
                            <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-all text-xs bg-background rounded p-2">
                              {message.toolArgs}
                            </pre>
                          </details>
                        )}
                        {message.toolResult && (
                          <details open={message.toolStatus === 'failure'}>
                            <summary className="cursor-pointer select-none text-xs opacity-70">
                              {t('chat.toolOutput', '输出')}
                            </summary>
                            <pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap break-all text-xs bg-background rounded p-2">
                              {message.toolResult}
                            </pre>
                          </details>
                        )}
                      </div>
                    )}
                  </div>
                ) : message.role === 'notice' ? (
                  <div className="flex justify-start">
                    <div className="max-w-[85%] rounded-lg border border-dashed border-border bg-surface px-3 py-2 text-xs text-text-secondary whitespace-pre-wrap break-words">
                      {message.content}
                    </div>
                  </div>
                ) : message.role === 'turn' ? (
                  <div className="flex flex-col items-center gap-1">
                    <span
                      className={`text-xs px-2 py-0.5 rounded-full bg-surface border border-border ${
                        FAILED_TURN_REASONS.includes(message.turnReason ?? '')
                          ? 'text-error'
                          : WARNING_TURN_REASONS.includes(message.turnReason ?? '')
                            ? 'text-warning'
                            : 'text-text-secondary'
                      }`}
                      title={
                        message.turnReason
                          ? t(`chat.turnReasonHint.${message.turnReason}`, '')
                          : undefined
                      }
                    >
                      {formatDuration(message.turnDurationMs)}
                      {message.turnTokens &&
                        ` · ${message.turnTokens.total.toLocaleString()} tokens`}
                      {message.turnReason && message.turnReason !== 'completed'
                        ? ` · ${t(`chat.turnReason.${message.turnReason}`, message.turnReason)}`
                        : ''}
                    </span>
                    {message.turnFiles && (
                      <details className="text-xs text-text-secondary">
                        <summary className="cursor-pointer select-none">
                          {t('chat.filesChanged', {
                            count: message.turnFiles.length,
                          })}
                        </summary>
                        <ul className="mt-1 space-y-0.5 font-mono">
                          {message.turnFiles.map((file) => (
                            <li key={file.path}>
                              <button
                                type="button"
                                onClick={() =>
                                  message.turnId &&
                                  setFocusFile({ turnId: message.turnId, path: file.path })
                                }
                                className="hover:text-text text-left"
                                title={t('chat.viewDiff', '查看改动')}
                              >
                                <span className="opacity-60">{file.kind}</span> {file.path}
                              </button>
                            </li>
                          ))}
                        </ul>
                      </details>
                    )}
                  </div>
                ) : (
                  <div
                    className={`max-w-[70%] rounded-lg px-4 py-2 shadow-sm ${
                      message.role === 'user'
                        ? 'bg-primary text-white'
                        : 'bg-background border border-border text-text'
                    }`}
                  >
                    {message.reasoning && (
                      <details className="mb-2 text-xs text-text-secondary">
                        <summary className="cursor-pointer select-none">
                          {t('chat.reasoning', '思考过程')}
                        </summary>
                        <div className="mt-1 whitespace-pre-wrap break-words opacity-80">
                          {message.reasoning}
                        </div>
                      </details>
                    )}
                    <div className="text-sm break-words">
                      {message.role === 'assistant' ? (
                        <>
                          <Markdown content={message.content} />
                          {message.streaming && (
                            <span className="inline-block w-[2px] h-4 ml-0.5 align-middle bg-current animate-pulse" />
                          )}
                        </>
                      ) : (
                        <span className="whitespace-pre-wrap">{message.content}</span>
                      )}
                    </div>
                    <div
                      className={`text-xs mt-1 opacity-70 ${
                        message.role === 'user' ? 'text-white' : 'text-text-secondary'
                      }`}
                    >
                      {new Date(message.timestamp).toLocaleTimeString()}
                    </div>
                  </div>
                )}
              </div>
            ))
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* Mounted on first use and only hidden afterwards, so switching tabs keeps the shell. */}
        <div className={view === 'terminal' ? 'flex-1 min-h-0' : 'hidden'}>
          {terminalMounted && (
            <Suspense
              fallback={
                <div className="p-3 text-xs text-text-secondary">{t('common.loading')}</div>
              }
            >
              <TerminalPanel
                projectId={project?.id ?? null}
                active={view === 'terminal'}
                onClose={() => setView('chat')}
              />
            </Suspense>
          )}
        </div>

        <div className={view === 'team' ? 'flex-1 min-h-0' : 'hidden'}>
          {teamMounted && (
            <Suspense
              fallback={
                <div className="p-3 text-xs text-text-secondary">{t('common.loading')}</div>
              }
            >
              <TeamPanel live={subagents} />
            </Suspense>
          )}
        </div>

        {view === 'chat' && (
        <div className="border-t border-border p-4">
          {matchingCommands.length > 0 && (
            <div className="mb-2 rounded-lg border border-border bg-surface overflow-hidden">
              <div className="px-3 py-1 text-[11px] text-text-secondary border-b border-border">
                {t('chat.commands', '命令')}
              </div>
              {matchingCommands.map((command) => (
                <button
                  key={command.name}
                  type="button"
                  onClick={() => pickCommand(command.name)}
                  className="w-full flex items-baseline gap-2 px-3 py-1 text-left text-xs hover:bg-background"
                >
                  <span className="font-mono text-text">/{command.name}</span>
                  <span className="truncate text-text-secondary">{command.description}</span>
                </button>
              ))}
            </div>
          )}
          <form onSubmit={handleSubmit} className="flex gap-2">
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={
                connected
                  ? t('chat.typeMessage')
                  : t('chat.waitingConnection')
              }
              disabled={!connected}
              className="flex-1 px-4 py-2 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-surface text-text disabled:bg-surface/50 disabled:cursor-not-allowed"
            />
            <button
              type="submit"
              disabled={!connected || !input.trim() || running}
              className="px-6 py-2 bg-primary text-white rounded-lg hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed transition-colors"
            >
              {t('chat.send')}
            </button>
            {running && (
              <button
                type="button"
                onClick={cancel}
                className="px-4 py-2 border border-border bg-surface text-text rounded-lg hover:bg-background transition-colors"
              >
                {t('chat.stop', '停止')}
              </button>
            )}
          </form>
        </div>
        )}
        </div>

        <ProjectPanel
          project={project}
          running={running}
          changedFiles={lastTurnFiles}
          turnId={lastTurnId}
          focusFile={focusFile}
          planVersion={planVersion}
          onSelect={selectProject}
        />
      </div>
    </div>
  );
}
