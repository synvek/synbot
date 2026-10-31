import { useEffect, useMemo, useRef, useState } from 'react';
import { useAgentSocket } from '../hooks/useAgentSocket';
import { useI18n } from '../i18n/I18nContext';
import { notifyDesktop } from '../desktop/bridge';
import { apiClient } from '../api';
import type { CommandInfo } from '../types/api';
import { ChatSurface } from '../features/chat';

/**
 * The chat page is the socket's owner and nothing else: every pixel of the workbench is a
 * presentational component under `features/chat`, so the protocol layer stays untouched by the
 * redesign.
 */
export default function Chat() {
  const { t } = useI18n();
  /** Workspace slash-command templates for the composer picker. */
  const [commands, setCommands] = useState<CommandInfo[]>([]);

  // Same-origin on the web, absolute loopback URL plus the desktop token in the app.
  const wsUrl = useMemo(() => apiClient.wsUrl('/ws/agent'), []);
  const {
    connected,
    running,
    messages,
    send,
    cancel,
    sendApprovalResponse,
    project,
    planVersion,
    subagents,
    selectProject,
  } = useAgentSocket({
    url: wsUrl,
    autoConnect: true,
  });

  /**
   * Desktop only: a turn that finishes while the window is in the background raises a system
   * notification, so long runs do not have to be watched.
   */
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running && document.hidden) {
      const lastReply = [...messages].reverse().find((message) => message.role === 'assistant');
      const preview = (lastReply?.content ?? '').replace(/\s+/g, ' ').trim().slice(0, 200);
      void notifyDesktop(
        t('desktop.turnFinishedTitle', 'Synbot 已完成'),
        preview || t('desktop.turnFinishedBody', '本轮已结束，回到 Synbot 查看结果。')
      );
    }
    wasRunning.current = running;
  }, [running, messages, t]);

  // Workspace commands follow the selected folder: switching projects rescans `.synbot/commands`.
  useEffect(() => {
    let active = true;
    apiClient
      .getCommands(project?.id)
      .then((list) => {
        if (active) setCommands(list);
      })
      .catch(() => {
        if (active) setCommands([]);
      });
    return () => {
      active = false;
    };
  }, [project]);

  return (
    <ChatSurface
      connected={connected}
      running={running}
      messages={messages}
      project={project}
      planVersion={planVersion}
      subagents={subagents}
      commands={commands}
      onSend={send}
      onStop={cancel}
      onApprove={sendApprovalResponse}
      onSelectProject={selectProject}
    />
  );
}
