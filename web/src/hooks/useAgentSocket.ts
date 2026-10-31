import { useCallback, useEffect, useRef, useState } from 'react';
import {
  AgentClientMessage,
  AgentEvent,
  AgentServerMessage,
  PlanStep,
  ProjectInfo,
  SequencedEvent,
  SubagentInfo,
} from '../types/agentEvents';
import { ChatMessage, HistoryMessage } from '../types/websocket';
import { endsTurn, isControlCommand } from './controlCommands';

interface UseAgentSocketOptions {
  url: string;
  autoConnect?: boolean;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

interface UseAgentSocketReturn {
  connected: boolean;
  /** A turn is in flight (between `turn_started` and `turn_finished`). */
  running: boolean;
  messages: ChatMessage[];
  sessionId: string | null;
  /** Folder this conversation works in (`null` = the global workspace). */
  project: ProjectInfo | null;
  /** Latest plan published by `plan_write` (also mirrored on the task board). */
  planSteps: PlanStep[];
  /** Bumped on every `plan_updated`, so panels can refetch the persisted board. */
  planVersion: number;
  /** Teammates seen on this connection (oldest first); `/api/subagents` has the full snapshot. */
  subagents: SubagentInfo[];
  send: (content: string) => void;
  cancel: () => void;
  sendApprovalResponse: (requestId: string, approved: boolean) => void;
  /** Switch workspace for the next turn; the server echoes the active project. */
  selectProject: (projectId: string | null) => void;
}

const KNOWN_ROLES: ChatMessage['role'][] = [
  'user',
  'assistant',
  'tool_call',
  'tool_result',
  'approval',
];

function historyToMessages(history: HistoryMessage[]): ChatMessage[] {
  return history.map((msg, index) => {
    const role = KNOWN_ROLES.includes(msg.role as ChatMessage['role'])
      ? (msg.role as ChatMessage['role'])
      : 'assistant';
    return {
      id: `history-${index}-${msg.timestamp}`,
      role,
      content: msg.content,
      timestamp: msg.timestamp,
    };
  });
}

/**
 * `/ws/agent` (protocol v2): streams deltas, tool lifecycles and turn boundaries.
 *
 * History is seeded once per connection; events carry a monotonic `seq`, so a reconnect asks for
 * `after_seq = <last applied>` and only receives what was missed.
 */
export function useAgentSocket({
  url,
  autoConnect = true,
  reconnectInterval = 3000,
  maxReconnectAttempts = 5,
}: UseAgentSocketOptions): UseAgentSocketReturn {
  const [connected, setConnected] = useState(false);
  const [running, setRunning] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [planSteps, setPlanSteps] = useState<PlanStep[]>([]);
  const [planVersion, setPlanVersion] = useState(0);
  const [subagents, setSubagents] = useState<SubagentInfo[]>([]);

  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const shouldReconnectRef = useRef(autoConnect);
  const isConnectingRef = useRef(false);
  const connectFnRef = useRef<(() => void) | null>(null);
  /** Last event seq applied locally; `null` until the first event arrives. */
  const lastSeqRef = useRef<number | null>(null);
  /** Id of the assistant bubble currently receiving deltas (reset per model round). */
  const activeAssistantRef = useRef<string | null>(null);
  /** Start of the current turn, used for the duration line rendered on `turn_finished`. */
  const turnStartedAtRef = useRef<number | null>(null);
  /** Token usage accumulated over the current turn's model rounds. */
  const turnUsageRef = useRef({ prompt: 0, completion: 0, total: 0 });
  /** Files touched by the current turn, in first-touch order. */
  const changedFilesRef = useRef<{ path: string; kind: string }[]>([]);

  const sendRaw = useCallback((message: AgentClientMessage) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return false;
    try {
      wsRef.current.send(JSON.stringify(message));
      return true;
    } catch (error) {
      console.error('Failed to send agent message:', error);
      return false;
    }
  }, []);

  const requestReplay = useCallback(() => {
    const after = lastSeqRef.current;
    if (after === null) return;
    sendRaw({ type: 'hello', after_seq: after });
  }, [sendRaw]);

  const appendAssistant = useCallback(
    (id: string, delta: string, field: 'content' | 'reasoning') => {
      setMessages((prev) => {
        const index = prev.findIndex((m) => m.id === id);
        if (index === -1) {
          const created: ChatMessage = {
            id,
            role: 'assistant',
            content: field === 'content' ? delta : '',
            reasoning: field === 'reasoning' ? delta : undefined,
            timestamp: new Date().toISOString(),
            streaming: true,
          };
          return [...prev, created];
        }
        const next = prev.slice();
        const current = next[index];
        next[index] =
          field === 'content'
            ? { ...current, content: current.content + delta, streaming: true }
            : { ...current, reasoning: (current.reasoning ?? '') + delta };
        return next;
      });
    },
    []
  );

  const ensureAssistantId = useCallback((turnId: string) => {
    if (activeAssistantRef.current) return activeAssistantRef.current;
    const id = `assistant-${turnId}-${Date.now()}`;
    activeAssistantRef.current = id;
    return id;
  }, []);

  const applyEvent = useCallback(
    (event: AgentEvent) => {
      switch (event.type) {
        case 'turn_started':
          activeAssistantRef.current = null;
          turnStartedAtRef.current = Date.now();
          turnUsageRef.current = { prompt: 0, completion: 0, total: 0 };
          changedFilesRef.current = [];
          setRunning(true);
          break;

        case 'message_delta':
          appendAssistant(ensureAssistantId(event.turn_id), event.delta, 'content');
          break;

        case 'reasoning_delta':
          appendAssistant(ensureAssistantId(event.turn_id), event.delta, 'reasoning');
          break;

        case 'tool_call_started': {
          activeAssistantRef.current = null;
          const id = `tool-${event.call_id || `${event.turn_id}-${event.tool_name}`}`;
          setMessages((prev) => {
            if (prev.some((m) => m.id === id)) return prev;
            return [
              ...prev,
              {
                id,
                role: 'tool_call',
                content: event.tool_name,
                timestamp: new Date().toISOString(),
                toolName: event.tool_name,
                toolArgs: event.args_preview,
                toolStatus: 'running',
              },
            ];
          });
          break;
        }

        case 'tool_call_finished': {
          activeAssistantRef.current = null;
          const id = `tool-${event.call_id || `${event.turn_id}-${event.tool_name}`}`;
          setMessages((prev) => {
            const next = prev.slice();
            const index = next.findIndex((m) => m.id === id);
            const patch = {
              toolName: event.tool_name,
              toolResult: event.result_preview,
              toolStatus: event.status,
              toolDurationMs: event.duration_ms,
            };
            if (index === -1) {
              next.push({
                id,
                role: 'tool_call',
                content: event.tool_name,
                timestamp: new Date().toISOString(),
                ...patch,
              });
            } else {
              next[index] = { ...next[index], ...patch };
            }
            return next;
          });
          break;
        }

        case 'usage':
          turnUsageRef.current = {
            prompt: turnUsageRef.current.prompt + event.prompt_tokens,
            completion: turnUsageRef.current.completion + event.completion_tokens,
            total: turnUsageRef.current.total + event.total_tokens,
          };
          break;

        case 'approval_requested': {
          const request = event.request;
          setMessages((prev) => [
            ...prev,
            {
              id: `approval-${request.id}`,
              role: 'approval',
              content: '',
              timestamp: request.timestamp,
              approvalRequest: request,
            },
          ]);
          break;
        }

        case 'error':
          // Terminal for the turn (`turn_finished` follows); clearing here as well keeps the
          // composer usable if a turn ever dies without publishing its end event.
          setRunning(false);
          setMessages((prev) => [
            ...prev,
            {
              id: `error-${event.turn_id}-${Date.now()}`,
              role: 'assistant',
              content: `[error] ${event.message}`,
              timestamp: new Date().toISOString(),
            },
          ]);
          break;

        // Control-command replies live outside any turn (`src/agent/events.rs`): show them as a
        // system line instead of dropping them, which made `/clear` look like a freeze.
        case 'notice':
          setMessages((prev) => [
            ...prev,
            {
              id: `notice-${Date.now()}-${prev.length}`,
              role: 'notice',
              content: event.message,
              timestamp: new Date().toISOString(),
            },
          ]);
          break;

        case 'file_changed': {
          const files = changedFilesRef.current;
          const existing = files.findIndex((f) => f.path === event.path);
          if (existing === -1) {
            files.push({ path: event.path, kind: event.change_kind });
          } else {
            files[existing] = { path: event.path, kind: event.change_kind };
          }
          break;
        }

        case 'turn_finished': {
          activeAssistantRef.current = null;
          const startedAt = turnStartedAtRef.current;
          const usage = turnUsageRef.current;
          const files = changedFilesRef.current;
          turnStartedAtRef.current = null;
          changedFilesRef.current = [];
          setRunning(false);
          setMessages((prev) => {
            const next = prev.map((m) => (m.streaming ? { ...m, streaming: false } : m));
            return [
              ...next,
              {
                id: `turn-${event.turn_id}`,
                role: 'turn',
                content: '',
                timestamp: new Date().toISOString(),
                turnDurationMs: startedAt === null ? undefined : Date.now() - startedAt,
                turnTokens: usage.total > 0 ? { ...usage } : undefined,
                turnReason: event.reason,
                turnId: event.turn_id,
                turnFiles: files.length > 0 ? [...files] : undefined,
              },
            ];
          });
          break;
        }

        // The plan is persisted server-side; keep the live copy for chat-side rendering and bump
        // a version so the task board refetches the file (ids/timestamps live there).
        case 'plan_updated': {
          setPlanSteps(event.steps);
          setPlanVersion((v) => v + 1);
          break;
        }

        // Teammates: upsert by id so the panel shows live progress without a refetch.
        case 'subagent_updated': {
          setSubagents((prev) => {
            const index = prev.findIndex((agent) => agent.id === event.subagent.id);
            const next = prev.slice();
            if (index === -1) next.push(event.subagent);
            else next[index] = event.subagent;
            return next.sort((a, b) => a.created_at.localeCompare(b.created_at));
          });
          break;
        }

        default:
          break;
      }
    },
    [appendAssistant, ensureAssistantId]
  );

  const applySequenced = useCallback(
    (sequenced: SequencedEvent) => {
      const last = lastSeqRef.current;
      if (last !== null && sequenced.seq > last + 1) {
        // Dropped events (broadcast lag): ask for the missing range before continuing.
        requestReplay();
      }
      lastSeqRef.current = sequenced.seq;
      applyEvent(sequenced.event);
    },
    [applyEvent, requestReplay]
  );

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;
    if (wsRef.current?.readyState === WebSocket.CONNECTING || isConnectingRef.current) return;

    try {
      isConnectingRef.current = true;
      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        setConnected(true);
        reconnectAttemptsRef.current = 0;
        isConnectingRef.current = false;
        // Reconnect: catch up from the last applied seq (skipped on the very first connection,
        // where history already covers the past).
        requestReplay();
      };

      ws.onmessage = (event) => {
        try {
          const message: AgentServerMessage = JSON.parse(event.data);
          switch (message.type) {
            case 'hello':
              setSessionId(message.session_id);
              break;

            case 'project':
              setProject(message.project);
              break;

            case 'history':
              setMessages(historyToMessages(message.messages));
              break;

            case 'replay':
              for (const item of message.events) applySequenced(item);
              break;

            case 'event':
              applySequenced({
                seq: message.seq,
                channel: message.channel,
                chat_id: message.chat_id,
                event: message.event,
              });
              break;

            case 'approval_request':
              setMessages((prev) =>
                prev.some((m) => m.id === `approval-${message.request.id}`)
                  ? prev
                  : [
                      ...prev,
                      {
                        id: `approval-${message.request.id}`,
                        role: 'approval',
                        content: '',
                        timestamp: message.request.timestamp,
                        approvalRequest: message.request,
                      },
                    ]
              );
              break;

            case 'approval_result':
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === `approval-${message.request_id}`
                    ? {
                        ...m,
                        approvalResult: {
                          approved: message.approved,
                          message: message.message,
                          responder: message.responder,
                        },
                      }
                    : m
                )
              );
              break;

            case 'error':
              console.error('Agent socket error:', message.message);
              break;

            case 'pong':
              break;
          }
        } catch (error) {
          console.error('Failed to parse agent message:', error);
        }
      };

      ws.onerror = (error) => {
        console.error('Agent WebSocket error:', error);
        isConnectingRef.current = false;
      };

      ws.onclose = () => {
        setConnected(false);
        wsRef.current = null;
        isConnectingRef.current = false;
        if (
          shouldReconnectRef.current &&
          reconnectAttemptsRef.current < maxReconnectAttempts
        ) {
          reconnectAttemptsRef.current += 1;
          const delay = reconnectInterval * Math.pow(2, reconnectAttemptsRef.current - 1);
          reconnectTimeoutRef.current = setTimeout(() => {
            connectFnRef.current?.();
          }, delay);
        }
      };
    } catch (error) {
      console.error('Failed to create agent WebSocket:', error);
      isConnectingRef.current = false;
    }
  }, [url, reconnectInterval, maxReconnectAttempts, requestReplay, applySequenced]);

  const send = useCallback(
    (content: string) => {
      if (!sendRaw({ type: 'chat', content })) {
        console.error('Agent socket is not connected');
        return;
      }
      // Only a real turn is bracketed by `turn_started`/`turn_finished`. Control commands are
      // answered inline by the daemon, so setting `running` for them would never be cleared and
      // would leave the composer disabled (`/clear` was the worst case: nothing was running).
      if (endsTurn(content)) setRunning(false);
      else if (!isControlCommand(content)) setRunning(true);
      setMessages((prev) => [
        ...prev,
        {
          id: `${new Date().toISOString()}-user-${Date.now()}`,
          role: 'user',
          content,
          timestamp: new Date().toISOString(),
        },
      ]);
    },
    [sendRaw]
  );

  const cancel = useCallback(() => {
    sendRaw({ type: 'cancel' });
    // A cancelled turn publishes `turn_finished`; `/stop` with nothing running only answers with a
    // notice, so clearing the flag here is what un-sticks a UI that already thinks it is running.
    setRunning(false);
  }, [sendRaw]);

  const sendApprovalResponse = useCallback(
    (requestId: string, approved: boolean) => {
      sendRaw({ type: 'approval_response', request_id: requestId, approved });
    },
    [sendRaw]
  );

  const selectProject = useCallback(
    (projectId: string | null) => {
      sendRaw({ type: 'select_project', project_id: projectId });
    },
    [sendRaw]
  );

  connectFnRef.current = connect;

  useEffect(() => {
    if (autoConnect) {
      shouldReconnectRef.current = true;
      connect();
    }

    return () => {
      shouldReconnectRef.current = false;
      isConnectingRef.current = false;
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
      if (wsRef.current) {
        const ws = wsRef.current;
        wsRef.current = null;
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
    running,
    messages,
    sessionId,
    project,
    planSteps,
    planVersion,
    subagents,
    send,
    cancel,
    sendApprovalResponse,
    selectProject,
  };
}
