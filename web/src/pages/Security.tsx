import { useCallback, useEffect, useMemo, useState } from 'react'
import { RefreshCw, ShieldCheck, Trash2 } from 'lucide-react'
import { apiClient } from '../api/client'
import type {
  PolicyExplain,
  SecurityEventsResponse,
  SecurityKind,
  SecurityOutcome,
  SecurityOverview,
} from '../types/api'
import { useI18n } from '../i18n/I18nContext'
import { formatClock } from '../utils/datetime'
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  EmptyState,
  Field as FormField,
  Input,
  PageHeader,
  type BadgeTone,
  type Column,
} from '../ui'

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
}

const OUTCOME_KEY: Record<SecurityOutcome, string> = {
  blocked: 'security.outcomeBlocked',
  needs_user: 'security.outcomeNeedsUser',
  allowed: 'security.outcomeAllowed',
}

/** Outcome → badge tone, mapped once: no class name is ever built from the payload. */
const OUTCOME_TONE: Record<SecurityOutcome, BadgeTone> = {
  blocked: 'danger',
  needs_user: 'warning',
  allowed: 'success',
}

const FINAL_TONE: Record<string, BadgeTone> = {
  blocked: 'danger',
  needs_approval: 'warning',
  allowed: 'success',
}

const PROFILE_KEY: Record<string, string> = {
  safe: 'security.profileSafe',
  developer: 'security.profileDeveloper',
  trusted: 'security.profileTrusted',
}

const LEVEL_KEY: Record<string, string> = {
  allow: 'security.levelAllow',
  require_approval: 'security.levelRequireApproval',
  deny: 'security.levelDeny',
}

/** `label: value` row used by the policy panels. */
function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-baseline gap-2 text-micro">
      <span className="shrink-0 text-ink-3">{label}</span>
      <span className="break-all text-ink">{children}</span>
    </div>
  )
}

function Chips({ values, tone = 'neutral' }: { values: string[]; tone?: BadgeTone }) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {values.map((value) => (
        <Badge key={value} tone={tone}>
          <span className="font-mono">{value}</span>
        </Badge>
      ))}
    </div>
  )
}

export default function Security() {
  const { t } = useI18n()
  const [overview, setOverview] = useState<SecurityOverview | null>(null)
  const [trail, setTrail] = useState<SecurityEventsResponse | null>(null)
  const [filter, setFilter] = useState<SecurityKind | 'all'>('all')
  const [command, setCommand] = useState('')
  const [verdict, setVerdict] = useState<PolicyExplain | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)

  const load = useCallback(async () => {
    try {
      const [policy, events] = await Promise.all([
        apiClient.securityOverview(),
        apiClient.listSecurityEvents(filter === 'all' ? undefined : filter),
      ])
      setOverview(policy)
      setTrail(events)
      setError(null)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [filter])

  useEffect(() => {
    void load()
  }, [load])

  const explain = async (event?: React.FormEvent) => {
    event?.preventDefault()
    const cmd = command.trim()
    if (!cmd) return
    setBusy('explain')
    setError(null)
    setNotice(null)
    try {
      setVerdict(await apiClient.explainPolicy(cmd))
    } catch (e: unknown) {
      setVerdict(null)
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  const clearTrail = async () => {
    setConfirmClear(false)
    setBusy('clear')
    setError(null)
    setNotice(null)
    try {
      const cleared = await apiClient.clearSecurityEvents()
      setNotice(t('security.cleared', { count: cleared }))
      await load()
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  const counts = trail?.counts ?? overview?.audit.counts ?? []
  const exec = overview?.exec
  const sandbox = overview?.tool_sandbox
  const appSandbox = overview?.app_sandbox

  const columns: Array<Column<SecurityEventsResponse['events'][number]>> = useMemo(
    () => [
      {
        key: 'at',
        header: t('logs.timestamp', 'Time'),
        width: '160px',
        render: (row) => (
          <span className="tabular font-mono text-micro text-ink-3">
            {formatClock(row.at)}
          </span>
        ),
      },
      {
        key: 'outcome',
        header: t('security.outcome', 'Outcome'),
        width: '130px',
        render: (row) => (
          <Badge tone={OUTCOME_TONE[row.outcome]}>
            {t(OUTCOME_KEY[row.outcome] ?? row.outcome, row.outcome)}
          </Badge>
        ),
      },
      {
        key: 'kind',
        header: t('security.kind', 'Kind'),
        width: '180px',
        render: (row) => (
          <span className="text-micro text-ink-2">{t(KIND_KEY[row.kind] ?? row.kind, row.kind)}</span>
        ),
      },
      {
        key: 'subject',
        header: t('security.subject', 'Subject'),
        render: (row) => (
          <div className="min-w-0">
            <div className="truncate font-mono text-micro text-ink" title={row.subject}>
              {row.subject}
            </div>
            {row.detail && (
              <div className="truncate text-micro text-ink-3" title={row.detail}>
                {row.detail}
              </div>
            )}
          </div>
        ),
      },
    ],
    [t]
  )

  return (
    <div className="space-y-4">
      <PageHeader
        title={t('security.title', 'Security')}
        count={trail?.retained}
        description={t('security.description', 'Effective exec policy, sandbox limits, and the audit trail')}
        actions={
          <Button
            variant="ghost"
            size="sm"
            icon={<RefreshCw className="h-3.5 w-3.5" />}
            onClick={() => void load()}
          >
            {t('common.refresh', 'Refresh')}
          </Button>
        }
      />

      {error && <div className="rounded-panel border px-3 py-2 text-small tone-danger">{error}</div>}
      {notice && <div className="rounded-panel border px-3 py-2 text-small tone-success">{notice}</div>}

      <div className="grid gap-px overflow-hidden rounded-panel border border-line bg-line lg:grid-cols-2">
        <section className="space-y-1.5 bg-surface p-4">
          <div className="mb-2 flex items-center gap-2">
            <h2 className="text-section font-semibold text-ink">{t('security.execPolicy', 'Exec policy')}</h2>
            {overview && (
              <Badge tone="accent" className="ml-auto">
                {t(PROFILE_KEY[overview.profile] ?? 'security.profileUnknown', overview.profile)}
              </Badge>
            )}
          </div>
          {exec && (
            <>
              <Row label={t('security.defaultLevel', 'Default level')}>
                {t(LEVEL_KEY[exec.default_level] ?? exec.default_level, exec.default_level)}
              </Row>
              <Row label={t('security.permissions', 'Permissions')}>
                {exec.permissions_enabled
                  ? t('security.enabled', 'enabled')
                  : t('security.disabled', 'disabled')}{' '}
                · {t('security.rules', { count: exec.rule_count })}
              </Row>
              <Row label={t('security.timeout', 'Timeout')}>
                {t('security.seconds', { count: exec.timeout_secs })}
              </Row>
              <Row label={t('security.approvalTimeout', 'Approval timeout')}>
                {t('security.seconds', { count: exec.approval_timeout_secs })}
              </Row>
              <Row label={t('security.restrict', 'Restrict to workspace')}>
                {exec.restrict_to_workspace ? t('security.yes', 'yes') : t('security.no', 'no')}
              </Row>
              <div className="space-y-1 pt-2">
                <Row label={`${t('security.denyPatterns', 'Deny patterns')}:`}>
                  {exec.deny_patterns.length === 0 ? (
                    <span className="text-ink-3">{t('security.none', 'none')}</span>
                  ) : (
                    <Chips values={exec.deny_patterns} tone="danger" />
                  )}
                </Row>
                <Row label={`${t('security.allowPatterns', 'Allow patterns')}:`}>
                  {!exec.allow_patterns ? (
                    <span className="text-ink-3">{t('security.none', 'none')}</span>
                  ) : exec.allow_patterns.length === 0 ? (
                    // A configured-but-empty allowlist rejects every command.
                    <span className="text-warning">{t('security.allowEmpty', 'Empty allowlist: every command is rejected')}</span>
                  ) : (
                    <Chips values={exec.allow_patterns} />
                  )}
                </Row>
              </div>
            </>
          )}
          {overview && (
            <div className="mt-2 space-y-1 border-t border-line pt-2">
              <Row label={t('security.workspace', 'Workspace')}>
                <span className="font-mono">{overview.workspace}</span>
              </Row>
              <Row label={t('security.platform', 'Platform')}>
                <span className="font-mono">
                  {overview.platform.os} {overview.platform.version} ({overview.platform.arch})
                </span>{' '}
                {overview.platform.supported ? '✓' : t('security.unsupported', 'unsupported')}
              </Row>
            </div>
          )}
        </section>

        <section className="space-y-1.5 bg-surface p-4">
          <h2 className="mb-2 text-section font-semibold text-ink">{t('security.sandbox', 'Sandbox')}</h2>
          {overview && !sandbox && (
            <p className="text-small text-warning">{t('security.notConfigured', 'No tool sandbox configured')}</p>
          )}
          {sandbox && (
            <>
              <Row label={t('security.backend', 'Backend')}>
                <span className="font-mono">{sandbox.backend}</span>
                {!sandbox.backend_explicit && (
                  <span className="text-ink-3"> ({t('security.backendDefault', 'built-in default')})</span>
                )}
              </Row>
              <Row label={t('security.execKind', 'Exec kind')}>
                {sandbox.exec_kind === 'docker'
                  ? t('security.execDocker', 'docker')
                  : t('security.execHostNative', 'host filesystem')}
              </Row>
              {sandbox.image && (
                <Row label={t('security.image', 'Image')}>
                  <span className="font-mono">{sandbox.image}</span>
                </Row>
              )}
              {sandbox.network && (
                <>
                  <Row label={t('security.network', 'Network')}>
                    {sandbox.network.enabled
                      ? t('security.enabled', 'enabled')
                      : t('security.disabled', 'disabled')}
                  </Row>
                  {sandbox.network.hosts.length > 0 && (
                    <Row label={`${t('security.networkHosts', 'Hosts')}:`}>
                      <Chips values={sandbox.network.hosts} />
                    </Row>
                  )}
                  {sandbox.network.ports.length > 0 && (
                    <Row label={`${t('security.networkPorts', 'Ports')}:`}>
                      <Chips values={sandbox.network.ports} />
                    </Row>
                  )}
                </>
              )}
              {sandbox.filesystem && (
                <>
                  <Row label={`${t('security.readonly', 'Read-only')}:`}>
                    {sandbox.filesystem.readonly.length === 0 ? (
                      <span className="text-ink-3">{t('security.none', 'none')}</span>
                    ) : (
                      <Chips values={sandbox.filesystem.readonly} />
                    )}
                  </Row>
                  <Row label={`${t('security.writable', 'Writable')}:`}>
                    {sandbox.filesystem.writable.length === 0 ? (
                      <span className="text-ink-3">{t('security.none', 'none')}</span>
                    ) : (
                      <Chips values={sandbox.filesystem.writable} />
                    )}
                  </Row>
                </>
              )}
              {sandbox.resources && (
                <Row label={t('security.resources', 'Resources')}>
                  <span className="font-mono">
                    {t('security.memory', 'memory')}={sandbox.resources.max_memory}{' '}
                    {t('security.cpu', 'cpu')}={sandbox.resources.max_cpu}{' '}
                    {t('security.disk', 'disk')}={sandbox.resources.max_disk}
                  </span>
                </Row>
              )}
            </>
          )}
          {appSandbox && (
            <div className="mt-2 border-t border-line pt-2">
              <Row label={t('security.appSandbox', 'App sandbox')}>
                <span className="font-mono">{appSandbox.platform}</span> ·{' '}
                <span className="font-mono">{appSandbox.work_dir}</span>
              </Row>
            </div>
          )}
          {overview && (
            <div className="mt-2 space-y-1 border-t border-line pt-2">
              <Row label={t('security.secrets', 'Secrets')}>
                {t('security.secretsDetail', { count: overview.secrets.patterns })}
              </Row>
              <Row label={t('security.plugins', 'Plugins')}>
                {t('security.pluginsDetail', {
                  packaged: overview.plugins.packaged,
                  legacy: overview.plugins.legacy,
                })}
              </Row>
            </div>
          )}
        </section>
      </div>

      <section className="rounded-panel border border-line p-4">
        <h2 className="text-section font-semibold text-ink">{t('security.tryTitle', 'Try a command')}</h2>
        <p className="mb-2 mt-0.5 text-small text-ink-2">{t('security.tryHint', 'See how the policy would judge it, without running anything.')}</p>
        <form onSubmit={explain} className="flex flex-wrap items-end gap-2">
          <FormField label={t('security.command', 'Command')} htmlFor="security-command" className="min-w-[16rem] flex-1">
            <Input
              id="security-command"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder={t('security.commandPlaceholder', 'rm -rf build')}
              className="font-mono"
            />
          </FormField>
          <Button
            type="submit"
            variant="primary"
            size="sm"
            disabled={busy !== null || !command.trim()}
            loading={busy === 'explain'}
          >
            {t('security.explain', 'Explain')}
          </Button>
          {verdict && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setVerdict(null)
                setCommand('')
              }}
            >
              {t('common.clear', 'Clear')}
            </Button>
          )}
        </form>

        {verdict && (
          <div className="mt-3 space-y-1.5 rounded-panel bg-surface-2 px-3 py-2">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={FINAL_TONE[verdict.final] ?? 'neutral'}>
                {t(`security.final_${verdict.final}`, verdict.final)}
              </Badge>
              <span className="break-all font-mono text-micro text-ink-2">{verdict.command}</span>
            </div>
            {verdict.blocked_by_pattern && (
              <Row label={t('security.blockedByPattern', 'Blocked by pattern')}>{verdict.blocked_by_pattern}</Row>
            )}
            <Row label={t('security.level', 'Level')}>
              {t(LEVEL_KEY[verdict.level] ?? verdict.level, verdict.level)}
            </Row>
            {verdict.matched_rule ? (
              <>
                <Row label={t('security.matchedRule', 'Matched rule')}>
                  <Chips values={[verdict.matched_rule.pattern]} /> →{' '}
                  {t(
                    LEVEL_KEY[verdict.matched_rule.level] ?? verdict.matched_rule.level,
                    verdict.matched_rule.level
                  )}
                </Row>
                {verdict.matched_rule.description && (
                  <p className="text-micro text-ink-3">{verdict.matched_rule.description}</p>
                )}
              </>
            ) : (
              <p className="text-micro text-ink-3">{t('security.noRule', 'No rule matched; the default level applies.')}</p>
            )}
            {verdict.final === 'needs_approval' && (
              <p className="text-micro text-ink-3">
                {t('security.approvalWithin', {
                  seconds: verdict.approval_timeout_secs,
                })}
              </p>
            )}
          </div>
        )}
      </section>

      <section className="overflow-hidden rounded-panel border border-line">
        <div className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
          <h2 className="text-section font-semibold text-ink">{t('security.events', 'Audit trail')}</h2>
          {trail && (
            <span className="text-micro text-ink-3">
              {t('security.showing', {
                shown: trail.events.length,
                retained: trail.retained,
                capacity: trail.capacity,
              })}
            </span>
          )}
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="danger"
              size="sm"
              icon={<Trash2 className="h-3.5 w-3.5" />}
              onClick={() => setConfirmClear(true)}
              disabled={busy !== null || (trail?.retained ?? 0) === 0}
            >
              {t('security.clear', 'Clear trail')}
            </Button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1 border-b border-line px-3 py-2">
          <button
            type="button"
            onClick={() => setFilter('all')}
            aria-pressed={filter === 'all'}
            className={
              filter === 'all'
                ? 'rounded-control bg-surface-2 px-1.5 py-0.5 text-micro font-medium text-ink'
                : 'rounded-control px-1.5 py-0.5 text-micro text-ink-2 transition-colors duration-fast hover:bg-surface-2 hover:text-ink'
            }
          >
            {t('security.filterAll', 'All')}
            {trail && ` · ${trail.retained}`}
          </button>
          {counts.map((entry) => (
            <button
              key={entry.kind}
              type="button"
              onClick={() => setFilter(entry.kind)}
              aria-pressed={filter === entry.kind}
              className={
                filter === entry.kind
                  ? 'rounded-control bg-surface-2 px-1.5 py-0.5 text-micro font-medium text-ink'
                  : 'rounded-control px-1.5 py-0.5 text-micro text-ink-2 transition-colors duration-fast hover:bg-surface-2 hover:text-ink'
              }
            >
              {t(KIND_KEY[entry.kind] ?? entry.kind, entry.kind)} · {entry.count}
            </button>
          ))}
          {(trail?.dropped ?? 0) > 0 && (
            <span className="ml-auto text-micro text-ink-3">
              {t('security.dropped', { count: trail?.dropped ?? 0 })}
            </span>
          )}
        </div>

        <DataTable
          rows={trail?.events ?? []}
          columns={columns}
          rowKey={(row) => String(row.id)}
          density="compact"
          empty={
            <EmptyState
              size="inline"
              icon={<ShieldCheck className="h-5 w-5" />}
              title={t('security.empty', 'Nothing blocked yet')}
            />
          }
        />
      </section>

      {confirmClear && (
        <Dialog
          open
          onClose={() => setConfirmClear(false)}
          title={t('security.clear', 'Clear trail')}
          width="sm"
          closeLabel={t('common.close', 'Close')}
          footer={
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)}>
                {t('common.cancel', 'Cancel')}
              </Button>
              <Button variant="danger" size="sm" onClick={() => void clearTrail()}>
                {t('common.confirm', 'Confirm')}
              </Button>
            </div>
          }
        >
          <p className="text-small text-ink-2">
            {t('security.clearConfirm', 'Clear the in-memory audit trail?')}
          </p>
        </Dialog>
      )}
    </div>
  )
}
