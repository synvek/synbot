import axios, { AxiosInstance, AxiosError } from 'axios';
import type {
  SystemStatus,
  SessionSummary,
  SessionDetail,
  ChannelInfo,
  CronJobInfo,
  AgentInfo,
  SkillInfo,
  SkillDetail,
  ConfigApiPayload,
  PutConfigResponse,
  LogEntry,
  LogQueryParams,
  PaginatedResponse,
  Project,
  ProjectDirectory,
  ProjectFileContent,
  DeliverableList,
  DeliverablePreviewResponse,
  CheckpointView,
  CheckpointSummary,
  UndoReport,
  GitStatus,
  GitCommit,
  TaskBoardResponse,
  TaskStatus,
  CommandInfo,
  RepoIndexStats,
  SymbolSearchResponse,
  SubagentListResponse,
  PluginListResponse,
  InstalledPlugin,
  PluginKeysResponse,
  TrustedKey,
  MarketResponse,
  MarketInstallReport,
  SecurityOverview,
  SecurityEventsResponse,
  SecurityKind,
  PolicyExplain,
  ProviderCatalog,
} from '../types/api';

// API response wrapper from backend
interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export class ApiClient {
  private client: AxiosInstance;
  private authHeader?: string;
  /** Per-launch bearer secret handed over by the desktop shell (never persisted). */
  private desktopToken?: string;
  /** `ws://127.0.0.1:18888` in the desktop app; `undefined` means "same origin as the page". */
  private wsBase?: string;
  private desktop = false;

  constructor(baseUrl: string = '') {
    this.client = axios.create({
      baseURL: baseUrl,
      headers: {
        'Content-Type': 'application/json',
      },
    });

    // Add auth header to requests if set
    this.client.interceptors.request.use((config) => {
      if (this.authHeader) {
        config.headers.Authorization = this.authHeader;
      }
      return config;
    });

    // Handle errors globally
    this.client.interceptors.response.use(
      (response) => response,
      (error: AxiosError) => {
        if (error.response?.status === 401) {
          // The desktop shell owns the credentials; a 401 there means the daemon restarted with a
          // new secret, so bounce through a reload (which re-runs the IPC bootstrap) instead of
          // showing the web login form.
          if (this.desktop) {
            window.location.reload();
            return Promise.reject(error);
          }
          // Clear auth and redirect to login
          this.clearAuth();
          window.location.href = '/login';
        }
        return Promise.reject(error);
      }
    );
  }

  setAuth(username: string, password: string): void {
    const credentials = btoa(`${username}:${password}`);
    this.authHeader = `Basic ${credentials}`;
    sessionStorage.setItem('auth', this.authHeader);
  }

  /**
   * Switches this client to the desktop transport: absolute loopback base URL, bearer token
   * instead of the web password, and an explicit WebSocket origin.
   */
  configureDesktop(session: { apiBase: string; wsBase: string; token: string }): void {
    this.desktop = true;
    this.client.defaults.baseURL = session.apiBase;
    this.wsBase = session.wsBase.replace(/\/+$/, '');
    this.desktopToken = session.token;
    this.authHeader = `Bearer ${session.token}`;
    // A stale Basic header from an earlier web session would win over the token on reload.
    sessionStorage.removeItem('auth');
  }

  /** True when the client was configured by the desktop shell. */
  isDesktop(): boolean {
    return this.desktop;
  }

  /**
   * Absolute WebSocket URL for a `/ws/*` endpoint, carrying the desktop token as a query parameter
   * (browsers cannot set headers on a handshake). On the web the URL stays origin-relative.
   */
  wsUrl(path: string): string {
    const normalized = path.startsWith('/') ? path : `/${path}`;
    const base =
      this.wsBase ??
      `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}`;
    const url = `${base}${normalized}`;
    if (!this.desktopToken) return url;
    const separator = url.includes('?') ? '&' : '?';
    return `${url}${separator}token=${encodeURIComponent(this.desktopToken)}`;
  }

  loadAuth(): boolean {
    const stored = sessionStorage.getItem('auth');
    if (stored) {
      this.authHeader = stored;
      return true;
    }
    return false;
  }

  clearAuth(): void {
    this.authHeader = undefined;
    this.desktopToken = undefined;
    sessionStorage.removeItem('auth');
  }

  isAuthenticated(): boolean {
    return !!this.authHeader;
  }

  // System Status
  async getStatus(): Promise<SystemStatus> {
    const response = await this.client.get<ApiResponse<SystemStatus>>('/api/status');
    return response.data.data!;
  }

  // Sessions
  async getSessions(
    page?: number,
    pageSize?: number,
    channel?: string,
    scope?: string
  ): Promise<PaginatedResponse<SessionSummary>> {
    const params: Record<string, unknown> = {};
    if (page !== undefined) params.page = page;
    if (pageSize !== undefined) params.page_size = pageSize;
    if (channel) params.channel = channel;
    if (scope) params.scope = scope;

    const response = await this.client.get<ApiResponse<PaginatedResponse<SessionSummary>>>(
      '/api/sessions',
      { params }
    );
    return response.data.data!;
  }

  async getSession(id: string): Promise<SessionDetail> {
    const response = await this.client.get<ApiResponse<SessionDetail>>(`/api/sessions/${id}`);
    return response.data.data!;
  }

  // Channels
  async getChannels(): Promise<ChannelInfo[]> {
    const response = await this.client.get<ApiResponse<ChannelInfo[]>>('/api/channels');
    return response.data.data!;
  }

  // Cron Jobs
  async getCronJobs(): Promise<CronJobInfo[]> {
    const response = await this.client.get<ApiResponse<CronJobInfo[]>>('/api/cron');
    return response.data.data!;
  }

  async updateCronJob(id: string, enabled: boolean): Promise<CronJobInfo> {
    const response = await this.client.patch<ApiResponse<CronJobInfo>>(`/api/cron/${id}`, {
      enabled,
    });
    return response.data.data!;
  }

  // Agents
  async getAgents(): Promise<AgentInfo[]> {
    const response = await this.client.get<ApiResponse<AgentInfo[]>>('/api/agents');
    return response.data.data!;
  }

  // Skills
  async getSkills(): Promise<SkillInfo[]> {
    const response = await this.client.get<ApiResponse<SkillInfo[]>>('/api/skills');
    return response.data.data!;
  }

  async getSkill(name: string): Promise<SkillDetail> {
    const response = await this.client.get<ApiResponse<SkillDetail>>(`/api/skills/${name}`);
    return response.data.data!;
  }

  // Projects
  async getProjects(): Promise<Project[]> {
    const response = await this.client.get<ApiResponse<Project[]>>('/api/projects');
    return response.data.data ?? [];
  }

  async openProject(path: string): Promise<Project> {
    const response = await this.client.post<ApiResponse<Project>>('/api/projects', { path });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to open project');
    }
    return response.data.data;
  }

  async closeProject(id: string): Promise<void> {
    await this.client.delete(`/api/projects/${encodeURIComponent(id)}`);
  }

  async getProjectFiles(id: string, path = ''): Promise<ProjectDirectory> {
    const response = await this.client.get<ApiResponse<ProjectDirectory>>(
      `/api/projects/${encodeURIComponent(id)}/files`,
      { params: { path } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to list project files');
    }
    return response.data.data;
  }

  async getProjectFile(id: string, path: string): Promise<ProjectFileContent> {
    const response = await this.client.get<ApiResponse<ProjectFileContent>>(
      `/api/projects/${encodeURIComponent(id)}/file`,
      { params: { path } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to read project file');
    }
    return response.data.data;
  }

  // Deliverables: office documents the agent produced, previewed and exported.
  async getProjectDeliverables(id: string, path = ''): Promise<DeliverableList> {
    const response = await this.client.get<ApiResponse<DeliverableList>>(
      `/api/projects/${encodeURIComponent(id)}/deliverables`,
      { params: { path } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to list deliverables');
    }
    return response.data.data;
  }

  async getDeliverablePreview(id: string, path: string): Promise<DeliverablePreviewResponse> {
    const response = await this.client.get<ApiResponse<DeliverablePreviewResponse>>(
      `/api/projects/${encodeURIComponent(id)}/preview`,
      { params: { path } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to preview deliverable');
    }
    return response.data.data;
  }

  /**
   * Raw bytes of a deliverable. Goes through axios (not a plain link) because the Basic auth header
   * only exists on requests this client makes.
   */
  async downloadDeliverable(id: string, path: string): Promise<Blob> {
    const response = await this.client.get<Blob>(
      `/api/projects/${encodeURIComponent(id)}/download`,
      { params: { path }, responseType: 'blob' }
    );
    return response.data;
  }

  // Checkpoints: what a turn wrote, with diffs, and rolling it back.
  async getCheckpoint(turnId: string): Promise<CheckpointView> {
    const response = await this.client.get<ApiResponse<CheckpointView>>(
      `/api/checkpoints/${encodeURIComponent(turnId)}`
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Checkpoint not found');
    }
    return response.data.data;
  }

  async listCheckpoints(limit = 20): Promise<CheckpointSummary[]> {
    const response = await this.client.get<ApiResponse<CheckpointSummary[]>>('/api/checkpoints', {
      params: { limit },
    });
    return response.data.data ?? [];
  }

  async undoCheckpoint(turnId: string): Promise<UndoReport> {
    const response = await this.client.post<ApiResponse<UndoReport>>(
      `/api/checkpoints/${encodeURIComponent(turnId)}/undo`
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Undo failed');
    }
    return response.data.data;
  }

  // Git (read-only): what the agent sees through `git_status` / `git_diff` / `git_log`.
  async getGitStatus(projectId?: string | null): Promise<GitStatus> {
    const response = await this.client.get<ApiResponse<GitStatus>>('/api/git/status', {
      params: projectId ? { project_id: projectId } : {},
    });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to read git status');
    }
    return response.data.data;
  }

  async getGitDiff(path?: string, staged = false, projectId?: string | null): Promise<string> {
    const response = await this.client.get<ApiResponse<{ diff: string }>>('/api/git/diff', {
      params: {
        ...(path ? { path } : {}),
        staged,
        ...(projectId ? { project_id: projectId } : {}),
      },
    });
    return response.data.data?.diff ?? '';
  }

  async getGitLog(limit = 20, projectId?: string | null): Promise<GitCommit[]> {
    const response = await this.client.get<ApiResponse<GitCommit[]>>('/api/git/log', {
      params: { limit, ...(projectId ? { project_id: projectId } : {}) },
    });
    return response.data.data ?? [];
  }

  // Task board
  async getTasks(projectId?: string | null): Promise<TaskBoardResponse> {
    const response = await this.client.get<ApiResponse<TaskBoardResponse>>('/api/tasks', {
      params: projectId ? { project_id: projectId } : {},
    });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to read the task board');
    }
    return response.data.data;
  }

  async updateTask(
    id: string,
    status: TaskStatus,
    projectId?: string | null
  ): Promise<TaskBoardResponse> {
    const response = await this.client.patch<ApiResponse<TaskBoardResponse>>(
      `/api/tasks/${encodeURIComponent(id)}`,
      { status },
      { params: projectId ? { project_id: projectId } : {} }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to update the task');
    }
    return response.data.data;
  }

  // Custom slash commands for the composer picker
  async getCommands(projectId?: string | null): Promise<CommandInfo[]> {
    const response = await this.client.get<ApiResponse<CommandInfo[]>>('/api/commands', {
      params: projectId ? { project_id: projectId } : {},
    });
    return response.data.data ?? [];
  }

  // Repo symbol index: search is what the files tab uses; refresh rebuilds incrementally.
  async searchSymbols(
    q: string,
    projectId?: string | null,
    limit = 20
  ): Promise<SymbolSearchResponse> {
    const response = await this.client.get<ApiResponse<SymbolSearchResponse>>(
      '/api/index/symbols',
      { params: { q, limit, ...(projectId ? { project_id: projectId } : {}) } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Symbol search failed');
    }
    return response.data.data;
  }

  async refreshRepoIndex(projectId?: string | null): Promise<RepoIndexStats> {
    const response = await this.client.post<ApiResponse<RepoIndexStats>>(
      '/api/index/refresh',
      {},
      { params: projectId ? { project_id: projectId } : {} }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Index refresh failed');
    }
    return response.data.data;
  }

  // Agent team (subagents)
  async listSubagents(): Promise<SubagentListResponse> {
    const response = await this.client.get<ApiResponse<SubagentListResponse>>('/api/subagents');
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load subagents');
    }
    return response.data.data;
  }

  /** Stop one teammate (or every running one with `all`). */
  async cancelSubagent(id: string, all = false): Promise<number> {
    const response = await this.client.post<ApiResponse<{ cancelled: number }>>(
      `/api/subagents/${encodeURIComponent(id)}/cancel`,
      { all }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Cancel failed');
    }
    return response.data.data.cancelled;
  }

  // Plugins (packages + trusted publisher keys)
  async listPlugins(): Promise<PluginListResponse> {
    const response = await this.client.get<ApiResponse<PluginListResponse>>('/api/plugins');
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load plugins');
    }
    return response.data.data;
  }

  /** Install a package directory (`plugin.json` + wasm entry) from the daemon's filesystem. */
  async installPlugin(path: string): Promise<InstalledPlugin> {
    const response = await this.client.post<ApiResponse<InstalledPlugin>>('/api/plugins/install', {
      path,
    });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Install failed');
    }
    return response.data.data;
  }

  async uninstallPlugin(id: string): Promise<void> {
    const response = await this.client.delete<ApiResponse<{ removed: string }>>(
      `/api/plugins/${encodeURIComponent(id)}`
    );
    if (!response.data.success) {
      throw new Error(response.data.error ?? 'Uninstall failed');
    }
  }

  async setPluginEnabled(id: string, enabled: boolean): Promise<void> {
    const response = await this.client.post<ApiResponse<{ id: string; enabled: boolean }>>(
      `/api/plugins/${encodeURIComponent(id)}/enabled`,
      { enabled }
    );
    if (!response.data.success) {
      throw new Error(response.data.error ?? 'Toggle failed');
    }
  }

  async listPluginKeys(): Promise<TrustedKey[]> {
    const response = await this.client.get<ApiResponse<PluginKeysResponse>>('/api/plugin-keys');
    return response.data.data?.keys ?? [];
  }

  async addPluginKey(name: string, publicKey: string): Promise<TrustedKey> {
    const response = await this.client.post<ApiResponse<TrustedKey>>('/api/plugin-keys', {
      name,
      public_key: publicKey,
    });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Could not trust that key');
    }
    return response.data.data;
  }

  async removePluginKey(name: string): Promise<void> {
    const response = await this.client.delete<ApiResponse<{ removed: string }>>(
      `/api/plugin-keys/${encodeURIComponent(name)}`
    );
    if (!response.data.success) {
      throw new Error(response.data.error ?? 'Remove failed');
    }
  }

  // Plugin market (published index + install/update)
  async listMarket(): Promise<MarketResponse> {
    const response = await this.client.get<ApiResponse<MarketResponse>>('/api/market');
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load the plugin market');
    }
    return response.data.data;
  }

  /** Install `id` plus whatever it depends on; `version` pins an exact version. */
  async installMarketPlugin(id: string, version?: string): Promise<MarketInstallReport> {
    const response = await this.client.post<ApiResponse<MarketInstallReport>>(
      '/api/market/install',
      version ? { id, version } : { id }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Install failed');
    }
    return response.data.data;
  }

  /** Install every newer version the market offers for the installed plugins. */
  async updateMarketPlugins(): Promise<MarketInstallReport> {
    const response = await this.client.post<ApiResponse<MarketInstallReport>>(
      '/api/market/update-all'
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Update failed');
    }
    return response.data.data;
  }

  // Security (effective policy, audit trail, command dry-run)
  async securityOverview(): Promise<SecurityOverview> {
    const response = await this.client.get<ApiResponse<SecurityOverview>>('/api/security/overview');
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load the security overview');
    }
    return response.data.data;
  }

  async listSecurityEvents(kind?: SecurityKind, limit = 100): Promise<SecurityEventsResponse> {
    const response = await this.client.get<ApiResponse<SecurityEventsResponse>>(
      '/api/security/events',
      { params: { limit, ...(kind ? { kind } : {}) } }
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load security events');
    }
    return response.data.data;
  }

  /** Drop the in-memory trail; returns how many events were discarded. */
  async clearSecurityEvents(): Promise<number> {
    const response = await this.client.delete<ApiResponse<{ cleared: number }>>(
      '/api/security/events'
    );
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Clear failed');
    }
    return response.data.data.cleared;
  }

  /** Dry-run one command against the deny/allow patterns and the permission rules. */
  async explainPolicy(command: string): Promise<PolicyExplain> {
    const response = await this.client.post<ApiResponse<PolicyExplain>>('/api/security/policy', {
      command,
    });
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Explain failed');
    }
    return response.data.data;
  }

  // Config
  async getConfig(): Promise<ConfigApiPayload> {
    const response = await this.client.get<ApiResponse<ConfigApiPayload>>('/api/config');
    return response.data.data!;
  }

  /**
   * Provider metadata for the settings forms: which wire format each built-in speaks, the endpoint
   * used when `apiBase` is empty, whether a key is set, and where each name is selected.
   */
  async listProviders(): Promise<ProviderCatalog> {
    const response = await this.client.get<ApiResponse<ProviderCatalog>>('/api/providers');
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Failed to load providers');
    }
    return response.data.data;
  }

  async updateConfig(config: unknown): Promise<PutConfigResponse> {
    const response = await this.client.put<ApiResponse<PutConfigResponse>>('/api/config', config);
    if (!response.data.success || !response.data.data) {
      throw new Error(response.data.error ?? 'Save failed');
    }
    return response.data.data;
  }

  // Logs
  async getLogs(params: LogQueryParams): Promise<PaginatedResponse<LogEntry>> {
    const response = await this.client.get<ApiResponse<PaginatedResponse<LogEntry>>>(
      '/api/logs',
      { params }
    );
    return response.data.data!;
  }
}

// Export singleton instance
export const apiClient = new ApiClient();
