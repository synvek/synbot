import React, { useCallback, useEffect, useState } from 'react';
import { apiClient } from '../api/client';
import type {
  PolicyExplain,
  SecurityEventsResponse,
  SecurityKind,
  SecurityOutcome,
  SecurityOverview,
} from '../types/api';
import { useI18n } from '../i18n/I18nContext';

const KIND_KEY: Record<SecurityKind, string> = {
  command_blocked: 'security.kindCommandBlocked',
  command_denied: 'security.kindCommandDenied',
  approval_unavailable: 'security.kindApprovalUnavailable',
  approval_requested: 'security.kindApprovalRequested',
  approval_approved: 'security.kindApprovalApproved',
  approval_rejected: 'security.kindApprovalRejected',
  approval_timeout: 'security.kindApprovalTimeout',
  sandbox_required: 'security.kindSandboxRequired',
  path_escape: 'security.kindPathEscape',
  plugin_permission: 'security.kindPluginPermission',
};

const OUTCOME_KEY: Record<SecurityOutcome, string> = {
  blocked: 'security.outcomeBlocked',
  needs_user: 'security.outcomeNeedsUser',
  allowed: 'security.outcomeAllowed',
};

const OUTCOME_STYLE: Record<SecurityOutcome, string> = {
  blocked: 'bg-error/10 text-error border-error/40',
  needs_user: 'bg-warning/10 text-warning border-warning/40',
  allowed: 'bg-success/10 text-success border-success/40',
};

const FINAL_STYLE: Record<string, string> = {
  blocked: OUTCOME_STYLE.blocked,
  needs_approval: OUTCOME_STYLE.needs_user,
  allowed: OUTCOME_STYLE.allowed,
};

const PROFILE_KEY: Record<string, string> = {
  safe: 'security.profileSafe',
  developer: 'security.profileDeveloper',
  trusted: 'security.profileTrusted',
};

const LEVEL_KEY: Record<string, string> = {
  allow: 'security.levelAllow',
  require_approval: 'security.levelRequireApproval',
  deny: 'security.levelDeny',
};

const Chip: React.FC<{ children: React.ReactNode; title?: string }> = ({ children, title }) => (
  <span
    title={title}
    className="px-1.5 py-0.5 rounded bg-background border border-border font-mono text-[11px] text-text-secondary"
  >
    {children}
  </span>
);

/** `label: value` row used by the policy cards. */
const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className="flex flex-wrap items-baseline gap-2 text-xs">
    <span className="text-text-secondary">{label}</span>
    <span className="text-text break-all">{children}</span>
  </div>
);

const Security: React.FC = () => {
  const { t } = useI18n();
  const [overview, setOverview] = useState<SecurityOverview | null>(null);
  const [trail, setTrail] = useState<SecurityEventsResponse | null>(null);
  const [filter, setFilter] = useState<SecurityKind | 'all'>('all');
  const [command, setCommand] = useState('');
  const [verdict, setVerdict] = useState<PolicyExplain | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [policy, events] = await Promise.all([
        apiClient.securityOverview(),
        apiClient.listSecurityEvents(filter === 'all' ? undefined : filter),
      ]);
      setOverview(policy);
      setTrail(events);
      setError(null);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [filter]);

  useEffect(() => {
    void load();
  }, [load]);

  const explain = async (event?: React.FormEvent) => {
    event?.preventDefault();
    const cmd = command.trim();
    if (!cmd) return;
    setBusy('explain');
    setError(null);
    setNotice(null);
    try {
      setVerdict(await apiClient.explainPolicy(cmd));
    } catch (e: unknown) {
      setVerdict(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const clearTrail = async () => {
    if (!window.confirm(t('security.clearConfirm', 'Clear the in-memory audit trail?'))) return;
    setBusy('clear');
    setError(null);
    setNotice(null);
    try {
      const cleared = await apiClient.clearSecurityEvents();
      setNotice(t('security.cleared', { count: cleared }));
      await load();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const counts = trail?.counts ?? overview?.audit.counts ?? [];
  const exec = overview?.exec;
  const sandbox = overview?.tool_sandbox;
  const appSandbox = overview?.app_sandbox;

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-text">{t('security.title')}</h2>
        <p className="text-text-secondary mt-1">{t('security.description')}</p>
      </div>

      {error && (
        <div className="rounded-lg border border-error/40 bg-error/10 px-4 py-2 text-sm text-error">
          {error}
        </div>
      )}
      {notice && (
        <div className="rounded-lg border border-success/40 bg-success/10 px-4 py-2 text-sm text-success">
          {notice}
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="bg-surface rounded-lg shadow border border-border p-4 space-y-2">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-text">{t('security.execPolicy')}</h3>
            {overview && (
              <span className="ml-auto px-2 py-0.5 rounded border border-primary/40 bg-primary-muted text-[11px] text-primary">
                {t(PROFILE_KEY[overview.profile] ?? 'security.profileUnknown', overview.profile)}
              </span>
            )}
          </div>
          {exec && (
            <>
              <Field label={t('security.defaultLevel')}>
                {t(LEVEL_KEY[exec.default_level] ?? exec.default_level, exec.default_level)}
              </Field>
              <Field label={t('security.permissions')}>
                {exec.permissions_enabled
                  ? t('security.enabled', 'enabled')
                  : t('security.disabled', 'disabled')}{' '}
                · {t('security.rules', { count: exec.rule_count })}
              </Field>
              <Field label={t('security.timeout')}>
                {t('security.seconds', { count: exec.timeout_secs })}
              </Field>
              <Field label={t('security.approvalTimeout')}>
                {t('security.seconds', { count: exec.approval_timeout_secs })}
              </Field>
              <Field label={t('security.restrict')}>
                {exec.restrict_to_workspace
                  ? t('security.yes', 'yes')
                  : t('security.no', 'no')}
              </Field>
              <div className="pt-1 space-y-1">
                <div className="flex flex-wrap items-center gap-1 text-[11px]">
                  <span className="text-text-secondary">{t('security.denyPatterns')}:</span>
                  {exec.deny_patterns.length === 0 ? (
                    <span className="text-text-secondary">{t('security.none', 'none')}</span>
                  ) : (
                    exec.deny_patterns.map((pattern) => (
                      <Chip key={pattern} title={pattern}>
                        {pattern}
                      </Chip>
                    ))
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-1 text-[11px]">
                  <span className="text-text-secondary">{t('security.allowPatterns')}:</span>
                  {!exec.allow_patterns ? (
                    <span className="text-text-secondary">{t('security.none', 'none')}</span>
                  ) : exec.allow_patterns.length === 0 ? (
                    // A configured-but-empty allowlist rejects every command.
                    <span className="text-warning">{t('security.allowEmpty')}</span>
                  ) : (
                    exec.allow_patterns.map((pattern) => (
                      <Chip key={pattern} title={pattern}>
                        {pattern}
                      </Chip>
                    ))
                  )}
                </div>
              </div>
            </>
          )}
          {overview && (
            <div className="pt-1 space-y-1 border-t border-border">
              <Field label={t('security.workspace')}>
                <span className="font-mono">{overview.workspace}</span>
              </Field>
              <Field label={t('security.platform')}>
                <span className="font-mono">
                  {overview.platform.os} {overview.platform.version} ({overview.platform.arch})
                </span>{' '}
                {overview.platform.supported ? '✓' : t('security.unsupported', 'unsupported')}
              </Field>
            </div>
          )}
        </section>

        <section className="bg-surface rounded-lg shadow border border-border p-4 space-y-2">
          <h3 className="text-sm font-semibold text-text">{t('security.sandbox')}</h3>
          {overview && !sandbox && (
            <p className="text-xs text-warning">{t('security.notConfigured')}</p>
          )}
          {sandbox && (
            <>
              <Field label={t('security.backend')}>
                <span className="font-mono">{sandbox.backend}</span>
                {!sandbox.backend_explicit && (
                  <span className="text-text-secondary"> ({t('security.backendDefault')})</span>
                )}
              </Field>
              <Field label={t('security.execKind')}>
                {sandbox.exec_kind === 'docker'
                  ? t('security.execDocker', 'docker')
                  : t('security.execHostNative', 'host filesystem')}
              </Field>
              {sandbox.image && (
                <Field label={t('security.image')}>
                  <span className="font-mono">{sandbox.image}</span>
                </Field>
              )}
              {sandbox.network && (
                <>
                  <Field label={t('security.network')}>
                    {sandbox.network.enabled
                      ? t('security.enabled', 'enabled')
                      : t('security.disabled', 'disabled')}
                  </Field>
                  {sandbox.network.hosts.length > 0 && (
                    <div className="flex flex-wrap items-center gap-1 text-[11px]">
                      <span className="text-text-secondary">{t('security.networkHosts')}:</span>
                      {sandbox.network.hosts.map((host) => (
                        <Chip key={host}>{host}</Chip>
                      ))}
                    </div>
                  )}
                  {sandbox.network.ports.length > 0 && (
                    <div className="flex flex-wrap items-center gap-1 text-[11px]">
                      <span className="text-text-secondary">{t('security.networkPorts')}:</span>
                      {sandbox.network.ports.map((port) => (
                        <Chip key={port}>{port}</Chip>
                      ))}
                    </div>
                  )}
                </>
              )}
              {sandbox.filesystem && (
                <>
                  <div className="flex flex-wrap items-center gap-1 text-[11px]">
                    <span className="text-text-secondary">{t('security.readonly')}:</span>
                    {sandbox.filesystem.readonly.length === 0 ? (
                      <span className="text-text-secondary">{t('security.none', 'none')}</span>
                    ) : (
                      sandbox.filesystem.readonly.map((path) => <Chip key={path}>{path}</Chip>)
                    )}
                  </div>
                  <div className="flex flex-wrap items-center gap-1 text-[11px]">
                    <span className="text-text-secondary">{t('security.writable')}:</span>
                    {sandbox.filesystem.writable.length === 0 ? (
                      <span className="text-text-secondary">{t('security.none', 'none')}</span>
                    ) : (
                      sandbox.filesystem.writable.map((path) => <Chip key={path}>{path}</Chip>)
                    )}
                  </div>
                </>
              )}
              {sandbox.resources && (
                <Field label={t('security.resources')}>
                  <span className="font-mono">
                    {t('security.memory', 'memory')}={sandbox.resources.max_memory}{' '}
                    {t('security.cpu', 'cpu')}={sandbox.resources.max_cpu}{' '}
                    {t('security.disk', 'disk')}={sandbox.resources.max_disk}
                  </span>
                </Field>
              )}
            </>
          )}
          {appSandbox && (
            <div className="pt-1 border-t border-border">
              <Field label={t('security.appSandbox')}>
                <span className="font-mono">{appSandbox.platform}</span> ·{' '}
                <span className="font-mono">{appSandbox.work_dir}</span>
              </Field>
            </div>
          )}
          {overview && (
            <div className="pt-1 border-t border-border space-y-1">
              <Field label={t('security.secrets')}>
                {t('security.secretsDetail', { count: overview.secrets.patterns })}
              </Field>
              <Field label={t('security.plugins')}>
                {t('security.pluginsDetail', {
                  packaged: overview.plugins.packaged,
                  legacy: overview.plugins.legacy,
                })}
              </Field>
            </div>
          )}
        </section>
      </div>

      <section className="bg-surface rounded-lg shadow border border-border p-4 space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-text">{t('security.tryTitle')}</h3>
          <p className="text-xs text-text-secondary mt-1">{t('security.tryHint')}</p>
        </div>
        <form onSubmit={explain} className="flex flex-wrap gap-2">
          <input
            type="text"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder={t('security.commandPlaceholder', 'rm -rf build')}
            className="flex-1 min-w-[16rem] px-3 py-1.5 rounded border border-border bg-background text-text text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary"
          />
          <button
            type="submit"
            disabled={busy !== null || !command.trim()}
            className="px-4 py-1.5 rounded bg-primary text-white text-sm hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed"
          >
            {busy === 'explain' ? t('common.loading') : t('security.explain', 'Explain')}
          </button>
        </form>
        {verdict && (
          <div className="rounded-lg border border-border bg-background p-3 space-y-1 text-xs">
            <div className="flex flex-wrap items-center gap-2">
              <span
                className={`px-1.5 py-0.5 rounded border text-[11px] ${
                  FINAL_STYLE[verdict.final] ?? ''
                }`}
              >
                {t(`security.final_${verdict.final}`, verdict.final)}
              </span>
              <span className="font-mono text-text break-all">{verdict.command}</span>
            </div>
            {verdict.blocked_by_pattern && (
              <Field label={t('security.blockedByPattern')}>
                {verdict.blocked_by_pattern}
              </Field>
            )}
            <Field label={t('security.level')}>
              {t(LEVEL_KEY[verdict.level] ?? verdict.level, verdict.level)}
            </Field>
            {verdict.matched_rule ? (
              <>
                <Field label={t('security.matchedRule')}>
                  <Chip>{verdict.matched_rule.pattern}</Chip> →{' '}
                  {t(
                    LEVEL_KEY[verdict.matched_rule.level] ?? verdict.matched_rule.level,
                    verdict.matched_rule.level
                  )}
                </Field>
                {verdict.matched_rule.description && (
                  <p className="text-text-secondary">{verdict.matched_rule.description}</p>
                )}
              </>
            ) : (
              <p className="text-text-secondary">{t('security.noRule')}</p>
            )}
            {verdict.final === 'needs_approval' && (
              <p className="text-text-secondary">
                {t('security.approvalWithin', {
                  seconds: verdict.approval_timeout_secs,
                })}
              </p>
            )}
          </div>
        )}
      </section>

      <section className="bg-surface rounded-lg shadow border border-border">
        <div className="p-4 space-y-2 border-b border-border">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold text-text">{t('security.events')}</h3>
            {trail && (
              <span className="text-[11px] text-text-secondary">
                {t('security.showing', {
                  shown: trail.events.length,
                  retained: trail.retained,
                  capacity: trail.capacity,
                })}
              </span>
            )}
            <span className="ml-auto flex items-center gap-2">
              <button
                type="button"
                onClick={() => void load()}
                className="px-3 py-1 rounded border border-border text-xs text-text hover:bg-background"
              >
                {t('common.refresh', 'Refresh')}
              </button>
              <button
                type="button"
                onClick={clearTrail}
                disabled={busy !== null || (trail?.retained ?? 0) === 0}
                className="px-3 py-1 rounded border border-border text-xs text-error hover:bg-background disabled:opacity-50"
              >
                {busy === 'clear'
                  ? t('common.loading')
                  : t('security.clear', 'Clear trail')}
              </button>
            </span>
          </div>
          <p className="text-xs text-text-secondary">{t('security.eventsHint')}</p>
          <div className="flex flex-wrap items-center gap-1">
            <button
              type="button"
              onClick={() => setFilter('all')}
              className={`px-2 py-0.5 rounded border text-[11px] ${
                filter === 'all'
                  ? 'border-primary bg-primary-muted text-primary'
                  : 'border-border text-text-secondary hover:bg-background'
              }`}
            >
              {t('security.filterAll', 'All')}
              {trail && ` · ${trail.retained}`}
            </button>
            {counts.map((entry) => (
              <button
                key={entry.kind}
                type="button"
                onClick={() => setFilter(entry.kind)}
                className={`px-2 py-0.5 rounded border text-[11px] ${
                  filter === entry.kind
                    ? 'border-primary bg-primary-muted text-primary'
                    : 'border-border text-text-secondary hover:bg-background'
                }`}
              >
                {t(KIND_KEY[entry.kind] ?? entry.kind, entry.kind)} · {entry.count}
              </button>
            ))}
          </div>
          {(trail?.dropped ?? 0) > 0 && (
            <p className="text-[11px] text-text-secondary">
              {t('security.dropped', { count: trail?.dropped ?? 0 })}
            </p>
          )}
        </div>

        {trail && trail.events.length === 0 ? (
          <p className="px-4 py-3 text-sm text-text-secondary">{t('security.empty')}</p>
        ) : (
          (trail?.events ?? []).map((event) => (
            <div key={event.id} className="border-t border-border px-4 py-2 space-y-0.5">
              <div className="flex flex-wrap items-center gap-2 text-[11px]">
                <span className="font-mono text-text-secondary">
                  {new Date(event.at).toLocaleString()}
                </span>
                <span
                  className={`px-1.5 py-0.5 rounded border ${OUTCOME_STYLE[event.outcome] ?? ''}`}
                >
                  {t(OUTCOME_KEY[event.outcome] ?? event.outcome, event.outcome)}
                </span>
                <span className="text-text">{t(KIND_KEY[event.kind] ?? event.kind, event.kind)}</span>
                <span
                  className="font-mono text-text-secondary truncate max-w-[32rem]"
                  title={event.subject}
                >
                  {event.subject}
                </span>
              </div>
              {event.detail && (
                <p className="text-[11px] text-text-secondary break-words">{event.detail}</p>
              )}
            </div>
          ))
        )}
      </section>
    </div>
  );
};

export default Security;
