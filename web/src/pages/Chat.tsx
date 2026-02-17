import { useState, useRef, useEffect } from 'react';
import { useWebSocket } from '../hooks/useWebSocket';
import { useI18n } from '../i18n/I18nContext';
import ApprovalRequest from '../components/ApprovalRequest';

export default function Chat() {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const { t } = useI18n();
  
  const wsUrl = `ws://${window.location.hostname}:${window.location.port || '8080'}/ws/chat`;
  const { connected, messages, toolProgressList, send, sendApprovalResponse, sessionId } = useWebSocket({
    url: wsUrl,
    autoConnect: true,
  });

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, toolProgressList]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!input.trim() || !connected) {
      return;
    }

    send(input);
    setInput('');
  };

  return (
    <div className="flex flex-col h-[calc(100vh-8rem)]">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-text">{t('chat.title')}</h1>
        <div className="mt-2 flex items-center gap-4">
          <div className="flex items-center gap-2">
            <div
              className={`w-2 h-2 rounded-full ${
                connected ? 'bg-success' : 'bg-error'
              }`}
            />
            <span className="text-sm text-text-secondary">
              {connected ? t('common.connected') : t('common.disconnected')}
            </span>
          </div>
          {sessionId && (
            <span className="text-sm text-text-secondary">
              {t('chat.session')}: {sessionId.slice(0, 8)}...
            </span>
          )}
        </div>
      </div>

      <div className="flex-1 bg-surface rounded-lg shadow overflow-hidden flex flex-col min-h-0 border border-border">
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {messages.length === 0 ? (
            <div className="text-center text-text-secondary mt-8">
              <p>{t('chat.noMessages')}</p>
            </div>
          ) : (
            messages.map((message) => (
              <div
                key={message.id}
                className={`flex ${
                  message.role === 'user' ? 'justify-end' : 'justify-start'
                }`}
              >
                {message.role === 'approval' && message.approvalRequest ? (
                  <ApprovalRequest
                    request={message.approvalRequest}
                    onApprove={(requestId) => sendApprovalResponse(requestId, true)}
                    onReject={(requestId) => sendApprovalResponse(requestId, false)}
                    result={message.approvalResult}
                  />
                ) : message.role === 'tool_call' || message.role === 'tool_result' ? (
                  <div className="max-w-[70%] rounded-lg px-3 py-1.5 bg-tool-execution text-text-secondary text-sm">
                    <div className="whitespace-pre-wrap break-words">{message.content}</div>
                    <div className="text-xs mt-0.5 opacity-70 text-text-secondary">
                      {new Date(message.timestamp).toLocaleTimeString()}
                    </div>
                  </div>
                ) : (
                  <div
                    className={`max-w-[70%] rounded-lg px-4 py-2 shadow-sm ${
                      message.role === 'user'
                        ? 'bg-primary text-white'
                        : 'bg-background border border-border text-text'
                    }`}
                  >
                    <div className="text-sm whitespace-pre-wrap break-words">
                      {message.content}
                    </div>
                    <div
                      className={`text-xs mt-1 opacity-70 ${
                        message.role === 'user' ? 'text-white' : 'text-text-secondary'
                      }`}
                    >
                      {new Date(message.timestamp).toLocaleTimeString()}
                    </div>
                  </div>
                )}
              </div>
            ))
          )}
          {toolProgressList.length > 0 && (
            <>
              {toolProgressList.map((item, idx) => (
                <div key={`tool-${idx}-${item.tool_name}`} className="flex justify-start">
                  <div className="max-w-[70%] rounded-lg px-3 py-1.5 bg-tool-execution text-text-secondary text-sm">
                    <span className="font-medium">{item.tool_name}</span>
                    <span className="mx-2">—</span>
                    <span>{item.status}</span>
                    {item.result_preview && (
                      <div className="mt-1 text-xs opacity-80 truncate max-w-md" title={item.result_preview}>
                        {item.result_preview}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </>
          )}
          <div ref={messagesEndRef} />
        </div>

        <div className="border-t border-border p-4">
          <form onSubmit={handleSubmit} className="flex gap-2">
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={
                connected
                  ? t('chat.typeMessage')
                  : t('chat.waitingConnection')
              }
              disabled={!connected}
              className="flex-1 px-4 py-2 border border-border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary bg-surface text-text disabled:bg-surface/50 disabled:cursor-not-allowed"
            />
            <button
              type="submit"
              disabled={!connected || !input.trim()}
              className="px-6 py-2 bg-primary text-white rounded-lg hover:bg-secondary disabled:bg-border disabled:cursor-not-allowed transition-colors"
            >
              {t('chat.send')}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
