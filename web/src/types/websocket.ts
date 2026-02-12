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
  | { type: 'history'; messages: HistoryMessage[] };

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
  role: 'user' | 'assistant' | 'approval';
  content: string;
  timestamp: string;
  approvalRequest?: ApprovalRequest;
  approvalResult?: {
    approved: boolean;
    message: string;
  };
}
