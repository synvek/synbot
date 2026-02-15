import { useState, useEffect, useRef, useCallback } from 'react';
import { WsClientMessage, WsServerMessage, ChatMessage } from '../types/websocket';

interface UseWebSocketOptions {
  url: string;
  autoConnect?: boolean;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

export interface ToolProgressState {
  tool_name: string;
  status: string;
  result_preview: string;
}

interface UseWebSocketReturn {
  connected: boolean;
  messages: ChatMessage[];
  /** Accumulated tool progress for current turn (cleared when chat_response arrives) */
  toolProgressList: ToolProgressState[];
  send: (content: string) => void;
  sendApprovalResponse: (requestId: string, approved: boolean) => void;
  disconnect: () => void;
  connect: () => void;
  sessionId: string | null;
}

export function useWebSocket({
  url,
  autoConnect = true,
  reconnectInterval = 3000,
  maxReconnectAttempts = 5,
}: UseWebSocketOptions): UseWebSocketReturn {
  const [connected, setConnected] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [toolProgressList, setToolProgressList] = useState<ToolProgressState[]>([]);

  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const shouldReconnectRef = useRef(autoConnect);
  const isConnectingRef = useRef(false);
  const connectFnRef = useRef<(() => void) | null>(null);
  const toolProgressListRef = useRef<ToolProgressState[]>([]);
  toolProgressListRef.current = toolProgressList;

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      console.log('WebSocket already connected, skipping');
      return;
    }

    // Prevent multiple simultaneous connection attempts
    if (wsRef.current?.readyState === WebSocket.CONNECTING || isConnectingRef.current) {
      console.log('WebSocket connection in progress, skipping');
      return;
    }

    try {
      console.log('Creating new WebSocket connection to:', url);
      isConnectingRef.current = true;
      
      // Web channel uses a fixed global session for management
      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        console.log('WebSocket connected');
        setConnected(true);
        reconnectAttemptsRef.current = 0;
        isConnectingRef.current = false;
      };

      ws.onmessage = (event) => {
        console.log('WebSocket message received:', event.data.substring(0, 100));
        try {
          const message: WsServerMessage = JSON.parse(event.data);
          
          switch (message.type) {
            case 'connected':
              console.log('Connected with session_id:', message.session_id);
              setSessionId(message.session_id);
              break;
            
            case 'history':
              console.log('Received history with', message.messages.length, 'messages');
              const historyMessages: ChatMessage[] = message.messages.map((msg, index) => {
                const role = (['user', 'assistant', 'tool_call', 'tool_result', 'approval'] as const).includes(msg.role as any)
                  ? (msg.role as ChatMessage['role'])
                  : 'assistant';
                return {
                  id: `history-${index}-${msg.timestamp}`,
                  role,
                  content: msg.content,
                  timestamp: msg.timestamp,
                };
              });
              setMessages(historyMessages);
              break;
            
            case 'tool_progress': {
              const item = {
                tool_name: message.tool_name,
                status: message.status,
                result_preview: message.result_preview,
              };
              setToolProgressList((prev) => {
                const next = [...prev, item];
                toolProgressListRef.current = next;
                return next;
              });
              break;
            }

            case 'chat_response': {
              const steps = toolProgressListRef.current;
              setToolProgressList([]);
              console.log('Received chat_response:', {
                content: message.content.substring(0, 50),
                timestamp: message.timestamp,
              });
              setMessages((prev) => {
                const exists = prev.some(msg =>
                  msg.role === 'assistant' && msg.content === message.content
                );
                if (exists) return prev;
                const stepMessages: ChatMessage[] = steps.map((item, idx) => {
                  const content = item.result_preview
                    ? `${item.tool_name} — ${item.status}\n${item.result_preview}`
                    : `${item.tool_name} — ${item.status}`;
                  return {
                    id: `tool-${Date.now()}-${idx}-${item.tool_name}`,
                    role: 'tool_call' as const,
                    content,
                    timestamp: new Date().toISOString(),
                  };
                });
                return [
                  ...prev,
                  ...stepMessages,
                  {
                    id: `${message.timestamp}-assistant-${Date.now()}`,
                    role: 'assistant',
                    content: message.content,
                    timestamp: message.timestamp,
                  },
                ];
              });
              break;
            }
            
            case 'approval_request':
              setMessages((prev) => [
                ...prev,
                {
                  id: `approval-${message.request.id}`,
                  role: 'approval',
                  content: '',
                  timestamp: message.request.timestamp,
                  approvalRequest: message.request,
                },
              ]);
              break;
            
            case 'approval_result':
              // Update the approval message with the result
              setMessages((prev) =>
                prev.map((msg) =>
                  msg.id === `approval-${message.request_id}`
                    ? {
                        ...msg,
                        approvalResult: {
                          approved: message.approved,
                          message: message.message,
                        },
                      }
                    : msg
                )
              );
              break;
            
            case 'error':
              console.error('WebSocket error message:', message.message);
              break;
            
            case 'pong':
              break;
          }
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error);
        }
      };

      ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        isConnectingRef.current = false;
      };

      ws.onclose = () => {
        console.log('WebSocket disconnected');
        setConnected(false);
        wsRef.current = null;
        isConnectingRef.current = false;

        if (
          shouldReconnectRef.current &&
          reconnectAttemptsRef.current < maxReconnectAttempts
        ) {
          reconnectAttemptsRef.current += 1;
          const delay = reconnectInterval * Math.pow(2, reconnectAttemptsRef.current - 1);
          
          console.log(
            `Reconnecting in ${delay}ms (attempt ${reconnectAttemptsRef.current}/${maxReconnectAttempts})`
          );
          
          reconnectTimeoutRef.current = setTimeout(() => {
            connectFnRef.current?.();
          }, delay);
        }
      };
    } catch (error) {
      console.error('Failed to create WebSocket connection:', error);
      isConnectingRef.current = false;
    }
  }, [url, reconnectInterval, maxReconnectAttempts]);

  const disconnect = useCallback(() => {
    shouldReconnectRef.current = false;
    
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    
    setConnected(false);
    setSessionId(null);
  }, []);

  const send = useCallback((content: string) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      console.error('WebSocket is not connected');
      return;
    }

    const message: WsClientMessage = {
      type: 'chat',
      content,
    };

    try {
      const timestamp = new Date().toISOString();
      wsRef.current.send(JSON.stringify(message));
      
      setMessages((prev) => [
        ...prev,
        {
          id: `${timestamp}-user`,
          role: 'user',
          content,
          timestamp,
        },
      ]);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, []);

  const sendApprovalResponse = useCallback((requestId: string, approved: boolean) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
      console.error('WebSocket is not connected');
      return;
    }

    const message: WsClientMessage = {
      type: 'approval_response',
      request_id: requestId,
      approved,
    };

    try {
      wsRef.current.send(JSON.stringify(message));
      console.log(`Sent approval response: ${approved ? 'approved' : 'rejected'}`);
    } catch (error) {
      console.error('Failed to send approval response:', error);
    }
  }, []);

  // Store connect function in ref to avoid dependency issues
  connectFnRef.current = connect;

  useEffect(() => {
    if (autoConnect) {
      shouldReconnectRef.current = true;
      connect();
    }

    return () => {
      console.log('useWebSocket cleanup: closing connection');
      shouldReconnectRef.current = false;
      isConnectingRef.current = false;
      
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
      
      if (wsRef.current) {
        // Close the WebSocket connection immediately
        const ws = wsRef.current;
        wsRef.current = null;
        
        // Remove event handlers to prevent them from firing during cleanup
        ws.onopen = null;
        ws.onmessage = null;
        ws.onerror = null;
        ws.onclose = null;
        
        if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
          ws.close();
        }
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoConnect]);

  return {
    connected,
    messages,
    toolProgressList,
    send,
    sendApprovalResponse,
    disconnect,
    connect,
    sessionId,
  };
}
