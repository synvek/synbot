import { useChatSession } from '../components/chat-session/ChatSessionContext';
import { ChatSurface } from '../features/chat';

/**
 * The chat page is assembly and nothing else: the conversation lives in the shell
 * (`ChatSessionContext`), so switching to an admin page and back keeps the transcript, the
 * running turn, and the socket alive. Every pixel below is a presentational component under
 * `features/chat`.
 */
export default function Chat() {
  const {
    connected,
    running,
    messages,
    project,
    workspaceId,
    projects,
    modelOverride,
    pendingPlan,
    planApproved,
    planMode,
    planVersion,
    subagents,
    commands,
    send,
    cancel,
    approve,
    decidePlan,
    setPlanMode,
    setModelOverride,
    selectWorkspace,
  } = useChatSession();

  return (
    <ChatSurface
      connected={connected}
      running={running}
      messages={messages}
      project={project}
      workspaceId={workspaceId}
      projects={projects}
      model={modelOverride}
      pendingPlan={pendingPlan}
      planApproved={planApproved}
      planMode={planMode}
      planVersion={planVersion}
      subagents={subagents}
      commands={commands}
      onSend={send}
      onStop={cancel}
      onApprove={approve}
      onDecidePlan={decidePlan}
      onTogglePlanMode={setPlanMode}
      onSelectWorkspace={selectWorkspace}
      onSelectModel={(next) => {
        void setModelOverride(next);
      }}
    />
  );
}
