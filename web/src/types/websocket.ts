// WebSocket Message Types

// Client -> Server
export type WsClientMessage = 
  | { type: 'chat'; content: string }
  | { type: 'approval_response'; request_id: string; approved: boolean }
  | { type: 'ping' };

// Server -> Client
export type WsServerMessage = 
  | { type: 'chat_response'; content: string; timestamp: string }
  | { type: 'approval_request'; request: ApprovalRequest }
  | { type: 'approval_result'; request_id: string; approved: boolean; message: string }
  | { type: 'error'; message: string }
  | { type: 'pong' }
  | { type: 'connected'; session_id: string }
  | { type: 'history'; messages: HistoryMessage[] }
  | { type: 'tool_progress'; tool_name: string; status: string; result_preview: string };

export interface ApprovalRequest {
  id: string;
  session_id: string;
  channel: string;
  chat_id: string;
  command: string;
  working_dir: string;
  context: string;
  timestamp: string;
  timeout_secs: number;
}

export interface HistoryMessage {
  role: string;
  content: string;
  timestamp: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'approval' | 'tool_call' | 'tool_result' | 'turn';
  content: string;
  timestamp: string;
  approvalRequest?: ApprovalRequest;
  approvalResult?: {
    approved: boolean;
    message: string;
  };
  /** Assistant text is still streaming (protocol v2). */
  streaming?: boolean;
  /** Accumulated reasoning/thinking text (protocol v2). */
  reasoning?: string;
  /** Tool call lifecycle state (protocol v2). */
  toolStatus?: 'running' | 'success' | 'failure';
  toolDurationMs?: number;
  /** Tool name shown in the tool card header (protocol v2). */
  toolName?: string;
  /** Truncated call arguments, shown collapsed (protocol v2). */
  toolArgs?: string;
  /** Truncated tool output, shown collapsed (protocol v2). */
  toolResult?: string;
  /** Wall-clock duration of the finished turn (protocol v2). */
  turnDurationMs?: number;
  /** Token usage summed across the turn's model rounds (protocol v2). */
  turnTokens?: { prompt: number; completion: number; total: number };
  /** Why the turn ended: completed, cancelled, max_iterations, ... (protocol v2). */
  turnReason?: string;
  /** Files the turn touched (protocol v2 `file_changed`), later replaced by a diff view. */
  turnFiles?: { path: string; kind: string }[];
  /** Turn id — the key for `/api/checkpoints/{turn_id}` (diff + undo). */
  turnId?: string;
}
