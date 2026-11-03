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

/**
 * A plan from a plan-mode turn waiting for approval (`plan_proposed`, `plan` socket message). It
 * is persisted with the session, so the card survives a reload the replay buffer no longer covers.
 */
export interface PendingPlan {
  id: string;
  /** The agent's own summary of the plan (its last message). */
  summary: string;
  steps: PlanStep[];
  /** RFC 3339 timestamp. */
  created_at: string;
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
  | { type: 'turn_started'; turn_id: string; agent_id: string; model?: string }
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
  // A plan-mode turn ended with a plan that needs a decision before anything runs.
  | { type: 'plan_proposed'; plan_id: string; summary: string; steps: PlanStep[] }
  | { type: 'plan_cancelled'; plan_id: string }
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
  // Control-command replies (`/clear`, `/stop`, `/tools`, …) and workflow progress — outside any turn.
  | { type: 'notice'; message: string }
  // The message was accepted without starting a turn (workflow pending input, save-only): no
  // `turn_finished` follows, so this is the only signal that the composer may resume.
  | { type: 'turn_skipped' }
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
  // `model` pins this turn only; the session-level pin lives in the session meta.
  | { type: 'chat'; content: string; model?: { provider: string; model: string } }
  | { type: 'cancel' }
  | { type: 'select_project'; project_id: string | null }
  | { type: 'approval_response'; request_id: string; approved: boolean }
  // Arm the next turn for planning only; the runtime blocks every write until a plan is approved.
  | { type: 'plan_mode'; enabled: boolean }
  | { type: 'plan_decision'; approve: boolean; plan_id?: string }
  | { type: 'ping' };

export type AgentServerMessage =
  | { type: 'hello'; session_id: string; chat_id: string; last_seq: number }
  | { type: 'history'; chat_id: string; messages: HistoryMessage[] }
  | { type: 'replay'; events: SequencedEvent[] }
  | ({ type: 'event' } & SequencedEvent)
  | { type: 'approval_request'; request: ApprovalRequest }
  | { type: 'approval_result'; request_id: string; approved: boolean; message: string; responder?: string }
  | { type: 'project'; project: ProjectInfo | null }
  // Sent on connect: the plan waiting for approval in this conversation (`null` when none).
  | { type: 'plan'; plan: PendingPlan | null }
  | { type: 'pong' }
  | { type: 'error'; message: string };
