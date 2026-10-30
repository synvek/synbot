// API Response Types
import type { SubagentInfo } from './agentEvents';

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

// System Status
export type RuntimeChannelStatus =
  | 'starting'
  | 'connected'
  | 'reconnecting'
  | 'failed'
  | 'disabled'
  | 'stopped';

export interface RuntimeChannelSnapshot {
  instance_id: string;
  channel_type: string;
  name: string;
  enabled: boolean;
  status: RuntimeChannelStatus;
  started_at: string | null;
  last_connected_at: string | null;
  last_error: string | null;
  reconnect_count: number;
  last_received_at: string | null;
  last_sent_at: string | null;
  last_latency_ms: number | null;
  supports_send: boolean;
  supports_receive: boolean;
}

export interface SystemStatus {
  running: boolean;
  uptime_secs: number;
  session_count: number;
  channel_count: number;
  cron_job_count: number;
  agent_count: number;
  channels: RuntimeChannelSnapshot[];
}

// Session Types
export interface SessionSummary {
  id: string;
  channel: string;
  scope: string;
  identifier: string;
  message_count: number;
  created_at: string;
  updated_at: string;
}

export interface SessionMessage {
  role: string;
  content: string;
  timestamp: string;
}

export interface SessionMeta {
  id: string;
  channel: string;
  scope: string;
  identifier: string;
  created_at: string;
  updated_at: string;
}

export interface SessionDetail {
  meta: SessionMeta;
  messages: SessionMessage[];
}

// Channel Types
export type ChannelStatus = RuntimeChannelStatus;
export type ChannelInfo = RuntimeChannelSnapshot;

// Cron Job Types
export interface CronJobState {
  last_run_at_ms?: number;
  last_status?: string;
  next_run_at_ms?: number;
}

export interface CronJobInfo {
  id: string;
  name: string;
  schedule: string;
  enabled: boolean;
  state: CronJobState;
  payload: Record<string, unknown>;
}

// Agent Types
export interface AgentInfo {
  name: string;
  role: string;
  system_prompt: string;
  skills: string[];
  tools: string[];
  provider: string;
  model: string;
  max_tokens: number;
  temperature: number;
  max_iterations: number;
  max_idle_iterations: number;
  max_consecutive_tool_errors: number;
  workspace_dir: string;
}

// Skill Types
export interface SkillInfo {
  name: string;
  assigned_agents: string[];
}

export interface SkillDetail {
  name: string;
  content: string;
  assigned_agents: string[];
}

// Config Types (GET /api/config)
export interface ConfigApiPayload {
  config: Record<string, unknown>;
  configPath: string;
  restartNotice: string;
}

export interface PutConfigResponse {
  configPath: string;
  restartNotice: string;
}

export interface ValidationErrorItem {
  field: string;
  value: string;
  constraint: string;
}

// Log Types
export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

export interface LogQueryParams {
  level?: string;
  keyword?: string;
  page?: number;
  page_size?: number;
}

// Project Types (`/api/projects`)
export interface Project {
  id: string;
  name: string;
  path: string;
  added_at: string;
  git_head: string | null;
  rules: string[];
}

export interface ProjectFileEntry {
  name: string;
  /** Path relative to the project root, `/`-separated. */
  path: string;
  is_dir: boolean;
  size: number;
}

export interface ProjectDirectory {
  path: string;
  entries: ProjectFileEntry[];
}

export interface ProjectFileContent {
  path: string;
  content: string;
  truncated: boolean;
}

// Deliverable types (`/api/projects/{id}/deliverables|preview|download`): the office documents an
// agent produced. The server extracts text; PDFs and images are rendered by the browser itself.
export type DeliverableKind =
  | 'docx'
  | 'xlsx'
  | 'pptx'
  | 'pdf'
  | 'csv'
  | 'markdown'
  | 'html'
  | 'image'
  | 'text';

export interface Deliverable {
  name: string;
  /** Path relative to the project root, `/`-separated. */
  path: string;
  kind: DeliverableKind;
  size: number;
  /** Last modification, milliseconds since the Unix epoch. */
  modified_ms: number;
}

export interface DeliverableList {
  path: string;
  entries: Deliverable[];
  truncated: boolean;
  scanned: number;
}

export interface DocxPreview {
  title?: string;
  author?: string;
  paragraphs_total: number;
  words: number;
  paragraphs: string[];
  truncated: boolean;
}

export interface SheetPreview {
  name: string;
  rows: string[][];
  rows_total: number;
  cols: number;
  truncated: boolean;
}

export interface XlsxPreview {
  sheets: SheetPreview[];
  sheets_total: number;
  truncated: boolean;
}

export interface SlidePreview {
  index: number;
  title?: string;
  lines: string[];
}

export interface PptxPreview {
  slides_total: number;
  slides: SlidePreview[];
  truncated: boolean;
}

export interface DeliverablePreview {
  kind: DeliverableKind;
  size: number;
  /** The browser should fetch the bytes and render them (PDF, images). */
  passthrough: boolean;
  truncated: boolean;
  text?: string;
  docx?: DocxPreview;
  xlsx?: XlsxPreview;
  pptx?: PptxPreview;
}

export interface DeliverablePreviewResponse {
  path: string;
  name: string;
  preview: DeliverablePreview;
}

// Checkpoint types (`/api/checkpoints`): a turn's before/after images and their diff.
export type CheckpointChangeKind = 'created' | 'modified' | 'deleted' | 'renamed';

export interface DiffLine {
  kind: 'ctx' | 'add' | 'del';
  old_no?: number;
  new_no?: number;
  text: string;
}

export interface DiffHunk {
  old_start: number;
  new_start: number;
  lines: DiffLine[];
}

export interface CheckpointFile {
  path: string;
  display_path: string;
  change: CheckpointChangeKind;
  renamed_to: string | null;
  /** Why this file could not be snapshotted (binary / too large). */
  skipped: string | null;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export interface CheckpointView {
  id: string;
  created_at: string;
  workspace: string;
  undone_at: string | null;
  undo_skipped: string[];
  files: CheckpointFile[];
  additions: number;
  deletions: number;
}

export interface CheckpointSummary {
  id: string;
  created_at: string;
  files: number;
  undone_at: string | null;
}

export interface UndoReport {
  turn_id: string;
  restored: string[];
  skipped: string[];
  errors: string[];
}

// Git (`/api/git/*`) — read-only repository state for the workbench strip.
export interface GitFile {
  path: string;
  /** Porcelain letters: M, A, D, R, ??. */
  status: string;
  staged: boolean;
}

export interface GitStatus {
  is_repo: boolean;
  branch: string | null;
  ahead: number;
  behind: number;
  head: string | null;
  files: GitFile[];
}

export interface GitCommit {
  hash: string;
  author: string;
  date: string;
  subject: string;
}

// Task board (`/api/tasks`) — the plan persisted at `<workspace>/.synbot/plans/tasks.json`.
export type TaskStatus = 'pending' | 'in_progress' | 'completed' | 'cancelled';

export interface TaskItem {
  id: string;
  title: string;
  status: TaskStatus;
  notes?: string | null;
  created_at: string;
  updated_at: string;
}

export interface TaskBoardResponse {
  tasks: TaskItem[];
  updated_at: string | null;
  /** File the board lives in, shown in the panel footer. */
  path?: string;
}

// Custom slash commands (`/api/commands`) from `<workspace>/.synbot/commands/*.md`.
export interface CommandInfo {
  name: string;
  description: string;
  source: string;
}

// Repository symbol index (`/api/index*`) — the same file `find_symbols` reads.
export interface SymbolHit {
  path: string;
  line: number;
  column: number;
  kind: string;
  name: string;
  container?: string;
  signature: string;
  score: number;
}

export interface SymbolSearchResponse {
  hits: SymbolHit[];
  /** Files scanned in the index the search ran against. */
  files: number;
}

export interface RepoIndexStats {
  files: number;
  symbols: number;
  parsed: number;
  reused: number;
  removed: number;
  truncated: boolean;
  path: string;
  updated_at: string | null;
  languages: Record<string, number>;
}

// Agent team (`/api/subagents`) — the pool behind spawn / run_team / directive routing.
export interface SubagentListResponse {
  agents: SubagentInfo[];
  running: number;
}

// Plugin packages (`/api/plugins`) — manifest permissions, trust verdict and install state.
export type PluginTrust = 'trusted' | 'unsigned' | 'untrusted' | 'tampered';

export interface PluginPermissions {
  filesystem: { read: boolean; write: boolean; paths: string[] };
  network: { hosts: string[] };
  config: boolean;
}

export interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  homepage: string | null;
  permissions: PluginPermissions;
  /** Compact summary such as `fs:r net:api.example.com`, rendered as chips. */
  permission_summary: string;
  entry: string;
  trust: PluginTrust;
  sha256: string;
  wasm_bytes: number;
  path: string;
  installed_at: string | null;
  enabled: boolean;
  /** False for untrusted/tampered packages: kept on disk but never loaded. */
  loadable: boolean;
}

/** Config-only plugin (legacy `path` entry), shown but not managed as a package. */
export interface LegacyPlugin {
  id: string;
  enabled: boolean;
  internal: boolean;
}

export interface PluginListResponse {
  plugins: InstalledPlugin[];
  legacy: LegacyPlugin[];
  trust_keys: number;
  trust_store: string;
  root: string;
}

export interface TrustedKey {
  name: string;
  public_key: string;
}

export interface PluginKeysResponse {
  keys: TrustedKey[];
}

// Plugin market (`/api/market`) — published versions annotated with the local install state.
export interface MarketFile {
  path: string;
  url: string;
  sha256: string;
}

export interface MarketDependency {
  id: string;
  /** Version requirement (`>=1.0.0`, `^1.2`, …); null means any version. */
  version: string | null;
}

export interface MarketPlugin {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  homepage: string | null;
  tags: string[];
  manifest_url: string;
  entry_url: string | null;
  sha256: string;
  files: MarketFile[];
  dependencies: MarketDependency[];
  /** Source that published this version. */
  source: string;
  /** Locally installed version of the same id, when there is one. */
  installed_version: string | null;
  /** The market offers a strictly newer version than the installed one. */
  update_available: boolean;
}

export interface MarketSourceError {
  source: string;
  error: string;
}

export interface MarketResponse {
  sources: string[];
  plugins: MarketPlugin[];
  errors: MarketSourceError[];
}

export interface MarketInstalledSummary {
  id: string;
  version: string;
  trust: PluginTrust;
  dependencies: string[];
}

export interface MarketFailure {
  id: string;
  error: string;
}

/** Partial success is normal: `installed` and `failed` can both be non-empty. */
export interface MarketInstallReport {
  installed: MarketInstalledSummary[];
  failed: MarketFailure[];
}

// Security (`/api/security*`) — the effective policy and what the guard actually stopped.
export type SecurityKind =
  | 'command_blocked'
  | 'command_denied'
  | 'approval_unavailable'
  | 'approval_requested'
  | 'approval_approved'
  | 'approval_rejected'
  | 'approval_timeout'
  | 'sandbox_required'
  | 'path_escape'
  | 'plugin_permission';

export type SecurityOutcome = 'blocked' | 'needs_user' | 'allowed';

export interface SecurityEventItem {
  id: number;
  at: string;
  kind: SecurityKind;
  outcome: SecurityOutcome;
  /** Command, path, plugin id — whatever the event is about (truncated server-side). */
  subject: string;
  detail: string;
}

export interface SecurityCountItem {
  kind: SecurityKind;
  count: number;
}

export interface SecurityExecPolicy {
  timeout_secs: number;
  restrict_to_workspace: boolean;
  permissions_enabled: boolean;
  /** `allow` | `require_approval` | `deny`. */
  default_level: string;
  rule_count: number;
  approval_timeout_secs: number;
  deny_patterns: string[];
  /** `null` means "no allowlist configured" — different from an empty list, which rejects all. */
  allow_patterns: string[] | null;
}

export interface SecurityToolSandbox {
  backend: string;
  /** False when `backend` is only the built-in default rather than configured. */
  backend_explicit: boolean;
  /** `docker` or `host_native`. */
  exec_kind: string;
  image: string | null;
  network: { enabled: boolean; hosts: string[]; ports: string[] } | null;
  filesystem: { readonly: string[]; writable: string[]; hidden: string[] } | null;
  resources: { max_memory: string; max_cpu: string; max_disk: string } | null;
}

export interface SecurityOverview {
  /** `safe` | `developer` | `trusted`. */
  profile: string;
  platform: {
    os: string;
    version: string;
    arch: string;
    supported: boolean;
    recommended_app_sandbox: string | null;
    recommended_tool_sandbox: string | null;
  };
  workspace: string;
  app_sandbox: { platform: string; work_dir: string } | null;
  tool_sandbox: SecurityToolSandbox | null;
  exec: SecurityExecPolicy;
  secrets: { masking: boolean; patterns: number };
  plugins: { configured: number; packaged: number; legacy: number };
  audit: {
    capacity: number;
    retained: number;
    dropped: number;
    counts: SecurityCountItem[];
  };
}

export interface SecurityEventsResponse {
  events: SecurityEventItem[];
  counts: SecurityCountItem[];
  capacity: number;
  retained: number;
  dropped: number;
}

export interface SecurityRule {
  pattern: string;
  /** `allow` | `require_approval` | `deny`. */
  level: string;
  description?: string;
}

export interface PolicyExplain {
  command: string;
  /** `blocked` | `allowed` | `needs_approval`. */
  final: string;
  /** Reason from `deny_patterns` / `allow_patterns`, null when patterns let it through. */
  blocked_by_pattern: string | null;
  permissions_enabled: boolean;
  level: string;
  matched_rule: SecurityRule | null;
  default_applied: boolean;
  approval_timeout_secs: number;
}
