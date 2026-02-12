import { useState, useEffect, useRef, useCallback } from 'react';
import { WsClientMessage, WsServerMessage, ChatMessage } from '../types/websocket';

interface UseWebSocketOptions {
  url: string;
  autoConnect?: boolean;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

interface UseWebSocketReturn {
  connected: boolean;
  messages: ChatMessage[];
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
  
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const shouldReconnectRef = useRef(autoConnect);

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      return;
    }

    try {
      // Web channel uses a fixed global session for management
      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        console.log('WebSocket connected');
        setConnected(true);
        reconnectAttemptsRef.current = 0;
      };

      ws.onmessage = (event) => {
        try {
          const message: WsServerMessage = JSON.parse(event.data);
          
          switch (message.type) {
            case 'connected':
              setSessionId(message.session_id);
              break;
            
            case 'history':
              // Load history messages
              const historyMessages: ChatMessage[] = message.messages.map((msg, index) => ({
                id: `history-${index}-${msg.timestamp}`,
                role: msg.role as 'user' | 'assistant',
                content: msg.content,
                timestamp: msg.timestamp,
              }));
              setMessages(historyMessages);
              break;
            
            case 'chat_response':
              setMessages((prev) => [
                ...prev,
                {
                  id: `${Date.now()}-assistant`,
                  role: 'assistant',
                  content: message.content,
                  timestamp: message.timestamp,
                },
              ]);
              break;
            
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
      };

      ws.onclose = () => {
        console.log('WebSocket disconnected');
        setConnected(false);
        wsRef.current = null;

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
            connect();
          }, delay);
        }
      };
    } catch (error) {
      console.error('Failed to create WebSocket connection:', error);
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
      wsRef.current.send(JSON.stringify(message));
      
      setMessages((prev) => [
        ...prev,
        {
          id: `${Date.now()}-user`,
          role: 'user',
          content,
          timestamp: new Date().toISOString(),
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

  useEffect(() => {
    if (autoConnect) {
      shouldReconnectRef.current = true;
      connect();
    }

    return () => {
      shouldReconnectRef.current = false;
      
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, [autoConnect, connect]);

  return {
    connected,
    messages,
    send,
    sendApprovalResponse,
    disconnect,
    connect,
    sessionId,
  };
}
