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
export interface SystemStatus {
  running: boolean;
  uptime_secs: number;
  session_count: number;
  channel_count: number;
  cron_job_count: number;
  role_count: number;
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
export type ChannelStatus = 'connected' | 'disconnected' | 'error' | 'disabled';

export interface ChannelInfo {
  name: string;
  enabled: boolean;
  status: ChannelStatus;
  config?: Record<string, unknown>;
}

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

// Role Types
export interface RoleInfo {
  name: string;
  system_prompt: string;
  skills: string[];
  tools: string[];
  provider: string;
  model: string;
  max_tokens: number;
  temperature: number;
  workspace_dir: string;
}

// Skill Types
export interface SkillInfo {
  name: string;
  assigned_roles: string[];
}

export interface SkillDetail {
  name: string;
  content: string;
  assigned_roles: string[];
}

// Config Types
export interface SanitizedConfig {
  channels: Record<string, unknown>;
  providers: Record<string, unknown>;
  agent: Record<string, unknown>;
  tools: Record<string, unknown>;
  web?: Record<string, unknown>;
  log?: Record<string, unknown>;
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
