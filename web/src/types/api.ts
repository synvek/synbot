// API Response Types
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
