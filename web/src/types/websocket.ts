// WebSocket Message Types

// Client -> Server
export type WsClientMessage = 
  | { type: 'chat'; content: string }
  | { type: 'ping' };

// Server -> Client
export type WsServerMessage = 
  | { type: 'chat_response'; content: string; timestamp: string }
  | { type: 'error'; message: string }
  | { type: 'pong' }
  | { type: 'connected'; session_id: string }
  | { type: 'history'; messages: HistoryMessage[] };

export interface HistoryMessage {
  role: string;
  content: string;
  timestamp: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
}
