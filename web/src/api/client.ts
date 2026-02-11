import axios, { AxiosInstance, AxiosError } from 'axios';
import type {
  SystemStatus,
  SessionSummary,
  SessionDetail,
  ChannelInfo,
  CronJobInfo,
  RoleInfo,
  SkillInfo,
  SkillDetail,
  SanitizedConfig,
  LogEntry,
  LogQueryParams,
  PaginatedResponse,
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

  // Roles
  async getRoles(): Promise<RoleInfo[]> {
    const response = await this.client.get<ApiResponse<RoleInfo[]>>('/api/roles');
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

  // Config
  async getConfig(): Promise<SanitizedConfig> {
    const response = await this.client.get<ApiResponse<SanitizedConfig>>('/api/config');
    return response.data.data!;
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
