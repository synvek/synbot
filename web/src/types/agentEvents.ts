// Protocol types for /ws/agent (v2).
//
// Mirrors `src/web/handlers/ws_agent.rs` and `src/agent/events.rs`.

import { ApprovalRequest, HistoryMessage } from './websocket';

export type ToolCallStatus = 'success' | 'failure';
export type TurnFinishReason =
  | 'completed'
  | 'cancelled'
  | 'max_iterations'
  | 'no_progress'
  | 'max_tool_errors'
  | 'error';

export interface PlanStep {
  id: string;
  title: string;
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
}

/** A teammate (spawn / run_team / directive routing) as the team panel renders it. */
export type SubagentState = 'running' | 'completed' | 'failed' | 'cancelled';

export interface SubagentInfo {
  id: string;
  label: string;
  role?: string;
  state: SubagentState;
  created_at: string;
  finished_at?: string;
  activity?: string;
}

/** A folder opened as a workspace (`/api/projects`, `project` socket message). */
export interface ProjectInfo {
  id: string;
  name: string;
  path: string;
  added_at: string;
  git_head: string | null;
  rules: string[];
}

export type AgentEvent =
  | { type: 'turn_started'; turn_id: string; agent_id: string }
  | { type: 'message_delta'; turn_id: string; delta: string }
  | { type: 'reasoning_delta'; turn_id: string; delta: string }
  | {
      type: 'tool_call_started';
      turn_id: string;
      call_id: string;
      tool_name: string;
      args_preview: string;
    }
  | {
      type: 'tool_call_finished';
      turn_id: string;
      call_id: string;
      tool_name: string;
      status: ToolCallStatus;
      result_preview: string;
      duration_ms: number;
    }
  | { type: 'plan_updated'; turn_id: string; steps: PlanStep[] }
  | { type: 'subagent_updated'; subagent: SubagentInfo }
  | { type: 'approval_requested'; turn_id: string; request: ApprovalRequest }
  | { type: 'file_changed'; turn_id: string; path: string; change_kind: string }
  | {
      type: 'usage';
      turn_id: string;
      prompt_tokens: number;
      completion_tokens: number;
      total_tokens: number;
    }
  | { type: 'turn_finished'; turn_id: string; reason: TurnFinishReason; iterations: number }
  | { type: 'error'; turn_id: string; message: string };

/** Wire envelope: one sequenced event for a conversation. */
export interface SequencedEvent {
  seq: number;
  channel: string;
  chat_id: string;
  event: AgentEvent;
}

export type AgentClientMessage =
  | { type: 'hello'; after_seq?: number }
  | { type: 'chat'; content: string }
  | { type: 'cancel' }
  | { type: 'select_project'; project_id: string | null }
  | { type: 'approval_response'; request_id: string; approved: boolean }
  | { type: 'ping' };

export type AgentServerMessage =
  | { type: 'hello'; session_id: string; chat_id: string; last_seq: number }
  | { type: 'history'; messages: HistoryMessage[] }
  | { type: 'replay'; events: SequencedEvent[] }
  | ({ type: 'event' } & SequencedEvent)
  | { type: 'approval_request'; request: ApprovalRequest }
  | { type: 'approval_result'; request_id: string; approved: boolean; message: string }
  | { type: 'project'; project: ProjectInfo | null }
  | { type: 'pong' }
  | { type: 'error'; message: string };
