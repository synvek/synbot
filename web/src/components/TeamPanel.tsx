import { useCallback, useEffect, useMemo, useState } from 'react';
import { useI18n } from '../i18n/I18nContext';
import { apiClient } from '../api';
import type { SubagentInfo, SubagentState } from '../types/agentEvents';

interface TeamPanelProps {
  /** Live teammates from `/ws/agent` (`subagent_updated`), newest state wins over the snapshot. */
  live: SubagentInfo[];
}

const STATE_DOT: Record<SubagentState, string> = {
  running: 'bg-primary animate-pulse',
  completed: 'bg-success',
  failed: 'bg-error',
  cancelled: 'bg-border',
};

const STATE_KEY: Record<SubagentState, string> = {
  running: 'team.running',
  completed: 'team.completed',
  failed: 'team.failed',
  cancelled: 'team.cancelled',
};

function elapsedSeconds(agent: SubagentInfo): number {
  const start = new Date(agent.created_at).getTime();
  const end = agent.finished_at ? new Date(agent.finished_at).getTime() : Date.now();
  if (Number.isNaN(start) || Number.isNaN(end)) return 0;
  return Math.max(0, Math.round((end - start) / 1000));
}

/** Snapshot + live events, keyed by id (the live copy is fresher). */
function mergeAgents(snapshot: SubagentInfo[], live: SubagentInfo[]): SubagentInfo[] {
  const byId = new Map<string, SubagentInfo>();
  for (const agent of snapshot) byId.set(agent.id, agent);
  for (const agent of live) byId.set(agent.id, agent);
  return Array.from(byId.values()).sort((a, b) => a.created_at.localeCompare(b.created_at));
}

export default function TeamPanel({ live }: TeamPanelProps) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = useState<SubagentInfo[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    apiClient
      .listSubagents()
      .then((response) => setSnapshot(response.agents))
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const agents = useMemo(() => mergeAgents(snapshot, live), [snapshot, live]);
  const running = agents.filter((agent) => agent.state === 'running').length;

  // Events cover most progress; the poll keeps elapsed time honest while a teammate is between
  // tool calls (and picks up subagents spawned by another tab).
  useEffect(() => {
    if (running === 0) return;
    const timer = window.setInterval(load, 3000);
    return () => window.clearInterval(timer);
  }, [running, load]);

  const cancel = useCallback(
    async (id: string, all = false) => {
      setBusy(id);
      setError(null);
      try {
        await apiClient.cancelSubagent(id, all);
        load();
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(null);
      }
    },
    [load]
  );

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border text-xs shrink-0">
        <span className="text-text">{t('team.title', '代理团队')}</span>
        <span className="text-text-secondary">
          {t('team.counts', { running, total: agents.length })}
        </span>
        {running > 0 && (
          <button
            type="button"
            onClick={() => cancel('all', true)}
            disabled={busy !== null}
            className="ml-auto px-2 py-0.5 rounded border border-border hover:bg-background disabled:opacity-50"
          >
            {t('team.cancelAll', '全部停止')}
          </button>
        )}
        <button
          type="button"
          onClick={load}
          className={`px-2 py-0.5 rounded border border-border hover:bg-background ${
            running > 0 ? '' : 'ml-auto'
          }`}
        >
          {t('common.refresh', '刷新')}
        </button>
      </div>

      {error && <div className="px-3 py-1 text-xs text-error">{error}</div>}

      <div className="flex-1 overflow-y-auto p-3 space-y-2">
        {agents.length === 0 ? (
          <p className="text-xs text-text-secondary">{t('team.empty')}</p>
        ) : (
          agents.map((agent) => (
            <div
              key={agent.id}
              className="rounded border border-border bg-surface px-3 py-2 space-y-1"
            >
              <div className="flex items-center gap-2 text-xs">
                <span className={`w-1.5 h-1.5 rounded-full ${STATE_DOT[agent.state]}`} />
                <span className="font-medium text-text truncate" title={agent.label}>
                  {agent.label}
                </span>
                <span className="font-mono text-[11px] text-text-secondary">{agent.id}</span>
                {agent.role && (
                  <span className="px-1.5 rounded bg-background text-text-secondary">
                    {t('team.role', '角色')}: {agent.role}
                  </span>
                )}
                <span className="ml-auto text-text-secondary">
                  {t(STATE_KEY[agent.state])} ·{' '}
                  {t('team.elapsed', { seconds: elapsedSeconds(agent) })}
                </span>
                {agent.state === 'running' && (
                  <button
                    type="button"
                    onClick={() => cancel(agent.id)}
                    disabled={busy !== null}
                    className="px-2 py-0.5 rounded border border-border hover:bg-background disabled:opacity-50"
                  >
                    {t('team.cancel', '停止')}
                  </button>
                )}
              </div>
              {agent.activity && (
                <div className="text-[11px] text-text-secondary truncate" title={agent.activity}>
                  {t('team.activity', '当前动作')}:{' '}
                  <span className="font-mono">{agent.activity}</span>
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
